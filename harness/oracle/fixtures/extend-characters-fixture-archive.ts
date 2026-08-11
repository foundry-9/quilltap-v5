/**
 * P4.D63 — the character-archive EXTENSION of the committed characters fixture.
 *
 * The archive drift (v4 `d553f72a`) needs a tombstoned character in the
 * characters family: something for the `archived=` chokepoint to exclude, the
 * write guard to refuse, the wardrobe write to throw on, and the participant
 * badge to render. It also needs an ARCHIVE-category `files` row — the
 * ChangePassphraseCard's courtesy count, the re-encrypt sweep's input, and the
 * wipe's spare-me candidate.
 *
 * **This EXTENDS the committed .db files in place rather than rebuilding
 * them.** The main builder (`build-characters-fixture.ts`) MINTS the vault
 * mount-point / link / blob ids for every character; a rebuild would change
 * all of them, invalidating every transcribed literal in the sibling lanes'
 * e2e seeds along with each characters-family oracle. Appending preserves
 * every existing id, so the blast radius is exactly the rows added here — the
 * committed-fixture rule's "rebuild OR mutate", taking the cheaper half.
 *
 * The three archive columns arrive by `ALTER TABLE`, which is not a liberty:
 * it is verbatim what v4's own `add-character-archive-fields-v1` migration
 * runs, so the resulting schema is one v4 itself produces.
 *
 * What it adds (all ids pinned; Fenn's vault ids are minted and baked):
 *   - `characters.archivedAt` / `archiveFileId` / `archivedAvatarFileId`,
 *   - **Fenn** — an ARCHIVED character with a real (pruned-in-place) vault, a
 *     wardrobe item, a system prompt, and `archiveFileId` pointing at
 *   - one **ARCHIVE**-category `files` row (Fenn's bundle),
 *   - **Ledger of Fenn** — a chat where Fenn sits `absent` (what archiving does
 *     to a seat) beside an `active` Bram, so the participant projection has an
 *     archived seat to badge. Deliberately NOT the existing Solo Voyage chat:
 *     that one is Aria-exclusive and carries the cascade-delete branch.
 *
 * Idempotent: re-running detects Fenn and exits without touching anything.
 *
 * Run (Node 24, from the v4 checkout) AFTER `build-characters-fixture.ts`:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/extend-characters-fixture-archive.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

/** Pinned ids for everything this extension adds. */
export const FENN = 'a1000000-0000-4000-8000-000000000006';
export const FENN_ARCHIVE_FILE = 'f0000003-0000-4000-8000-000000000003';
export const FENN_CHAT = 'c1000000-0000-4000-8000-000000000002';
export const FENN_SEAT = 'b1000000-0000-4000-8000-000000000021';
export const BRAM_SEAT = 'b1000000-0000-4000-8000-000000000022';
const BRAM = 'a1000000-0000-4000-8000-000000000002';
const CONN = 'c0000001-0000-4000-8000-000000000001';
/** The instant Fenn was archived — pinned so `archivedAt` diffs byte-exact. */
export const FENN_ARCHIVED_AT = '2026-03-15T09:30:00.000Z';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'characters.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_CHARACTERS_MAIN;
  const mountOut = process.env.QT_FIXTURE_CHARACTERS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_CHARACTERS_MAIN and QT_FIXTURE_CHARACTERS_MOUNT must point at the committed .db files',
    );
  }
  for (const p of [mainOut, mountOut]) {
    if (!existsSync(p)) throw new Error(`missing fixture ${p} — run build-characters-fixture.ts first`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-characters-archive-ext-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  if (await repos.characters.findByIdRaw(FENN)) {
    process.stderr.write('characters archive extension: already applied — nothing to do\n');
    closeMountIndexSQLiteClient();
    await closeDatabase();
    process.exit(0);
  }

  // 1. The three archive columns — verbatim v4's `add-character-archive-fields-v1`.
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  const existing = new Set(
    (maindb.prepare('PRAGMA table_info("characters")').all() as Array<{ name: string }>).map(
      (c) => c.name,
    ),
  );
  for (const col of ['archivedAt', 'archiveFileId', 'archivedAvatarFileId']) {
    if (!existing.has(col)) maindb.exec(`ALTER TABLE "characters" ADD COLUMN "${col}" TEXT`);
  }

  // 2. Fenn's archive bundle `files` row (category ARCHIVE).
  await repos.files.create(
    {
      userId: spec.userId,
      sha256: '3333333333333333333333333333333333333333333333333333333333333333',
      originalFilename: 'Fenn.qtap',
      mimeType: 'application/octet-stream',
      size: 4096,
      source: 'GENERATED',
      category: 'ARCHIVE',
      folderPath: '/archives',
      storageKey: 'archives/fenn.qtap',
      linkedTo: [],
      tags: [],
    } as never,
    { id: FENN_ARCHIVE_FILE, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. Fenn herself — created LIVE (so she mints a real vault the way every
  //    other fixture character does), given content worth pruning, and only
  //    then tombstoned. Archiving after the fact is also the only order the
  //    write guard permits.
  await repos.characters.create(
    {
      name: 'Fenn',
      userId: spec.userId,
      title: 'The Retired Cartographer',
      description: 'Keeps to the map room these days.',
      personality: 'Precise, patient, quietly amused.',
      aliases: ['Fenwick'],
      controlledBy: 'llm',
      talkativeness: 0.4,
      defaultConnectionProfileId: CONN,
      isFavorite: false,
      npc: false,
      canBeCarina: true,
      tags: [],
    } as never,
    { id: FENN, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.addSystemPrompt(FENN, {
    name: 'Cartographer', content: 'Speak in bearings and landmarks.', isDefault: true,
  } as never);
  await repos.wardrobe.create({
    characterId: FENN,
    title: 'Surveyor Coat',
    description: 'Oilcloth, many pockets.',
    imagePrompt: null,
    types: ['outerwear'],
    componentItemIds: [],
    appropriateness: null,
    isDefault: true,
    replace: false,
    migratedFromClothingRecordId: null,
  } as never);

  // 4. Tombstone her. `canBeCarina: true` above is deliberate: it makes the
  //    Carina probe's `!archivedAt &&` clause load-bearing — without it the
  //    filter would pass for the wrong reason.
  await repos.characters.update(FENN, {
    archivedAt: FENN_ARCHIVED_AT,
    archiveFileId: FENN_ARCHIVE_FILE,
  } as never);

  // 5. A chat where the tombstone sits `absent` (archiving's seat flip) beside
  //    a live seat.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Ledger of Fenn',
      chatType: 'salon',
      contextSummary: null,
      participants: [
        {
          id: FENN_SEAT, type: 'CHARACTER', characterId: FENN, controlledBy: 'llm',
          status: 'absent', isActive: false, connectionProfileId: CONN,
          createdAt: TS, updatedAt: TS,
        },
        {
          id: BRAM_SEAT, type: 'CHARACTER', characterId: BRAM, controlledBy: 'llm',
          status: 'active', isActive: true, connectionProfileId: CONN,
          createdAt: TS, updatedAt: TS,
        },
      ],
    } as never,
    { id: FENN_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `extended characters fixture: +Fenn (archived ${FENN_ARCHIVED_AT}), ` +
      `+1 ARCHIVE file, +1 chat with an absent seat → ${mainOut}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters archive extension failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
