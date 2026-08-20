/**
 * Oracle case — the vault conversation-summary search's timeRange staging
 * (P4.d13 unit 4: v4 `searchVaultConversationSummaries`,
 * lib/memory/conversation-summary-search.ts).
 *
 * Drives v4's REAL function over a fresh copy of the committed episodic-recall
 * fixture (dated summary docs + TF-IDF-embedded chunks baked by
 * build-episodic-recall-fixture.ts). No mocks at all — embeddings run through
 * the fitted BUILTIN profile; the chunk search, frontmatter reads, readCap,
 * overlapsWindow, and the two-stage boost staging are all v4's real code.
 *
 * P4.D95 (v4 `870a57fa`) adds three `precomputedEmbedding` arms: the vector for
 * this query text (identical result), the vector for a DIFFERENT sentence (the
 * list must follow the vector, not the text), and a mismatched width (an empty
 * list via the document-search guard, not an error).
 *
 * Emits one NDJSON row per case:
 *   { name, matches: [{conversationId, conversationTitle, relativePath, score,
 *     firstMessageAt, lastMessageAt}] }
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ER_MAIN=$W/crates/quilltap-web/tests/fixtures/episodic-recall-main.db \
 *   QT_FIXTURE_ER_MOUNT=$W/crates/quilltap-web/tests/fixtures/episodic-recall-mount.db \
 *     $N/npx tsx $W/harness/oracle/cases/vault-conv-search.ts \
 *     > /tmp/oracle-vault-conv.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Case {
  name: string;
  query: string;
  limit: number;
  excludeConversationId?: string;
  timeRange?: { from: string; to: string } | null;
}

// The SAME case table the Rust side mirrors (vault_conv_search_equivalence.rs).
export const CASES: Case[] = [
  { name: 'no_window', query: 'the quay market at dawn', limit: 3 },
  {
    name: 'window_hard',
    query: 'the quay market at dawn',
    limit: 1,
    timeRange: { from: '2026-06-01T00:00:00.000Z', to: '2026-06-03T23:59:59.999Z' },
  },
  {
    name: 'window_starved_boost',
    // Matches BOTH dated docs (lockers → Winter, market/dawn → Quay Market);
    // only the June doc is in-window, so the starved arm's ×1.3 must lift it
    // over the stronger raw match — the boost is VISIBLE in the ordering.
    query: 'mending the sail lockers by the quay market at dawn',
    limit: 3,
    timeRange: { from: '2026-06-01T00:00:00.000Z', to: '2026-06-07T23:59:59.999Z' },
  },
  {
    name: 'window_unparsable',
    query: 'the quay market at dawn',
    limit: 3,
    timeRange: { from: 'whenever', to: '2026-06-07T23:59:59.999Z' },
  },
  {
    name: 'window_inverted',
    query: 'the quay market at dawn',
    limit: 3,
    timeRange: { from: '2026-06-08T00:00:00.000Z', to: '2026-06-01T00:00:00.000Z' },
  },
  { name: 'limit_zero', query: 'the quay market at dawn', limit: 0 },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'episodic-recall.json'), 'utf8'),
  ) as { testPepperBase64: string; userId: string; elowenId: string };

  const mainFixture = process.env.QT_FIXTURE_ER_MAIN;
  const mountFixture = process.env.QT_FIXTURE_ER_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_ER_MAIN and QT_FIXTURE_ER_MOUNT must point at the fixtures');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-conv-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main-work.db');
  const mountWork = join(scratch, 'mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();

  const { searchVaultConversationSummaries } = await import(
    '@/lib/memory/conversation-summary-search'
  );

  // The exclude case pins the top no-window match's id at runtime (stable —
  // the fixture's summaries carry fixed conversation ids).
  const baseline = await searchVaultConversationSummaries({
    characterId: spec.elowenId,
    query: 'the quay market at dawn',
    userId: spec.userId,
    limit: 3,
  });
  const excludeId = baseline[0]?.conversationId ?? 'none';

  const lines: string[] = [];
  for (const c of CASES) {
    const matches = await searchVaultConversationSummaries({
      characterId: spec.elowenId,
      query: c.query,
      userId: spec.userId,
      limit: c.limit,
      excludeConversationId: c.excludeConversationId,
      timeRange: c.timeRange,
    });
    lines.push(JSON.stringify({ name: c.name, matches }));
  }
  // The exclude arm (the top baseline id excluded).
  const excluded = await searchVaultConversationSummaries({
    characterId: spec.elowenId,
    query: 'the quay market at dawn',
    userId: spec.userId,
    limit: 3,
    excludeConversationId: excludeId,
  });
  lines.push(JSON.stringify({ name: 'exclude_top', matches: excluded, excludeId }));

  // ---- P4.D95 (v4 `870a57fa`): the precomputed-embedding arms. -------------
  // The vector is embedded here through v4's REAL builtin TF-IDF profile — the
  // same profile the Rust side embeds with — so passing it in exercises the
  // reuse seam without inventing numbers.
  const { generateEmbeddingForUser } = await import('@/lib/embedding/embedding-service');
  const embedOf = async (text: string): Promise<Float32Array> =>
    (await generateEmbeddingForUser(text, spec.userId)).embedding;

  // (1) the vector for THIS query text — must equal the embed-it-yourself arm.
  const sameVec = await embedOf('the quay market at dawn');
  lines.push(
    JSON.stringify({
      name: 'precomputed_same_query',
      matches: await searchVaultConversationSummaries({
        characterId: spec.elowenId,
        query: 'the quay market at dawn',
        userId: spec.userId,
        limit: 3,
        precomputedEmbedding: sameVec,
      }),
    }),
  );

  // (2) the vector for a DIFFERENT sentence while `query` stays put: the list
  //     must follow the VECTOR, so a re-embed of `query` would score the other
  //     ranking and diverge.
  const otherVec = await embedOf('mending the sail lockers');
  lines.push(
    JSON.stringify({
      name: 'precomputed_other_query',
      matches: await searchVaultConversationSummaries({
        characterId: spec.elowenId,
        query: 'the quay market at dawn',
        userId: spec.userId,
        limit: 3,
        precomputedEmbedding: otherVec,
      }),
    }),
  );

  // (3) a mismatched width — the document-search guard yields an EMPTY list
  //     rather than nonsense (and never throws).
  lines.push(
    JSON.stringify({
      name: 'precomputed_wrong_dimension',
      matches: await searchVaultConversationSummaries({
        characterId: spec.elowenId,
        query: 'the quay market at dawn',
        userId: spec.userId,
        limit: 3,
        precomputedEmbedding: new Float32Array([0.1, 0.2]),
      }),
    }),
  );

  await closeDatabase();
  for (const l of lines) process.stdout.write(l + '\n');
}

main().catch((err) => {
  process.stderr.write(`vault-conv-search oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
