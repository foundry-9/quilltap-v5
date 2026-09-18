import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, openSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Page } from './support/fixtures';

import { makeDbKeyFile } from './support/dbkey';
import {
  ARTIFACTS_DIR,
  cliBinary,
  E2E_PASSPHRASE,
  FIXTURE_USER,
  FIXTURES_DIR,
  SINGLE_USER_ID,
  spaDir,
  TEST_PEPPER,
  webBinary,
} from './support/env';

/**
 * P4.D188 — browser walk of the Avatar Rolls section in a character's Photo
 * Gallery tab (v4 `4dcbe0d21`). Runs its OWN locked server + instance dir over
 * the committed `characters-*` fixture pair, the `characters-flow.spec.ts`
 * recipe: copy the pair, write the locked .dbkey, rewrite the fixture user id
 * to SINGLE_USER_ID via `quilltap db --write` BEFORE launch, then boot
 * quilltap-web on a spec-private port. No LLM send happens, so no mock LLM.
 *
 * ACTIVATE-AT-UNIFY behind {@link P4D185_SERVER_LANDED}: the three
 * `characterAvatarRoll*` verbs and their REST sub-routes are P4.D185's, and
 * the `files.generationKey` column they read is P4.D182's. Until BOTH land,
 * the seed below cannot even be written — the column does not exist — so the
 * gate sits at describe level (the `P4.6t — Memories tab` precedent) and no
 * hook runs while it is false.
 *
 * ## The seed shape
 *
 * No verb writes a `generationKey` until P4.D184's avatar job does, and this
 * walk must not spend an image generation to get one. So the roll is PLANTED
 * with the CLI before the server boots, in three rows:
 *
 *  1. a `files` row — `category = 'IMAGE'`, a non-null `generationKey`, and
 *     the character id inside the JSON `tags` array. Those three together ARE
 *     v4's definition of a roll (`avatar-rolls-service.ts:329-331` plus
 *     `findRollsForCharacter`'s `files.findByTag(characterId)`); there is no
 *     path predicate, deliberately;
 *  2. a `doc_mount_blobs` row in the mount index carrying the bytes;
 *  3. a `doc_mount_file_links` row under `images/history/` in Aria's vault,
 *     which is what gives the tile a URL to render (`rollLink`).
 *
 * The album copy is NOT seeded: the walk's Keep beat is what creates it, and
 * its `photos/` link is what the bookmark reads back as "already kept".
 */

const P4D185_SERVER_LANDED = true;

const ROLLS_PORT = 4327;
const ROLLS_BASE_URL = `http://127.0.0.1:${ROLLS_PORT}`;
const ROLLS_INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'avatar-rolls-instance');
const ROLLS_DATA_DIR = resolve(ROLLS_INSTANCE_DIR, 'data');
const ROLLS_SERVER_LOG = resolve(ARTIFACTS_DIR, 'avatar-rolls-server.log');

/** Every fixture table the walk reads is filtered by userId — rewrite them all. */
const USER_TABLES = ['characters', 'chats', 'tags', 'files'];

/**
 * TWO planted rolls, because the walk's two halves cannot share one plate: a
 * roll promoted to the portrait loses its Discard control (v4 `GalleryImage`'s
 * `!isAvatar` gate — the first activation waited 90 s for a button the
 * portrait plate cannot have), so A is kept and promoted, and B is discarded.
 * Different bytes (a PNG and a GIF) so the two blobs carry different shas.
 */
const ROLL_FILE_ID = 'p4d188-roll-1';
const ROLL_MOUNT_FILE_ID = 'p4d188-roll-mount-file-1';
const ROLL_LINK_ID = 'p4d188-roll-link-1';
const ROLL_BLOB_ID = 'p4d188-roll-blob-1';
const ROLL_NAME = 'plate-of-aria.png';
const ROLL2_FILE_ID = 'p4d188-roll-2';
const ROLL2_MOUNT_FILE_ID = 'p4d188-roll-mount-file-2';
const ROLL2_LINK_ID = 'p4d188-roll-link-2';
const ROLL2_BLOB_ID = 'p4d188-roll-blob-2';
const ROLL2_NAME = 'second-plate-of-aria.gif';
/** A real 1×1 GIF, for the second plate. */
const ROLL2_BYTES = Buffer.from('R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==', 'base64');
const ROLL2_SHA = createHash('sha256').update(ROLL2_BYTES).digest('hex');
/**
 * A real 1×1 PNG: the tile is an `<img>`, and four bytes of "RIFF" would fire
 * its `error` handler, mark the roll missing, and take the Download button
 * with it. The sha is the bytes' own, so the link resolves by sha as the
 * service resolves it.
 */
