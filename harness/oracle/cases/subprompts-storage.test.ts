/**
 * @jest-environment node
 *
 * P4.D163 tier-2 ORACLE for the subprompts STORAGE + FAN-OUT (v4 `2f4254b42`,
 * `lib/subprompts/subprompts.ts` + `lib/subprompts/chat-fanout.ts`), ported to
 * `quilltap_core::subprompts::{storage, fanout}`.
 *
 * Drives v4's REAL module functions over a FRESH copy of the committed
 * `subprompts-{main,mount}.db` pair per case, with the REAL repositories and
 * the real cipher binding (the DB stack doMocked past jest.setup; the
 * character-vault bridge un-mocked — `jest-oracle-character-vault-bridge-is-
 * mocked` — or every listing would be an empty fake mount on this side). Only
 * two seams are recorders on BOTH sides: `compileIdentityStackForParticipant`
 * (P4.D164's block render is still landing; the `(chatId, participantId)`
 * calls are the comparand) and `publishRealtime` (the `(topic, id)` calls).
 *
 * The fixture was built under a frozen clock, so a baked file's `updatedAt`
 * compares EXACTLY (the `toISOString(mtime)` byte proof); the case run itself
 * is NOT frozen, so the writes mint the real clock on both sides and the Rust
 * side tokenizes anything that is not the sentinel.
 *
 * Emits one NDJSON line per case: { name, result, census?, recorded? } where
 * `result` is the returned value or `{ error: { name, message } }`, `census`
 * is the id-free semantic dump of the mount tables joined by path plus the
 * `chats` participants and the `characters` vault links, and `recorded` is the
 * two seams' call lists.
 *
 * Run (Node 24, from the v4 checkout — a pinned worktree while v4 HEAD is past
 * the baseline; cp to a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-subprompts-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/subprompts-storage.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/subprompts.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/subprompts-main.db \
 *   QT_FIXTURE_SP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/subprompts-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-subprompts-storage.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- subprompts-storage
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  seedNowMs: number;
  userId: string;
  ids: Record<string, string>;
}

type Recorded = { compile: Array<[string, string]>; publish: Array<[string, string | null]> };

function applyMocks(recorded: Recorded, compileFailsOnce: boolean): void {
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
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  // The two recorded seams.
  let compileCalls = 0;
  jest.doMock('@/lib/services/system-prompt-compiler/compiler', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/services/system-prompt-compiler/compiler'),
    compileIdentityStackForParticipant: async (chat: { id: string }, participantId: string) => {
      compileCalls += 1;
      recorded.compile.push([chat.id, participantId]);
      if (compileFailsOnce && compileCalls === 1) throw new Error('boom');
    },
  }));
  jest.doMock('@/lib/realtime/bus', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/realtime/bus'),
    publishRealtime: (topic: string, id?: string) => {
      recorded.publish.push([topic, id ?? null]);
    },
  }));
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });
}

/** Code-unit order (NOT `localeCompare` — the Rust side sorts bytes). */
const cmp = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);

/** Sentinel-or-`<ts>` for a timestamp string. */
function ts(v: unknown, sentinel: string): unknown {
  if (typeof v === 'string' && /^\d{4}-\d{2}-\d{2}T/.test(v)) return v === sentinel ? v : '<ts>';
  return v ?? null;
}

