/**
 * Tier-2 oracle case — v4's `CharactersRepository` array / sub-array ops (Phase-2,
 * the store-backed capstone sub-unit 4b).
 *
 * Drives v4's REAL repository methods over the baked fixture character:
 * `addSystemPrompt` / `updateSystemPrompt` / `setDefaultSystemPrompt` /
 * `deleteSystemPrompt`, `addScenario` / `updateScenario` / `removeScenario`,
 * `addPartnerLink` / `removePartnerLink`, and the `setFavorite` /
 * `setControlledBy` / `setCanBeCarina` setters. Each sub-array op internally does
 * findById (read overlay) -> mutate -> update (write overlay). We do NOT set
 * QUILLTAP_JOB_CHILD, so reindexSingleFile runs; since P4.6BK v5 chunks on
 * write too, so the differential diffs chunkCount exactly.
 *
 * The id-taking prompt/scenario ops carry a targetName / targetTitle in the spec;
 * we resolve it to the current item's id via `findById` right before the op (the
 * id is path-derived, so both sides agree). A character spans two DBs, so we dump
 * BOTH the main slim `characters` row and the mount-index store tables.
 *
 * NORMALIZATION (done identically on both dumps by the Rust harness): the
 * shared-id remap across all six tables (FKs verify by relationship) + timestamp
 * placeholders; chunkCount pinned.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARARR_MAIN=/tmp/qt-chararr-main.db \
 *   QT_FIXTURE_CHARARR_MOUNT=/tmp/qt-chararr-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-arrays.ts \
 *     > /tmp/oracle-chararr.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  op: string;
  name?: string;
  content?: string;
  isDefault?: boolean;
  title?: string;
  /**
   * [P4.D201 / v4 `baa85e19b`] `setDefaultSystemPrompt` accepts `null` — the
   * clear-the-default arm — so the spec spells it as an explicit `targetName:
   * null` and this driver passes it straight through rather than resolving.
   */
  targetName?: string | null;
  targetTitle?: string;
  data?: Record<string, unknown>;
  /** [P4.D201] a LITERAL id the character does not have (the refusal arm). */
  promptId?: string;
  partnerId?: string;
  value?: unknown;
  /** [P4.D219] `addScenario` on a character OTHER than the baked one (a planted
   *  vaultless row, or an id that does not exist); `plantVaultlessCharacter`'s id. */
  characterId?: string;
  /** [P4.D219] `plantScenarioFile`'s vault path + bytes. */
  path?: string;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'characters-arrays-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_CHARARR_MAIN;
  const mountFixture = process.env.QT_FIXTURE_CHARARR_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_CHARARR_MAIN and QT_FIXTURE_CHARARR_MOUNT must point at the fixtures from build-characters-arrays-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chararr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'chararr-main-work.db');
  const mountWork = join(scratch, 'chararr-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const idRow = (await rawQuery('SELECT id FROM characters LIMIT 1')) as Array<{ id: string }>;
  if (idRow.length === 0) throw new Error('fixture has no character row');
  const characterId = idRow[0].id;

  const resolvePromptId = async (name: string): Promise<string> => {
    const c = await repos.characters.findById(characterId);
    const p = (c?.systemPrompts ?? []).find((x) => x.name === name);
    if (!p) throw new Error(`prompt not found for targetName: ${name}`);
    return p.id;
  };
  const resolveScenarioId = async (title: string): Promise<string> => {
    const c = await repos.characters.findById(characterId);
    const s = (c?.scenarios ?? []).find((x) => x.title === title);
    if (!s) throw new Error(`scenario not found for targetTitle: ${title}`);
    return s.id;
  };

  /**
   * [P4.D201 / v4 `baa85e19b`] The per-op default-prompt trail.
   *
   * The six-table census is a FINAL-STATE diff, so an op whose effect a later op
   * overwrites is INVISIBLE to it — measured: inserting the three new
   * system-prompt arms left the dump byte-for-byte the size it already was,
   * because the sequence still ends with a delete that re-heals the column. The
   * lockstep is precisely a claim about what EACH write leaves behind, so it
   * needs a per-op comparand. After every op this records the slim
   * `defaultSystemPromptId` cell and the read-back prompt flags; the prompt ids
   * are path-derived from the shared fixture, so both sides mint the same ones
   * and the trail compares EXACTLY.
   */
  const trail: Array<{
    op: string;
    column: string | null;
    prompts: Array<[string, boolean]>;
  }> = [];
  const snapshot = async (opName: string): Promise<void> => {
    const rows = (await rawQuery(
      'SELECT defaultSystemPromptId FROM characters WHERE id = ?',
      [characterId]
    )) as Array<{ defaultSystemPromptId: string | null }>;
    const c = await repos.characters.findById(characterId);
    trail.push({
      op: opName,
      column: rows[0]?.defaultSystemPromptId ?? null,
      prompts: (c?.systemPrompts ?? []).map((p) => [p.name, Boolean(p.isDefault)] as [string, boolean]),
    });
  };
  await snapshot('<initial>');

  /**
   * [P4.D219 / v4 `d1c06cd9d`, bug 165] What each `addScenario` RETURNED. The
   * six-table census cannot see a return value, and the returned id is the
   * whole bug: v4 used to hand back the id `addToSubArray` minted, which the
   * vault re-keys from the file path on the very next read. Per op: the
   * returned item's KEY ORDER (the create route's reply body), its title /
   * content / archived, and `readbackIndex` — the item's position in the next
   * read-back, or null when the returned id is one no read will ever show (the
   * minted transient). The literal id rides along only when the character's
   * mount is the shared fixture's (a projected id is deterministic there:
   * `stableUuidFromString("scenario:<mount>:<path>")`); a vault provisioned
   * mid-op mints its mount id, so there only the index compares.
   */
  const bakedMount = (
    (await rawQuery('SELECT characterDocumentMountPointId AS m FROM characters WHERE id = ?', [
      characterId,
    ])) as Array<{ m: string }>
  )[0].m;
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  const addScenarioReturns: Array<Record<string, unknown>> = [];
  let opIndex = 0;

  for (const op of spec.ops) {
    switch (op.op) {
      case 'addSystemPrompt':
        await repos.characters.addSystemPrompt(characterId, {
          name: op.name as string,
          content: op.content as string,
          isDefault: op.isDefault as boolean,
        } as never);
        break;
      case 'updateSystemPrompt': {
        const id = await resolvePromptId(op.targetName as string);
        await repos.characters.updateSystemPrompt(characterId, id, op.data as never);
        break;
      }
      case 'setDefaultSystemPrompt': {
        // [P4.D201 / v4 `baa85e19b`] `targetName: null` is the CLEAR arm — it is
        // passed through as a literal `null`, not resolved to an id.
        const id =
          op.targetName === null || op.targetName === undefined
            ? null
            : await resolvePromptId(op.targetName);
        await repos.characters.setDefaultSystemPrompt(characterId, id);
        break;
      }
      // [P4.D201 / v4 `baa85e19b`] the refusal arm: a NON-null id the character
      // does not have. v4 warns and returns null having written nothing.
      case 'setDefaultSystemPromptMissing': {
        const refused = await repos.characters.setDefaultSystemPrompt(
          characterId,
          op.promptId as string
        );
        if (refused !== null) {
          throw new Error('setDefaultSystemPromptMissing: v4 accepted a foreign prompt id');
        }
        break;
      }
      case 'deleteSystemPrompt': {
        const id = await resolvePromptId(op.targetName as string);
        await repos.characters.deleteSystemPrompt(characterId, id);
        break;
      }
      case 'addScenario': {
        const target = op.characterId ?? characterId;
        const returned = await repos.characters.addScenario(target, {
          title: op.title as string,
          content: op.content as string,
          // [P4.D120 / v4 `d25dacc1`] the optional `archived` flag.
          ...(op.archived !== undefined && { archived: op.archived as boolean }),
        });
        const after = await repos.characters.findById(target);
        const ids = (after?.scenarios ?? []).map((s) => s.id);
        let record: Record<string, unknown> | null = null;
        if (returned) {
          const idx = ids.indexOf(returned.id);
          const r = returned as unknown as Record<string, unknown>;
          record = {
            keys: Object.keys(r),
            title: r.title,
            content: r.content,
            archived: r.archived ?? null,
            readbackIndex: idx >= 0 ? idx : null,
            id: idx >= 0 && target === characterId ? returned.id : null,
          };
        }
        addScenarioReturns.push({
          opIndex,
          title: op.title,
          readbackTitles: (after?.scenarios ?? []).map((s) => s.title),
          returned: record,
        });
        break;
      }
      // [P4.D219] a vault file NOT named after its title, planted through the
      // REAL writer: the next re-projection rewrites it under its title and
      // sweeps it, so its id is FRESH beside the new scenario's.
      case 'plantScenarioFile':
        await writeDatabaseDocument(bakedMount, op.path as string, op.content as string);
        break;
      // [P4.D219] a copy of the baked row with a new id/name and NO vault — the
      // same four statements the Rust side runs.
      case 'plantVaultlessCharacter':
        for (const [sql, params] of [
          ['CREATE TEMP TABLE qt_p4d219_plant AS SELECT * FROM characters WHERE id = ?', [characterId]],
          [
            'UPDATE qt_p4d219_plant SET id = ?, name = ?, characterDocumentMountPointId = NULL',
            [op.characterId, op.name],
          ],
          ['INSERT INTO characters SELECT * FROM qt_p4d219_plant', []],
          ['DROP TABLE qt_p4d219_plant', []],
        ] as Array<[string, unknown[]]>) {
          await rawQuery(sql, params);
        }
        break;
      case 'updateScenario': {
        const id = await resolveScenarioId(op.targetTitle as string);
        await repos.characters.updateScenario(characterId, id, op.data as never);
        break;
      }
      case 'removeScenario': {
        const id = await resolveScenarioId(op.targetTitle as string);
        await repos.characters.removeScenario(characterId, id);
        break;
      }
      case 'addPartnerLink':
        await repos.characters.addPartnerLink(
          characterId,
          op.partnerId as string,
          op.isDefault as boolean
        );
        break;
      case 'removePartnerLink':
        await repos.characters.removePartnerLink(characterId, op.partnerId as string);
        break;
      case 'setFavorite':
        await repos.characters.setFavorite(characterId, op.value as boolean);
        break;
      case 'setControlledBy':
        await repos.characters.setControlledBy(characterId, op.value as 'llm' | 'user');
        break;
      case 'setCanBeCarina':
        await repos.characters.setCanBeCarina(characterId, op.value as boolean);
        break;
      default:
        throw new Error(`unknown op: ${op.op}`);
    }
    await snapshot(op.op);
    opIndex += 1;
  }

  // MAIN db: the slim characters row.
  const charColumns = (
    (await rawQuery('PRAGMA table_info(characters)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const charRows = (await rawQuery('SELECT * FROM characters')) as Array<
    Record<string, unknown>
  >;
  const characters = canonicalizeRows({
    table: 'characters',
    columns: charColumns,
    rawRows: charRows,
    orderBy: 'name',
  });

  // MOUNT-INDEX db: the store tables.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable (degraded open?)');
  const dumpTable = (table: string, orderBy: string) => {
    const columns = (
      midb.pragma(`table_info(${table})`) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = midb
      .prepare(`SELECT * FROM ${table}`)
      .all() as Array<Record<string, unknown>>;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const points = dumpTable('doc_mount_points', 'name');
  const folders = dumpTable('doc_mount_folders', 'path');
  const files = dumpTable('doc_mount_files', 'sha256');
  const documents = dumpTable('doc_mount_documents', 'contentSha256');
  const links = dumpTable('doc_mount_file_links', 'relativePath');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'characters-arrays-tier2',
      defaultColumnTrail: trail,
      addScenarioReturns,
      characters,
      points,
      folders,
      files,
      documents,
      links,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters-arrays-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
