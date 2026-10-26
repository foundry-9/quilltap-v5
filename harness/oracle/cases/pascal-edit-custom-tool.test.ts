/**
 * @jest-environment node
 *
 * P4.D332 ORACLE for `edit_custom_tool` (v4 `ea2cb60ab` + `c56fe278c`, pinned
 * `322a5488e`): drives v4's REAL `executeEditCustomTool` (with the REAL doc
 * layer — NOT v4's mocks, whose `@/lib/doc-edit`, `doc-edit/shared`,
 * `text-handlers` and `loadToolsFromMount` stubs would hide exactly what this
 * differential must prove: that the write lands through `handleWriteFile`
 * with the policy, the Librarian and the reindex), `formatEditCustomToolResults`,
 * `validateEditCustomToolInput` and `toolDefinitionPathFor`.
 *
 * Four row kinds, ONE NDJSON (`QT_ORACLE_PASCAL_EDIT_CUSTOM_TOOL`):
 *   - `path`    the `toolDefinitionPathFor` table (pure);
 *   - `input`   `validateEditCustomToolInput` over the refusal shapes (pure);
 *   - `format`  `formatEditCustomToolResults` over the output shapes (pure);
 *   - `handler` `executeEditCustomTool` over a FRESH copy of the committed
 *               `pascal-run-custom-{main,mount}.db` pair per row. The row
 *               records the output object's BYTES, the `Tools/` rows of the
 *               mount-index partition (link + document, ids and clocks left
 *               out), the system-sender `chat_messages` the Librarian posted,
 *               and the captured log lines.
 *
 * Planted `Tools/` files go in as SQL on the mount copy BEFORE the database
 * opens (the same statements run on the Rust side's copy; the row records
 * them). A plant SELF-CHECKS: after `initializeDatabase` the case asks v4's
 * real `loadToolsFromMount` which names it sees, and the row records that
 * list — a file both sides fail to read would otherwise pass vacuously.
 *
 * Every store row runs against Quilltap General by default; ONE row addresses
 * the reserved `self` (the acting character's own vault), whose `uri` cell is
 * the bug-256 residue (P4.D338) and is masked by the Rust side by name until
 * the union.
 *
 * Run (Node 24, from the v4 checkout — mirror to /tmp; jest ignores `.claude/`):
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   M=/tmp/qt-pascal-edit-mirror; rm -rf $M; mkdir -p $M/cases $M/fixtures $M/db
 *   cp $V5W/harness/oracle/cases/pascal-edit-custom-tool.test.ts $M/cases/
 *   cp $V5W/harness/oracle/fixtures/pascal-run-custom.json $M/fixtures/
 *   cp $V5W/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db $M/db/
 *   cp $V5W/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db.meta.json $M/db/
 *   cp $V5W/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db $M/db/
 *   QT_FIXTURE_PASCAL_MAIN=$M/db/pascal-run-custom-main.db \
 *   QT_FIXTURE_PASCAL_MOUNT=$M/db/pascal-run-custom-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-edit-custom-tool.ndjson \
 *     npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" \
 *       --roots "$M/cases" --runTestsByPath "$M/cases/pascal-edit-custom-tool.test.ts"
 */

import * as fs from 'fs';
import * as crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

interface Meta {
  vaultA: string;
  vaultB: string;
  vaultC: string;
  vaultD: string;
}

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';
const GENERAL_MP = '93000000-0000-4000-8000-0000000000aa';
const GENERAL_NAME = 'Quilltap General';

/** Every line this order ports (the seven handler lines + the validator's two). */
const PINNED_MESSAGES = new Set([
  'edit_custom_tool validation failed',
  'edit_custom_tool: schema requested',
  'edit_custom_tool called',
  'edit_custom_tool: existing definition is invalid',
  'edit_custom_tool: refused an invalid definition',
  'Custom tool definition saved',
  'edit_custom_tool failed',
  'Custom tool carries keys this build does not know',
  'Custom tool definition failed schema validation',
]);