const ROLL_BYTES = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
  'base64',
);
const ROLL_SHA = createHash('sha256').update(ROLL_BYTES).digest('hex');

let server: ChildProcess | undefined;

test.describe('P4.D188 — Avatar Rolls in the Photo Gallery tab', () => {
  test.skip(
    !P4D185_SERVER_LANDED,
    'awaits P4.D185 (the characterAvatarRoll* verbs) and P4.D182 (files.generationKey)',
  );

  test.beforeAll(async () => {
    // Each `quilltap db --write` unwraps the .dbkey via PBKDF2 (≈5s).
    test.setTimeout(150_000);

    const web = webBinary();
    const cli = cliBinary();

    rmSync(ROLLS_INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(ROLLS_DATA_DIR, { recursive: true });
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-main.db'),
      resolve(ROLLS_DATA_DIR, 'quilltap.db'),
    );
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(ROLLS_DATA_DIR, 'quilltap-mount-index.db'),
    );
    writeFileSync(
      resolve(ROLLS_DATA_DIR, 'quilltap.dbkey'),
      makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE),
    );
    for (const table of USER_TABLES) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }

    // Aria's id and vault mount point, resolved by NAME: the fixture builder
    // mints ids, so a transcribed literal would rot the moment it is rebuilt.
    // The roll's link has to live in her vault, or the service classifies it
    // as neither the roll's link nor the album's.
    const ariaId = readOne(cli, `SELECT id FROM characters WHERE name = 'Aria' LIMIT 1;`);
    const vaultId = readOne(
      cli,
      `SELECT characterDocumentMountPointId FROM characters WHERE name = 'Aria' LIMIT 1;`,
    );
    if (!ariaId || !vaultId) {
      throw new Error('the characters fixture gave Aria no vault mount point to plant a roll in');
    }
    const now = '2026-09-12T10:00:00.000Z';

    // The committed pair USED to predate `files.generationKey`; boot heals a
    // live instance, but this plant runs BEFORE the server boots, so the copy
    // is healed the same way first — GUARDED, because an `ALTER TABLE … ADD
    // COLUMN` is not idempotent. The `baa85e19b` round's P4.D201 widened the
    // pair through v4's own migration statements (the column is now
    // committed), and the unconditional ALTER answered `duplicate column
    // name` on the beat's first run after it — the "a widened fixture's other
    // readers" class, found at that round's unified gate.
    const hasGenerationKey = readOne(
      cli,
      `SELECT COUNT(*) FROM pragma_table_info('files') WHERE name = 'generationKey';`,
    );
    if (hasGenerationKey === '0') {
      runCliWrite(cli, `ALTER TABLE files ADD COLUMN generationKey TEXT;`);
    }

    const plantRoll = (r: {
      fileId: string;
      mountFileId: string;
      linkId: string;
      blobId: string;
      name: string;
      mime: string;
      bytes: Buffer;
      sha: string;
      key: string;
    }): void => {
      // (1) the keyed, tagged files row — v4's whole definition of a roll. The
      //     storage key is the `mount-blob:<mountPointId>:<blobId>` shape every
      //     reader parses (`parse_mount_blob_storage_key`); a `mount:` prefix
      //     parses as nothing and silently skips the mount-point scoping.
      runCliWrite(
        cli,
        `INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, width, height,
           linkedTo, source, category, generationPrompt, generationModel, tags, storageKey,
           generationKey, createdAt, updatedAt)
         VALUES ('${r.fileId}', '${SINGLE_USER_ID}', '${r.sha}', '${r.name}',
           '${r.mime}', ${r.bytes.length}, 1, 1, '[]', 'GENERATED', 'IMAGE',
           'Aria in her flying coat', 'flux-1', '["${ariaId}"]',
           'mount-blob:${vaultId}:${r.blobId}', '${r.key}',
           '${now}', '${now}');`,
      );
      // (2) the bytes. `doc_mount_blobs.fileId` and `doc_mount_file_links.fileId`
      //     both name a `doc_mount_files` row (mount-index-local, NOT `files.id`),
      //     and the blob table's FK enforces it — so that row comes first, and the
      //     roll's link is later resolved BY SHA through it (`find_by_sha256`).
      runCliMountWrite(
        cli,
        `INSERT INTO doc_mount_files (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt)
         VALUES ('${r.mountFileId}', '${r.sha}', ${r.bytes.length}, '${r.mime}', 'UPLOAD',
           '${now}', '${now}');`,
      );
      runCliMountWrite(
        cli,
        `INSERT INTO doc_mount_blobs (id, fileId, sha256, sizeBytes, storedMimeType, data, createdAt, updatedAt)
         VALUES ('${r.blobId}', '${r.mountFileId}', '${r.sha}', ${r.bytes.length},
           '${r.mime}', x'${r.bytes.toString('hex')}', '${now}', '${now}');`,
      );
      // (3) the vault link under images/history/ — what gives the tile a URL.
      runCliMountWrite(
        cli,
        `INSERT INTO doc_mount_file_links (id, fileId, mountPointId, relativePath, fileName,
           originalFileName, originalMimeType, lastModified, createdAt, updatedAt)
         VALUES ('${r.linkId}', '${r.mountFileId}', '${vaultId}',
           'images/history/${r.name}', '${r.name}', '${r.name}', '${r.mime}',
           '${now}', '${now}', '${now}');`,
      );
    };
    plantRoll({
      fileId: ROLL_FILE_ID, mountFileId: ROLL_MOUNT_FILE_ID, linkId: ROLL_LINK_ID,
      blobId: ROLL_BLOB_ID, name: ROLL_NAME, mime: 'image/png', bytes: ROLL_BYTES,
      sha: ROLL_SHA, key: 'p4d188-config-key-v1',
    });
    plantRoll({
      fileId: ROLL2_FILE_ID, mountFileId: ROLL2_MOUNT_FILE_ID, linkId: ROLL2_LINK_ID,
      blobId: ROLL2_BLOB_ID, name: ROLL2_NAME, mime: 'image/gif', bytes: ROLL2_BYTES,
      sha: ROLL2_SHA, key: 'p4d188-config-key-v2',
    });

    const logFd = openSync(ROLLS_SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(ROLLS_PORT),
        '--data-dir',
        ROLLS_INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
  });

  test.afterAll(async () => {
    if (server?.pid) {
      try {
        process.kill(-server.pid, 'SIGTERM');
      } catch {
        try {
          process.kill(server.pid, 'SIGTERM');
        } catch {
          // already gone
        }
      }
    }
    rmSync(ROLLS_INSTANCE_DIR, { recursive: true, force: true });
  });

  /**
   * The whole section in one walk, because every beat depends on the last:
   * expand → keep → promote → discard, and the section's disappearance at the
   * end IS the `return null` arm, which only the last roll's removal can show.
   */
  test('expand → keep → set as avatar → discard the other plate; the portrait plate cannot be discarded', async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await page.goto(`${ROLLS_BASE_URL}/characters`);
    await unlockIfLocked(page);
    await openAriaGallery(page);

    // Collapsed by default, with the count badge and the description already
    // showing — the header is the affordance, not an empty shelf.
    const header = page.locator('qt-avatar-rolls-section button[aria-expanded]');
    await expect(header).toBeVisible({ timeout: 10_000 });
    await expect(header).toHaveAttribute('aria-expanded', 'false');
    await expect(header).toContainText('Avatar Rolls');
    await expect(header).toContainText('2 plates');
    await expect(
      page.getByText('Portraits the house has already developed for Aria', { exact: false }),
    ).toBeVisible();

    const section = page.locator('qt-avatar-rolls-section');
    await expect(section.locator('img')).toHaveCount(0);
    await header.click();
    await expect(section.locator('img')).toHaveCount(2);
    // Newest first: plate B (the GIF) was planted second at the same stamp, so
    // the tiles are addressed by their alt text rather than by position.
    const plateA = section.locator('.relative.group').filter({ has: page.locator(`img[alt="${ROLL_NAME}"]`) });
    const plateB = section.locator('.relative.group').filter({ has: page.locator(`img[alt="${ROLL2_NAME}"]`) });
    await expect(plateA).toHaveCount(1);
    await expect(plateB).toHaveCount(1);

    // Keep A: the bookmark flips to the done state and the album gains a photo.
    // The album grid's tiles ONLY: the section's tile is a copy of the album's
    // markup with the same class string, so an unscoped `button.aspect-square`
    // counts the rolls too and the arithmetic below passes by accident.
    // A `hasNot` filter asks whether the BUTTON contains a section tile, which
    // no button does — the first draft excluded nothing and counted the rolls.
    const allTiles = page.locator('button.aspect-square');
    const sectionTiles = section.locator('button.aspect-square');
    const albumCount = async (): Promise<number> =>
      (await allTiles.count()) - (await sectionTiles.count());
    const albumBefore = await albumCount();
    await plateA.getByTitle('Keep in the photo album').click();
    await expect(page.getByText('Kept in the photo album')).toBeVisible({ timeout: 10_000 });
    await expect(plateA.getByTitle('Already in the photo album')).toBeDisabled();
    await expect.poll(albumCount, { timeout: 10_000 }).toBe(albumBefore + 1);
    // ...and A's tooltip now says the album copy would survive a discard.
    await expect(plateA.getByTitle('Discard this plate (the album copy stays)')).toBeVisible();

    // Promote A: the portrait pointer moves to the ALBUM link the keep minted,
    // the section badges the plate the server resolved as the portrait, and
    // — v4's `!isAvatar` gate — the portrait plate loses both its Set-as-avatar
    // and its Discard controls (the first activation waited on the latter).
    await plateA.getByTitle('Set as avatar').click();
    await expect(page.getByText('Avatar updated!')).toBeVisible({ timeout: 10_000 });
    await expect(plateA.getByText('Avatar', { exact: true })).toBeVisible();
    await expect(plateA.getByTitle('Set as avatar')).toHaveCount(0);
    await expect(plateA.getByTitle('Discard this plate (the album copy stays)')).toHaveCount(0);
    await expect(plateB.getByTitle('Set as avatar')).toHaveCount(1);

    // Discard B: two clicks, the plain sentence (no album copy), and the
    // section stays with the one plate it still has.
    await plateB.getByTitle('Discard this plate').click();
    await plateB.getByTitle('Click again to confirm delete').click();
    await expect(page.getByText('Roll discarded', { exact: true })).toBeVisible({
      timeout: 10_000,
    });
    await expect(section.locator('img')).toHaveCount(1, { timeout: 10_000 });
    await expect(header).toContainText('1 plate');
    // The album copy of A stayed — nothing here touched the album.
    await expect.poll(albumCount, { timeout: 10_000 }).toBe(albumBefore + 1);
  });
});