/** The id-free semantic census both sides build identically. */
async function census(
  spec: Spec,
  vaults: Record<string, string>,
): Promise<Record<string, unknown>> {
  const { rawQuery } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const S = spec.seedTimestamp;
  const keyOf = new Map<string, string>(Object.entries(vaults).map(([k, v]) => [v, k]));
  const point = (id: unknown) =>
    typeof id === 'string' ? (keyOf.get(id) ?? '<new-point>') : null;
  const j = (v: unknown) => (typeof v === 'string' && v.length > 0 ? JSON.parse(v) : v ?? null);

  const points = (
    midb.prepare('SELECT id, name, storeType, createdAt, updatedAt FROM doc_mount_points').all() as Array<
      Record<string, unknown>
    >
  )
    .map((r) => ({ point: point(r.id), name: r.name, storeType: r.storeType, createdAt: ts(r.createdAt, S), updatedAt: ts(r.updatedAt, S) }))
    .sort((a, b) => cmp(String(a.point), String(b.point)) || cmp(String(a.name), String(b.name)));

  const folders = (
    midb
      .prepare(
        'SELECT f.mountPointId, f.name, f.path, p.path AS parentPath, f.createdAt, f.updatedAt ' +
          'FROM doc_mount_folders f LEFT JOIN doc_mount_folders p ON p.id = f.parentId',
      )
      .all() as Array<Record<string, unknown>>
  )
    .map((r) => ({ point: point(r.mountPointId), name: r.name, path: r.path, parentPath: r.parentPath ?? null, createdAt: ts(r.createdAt, S), updatedAt: ts(r.updatedAt, S) }))
    .sort((a, b) => cmp(String(a.point), String(b.point)) || cmp(String(a.path), String(b.path)));

  const links = (
    midb
      .prepare(
        'SELECT l.*, fo.path AS folderPath, ' +
          'fi.sha256 AS f_sha256, fi.fileSizeBytes AS f_size, fi.fileType AS f_type, fi.source AS f_source, fi.createdAt AS f_createdAt, fi.updatedAt AS f_updatedAt, ' +
          'd.content AS d_content, d.contentSha256 AS d_sha, d.plainTextLength AS d_len, d.createdAt AS d_createdAt, d.updatedAt AS d_updatedAt, d.id AS d_id ' +
          'FROM doc_mount_file_links l ' +
          'LEFT JOIN doc_mount_folders fo ON fo.id = l.folderId ' +
          'LEFT JOIN doc_mount_files fi ON fi.id = l.fileId ' +
          'LEFT JOIN doc_mount_documents d ON d.fileId = l.fileId',
      )
      .all() as Array<Record<string, unknown>>
  )
    .map((r) => ({
      point: point(r.mountPointId),
      relativePath: r.relativePath,
      fileName: r.fileName,
      folderPath: r.folderPath ?? null,
      linkGroup: r.linkGroupId != null,
      originalFileName: r.originalFileName ?? null,
      originalMimeType: r.originalMimeType ?? null,
      description: r.description ?? null,
      descriptionUpdatedAt: ts(r.descriptionUpdatedAt, S),
      conversionStatus: r.conversionStatus ?? null,
      conversionError: r.conversionError ?? null,
      plainTextLength: r.plainTextLength ?? null,
      extractedText: r.extractedText ?? null,
      extractedTextSha256: r.extractedTextSha256 ?? null,
      extractionStatus: r.extractionStatus ?? null,
      extractionError: r.extractionError ?? null,
      chunkCount: r.chunkCount ?? null,
      allowEmbed: r.allowEmbed ?? null,
      allowCharacterRead: r.allowCharacterRead ?? null,
      allowCharacterWrite: r.allowCharacterWrite ?? null,
      lastModified: ts(r.lastModified, S),
      createdAt: ts(r.createdAt, S),
      updatedAt: ts(r.updatedAt, S),
      file: r.f_sha256 == null ? null : { sha256: r.f_sha256, fileSizeBytes: r.f_size, fileType: r.f_type, source: r.f_source, createdAt: ts(r.f_createdAt, S), updatedAt: ts(r.f_updatedAt, S) },
      document: r.d_id == null ? null : { content: r.d_content, contentSha256: r.d_sha, plainTextLength: r.d_len, createdAt: ts(r.d_createdAt, S), updatedAt: ts(r.d_updatedAt, S) },
    }))
    .sort((a, b) => cmp(String(a.point), String(b.point)) || cmp(String(a.relativePath), String(b.relativePath)));

  const orphanFiles = (midb.prepare('SELECT COUNT(*) AS c FROM doc_mount_files fi WHERE NOT EXISTS (SELECT 1 FROM doc_mount_file_links l WHERE l.fileId = fi.id)').get() as { c: number }).c;
  const orphanDocuments = (midb.prepare('SELECT COUNT(*) AS c FROM doc_mount_documents d WHERE NOT EXISTS (SELECT 1 FROM doc_mount_file_links l WHERE l.fileId = d.fileId)').get() as { c: number }).c;

  const characters = ((await rawQuery(
    'SELECT id, characterDocumentMountPointId, archivedAt FROM characters ORDER BY id',
  )) as Array<Record<string, unknown>>).map((r) => ({
    id: r.id,
    vault: point(r.characterDocumentMountPointId),
    archivedAt: ts(r.archivedAt, S),
  }));

  const chats = ((await rawQuery(
    'SELECT id, participants, compiledIdentityStacks, updatedAt FROM chats ORDER BY id',
  )) as Array<Record<string, unknown>>).map((r) => ({
    id: r.id,
    participants: (j(r.participants) as Array<Record<string, unknown>>).map((p) => ({
      ...p,
      createdAt: ts(p.createdAt, S),
      updatedAt: ts(p.updatedAt, S),
    })),
    compiledIdentityStacks: j(r.compiledIdentityStacks),
    updatedAt: ts(r.updatedAt, S),
  }));

  return { points, folders, links, orphanFiles, orphanDocuments, characters, chats };
}