/** A definition the pin's schema accepts, with an unknown top-level key (tolerated)
 *  carrying an empty array, an empty object and floats — so the pretty-printer
 *  twin is measured where `serde_json::to_string_pretty` would differ. */
const VALID = {
  name: 'unlock',
  description: 'Try the lock.',
  outcomes: [{ when: true, message: 'The lock yields.', state: 'success' }],
  note: { spare: [], none: {}, odds: [0.5, 1, 1e21, -0.25], tags: ['a', 'b'] },
};
const VALID_PLAIN = {
  name: 'unlock',
  description: 'Try the lock.',
  outcomes: [{ when: true, message: 'The lock yields.', state: 'info' }],
};
const VERBATIM_TEXT =
  '{"name":"verbatim","description":"as written","outcomes":[{"when":true,"message":"m","state":"info"}]}';

interface PlantFile {
  path: string;
  /** Text content; a non-string is `JSON.stringify`d with 2-space indent. */
  content: unknown;
  /** The link's per-document policy flags (default 1 / 1). */
  allowRead?: 0 | 1;
  allowWrite?: 0 | 1;
}

interface CaseSpec {
  name: string;
  characterId: string | null;
  vaultKey: 'vaultA' | null;
  input: unknown;
  plant?: PlantFile[];
  /** The chat's project (the acting context's `projectId`). */
  projectId?: string;
}

function sha256(text: string): string {
  return crypto.createHash('sha256').update(text, 'utf8').digest('hex');
}

function sqlq(text: string): string {
  return `'${text.replace(/'/g, "''")}'`;
}

/** The statements that put one `Tools/` file into Quilltap General. */
function plantStatements(files: PlantFile[]): string[] {
  const out: string[] = [];
  files.forEach((f, i) => {
    const text = typeof f.content === 'string' ? f.content : JSON.stringify(f.content, null, 2);
    const n = String(i + 1).padStart(2, '0');
    const fileId = `f0000000-0000-4000-8000-0000000000${n}`;
    const linkId = `f1000000-0000-4000-8000-0000000000${n}`;
    const docId = `f2000000-0000-4000-8000-0000000000${n}`;
    const sha = sha256(text);
    const ts = '2026-04-01T00:00:00.000Z';
    out.push(
      `INSERT INTO "doc_mount_files" ("id","sha256","fileSizeBytes","fileType","source","createdAt","updatedAt") VALUES (${sqlq(fileId)},${sqlq(sha)},${Buffer.byteLength(text, 'utf8')},'json','database','${ts}','${ts}')`,
      `INSERT INTO "doc_mount_documents" ("id","fileId","content","contentSha256","plainTextLength","createdAt","updatedAt") VALUES (${sqlq(docId)},${sqlq(fileId)},${sqlq(text)},${sqlq(sha)},${text.length},'${ts}','${ts}')`,
      `INSERT INTO "doc_mount_file_links" ("id","fileId","mountPointId","relativePath","fileName","folderId","description","conversionStatus","plainTextLength","extractionStatus","chunkCount","allowEmbed","allowCharacterRead","allowCharacterWrite","lastModified","createdAt","updatedAt") VALUES (${sqlq(linkId)},${sqlq(fileId)},'${GENERAL_MP}',${sqlq(f.path)},${sqlq(f.path.split('/').pop() as string)},(SELECT "id" FROM "doc_mount_folders" WHERE "mountPointId"='${GENERAL_MP}' AND "path"='Tools'),'','converted',${text.length},'none',0,1,${f.allowRead ?? 1},${f.allowWrite ?? 1},'${ts}','${ts}','${ts}')`,
    );
  });
  return out;
}

