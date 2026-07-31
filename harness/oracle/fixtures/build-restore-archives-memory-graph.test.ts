/**
 * @jest-environment node
 *
 * P4.D31 restore-fixture builder — the ONE archive the memory-id contract needs.
 *
 * ── WHY A THIRD BUILDER FILE ─────────────────────────────────────────────────
 * `build-restore-archives.test.ts` writes the five original committed archives
 * and `build-restore-archives-dedupe.test.ts` the two P4.d23 ones. Re-running
 * either rewrites its whole set (every archive carries fresh `createdAt` /
 * manifest stamps), invalidating oracles this lane has no business moving. This
 * file therefore writes ONLY `restore-archive-memory-graph.zip` and never opens
 * the other seven. Additive, exactly like P4.d23's.
 *
 * ── WHY A NEW ARCHIVE AT ALL ─────────────────────────────────────────────────
 * v4 `4ac66c29` fixed the one entity restore renamed on the way in: memories
 * were created without their backed-up id, so the repository minted a fresh one.
 * Memories point at each other through `relatedMemoryIds`, so every edge came
 * back dangling and the Commonplace Book's graph quietly flattened.
 *
 * **None of the seven committed archives can see that.** All seven carry exactly
 * two memories with `relatedMemoryIds: []`:
 *
 *   - in `replace` mode the archived id survives verbatim, so the state diff
 *     catches a fresh mint — but only because the id is a literal in the archive;
 *   - in `new-account` mode it cannot. The archived ids are remapped before the
 *     restore applies, so BOTH a correct remapped id and an incorrectly minted
 *     one are UUIDs absent from the archive, and the differential's origin-based
 *     normalizer collapses each to `<minted-N>`. Mutation-testing the port
 *     against the existing archives confirmed it: reverting the fix reddened the
 *     four `replace` cases and left every `new-account` case GREEN.
 *
 * A populated edge is what makes the `new-account` arm sensitive. uuid-remap
 * rewrites `id` and `relatedMemoryIds` through ONE shared memo, so a correct
 * restore lands a row whose id is the SAME UUID as the edge pointing at it —
 * the normalizer gives both the same `<minted-N>` label — while a fresh mint
 * makes them two different labels. That is the whole detector, and it needs a
 * graph in the archive to exist.
 *
 * ── THE GRAPH ────────────────────────────────────────────────────────────────
 * Four memories on top of the fixture's own two, over the fixture's two
 * characters (Lorian `a1…0001`, Riya `a1…0002`):
 *
 *   G1  Lorian, in chat c1…0001   → [G2, G3]
 *   G2  Lorian                    → [G1]           back-edge (G1 ↔ G2)
 *   G3  Riya, ABOUT Lorian        → [G1, G4]       crosses `aboutCharacterId`,
 *                                                  and crosses character scope
 *   G4  Riya, ABOUT Lorian        → [G3]           back-edge (G3 ↔ G4)
 *
 * So: edges in both directions, an edge between two DIFFERENT characters'
 * memories, and two memories carrying `aboutCharacterId`. G3/G4 also populate
 * the episodic columns (`occurredAt`, `narrativeTime`, `entities`, `kind`) and
 * `witnessedContext` / `lastReinforcedAt` / `keywords`, so the state diff also
 * exercises this cycle's newer memory columns end to end.
 *
 * The rows are written through v4's **REAL** `repos.memories.create(data, {id})`
 * — the very call whose `CreateOptions` forwarding `4ac66c29` restored — and the
 * archive by v4's **REAL** `createBackup`, like all seven others. The restore
 * claim must never depend on v5's zip writer. The instance starts from the
 * committed `system-data-*` fixture family (test-pepper synthetic); those files
 * are only ever COPIED, never written.
 *
 * Run (Node 24, from a v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-restore-archives-memgraph
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/fixtures/build-restore-archives-memory-graph.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd /private/tmp/qt-v4-pin-p4d31-ff12f491
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ARCHIVE_OUT=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- build-restore-archives-memory-graph
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const IMAGE_STORAGE_KEY = (userId: string) => `${userId}/portrait.png`;
const IMAGE_BYTES = Buffer.from('quilltap-fixture-portrait-bytes\n', 'utf8');

/** The fixture's own two characters. */
const LORIAN = 'a1000000-0000-4000-8000-000000000001';
const RIYA = 'a1000000-0000-4000-8000-000000000002';
/** The fixture's own chat, so G1 carries a resolving `chatId` too. */
const CHAT = 'c1000000-0000-4000-8000-000000000001';

