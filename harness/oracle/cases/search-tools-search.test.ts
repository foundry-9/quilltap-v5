/**
 * @jest-environment node
 *
 * Differential ORACLE for the two EMBEDDING search tools: `help_search` and
 * `search` (v4 executeHelpSearchTool / executeSearchScriptoriumTool + their
 * format* fns). Drives v4's REAL handlers over the shared two-DB fixture, ONE fresh
 * copy per case (search's memory branch bumps lastAccessedAt via updateAccessTimeBulk).
 *
 * ONLY the model boundary is pinned: `generateEmbeddingForUser` is jest.mocked to
 * the corpus's canned vectors (keyed by exact query text; failures listed in
 * cannedFailures force the help keyword fallback + the search branches' swallow).
 * `Date.now()` is frozen to spec.$nowMs (the memory branch's effectiveWeight/decay
 * reads it) — the SAME value the Rust handler receives as its now_ms arg.
 *
 * Emits one NDJSON line per case: `{ label, resultJson: JSON.stringify(output),
 * formatted: format*(...) }`.
 *
 * Real-DB-under-jest (memory-gate-tier3 recipe): resetModules + doMock past
 * jest.setup's global DB mocks + better-sqlite3 -> better-sqlite3-multiple-ciphers.
 *
 * Run (Node 24, from the v4 checkout; stage the case OUTSIDE any .claude path — v4's
 * jest config ignores /\.claude/):
 *   see search_tools_equivalence.rs header for the full STAGE recipe.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  wrongUserId: string;
  charAId: string;
  projectId: string;
  chatAId: string;
  $nowMs: number;
  cannedEmbeddings: Record<string, number[]>;
  cannedFailures: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'search-tools.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_TMP_MAIN;
  const mountFixture = process.env.QT_FIXTURE_TMP_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_TMP_MAIN and QT_FIXTURE_TMP_MOUNT must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  // Freeze Date.now() to the pinned epoch millis (memory decay / effectiveWeight).
  const realDateNow = Date.now;
  Date.now = () => spec.$nowMs;

  const lines: string[] = [];

  type HelpCase = { label: string; tool: 'help_search'; args: unknown };
  type SearchCase = {
    label: string;
    tool: 'search';
    args: unknown;
    operatorSurface?: boolean;
    noCharacter?: boolean;
    wrongUser?: boolean;
  };
  type Case = HelpCase | SearchCase;

  const cases: Case[] = [
    // ---- help_search ----
    { label: 'hs_semantic_hit', tool: 'help_search', args: { query: 'how do I get started' } },
    { label: 'hs_semantic_limit', tool: 'help_search', args: { query: 'how do I get started', limit: 1 } },
    { label: 'hs_keyword_fallback', tool: 'help_search', args: { query: 'wardrobe outfits clothing help' } },
    { label: 'hs_keyword_miss', tool: 'help_search', args: { query: 'xyzzy plugh frobnitz quux' } },
    { label: 'hs_invalid_empty', tool: 'help_search', args: { query: '' } },
    // ---- search ----
    { label: 'search_memories_only', tool: 'search', args: { query: 'recall the star navigation notes', sources: ['memories'] } },
    { label: 'search_conversations_only', tool: 'search', args: { query: 'what did we say about the ledger', sources: ['conversations'] } },
    { label: 'search_documents_only', tool: 'search', args: { query: 'guide to celestial mechanics', sources: ['documents'] } },
    { label: 'search_knowledge_only', tool: 'search', args: { query: 'guide to celestial mechanics', sources: ['knowledge'] } },
    { label: 'search_combined', tool: 'search', args: { query: 'guide to celestial mechanics' } },
    { label: 'search_empty_result', tool: 'search', args: { query: 'unrelated query that matches nothing' } },
    { label: 'search_limit_cap', tool: 'search', args: { query: 'guide to celestial mechanics', limit: 2 } },
    { label: 'search_truncation', tool: 'search', args: { query: 'a passage with a very long body', sources: ['documents'] } },
    { label: 'search_operator_surface', tool: 'search', args: { query: 'guide to celestial mechanics' }, operatorSurface: true, noCharacter: true },
    { label: 'search_invalid_empty', tool: 'search', args: { query: '' } },
    // ---- episodic recall (P4.d13 unit 6): since/until + aboutCharacter ----
    { label: 'search_window_dateonly', tool: 'search', args: { query: 'recall the star navigation notes', sources: ['memories'], since: '2025-12-01', until: '2025-12-31' } },
    { label: 'search_until_createdat_fallback', tool: 'search', args: { query: 'recall the star navigation notes', sources: ['memories'], until: '2025-06-01' } },
    { label: 'search_since_full_iso', tool: 'search', args: { query: 'recall the star navigation notes', sources: ['memories'], since: '2025-12-20T00:00:00.000Z' } },
    { label: 'search_window_empty', tool: 'search', args: { query: 'recall the star navigation notes', sources: ['memories'], since: '2020-01-01', until: '2020-12-31' } },
    { label: 'search_about_resolved', tool: 'search', args: { query: 'recall the star navigation notes', sources: ['memories'], aboutCharacter: 'Bram Roster' } },
    { label: 'search_about_alias', tool: 'search', args: { query: 'recall the star navigation notes', sources: ['memories'], aboutCharacter: 'the quartermaster' } },
    { label: 'search_about_trimmed_case', tool: 'search', args: { query: 'recall the star navigation notes', sources: ['memories'], aboutCharacter: '  BRAM roster  ' } },
    { label: 'search_about_unresolved', tool: 'search', args: { query: 'recall the star navigation notes', sources: ['memories'], aboutCharacter: 'Nobody Atall' } },
    { label: 'search_conversations_window_in', tool: 'search', args: { query: 'what did we say about the ledger', sources: ['conversations'], since: '2025-05-01', until: '2025-07-01' } },
    { label: 'search_conversations_window_out', tool: 'search', args: { query: 'what did we say about the ledger', sources: ['conversations'], since: '2020-01-01', until: '2020-12-31' } },
    { label: 'search_invalid_since', tool: 'search', args: { query: 'recall the star navigation notes', since: 'last week' } },
  ];

  for (const c of cases) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-search-oracle-'));
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

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
    jest.doMock('@/lib/embedding/embedding-service', () => {
      const actual = jest.requireActual('@/lib/embedding/embedding-service');
      const { EmbeddingError } = actual;
      return {
        __esModule: true,
        ...actual,
        generateEmbeddingForUser: async (text: string) => {
          if (spec.cannedFailures.includes(text)) {
            throw new EmbeddingError(`canned failure for input (${text.length} chars)`);
          }
          const vec = spec.cannedEmbeddings[text];
          if (!vec) {
            throw new EmbeddingError(`no canned embedding registered for input (${text.length} chars)`);
          }
          return { embedding: new Float32Array(vec), model: 'canned', dimensions: vec.length, provider: 'canned' };
        },
      };
    });

    // The help-doc disk sync is PINNED to a no-op (P4.d13): v4's
    // `ensureHelpDocsSynced` (since 551f090b) prunes any row whose file is
    // missing from `<cwd>/help/`, which would sweep the SEEDED fixture docs
    // and sync the real checkout's help tree instead. The family's target is
    // the help_search TOOL over the seeded rows — the sync subsystem is out of
    // scope on both sides (the Rust harness runs no disk sync either).
    jest.doMock('@/lib/help/help-doc-sync', () => {
      const actual = jest.requireActual('@/lib/help/help-doc-sync');
      return { __esModule: true, ...actual, ensureHelpDocsSynced: async () => undefined };
    });

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    await initializeDatabase();

    let resultJson: string;
    let formatted: string;

    if (c.tool === 'help_search') {
      // HelpSearch is a module singleton caching docs; reset it per case so it
      // reloads from THIS fresh DB copy.
      const { resetHelpSearch } = await import('@/lib/help-search');
      resetHelpSearch();
      const { executeHelpSearchTool, formatHelpSearchResults } = await import(
        '@/lib/tools/handlers/help-search-handler'
      );
      const out = await executeHelpSearchTool(c.args, { userId: spec.userId });
      resultJson = JSON.stringify(out);
      formatted = formatHelpSearchResults(out.results ?? []);
    } else {
      const { executeSearchScriptoriumTool, formatSearchScriptoriumResults } = await import(
        '@/lib/tools/handlers/search-scriptorium-handler'
      );
      const sc = c as SearchCase;
      const context: Record<string, unknown> = {
        userId: sc.wrongUser ? spec.wrongUserId : spec.userId,
        embeddingProfileId: undefined,
        projectId: spec.projectId,
      };
      if (!sc.noCharacter) context.characterId = spec.charAId;
      if (sc.operatorSurface) context.operatorSurface = true;
      const out = await executeSearchScriptoriumTool(c.args, context);
      resultJson = JSON.stringify(out);
      formatted = formatSearchScriptoriumResults(out.results ?? []);
    }

    // Let fire-and-forget promises settle before closing.
    await new Promise((resolve) => setTimeout(resolve, 50));
    await closeDatabase();

    lines.push(JSON.stringify({ label: c.label, resultJson, formatted }));
  }

  Date.now = realDateNow;
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`search-tools-search oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('search-tools search+help oracle', async () => {
  await main();
});