type Result = unknown;
interface CaseSpec {
  name: string;
  /** The op; returns the JSON-able result. */
  run: (m: typeof import('@/lib/subprompts/subprompts'), f: typeof import('@/lib/subprompts/chat-fanout')) => Promise<Result>;
  census?: boolean;
  recorded?: boolean;
  compileFailsOnce?: boolean;
}

async function attempt(fn: () => Promise<Result>): Promise<Result> {
  try {
    return await fn();
  } catch (error) {
    const e = error as { name?: string; message?: string; code?: string };
    return { error: { name: e?.name ?? 'Error', message: e?.message ?? String(error) } };
  }
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
  vaults: Record<string, string>,
): Promise<Record<string, unknown>> {
  jest.resetModules();
  const recorded: Recorded = { compile: [], publish: [] };
  applyMocks(recorded, c.compileFailsOnce === true);

  const work = mkdtempSync(join(scratch, 'sp-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  // NO clock freeze here (the builder froze it): the writes mint the real
  // clock on both sides and the harness tokenizes anything that is not the
  // seed sentinel — freezing the oracle's run would make v4's minted values
  // EQUAL the sentinel and defeat that normalization.

  try {
    const m = await import('@/lib/subprompts/subprompts');
    const f = await import('@/lib/subprompts/chat-fanout');
    const result = await attempt(() => c.run(m, f));
    const out: Record<string, unknown> = { name: c.name, result };
    if (c.census) out.census = await census(spec, vaults);
    if (c.recorded) out.recorded = recorded;
    return out;
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`subprompts-storage oracle must run under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'subprompts.json'), 'utf8'),
  ) as Spec;
  const I = spec.ids;

  const fixtures = {
    main: process.env.QT_FIXTURE_SP_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SP_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const vaults = (JSON.parse(fs.readFileSync(fixtures.main + '.meta.json', 'utf8')) as { vaults: Record<string, string> }).vaults;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-subprompts-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const A = I.charA, B = I.charB, C = I.charC, D = I.charD, X = I.missing;
  const cases: CaseSpec[] = [
    // ── lists ──────────────────────────────────────────────────────────────
    { name: 'list_a', run: (m) => m.listCharacterSubprompts(A) },
    { name: 'list_b_no_folder', run: (m) => m.listCharacterSubprompts(B) },
    { name: 'list_c_no_vault', run: (m) => m.listCharacterSubprompts(C) },
    { name: 'list_d_archived_reads', run: (m) => m.listCharacterSubprompts(D) },
    { name: 'list_missing_character', run: (m) => m.listCharacterSubprompts(X) },
    // ── reads ──────────────────────────────────────────────────────────────
    { name: 'read_a_terse', run: (m) => m.readCharacterSubprompt(A, 'terse') },
    { name: 'read_a_Verse_exact_case', run: (m) => m.readCharacterSubprompt(A, 'Verse') },
    { name: 'read_a_verse_lower_case', run: (m) => m.readCharacterSubprompt(A, 'verse') },
    { name: 'read_a_no_title', run: (m) => m.readCharacterSubprompt(A, 'no-title') },
    { name: 'read_a_gone', run: (m) => m.readCharacterSubprompt(A, 'gone') },
    { name: 'read_a_nested_invalid_id', run: (m) => m.readCharacterSubprompt(A, 'drafts/x') },
    { name: 'read_a_broken_document', run: (m) => m.readCharacterSubprompt(A, 'broken') },
    { name: 'read_a_notes_txt_not_a_subprompt', run: (m) => m.readCharacterSubprompt(A, 'notes.txt') },
    { name: 'read_d_keep_archived_reads', run: (m) => m.readCharacterSubprompt(D, 'keep') },
    { name: 'read_c_no_vault', run: (m) => m.readCharacterSubprompt(C, 'terse') },
    { name: 'read_missing_character', run: (m) => m.readCharacterSubprompt(X, 'terse') },
    // ── the selection resolver ─────────────────────────────────────────────
    { name: 'resolve_a_terse_VERSE_gone', run: (m) => m.resolveSelectedSubprompts(A, ['terse', 'VERSE', 'gone']) },
    { name: 'resolve_a_listing_order_not_tick_order', run: (m) => m.resolveSelectedSubprompts(A, ['alpha', 'zulu', 'no-title']) },
    { name: 'resolve_a_empty', run: (m) => m.resolveSelectedSubprompts(A, []) },
    { name: 'resolve_a_dupes_one_wanted', run: (m) => m.resolveSelectedSubprompts(A, ['terse', 'TERSE']) },
    { name: 'resolve_a_broken_dropped', run: (m) => m.resolveSelectedSubprompts(A, ['broken']) },
    { name: 'resolve_c_no_vault', run: (m) => m.resolveSelectedSubprompts(C, ['terse']) },
    { name: 'resolve_d_archived_reads', run: (m) => m.resolveSelectedSubprompts(D, ['keep']) },
    { name: 'resolve_missing_character', run: (m) => m.resolveSelectedSubprompts(X, ['terse']) },
    // ── creates ────────────────────────────────────────────────────────────
    {
      name: 'create_a_collision_suffixes',
      census: true,
      run: async (m) => [
        await m.createCharacterSubprompt(A, { title: 'Be terse', content: 'one' }),
        await m.createCharacterSubprompt(A, { title: 'Be Terse!', content: 'two' }),
        await m.createCharacterSubprompt(A, { title: '  be   terse ', content: 'three' }),
      ],
    },
    { name: 'create_a_trims_title_and_content', census: true, run: (m) => m.createCharacterSubprompt(A, { title: '  Padded  ', content: '\n\n  body with edges  \n\n' }) },
    { name: 'create_b_ensures_folder', census: true, run: (m) => m.createCharacterSubprompt(B, { title: 'First', content: 'x' }) },
    {
      name: 'create_b_twice_ensures_folder_each_time',
      census: true,
      run: async (m) => [
        await m.createCharacterSubprompt(B, { title: 'First', content: 'x' }),
        await m.createCharacterSubprompt(B, { title: 'Second', content: 'y' }),
      ],
    },
    { name: 'create_c_provisions_vault', census: true, run: (m) => m.createCharacterSubprompt(C, { title: 'Fresh', content: 'x' }) },
    { name: 'create_d_archived_409', census: true, run: (m) => m.createCharacterSubprompt(D, { title: 'Nope', content: 'x' }) },
    { name: 'create_a_blank_title', census: true, run: (m) => m.createCharacterSubprompt(A, { title: '   ', content: 'x' }) },
    { name: 'create_a_blank_content', census: true, run: (m) => m.createCharacterSubprompt(A, { title: 'T', content: ' \n ' }) },
    { name: 'create_a_title_101_units', run: (m) => m.createCharacterSubprompt(A, { title: 'x'.repeat(101), content: 'x' }) },
    { name: 'create_a_title_100_astral_is_200_units', run: (m) => m.createCharacterSubprompt(A, { title: '😀'.repeat(100), content: 'x' }) },
    { name: 'create_a_title_validated_before_content', run: (m) => m.createCharacterSubprompt(A, { title: ' ', content: ' ' }) },
    { name: 'create_missing_character', run: (m) => m.createCharacterSubprompt(X, { title: 'T', content: 'x' }) },
    // ── updates ────────────────────────────────────────────────────────────
    { name: 'update_a_title_only', census: true, run: (m) => m.updateCharacterSubprompt(A, 'terse', { title: 'Be brief' }) },
    { name: 'update_a_content_only', census: true, run: (m) => m.updateCharacterSubprompt(A, 'terse', { content: 'Two lines at most.' }) },
    { name: 'update_a_both', census: true, run: (m) => m.updateCharacterSubprompt(A, 'terse', { title: 'Brief', content: 'Short.' }) },
    { name: 'update_a_empty_patch_rewrites', census: true, run: (m) => m.updateCharacterSubprompt(A, 'terse', {}) },
    { name: 'update_a_no_title_file_keeps_id_title', census: true, run: (m) => m.updateCharacterSubprompt(A, 'no-title', { content: 'Now with a body.' }) },
    { name: 'update_a_Verse_exact_case', census: true, run: (m) => m.updateCharacterSubprompt(A, 'Verse', { title: 'In verse' }) },
    { name: 'update_a_verse_lower_case', run: (m) => m.updateCharacterSubprompt(A, 'verse', { title: 'In verse' }) },
    { name: 'update_a_invalid_id', run: (m) => m.updateCharacterSubprompt(A, '../x', { title: 'x' }) },
    { name: 'update_a_missing_id', run: (m) => m.updateCharacterSubprompt(A, 'ghost', { title: 'x' }) },
    { name: 'update_a_broken_document', run: (m) => m.updateCharacterSubprompt(A, 'broken', { title: 'x' }) },
    { name: 'update_a_blank_title', census: true, run: (m) => m.updateCharacterSubprompt(A, 'terse', { title: '  ' }) },
    { name: 'update_a_blank_content', run: (m) => m.updateCharacterSubprompt(A, 'terse', { content: '' }) },
    { name: 'update_d_archived_409', run: (m) => m.updateCharacterSubprompt(D, 'keep', { title: 'x' }) },
    { name: 'update_c_no_vault_provisions_then_404', census: true, run: (m) => m.updateCharacterSubprompt(C, 'terse', { title: 'x' }) },
    { name: 'update_missing_character', run: (m) => m.updateCharacterSubprompt(X, 'terse', { title: 'x' }) },
    // ── deletes ────────────────────────────────────────────────────────────
    { name: 'delete_a_terse', census: true, run: (m) => m.deleteCharacterSubprompt(A, 'terse') },
    { name: 'delete_a_broken_link_only', census: true, run: (m) => m.deleteCharacterSubprompt(A, 'broken') },
    { name: 'delete_a_gone', census: true, run: (m) => m.deleteCharacterSubprompt(A, 'gone') },
    { name: 'delete_a_invalid_id', run: (m) => m.deleteCharacterSubprompt(A, 'a/b') },
    { name: 'delete_d_archived_409', census: true, run: (m) => m.deleteCharacterSubprompt(D, 'keep') },
    { name: 'delete_d_invalid_id_before_archive_check', run: (m) => m.deleteCharacterSubprompt(D, 'a/b') },
    { name: 'delete_c_no_vault_provisions_then_false', census: true, run: (m) => m.deleteCharacterSubprompt(C, 'terse') },
    { name: 'delete_missing_character', run: (m) => m.deleteCharacterSubprompt(X, 'terse') },
    // ── the fan-out ────────────────────────────────────────────────────────
    { name: 'fanout_terse', census: true, recorded: true, run: (_m, f) => f.fanOutSubpromptChange(A, 'terse') },
    { name: 'fanout_verse_case_insensitive', census: true, recorded: true, run: (_m, f) => f.fanOutSubpromptChange(A, 'verse') },
    { name: 'fanout_terse_remove_selection', census: true, recorded: true, run: (_m, f) => f.fanOutSubpromptChange(A, 'terse', { removeSelection: true }) },
    { name: 'fanout_TERSE_remove_selection_case_insensitive', census: true, recorded: true, run: (_m, f) => f.fanOutSubpromptChange(A, 'TERSE', { removeSelection: true }) },
    // The strip is case-insensitive too: deleting `verse` strips the seat's
    // stored `"VERSE"` (the mutation `*id != wanted` leaves it behind).
    { name: 'fanout_verse_remove_selection_strips_VERSE', census: true, recorded: true, run: (_m, f) => f.fanOutSubpromptChange(A, 'verse', { removeSelection: true }) },
    { name: 'fanout_gone', census: true, recorded: true, run: (_m, f) => f.fanOutSubpromptChange(A, 'gone') },
    { name: 'fanout_b_terse', census: true, recorded: true, run: (_m, f) => f.fanOutSubpromptChange(B, 'terse') },
    { name: 'fanout_missing_character', census: true, recorded: true, run: (_m, f) => f.fanOutSubpromptChange(X, 'terse') },
    { name: 'fanout_terse_compile_fails_once', census: true, recorded: true, compileFailsOnce: true, run: (_m, f) => f.fanOutSubpromptChange(A, 'terse') },
    { name: 'fanout_terse_remove_compile_fails_once', census: true, recorded: true, compileFailsOnce: true, run: (_m, f) => f.fanOutSubpromptChange(A, 'terse', { removeSelection: true }) },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures, vaults);
    // Normalize minted `updatedAt`s on returned records (the census does its own).
    const norm = (v: unknown): unknown => {
      if (Array.isArray(v)) return v.map(norm);
      if (v && typeof v === 'object') {
        const o = v as Record<string, unknown>;
        const out: Record<string, unknown> = {};
        for (const [k, val] of Object.entries(o)) {
          out[k] = k === 'updatedAt' ? ts(val, spec.seedTimestamp) : norm(val);
        }
        return out;
      }
      return v;
    };
    payload.result = norm(payload.result);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`subprompts-storage oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('subprompts-storage tier-2 oracle', async () => {
  await main();
});
