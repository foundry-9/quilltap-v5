/**
 * @jest-environment node
 *
 * Differential ORACLE for the P4.d10 state-family modules: the pure path
 * helpers (`lib/state/state-paths.ts`), the four-tier cascade resolver
 * (`lib/state/state-cascade.ts`), and the general-state document helpers
 * (`lib/mount-index/general-state.ts`) — all driven as v4's REAL code at the
 * round baseline `7e6d13e5`.
 *
 * Four sections, one NDJSON row per case:
 *   - paths   (pure, no DB): { label, kind:'paths', result }
 *   - general (real DB, fresh copies per case): { label, kind:'general', result }
 *   - cascade (real DB): { label, kind:'cascade', resultJson } —
 *       resultJson = JSON.stringify of the StateCascadeResult with `projectId`
 *       key omitted when undefined (JSON.stringify drops it).
 *   - groupref (real DB): { label, kind:'groupref', result }
 *
 * Real-DB-under-jest per the state-sql-tools recipe (resetModules + doMock past
 * jest.setup's global DB mocks).
 *
 * Run (Node 24, from the PINNED v4 worktree; case files under .claude/ are
 * invisible to jest → stage them outside first):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<v5 worktree> ; STAGE=/tmp/qt-oracle-stage
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $WT/harness/oracle/cases/state-cascade.test.ts $STAGE/harness/oracle/cases/
 *   cp $WT/harness/oracle/fixtures/state-sql-tools.json $STAGE/harness/oracle/fixtures/
 *   cd /private/tmp/qt-v4-pin-7e6d13e5
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
 *   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db \
 *     $N/node --import tsx $WT/harness/oracle/fixtures/build-state-sql-tools-fixture.ts
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
 *   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db QT_ORACLE_OUT=/tmp/oracle-state-cascade.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- state-cascade
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  chatProjectId: string;
  chatSoloId: string;
  chatUnionId: string;
  projectId: string;
  charAId: string;
  charBId: string;
  charCId: string;
  charDId: string;
  groupAlphaId: string;
  groupTwin2Id: string;
  generalMountPointId: string;
  generalState: Record<string, unknown>;
}

/** JS `undefined` made JSON-visible. */
function definedProbe(v: unknown): { defined: boolean; value: unknown } {
  return { defined: v !== undefined, value: v === undefined ? null : v };
}

// ── Section A: the pure path helpers (no DB) ────────────────────────────────

interface PathsCase {
  label: string;
  run: (helpers: {
    parsePath: (p: string | undefined) => (string | number)[];
    getAtPath: (o: Record<string, unknown>, p: (string | number)[]) => unknown;
    setAtPath: (o: Record<string, unknown>, p: (string | number)[], v: unknown) => Record<string, unknown>;
    deleteAtPath: (o: Record<string, unknown>, p: (string | number)[]) => boolean;
  }) => unknown;
}