/**
 * ── THE SIXTEEN 4.8 COLUMNS ──────────────────────────────────────────────────
 *
 * `4ac66c29` also carries v4's release-checklist claim that all sixteen columns
 * added this cycle ride the schema-driven backup pipeline, pinned v4-side by its
 * new `restore-field-fidelity.test.ts`. v5's side of that claim is the
 * `system_restore_state` row diff — but a diff can only see a column the ARCHIVE
 * populates: where a column is absent, both engines write the same default and a
 * port that dropped it passes.
 *
 * Audited at P4.D31: of the sixteen, only the four `memories` columns were
 * reachable, and only because THIS builder sets them. Ten (`chats` ×4,
 * `chat_messages` ×6) are absent from all seven committed archives, and the two
 * `chat_settings` ones sit at their defaults. So the four bags below give every
 * one of the sixteen a non-default value in this archive, which is what turns
 * v5's half of the claim from inspection into measurement.
 *
 * The values mirror v4's own test bags where the shape allows; they are written
 * through v4's REAL `update` / `updateMessage` / `updateForUser`, so each one is
 * Zod-valid before `createBackup` ever sees it.
 */
const NEW_CHAT_FIELDS = {
  answerConfirmationOverride: 'ON',
  commonplaceRecallHistory: { turns: [['ae000000-0000-4000-8000-000000000001']] },
  timelineMode: 'narrative',
  turnSkippingEnabled: true,
};
const NEW_MESSAGE_FIELDS = {
  confirmed: true,
  confirmationChecked: true,
  confirmationRevised: false,
  confirmationNotes: 'checked against the ledger',
  confirmationOriginalContent: 'the original phrasing',
  // v4's own field-fidelity test mocks the repositories, so its `pascalMeta`
  // bag ({toolName, outcome, roll}) never meets `PascalMetaSchema`. This
  // builder writes through the REAL repository, which Zod-validates, so the
  // bag here is the schema's actual shape — a plausible d20 roll.
  pascalMeta: {
    tool: 'dice',
    toolTitle: 'The Dice',
    definitionTier: 'global',
    definitionMountId: 'general',
    params: { sides: 20, modifier: 2 },
    rollForm: 'dice',
    notation: '1d20+2',
    raw: 15,
    diceRolls: [15],
    value: 17,
    state: 'success',
    outcomeIndex: 0,
    invokedBy: 'llm',
  },
};
const NEW_CHAT_SETTINGS_FIELDS = {
  customTools: false,
  // v4's test bag adds `model: 'some-model'`; `AnswerConfirmationSettingsSchema`
  // is `{ enabled }` alone and strips it. `enabled: true` IS the non-default.
  answerConfirmationSettings: { enabled: true },
};

/** The graph's four ids. Deliberately a distinct prefix from the fixture's own
 *  `ad…` memories, so a failure message says at a glance which row it is. */
const G1 = 'ae000000-0000-4000-8000-000000000001';
const G2 = 'ae000000-0000-4000-8000-000000000002';
const G3 = 'ae000000-0000-4000-8000-000000000003';
const G4 = 'ae000000-0000-4000-8000-000000000004';

/** `createdAt`/`updatedAt` are the repository's write clock, so they are
 *  normalized away downstream; every OTHER field here is compared. */