const TOOLS_DUMP_SQL = `SELECT l."mountPointId" AS mountPointId, l."relativePath" AS relativePath, l."fileName" AS fileName,
  l."allowEmbed" AS allowEmbed, l."allowCharacterRead" AS allowCharacterRead, l."allowCharacterWrite" AS allowCharacterWrite,
  f."fileType" AS fileType, f."fileSizeBytes" AS fileSizeBytes, f."sha256" AS sha256,
  d."content" AS content, d."contentSha256" AS contentSha256, d."plainTextLength" AS plainTextLength
FROM "doc_mount_file_links" l
JOIN "doc_mount_files" f ON f."id" = l."fileId"
LEFT JOIN "doc_mount_documents" d ON d."fileId" = l."fileId"
WHERE l."relativePath" LIKE 'Tools/%'
ORDER BY l."mountPointId", l."relativePath"`;

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

async function runCase(
  spec: Spec,
  meta: Meta,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));

  const work = mkdtempSync(join(scratch, 'edit-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);

  const plantSql = c.plant ? plantStatements(c.plant) : [];
  if (plantSql.length > 0) {
    // Before the database opens: no cross-case singleton can hold the copy.
    const Cipher = jest.requireActual(cipherDriverPath);
    const conn = new Cipher(mountWork);
    conn.pragma(`key = "x'${Buffer.from(spec.testPepperBase64, 'base64').toString('hex')}'"`);
    for (const sql of plantSql) conn.exec(sql);
    conn.close();
  }

  process.env.LOG_LEVEL = 'debug';
  const { logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message: string };
      };
      if (!PINNED_MESSAGES.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = (data.context ?? {}) as Record<
        string,
        unknown
      >;
      if (data.error) fields.error = data.error.message;
      logs.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm-logs.db');

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
  const { executeEditCustomTool } = await import('@/lib/tools/handlers/edit-custom-tool-handler');
  const { loadToolsFromMount } = await import('@/lib/pascal/custom-tools');

  await initializeDatabase();

  try {
    // The plant self-check: what discovery itself sees in General before the
    // call. A file both sides fail to read would pass vacuously.
    const before = await loadToolsFromMount(GENERAL_MP, 'global');
    const sees = {
      found: before.found.map((e) => `${e.definitionPath}=${e.definition.name}`).sort(),
      errors: before.errors.map((e) => `${e.definitionPath}`).sort(),
    };
    const mdbBefore = getRawMountIndexDatabase();
    if (!mdbBefore) throw new Error('mount-index DB handle unavailable');
    const toolsBefore = (mdbBefore.prepare(TOOLS_DUMP_SQL).all() as Array<Record<string, unknown>>).map(
      (r) => Object.fromEntries(Object.entries(r).map(([k, v]) => [k, canonValue(v)])),
    );

    // Clear the log capture of anything the self-check emitted.
    logs.length = 0;

    const context = {
      userId: spec.userId,
      chatId: CHAT,
      characterId: c.characterId ?? undefined,
      projectId: c.projectId,
      operatorOverride: false,
      mountPool: undefined,
    };
    void meta;
    const output = await executeEditCustomTool(c.input, context as never);

    // Let any fire-and-forget tail (the Librarian post, the reindex) settle.
    await new Promise((resolve) => setTimeout(resolve, 200));

    const mdb = getRawMountIndexDatabase();
    if (!mdb) throw new Error('mount-index DB handle unavailable');
    const toolsAfter = (mdb.prepare(TOOLS_DUMP_SQL).all() as Array<Record<string, unknown>>).map((r) =>
      Object.fromEntries(Object.entries(r).map(([k, v]) => [k, canonValue(v)])),
    );

    const main = getRawDatabase();
    if (!main) throw new Error('main DB handle unavailable');
    const columns = (
      main.prepare(`PRAGMA table_info(chat_messages)`).all() as Array<{ name: string }>
    ).map((col) => col.name);
    const rawRows = main.prepare(`SELECT * FROM chat_messages ORDER BY createdAt`).all() as Array<
      Record<string, unknown>
    >;
    const rows = rawRows.map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    });

    // The handler's lines are the ones captured SO FAR; the discovery read below
    // is the case's own and must not pollute them.
    const handlerLogs = logs.slice();
    const after = await loadToolsFromMount(GENERAL_MP, 'global');

    return {
      kind: 'handler',
      name: c.name,
      characterId: c.characterId,
      vaultKey: c.vaultKey,
      vault: c.vaultKey ? meta[c.vaultKey] : null,
      projectId: c.projectId ?? null,
      input: c.input,
      plantSql,
      sees,
      output,
      outputJson: JSON.stringify(output),
      toolsBefore,
      toolsAfter,
      seesAfter: {
        found: after.found.map((e) => `${e.definitionPath}=${e.definition.name}`).sort(),
        errors: after.errors.map((e) => `${e.definitionPath}`).sort(),
      },
      columns,
      rows,
      logs: handlerLogs,
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

/** The `path` table: the 11 rows of the order PLUS the case rows (trap 5) and
 *  a U+FEFF-padded stem (`js_trim` — JS `\s` includes U+FEFF). */
const PATH_INPUTS: string[] = [
  'unlock',
  'unlock.tool.json',
  'unlock.TOOL.JSON',
  'Tools/unlock.tool.json',
  '  unlock  ',
  'Tools/nested/unlock',
  'Notes/unlock.tool.json',
  '../unlock',
  '.tool.json',
  'Tools/',
  'a\\b',
  'Unlock',
  'TOOLS/x.Tool.Json',
  '﻿unlock﻿',
  'tools/unlock',
  '..',
  '.',
  'Tools/.tool.json',
  'dir\\unlock.tool.json',
];

const INPUT_CASES: unknown[] = [
  { action: 'schema' },
  { file: 'unlock' },
  { mount_point: 'V' },
  { mount_point: '', file: 'x' },
  { mount_point: 'V', file: '' },
  { action: 'save', mount_point: 'V', file: 'x' },
  { action: null, mount_point: 'V', file: 'x' },
  { mount_point: 'V', file: 'x', content: null },
  { mount_point: 'V', file: 'x', extra: 1 },
  { mount_point: 'V', file: 'x' },
  { action: 'schema', mount_point: 'V', file: 'x', content: { a: 1 } },
  { mount_point: 5, file: 'x' },
  'schema',
  null,
  [],
];

const FORMAT_CASES: Array<{ name: string; output: Record<string, unknown> }> = [
  { name: 'error', output: { success: false, path: 'Tools/x.tool.json', error: 'Not saved. boom' } },
  { name: 'error-without-text', output: { success: false } },
  { name: 'schema', output: { success: true, action: 'schema', schema: {}, examples: [] } },
  {
    name: 'saved-with-uri',
    output: { success: true, action: 'saved', path: 'Tools/u.tool.json', uri: 'qtap://S/Tools/u.tool.json', name: 'u' },
  },
  {
    name: 'saved-without-uri',
    output: { success: true, action: 'saved', path: 'Tools/u.tool.json', name: 'u' },
  },
  {
    name: 'validated-with-uri',
    output: { success: true, action: 'validated', path: 'Tools/u.tool.json', uri: 'qtap://S/Tools/u.tool.json', name: 'u' },
  },
  {
    name: 'validated-without-uri',
    output: { success: true, action: 'validated', path: 'Tools/u.tool.json', name: 'u' },
  },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'pascal-run-custom.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_PASCAL_MAIN ?? '',
    mount: process.env.QT_FIXTURE_PASCAL_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const meta = JSON.parse(fs.readFileSync(fixtures.main + '.meta.json', 'utf8')) as Meta;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-edit-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const out = fs.createWriteStream(outPath);

  // --- the pure rows (no database) -----------------------------------------
  const { toolDefinitionPathFor } = await import('@/lib/pascal/custom-tools');
  const { validateEditCustomToolInput } = await import('@/lib/tools/edit-custom-tool-tool');
  const { formatEditCustomToolResults } = await import(
    '@/lib/tools/handlers/edit-custom-tool-handler'
  );
  for (const file of PATH_INPUTS) {
    out.write(JSON.stringify({ kind: 'path', file, path: toolDefinitionPathFor(file) }) + '\n');
  }
  for (const input of INPUT_CASES) {
    const parsed = validateEditCustomToolInput(input);
    out.write(JSON.stringify({ kind: 'input', input, parsed }) + '\n');
  }
  for (const f of FORMAT_CASES) {
    out.write(
      JSON.stringify({
        kind: 'format',
        name: f.name,
        output: f.output,
        text: formatEditCustomToolResults(f.output as never),
      }) + '\n',
    );
  }

  // --- the handler rows -----------------------------------------------------
  const A = { characterId: CHAR_A, vaultKey: 'vaultA' as const };
  const store = (extra: Record<string, unknown>) => ({ mount_point: GENERAL_NAME, ...extra });
  const planted = (name: string, content: unknown): PlantFile => ({ path: `Tools/${name}.tool.json`, content });
  const cases: CaseSpec[] = [
    // --- saving ---
    {
      name: 'saving-validates-then-writes-through-doc-write-file',
      ...A,
      input: store({ file: 'unlock', content: VALID }),
    },
    {
      name: 'saving-writes-string-content-verbatim',
      ...A,
      input: store({ file: 'verbatim.tool.json', content: VERBATIM_TEXT }),
    },
    {
      name: 'saving-refuses-a-definition-that-fails-the-schema',
      ...A,
      input: store({ file: 'unlock', content: { name: 'unlock', outcomes: [] } }),
    },
    {
      name: 'saving-refuses-text-that-is-not-json',
      ...A,
      input: store({ file: 'unlock', content: 'not json {' }),
    },
    {
      name: 'saving-refuses-a-name-another-file-in-the-same-store-already-defines',
      ...A,
      input: store({ file: 'unlock', content: VALID_PLAIN }),
      plant: [planted('old-lock', { ...VALID_PLAIN, description: 'The old lock.' })],
    },
    {
      name: 'saving-lets-a-file-keep-its-own-name-when-it-is-rewritten',
      ...A,
      input: store({ file: 'unlock', content: { ...VALID_PLAIN, description: 'Rewritten.' } }),
      plant: [planted('unlock', VALID_PLAIN)],
    },
    {
      name: 'saving-lets-a-file-keep-its-own-name-under-another-case',
      ...A,
      input: store({ file: 'UNLOCK', content: { ...VALID_PLAIN, description: 'Case-folded.' } }),
      plant: [planted('Unlock', VALID_PLAIN)],
    },
    {
      name: 'saving-refuses-a-file-outside-the-top-level-of-tools',
      ...A,
      input: store({ file: 'Tools/nested/unlock', content: VALID_PLAIN }),
    },
    {
      name: 'saving-reports-a-store-the-caller-may-not-reach-as-a-failure',
      ...A,
      input: { mount_point: 'Elsewhere', file: 'unlock', content: VALID_PLAIN },
    },
    {
      name: 'saving-to-the-reserved-self-vault',
      ...A,
      input: { mount_point: 'self', file: 'unlock', content: VALID_PLAIN },
    },
    {
      name: 'saving-to-the-characters-own-vault-by-name',
      ...A,
      input: { mount_point: 'Bertie Character Vault', file: 'vaulted', content: VALID_PLAIN },
    },
    {
      name: 'validating-in-the-characters-own-vault-by-name',
      ...A,
      input: { mount_point: 'Bertie Character Vault', file: 'vaulted' },
    },
    {
      name: 'saving-over-a-file-the-policy-forbids-a-character-to-write',
      ...A,
      input: store({ file: 'locked', content: { ...VALID_PLAIN, name: 'locked' } }),
      plant: [{ ...planted('locked', { ...VALID_PLAIN, name: 'locked' }), allowWrite: 0 }],
    },
    {
      name: 'validating-a-file-the-policy-hides-from-a-character',
      ...A,
      input: store({ file: 'hidden' }),
      plant: [{ ...planted('hidden', { ...VALID_PLAIN, name: 'hidden' }), allowRead: 0 }],
    },
    {
      name: 'saving-into-the-chats-project-store',
      ...A,
      projectId: 'a3000000-0000-4000-8000-0000000000aa',
      input: { mount_point: 'Project Files: The Ledger Office', file: 'ledgered', content: { ...VALID_PLAIN, name: 'ledgered' } },
    },
    {
      name: 'validating-in-the-chats-project-store-before-anything-is-there',
      ...A,
      projectId: 'a3000000-0000-4000-8000-0000000000aa',
      input: { mount_point: 'Project Files: The Ledger Office', file: 'ledgered' },
    },
    {
      name: 'validating-in-a-peers-vault',
      ...A,
      input: { mount_point: 'Jeeves Character Vault', file: 'peers' },
    },
    {
      name: 'saving-into-a-peers-vault',
      ...A,
      input: { mount_point: 'Jeeves Character Vault', file: 'peers', content: VALID_PLAIN },
    },
    {
      name: 'validating-in-the-groups-store',
      ...A,
      input: { mount_point: 'Group Files: Table Stakes', file: 'grouped' },
    },
    {
      name: 'saving-into-the-groups-store',
      ...A,
      input: { mount_point: 'Group Files: Table Stakes', file: 'grouped', content: { ...VALID_PLAIN, name: 'grouped' } },
    },
    {
      name: 'saving-an-empty-string-is-text-not-omitted',
      ...A,
      input: store({ file: 'blank', content: '' }),
    },
    // --- validating without content ---
    {
      name: 'validating-the-file-as-it-stands-and-writes-nothing',
      ...A,
      input: store({ file: 'unlock' }),
      plant: [planted('unlock', VALID_PLAIN)],
    },
    {
      name: 'validating-treats-null-content-as-omitted',
      ...A,
      input: store({ file: 'unlock', content: null }),
      plant: [planted('unlock', VALID_PLAIN)],
    },
    {
      name: 'validating-reports-what-is-wrong-with-an-invalid-file',
      ...A,
      input: store({ file: 'broken' }),
      plant: [planted('broken', { name: 'broken', outcomes: [] })],
    },
    {
      name: 'validating-reports-a-file-that-is-not-json',
      ...A,
      input: store({ file: 'garbled' }),
      plant: [planted('garbled', 'not json {')],
    },
    {
      name: 'validating-says-so-when-there-is-no-such-file',
      ...A,
      input: store({ file: 'ghost' }),
    },
    {
      name: 'validating-flags-a-same-store-clash-discovery-has-already-set-aside',
      ...A,
      input: store({ file: 'twin-b' }),
      plant: [planted('twin-a', VALID_PLAIN), planted('twin-b', { ...VALID_PLAIN, description: 'The twin.' })],
    },
    {
      name: 'validating-a-file-with-an-unknown-top-level-key',
      ...A,
      input: store({ file: 'future' }),
      plant: [planted('future', { ...VALID_PLAIN, persist: true })],
    },
    // --- fetching the schema ---
    { name: 'schema-returns-the-published-schema-and-specimens', ...A, input: { action: 'schema' } },
    {
      name: 'schema-ignores-any-store-fields-passed-alongside-it',
      ...A,
      input: { action: 'schema', mount_point: 'Elsewhere', file: 'x', content: { a: 1 } },
    },
    { name: 'schema-still-requires-mount-point-and-file-without-it', ...A, input: { file: 'x' } },
    { name: 'schema-with-no-acting-character', characterId: null, vaultKey: null, input: { action: 'schema' } },
  ];

  for (const c of cases) {
    const row = await runCase(spec, meta, c, scratch, fixtures);
    out.write(JSON.stringify(row) + '\n');
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  rmSync(scratch, { recursive: true, force: true });
}

it('emits the pascal edit_custom_tool oracle', async () => {
  await main();
}, 300000);