const PATHS_CASES: PathsCase[] = [
  { label: 'parse_undefined', run: (h) => h.parsePath(undefined) },
  { label: 'parse_empty', run: (h) => h.parsePath('') },
  { label: 'parse_whitespace', run: (h) => h.parsePath('   ') },
  { label: 'parse_dots', run: (h) => h.parsePath('player.health') },
  { label: 'parse_index', run: (h) => h.parsePath('inventory[0].name') },
  { label: 'parse_double_index', run: (h) => h.parsePath('a[2][3]') },
  { label: 'parse_underscore', run: (h) => h.parsePath('_secret.x') },
  { label: 'parse_hyphen_splits', run: (h) => h.parsePath('weird-key') },
  { label: 'parse_space_splits', run: (h) => h.parsePath('a b') },
  { label: 'parse_alpha_index', run: (h) => h.parsePath('x[abc]') },
  { label: 'parse_unclosed_index', run: (h) => h.parsePath('x[1') },
  { label: 'parse_bare_index', run: (h) => h.parsePath('[0]') },
  { label: 'parse_non_ascii', run: (h) => h.parsePath('é.x') },
  {
    label: 'get_root',
    run: (h) => definedProbe(h.getAtPath({ a: 1 }, [])),
  },
  {
    label: 'get_nested',
    run: (h) =>
      definedProbe(
        h.getAtPath({ player: { inv: [{ name: 'sword' }] } }, h.parsePath('player.inv[0].name')),
      ),
  },
  {
    label: 'get_missing',
    run: (h) => definedProbe(h.getAtPath({ a: 1 }, h.parsePath('a.b'))),
  },
  {
    label: 'get_null_leaf',
    run: (h) => definedProbe(h.getAtPath({ a: { b: null } }, h.parsePath('a.b'))),
  },
  {
    label: 'get_null_mid',
    run: (h) => definedProbe(h.getAtPath({ a: null }, h.parsePath('a.b'))),
  },
  {
    label: 'get_string_key_on_array',
    run: (h) => definedProbe(h.getAtPath({ l: [1, 2] }, h.parsePath('l.foo'))),
  },
  {
    label: 'set_nested_create',
    run: (h) => {
      const obj: Record<string, unknown> = {};
      return h.setAtPath(obj, h.parsePath('player.stats.hp'), 5);
    },
  },
  {
    label: 'set_array_create',
    run: (h) => {
      const obj: Record<string, unknown> = {};
      return h.setAtPath(obj, h.parsePath('list[2]'), 'x');
    },
  },
  {
    label: 'set_overwrites_primitive_mid_path',
    run: (h) => {
      const obj: Record<string, unknown> = { a: 5 };
      return h.setAtPath(obj, h.parsePath('a.b'), 1);
    },
  },
  {
    label: 'set_root_object',
    run: (h) => h.setAtPath({ a: 1 }, [], { b: 2 }),
  },
  {
    label: 'set_root_non_object_throws',
    run: (h) => {
      try {
        h.setAtPath({}, [], 5);
        return { threw: false };
      } catch (e) {
        return { threw: true, message: e instanceof Error ? e.message : String(e) };
      }
    },
  },
  {
    label: 'set_root_array_throws',
    run: (h) => {
      try {
        h.setAtPath({}, [], [1, 2]);
        return { threw: false };
      } catch (e) {
        return { threw: true, message: e instanceof Error ? e.message : String(e) };
      }
    },
  },
  {
    label: 'delete_object_key',
    run: (h) => {
      const obj: Record<string, unknown> = { player: { health: 10, mana: 3 } };
      const deleted = h.deleteAtPath(obj, h.parsePath('player.mana'));
      return { deleted, obj };
    },
  },
  {
    label: 'delete_middle_key_order',
    run: (h) => {
      // JS `delete` preserves the remaining keys' order — pins the
      // shift-remove (not swap-remove) semantics.
      const obj: Record<string, unknown> = { a: 1, b: 2, c: 3, d: 4 };
      const deleted = h.deleteAtPath(obj, h.parsePath('b'));
      return { deleted, obj };
    },
  },
  {
    label: 'delete_array_splices',
    run: (h) => {
      const obj: Record<string, unknown> = { list: ['a', 'b', 'c'] };
      const deleted = h.deleteAtPath(obj, h.parsePath('list[1]'));
      return { deleted, obj };
    },
  },
  {
    label: 'delete_missing',
    run: (h) => {
      const obj: Record<string, unknown> = { a: 1 };
      return { deleted: h.deleteAtPath(obj, h.parsePath('b')), obj };
    },
  },
  {
    label: 'delete_root_refused',
    run: (h) => {
      const obj: Record<string, unknown> = { a: 1 };
      return { deleted: h.deleteAtPath(obj, []), obj };
    },
  },
  {
    label: 'delete_primitive_mid_path',
    run: (h) => {
      const obj: Record<string, unknown> = { a: 1 };
      return { deleted: h.deleteAtPath(obj, h.parsePath('a.deep')), obj };
    },
  },
  {
    label: 'delete_string_key_on_array',
    run: (h) => {
      const obj: Record<string, unknown> = { l: [1, 2] };
      return { deleted: h.deleteAtPath(obj, h.parsePath('l.foo')), obj };
    },
  },
];

// ── Sections B–D: DB-backed cases ────────────────────────────────────────────

type DbRunner = (mods: {
  spec: Spec;
  cascade: typeof import('@/lib/state/state-cascade');
  general: typeof import('@/lib/mount-index/general-state');
  dbStore: typeof import('@/lib/mount-index/database-store');
  rawMain: () => { exec: (sql: string) => void; prepare: (sql: string) => { run: (...a: unknown[]) => void } };
}) => Promise<unknown>;

interface DbCase {
  label: string;
  kind: 'general' | 'cascade' | 'groupref';
  run: DbRunner;
}

/** Serialize a StateCascadeResult the way the route/tool sees it. */
function cascadeJson(result: {
  chatState: unknown;
  projectState: unknown;
  groupState: unknown;
  generalState: unknown;
  merged: unknown;
  groupTier: unknown;
  projectId?: string;
}): string {
  return JSON.stringify(result);
}