async function unlockIfLocked(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const roster = page.getByRole('heading', { name: 'Characters', exact: true });
  await expect(passphrase.or(roster).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.isVisible()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
  await expect(roster).toBeVisible({ timeout: 10_000 });
}

async function openAriaGallery(page: Page): Promise<void> {
  const aria = page
    .locator('.character-card-grid .character-card')
    .filter({ hasText: 'Aria' })
    .first();
  await aria.locator('p.line-clamp-3').click();
  await expect(page.getByRole('heading', { name: 'Aria' })).toBeVisible();
  await page.getByRole('button', { name: 'Photo Gallery' }).click();
}

function runCliWrite(cli: string, sql: string): void {
  const res = spawnSync(cli, ['db', '--data-dir', ROLLS_INSTANCE_DIR, '--write', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    throw new Error(`CLI rewrite failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
}

function runCliMountWrite(cli: string, sql: string): void {
  const res = spawnSync(
    cli,
    ['db', '--data-dir', ROLLS_INSTANCE_DIR, '--mount-points', '--write', sql],
    {
      env: {
        ...withoutPepper(),
        QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE,
        QUILLTAP_QUIET_HINTS: '1',
      },
      encoding: 'utf8',
    },
  );
  if (res.status !== 0) {
    throw new Error(`CLI mount rewrite failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
}

/** One scalar out of the main DB, for the ids the plant has to reference. */
function readOne(cli: string, sql: string): string | null {
  const res = spawnSync(cli, ['db', '--data-dir', ROLLS_INSTANCE_DIR, '--json', sql], {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0) {
    throw new Error(`CLI read failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
  const rows = JSON.parse(res.stdout) as Array<Record<string, unknown>>;
  const first = rows[0];
  if (!first) return null;
  const value = Object.values(first)[0];
  return value == null ? null : String(value);
}

function withoutPepper(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['ENCRYPTION_MASTER_PEPPER'];
  return env;
}

async function waitForHealth(): Promise<void> {
  const deadline = Date.now() + 30_000;
  let lastErr = '';
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${ROLLS_BASE_URL}/health`);
      if (res.status === 423 || res.status === 200) return;
      lastErr = `health status ${res.status}`;
    } catch (e) {
      lastErr = e instanceof Error ? e.message : String(e);
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(
    `avatar-rolls server did not become ready within 30s (${lastErr}); see ${ROLLS_SERVER_LOG}`,
  );
}