const GRAPH: Array<Record<string, unknown>> = [
  {
    id: G1,
    characterId: LORIAN,
    chatId: CHAT,
    content: 'Lorian lit the lantern at the crossing, and Riya saw it from the water.',
    summary: 'the lantern at the crossing',
    keywords: ['lantern', 'crossing'],
    tags: [],
    importance: 0.8,
    source: 'AUTO',
    entities: ['the Crossing', 'Riya'],
    kind: 'episodic',
    occurredAt: '2026-02-14T21:05:00.000Z',
    narrativeTime: 'the third night at the crossing',
    witnessedContext: 'user_present',
    reinforcementCount: 3,
    lastReinforcedAt: '2026-02-20T08:00:00.000Z',
    relatedMemoryIds: [G2, G3],
    reinforcedImportance: 0.9,
  },
  {
    id: G2,
    characterId: LORIAN,
    content: 'Lorian keeps the lantern oiled against the fog.',
    summary: 'the lantern kept ready',
    keywords: ['lantern'],
    tags: [],
    importance: 0.5,
    source: 'MANUAL',
    entities: [],
    kind: 'semantic',
    witnessedContext: 'manual',
    reinforcementCount: 1,
    relatedMemoryIds: [G1],
    reinforcedImportance: 0.5,
  },
  {
    id: G3,
    characterId: RIYA,
    aboutCharacterId: LORIAN,
    content: 'Riya remembers that Lorian never once looked away from the water.',
    summary: 'Lorian watching the water',
    keywords: ['water', 'watch'],
    tags: [],
    importance: 0.7,
    source: 'AUTO',
    entities: ['Lorian', 'the Harbour'],
    kind: 'episodic',
    occurredAt: '2026-02-14T21:40:00.000Z',
    narrativeTime: 'later that same night',
    witnessedContext: 'autonomous_room',
    reinforcementCount: 2,
    lastReinforcedAt: '2026-02-18T12:30:00.000Z',
    relatedMemoryIds: [G1, G4],
    reinforcedImportance: 0.78,
  },
  {
    id: G4,
    characterId: RIYA,
    aboutCharacterId: LORIAN,
    content: 'Riya decided not to ask Lorian what he was waiting for.',
    summary: 'the question Riya did not ask',
    keywords: ['question'],
    tags: [],
    importance: 0.3,
    source: 'AUTO',
    entities: ['Lorian'],
    kind: 'episodic',
    occurredAt: '2026-02-14T22:15:00.000Z',
    narrativeTime: 'before the tide turned',
    witnessedContext: 'user_present',
    reinforcementCount: 1,
    relatedMemoryIds: [G3],
    reinforcedImportance: 0.3,
  },
];

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'system-data.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SD_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_SD_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outDir = process.env.QT_ARCHIVE_OUT;
  if (!outDir) throw new Error('QT_ARCHIVE_OUT must point at the fixture directory to write');
  mkdirSync(outDir, { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const scratchRoot = mkdtempSync(join(tmpdir(), 'qt-memgraph-archive-'));
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratchRoot, 'bk-'));
  mkdirSync(join(work, 'data'), { recursive: true });
  copyFileSync(fixtures.main, join(work, 'main.db'));
  copyFileSync(fixtures.mount, join(work, 'mount.db'));
  copyFileSync(fixtures.llm, join(work, 'llm.db'));
  process.env.SQLITE_PATH = join(work, 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm.db');
  process.env.QUILLTAP_DATA_DIR = work;

  // The same `files/` seed the base archive carries, so the file phase behaves
  // identically to `restore-archive.zip` and this archive differs from it in
  // exactly one respect: the memory graph.
  const imgDest = join(work, 'files', ...IMAGE_STORAGE_KEY(spec.userId).split('/'));
  mkdirSync(dirname(imgDest), { recursive: true });
  writeFileSync(imgDest, IMAGE_BYTES);

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();

  try {
    // v4's REAL user-scoped memories repository — the one whose `CreateOptions`
    // forwarding `4ac66c29` restored. Writing the fixture through it is itself a
    // small proof that the pinned tree carries the fix: on the pre-fix tree the
    // ids below would be silently replaced and the assertion after the loop
    // would fail.
    const { getRepositories } = await import('@/lib/repositories/factory');
    const repos = getRepositories() as Record<string, any>;
    for (const row of GRAPH) {
      const { id, ...data } = row;
      const created = await repos.memories.create(data, { id });
      if (created.id !== id) {
        throw new Error(
          `the v4 tree at this pin does not preserve memory ids ` +
            `(asked for ${String(id)}, got ${String(created.id)}) — this builder ` +
            `must run against a tree carrying 4ac66c29`,
        );
      }
    }

    // ── the sixteen 4.8 columns, given non-default values ────────────────────
    // See NEW_*_FIELDS above for why. Written through v4's real repositories so
    // every value is Zod-valid; asserted read-back, because a repository that
    // silently dropped one would leave this archive quietly unable to prove the
    // very thing it exists to prove.
    const chat = (await repos.chats.findById(CHAT)) as Record<string, unknown> | null;
    if (!chat) throw new Error(`the fixture must carry chat ${CHAT}`);
    const updatedChat = (await repos.chats.update(CHAT, NEW_CHAT_FIELDS)) as Record<
      string,
      unknown
    >;
    for (const [k, v] of Object.entries(NEW_CHAT_FIELDS)) {
      if (JSON.stringify(updatedChat?.[k]) !== JSON.stringify(v)) {
        throw new Error(`chats.${k} did not take: ${JSON.stringify(updatedChat?.[k])}`);
      }
    }

    const events = (await repos.chats.getMessages(CHAT)) as Array<Record<string, unknown>>;
    const firstMessage = events.find((e) => e.type === 'message');
    if (!firstMessage) throw new Error(`chat ${CHAT} must carry at least one message event`);
    const updatedMessage = (await repos.chats.updateMessage(
      CHAT,
      firstMessage.id as string,
      NEW_MESSAGE_FIELDS,
    )) as Record<string, unknown>;
    for (const [k, v] of Object.entries(NEW_MESSAGE_FIELDS)) {
      if (JSON.stringify(updatedMessage?.[k]) !== JSON.stringify(v)) {
        throw new Error(`chat_messages.${k} did not take: ${JSON.stringify(updatedMessage?.[k])}`);
      }
    }

    const updatedSettings = (await repos.chatSettings.updateForUser(
      spec.userId,
      NEW_CHAT_SETTINGS_FIELDS,
    )) as Record<string, unknown>;
    for (const [k, v] of Object.entries(NEW_CHAT_SETTINGS_FIELDS)) {
      if (JSON.stringify(updatedSettings?.[k]) !== JSON.stringify(v)) {
        throw new Error(`chat_settings.${k} did not take: ${JSON.stringify(updatedSettings?.[k])}`);
      }
    }

    const { createBackup } = await import('@/lib/backup/backup-service');
    const { zipPath } = await createBackup(spec.userId);
    const out = join(outDir, 'restore-archive-memory-graph.zip');
    copyFileSync(zipPath, out);
    process.stderr.write(`  restore-archive-memory-graph.zip  ${fs.statSync(out).size} bytes\n`);
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(scratchRoot, { recursive: true, force: true });
  }
}

test('build the memory-graph restore archive', async () => {
  await main();
});