const DB_CASES: DbCase[] = [
  // ── general-state ──
  {
    label: 'gen_read_seeded',
    kind: 'general',
    run: async ({ general }) => general.readGeneralState(),
  },
  {
    label: 'gen_ensure_existing',
    kind: 'general',
    run: async ({ general }) => ({ created: await general.ensureGeneralStateFile() }),
  },
  {
    label: 'gen_ensure_absent_seeds',
    kind: 'general',
    run: async ({ general, dbStore, spec }) => {
      await dbStore.deleteDatabaseDocument(spec.generalMountPointId, 'state.json');
      const created = await general.ensureGeneralStateFile();
      const readBack = await general.readGeneralState();
      return { created, readBack };
    },
  },
  {
    label: 'gen_write_roundtrip',
    kind: 'general',
    run: async ({ general }) => {
      await general.writeGeneralState({ weather_default: 'fog', depth: { z: 3 } });
      return general.readGeneralState();
    },
  },
  {
    label: 'gen_corrupt_body',
    kind: 'general',
    run: async ({ general, dbStore, spec }) => {
      await dbStore.writeDatabaseDocument(spec.generalMountPointId, 'state.json', '{ not json');
      return general.readGeneralState();
    },
  },
  {
    label: 'gen_array_body',
    kind: 'general',
    run: async ({ general, dbStore, spec }) => {
      await dbStore.writeDatabaseDocument(spec.generalMountPointId, 'state.json', '[1,2,3]');
      return general.readGeneralState();
    },
  },
  {
    label: 'gen_null_body',
    kind: 'general',
    run: async ({ general, dbStore, spec }) => {
      await dbStore.writeDatabaseDocument(spec.generalMountPointId, 'state.json', 'null');
      return general.readGeneralState();
    },
  },
  {
    label: 'gen_missing_doc',
    kind: 'general',
    run: async ({ general, dbStore, spec }) => {
      await dbStore.deleteDatabaseDocument(spec.generalMountPointId, 'state.json');
      return general.readGeneralState();
    },
  },
  {
    label: 'gen_unprovisioned',
    kind: 'general',
    run: async ({ general, rawMain }) => {
      rawMain()
        .prepare('DELETE FROM "instance_settings" WHERE "key" = ?')
        .run('generalMountPointId');
      const ensured = await general.ensureGeneralStateFile();
      const read = await general.readGeneralState();
      let writeError: string | null = null;
      try {
        await general.writeGeneralState({ a: 1 });
      } catch (e) {
        writeError = e instanceof Error ? e.message : String(e);
      }
      return { ensured, read, writeError };
    },
  },
];

async function findChat(id: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const chat = await getRepositories().chats.findById(id);
  if (!chat) throw new Error(`fixture chat missing: ${id}`);
  return chat;
}

const CASCADE_CASES: Array<{
  label: string;
  chat: (spec: Spec) => Promise<unknown> | unknown;
  scope: (spec: Spec) => unknown;
}> = [
  {
    label: 'cascade_charA_single',
    chat: (s) => findChat(s.chatProjectId),
    scope: (s) => ({ kind: 'character', characterId: s.charAId }),
  },
  {
    label: 'cascade_charB_ambiguous',
    chat: (s) => findChat(s.chatProjectId),
    scope: (s) => ({ kind: 'character', characterId: s.charBId }),
  },
  {
    label: 'cascade_charD_no_groups',
    chat: (s) => findChat(s.chatProjectId),
    scope: (s) => ({ kind: 'character', characterId: s.charDId }),
  },
  {
    label: 'cascade_none_scope',
    chat: (s) => findChat(s.chatProjectId),
    scope: () => ({ kind: 'none' }),
  },
  {
    label: 'cascade_union_ambiguous',
    chat: (s) => findChat(s.chatUnionId),
    scope: () => ({ kind: 'participants-union' }),
  },
  {
    label: 'cascade_union_single_no_project',
    chat: (s) => findChat(s.chatSoloId),
    scope: () => ({ kind: 'participants-union' }),
  },
  {
    label: 'cascade_synthetic_type_skip',
    chat: (s) => ({
      id: 'synthetic-1',
      state: { s: 1 },
      participants: [
        { type: 'USER', characterId: s.charBId, status: 'active' },
        { type: 'CHARACTER', characterId: s.charAId, status: 'active' },
      ],
    }),
    scope: () => ({ kind: 'participants-union' }),
  },
  {
    label: 'cascade_missing_project',
    chat: (s) => ({
      id: 'synthetic-2',
      state: { c: 1 },
      projectId: 'deadbeef-0000-4000-8000-000000000000',
      participants: [],
    }),
    scope: () => ({ kind: 'none' }),
  },
];

const GROUPREF_CASES: Array<{
  label: string;
  characterId: (spec: Spec) => string;
  groupRef?: string | ((spec: Spec) => string);
}> = [
  { label: 'ref_sole_omitted', characterId: (s) => s.charAId },
  { label: 'ref_required', characterId: (s) => s.charBId },
  { label: 'ref_by_id', characterId: (s) => s.charBId, groupRef: (s) => s.groupTwin2Id },
  { label: 'ref_by_name_ci', characterId: (s) => s.charAId, groupRef: 'alpha lodge' },
  { label: 'ref_ambiguous_name', characterId: (s) => s.charBId, groupRef: 'TWIN LODGE' },
  { label: 'ref_not_found', characterId: (s) => s.charBId, groupRef: 'Zed' },
  { label: 'ref_no_groups', characterId: (s) => s.charDId },
  { label: 'ref_whitespace', characterId: (s) => s.charBId, groupRef: '   ' },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'state-sql-tools.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_TMP_MAIN;
  const mountFixture = process.env.QT_FIXTURE_TMP_MOUNT;
  const llmFixture = process.env.QT_FIXTURE_TMP_LLM;
  if (!mainFixture || !mountFixture || !llmFixture) {
    throw new Error('QT_FIXTURE_TMP_MAIN / _MOUNT / _LLM must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const lines: string[] = [];

  // ── Section A: pure paths — one import, no DB. ──
  {
    const helpers = await import('@/lib/state/state-paths');
    for (const c of PATHS_CASES) {
      lines.push(JSON.stringify({ label: c.label, kind: 'paths', result: c.run(helpers) }));
    }
  }

  // ── Sections B–D: fresh DB copies per case. ──
  const dbCases: Array<{ label: string; kind: string; exec: () => Promise<unknown> }> = [];

  for (const c of DB_CASES) {
    dbCases.push({
      label: c.label,
      kind: c.kind,
      exec: async () => {
        const general = await import('@/lib/mount-index/general-state');
        const dbStore = await import('@/lib/mount-index/database-store');
        const cascade = await import('@/lib/state/state-cascade');
        const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
        return c.run({
          spec,
          cascade,
          general,
          dbStore,
          rawMain: () => {
            const db = getRawDatabase();
            if (!db) throw new Error('main DB handle unavailable');
            return db as never;
          },
        });
      },
    });
  }

  for (const c of CASCADE_CASES) {
    dbCases.push({
      label: c.label,
      kind: 'cascade',
      exec: async () => {
        const { resolveStateCascade } = await import('@/lib/state/state-cascade');
        const chat = await c.chat(spec);
        const result = await resolveStateCascade({
          chat: chat as never,
          groupScope: c.scope(spec) as never,
        });
        return { resultJson: cascadeJson(result) };
      },
    });
  }

  for (const c of GROUPREF_CASES) {
    dbCases.push({
      label: c.label,
      kind: 'groupref',
      exec: async () => {
        const { resolveGroupCandidates, resolveGroupForContext, StateGroupResolutionError } =
          await import('@/lib/state/state-cascade');
        const chat = await findChat(spec.chatProjectId);
        const candidates = await resolveGroupCandidates(chat as never, {
          kind: 'character',
          characterId: c.characterId(spec),
        });
        const groupRef =
          typeof c.groupRef === 'function' ? c.groupRef(spec) : c.groupRef;
        try {
          const g = resolveGroupForContext({ groupRef, candidates });
          const picked = g as { id: string; name: string; state?: unknown };
          return { ok: true, group: { id: picked.id, name: picked.name, state: picked.state ?? {} } };
        } catch (e) {
          if (e instanceof StateGroupResolutionError) {
            return { ok: false, code: e.code, message: e.message, candidates: e.candidates };
          }
          throw e;
        }
      },
    });
  }

  for (const c of dbCases) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-cascade-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    const mountWork = join(scratch, 'mount-work.db');
    const llmWork = join(scratch, 'llm-work.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);
    copyFileSync(llmFixture, llmWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.SQLITE_LLM_LOGS_PATH = llmWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/database/backends/sqlite/client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/client'),
    );
    jest.doMock('@/lib/database/backends/sqlite/llm-logs-client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/llm-logs-client'),
    );
    jest.doMock('@/lib/database/backends/sqlite/mount-index-client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/mount-index-client'),
    );

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    await initializeDatabase();

    const result = await c.exec();
    lines.push(JSON.stringify({ label: c.label, kind: c.kind, result }));

    // Close the mount-index client so the NEXT case's fresh copy is read through
    // a fresh client.
    const { closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    closeMountIndexSQLiteClient();
    await closeDatabase();
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`state-cascade oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('state-cascade oracle', async () => {
  await main();
});
