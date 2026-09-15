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

const ROLL_FILE_ID = 'p4d188-roll-1';
const ROLL_MOUNT_FILE_ID = 'p4d188-roll-mount-file-1';
const ROLL_LINK_ID = 'p4d188-roll-link-1';
const ROLL_BLOB_ID = 'p4d188-roll-blob-1';
const ROLL_NAME = 'plate-of-aria.png';
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

    // The committed pair predates `files.generationKey`; boot heals a live
    // instance, but this plant runs BEFORE the server boots, so heal the copy
    // the same way first (idempotent — the boot ensure then finds it present).
    runCliWrite(cli, `ALTER TABLE files ADD COLUMN generationKey TEXT;`);

    // (1) the keyed, tagged files row — v4's whole definition of a roll. The
    //     storage key is the `mount-blob:<mountPointId>:<blobId>` shape every
    //     reader parses (`parse_mount_blob_storage_key`); a `mount:` prefix
    //     parses as nothing and silently skips the mount-point scoping.
    runCliWrite(
      cli,
      `INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, width, height,
         linkedTo, source, category, generationPrompt, generationModel, tags, storageKey,
         generationKey, createdAt, updatedAt)
       VALUES ('${ROLL_FILE_ID}', '${SINGLE_USER_ID}', '${ROLL_SHA}', '${ROLL_NAME}',
         'image/png', ${ROLL_BYTES.length}, 1, 1, '[]', 'GENERATED', 'IMAGE',
         'Aria in her flying coat', 'flux-1', '["${ariaId}"]',
         'mount-blob:${vaultId}:${ROLL_BLOB_ID}', 'p4d188-config-key-v1',
         '${now}', '${now}');`,
    );

    // (2) the bytes. `doc_mount_blobs.fileId` and `doc_mount_file_links.fileId`
    //     both name a `doc_mount_files` row (mount-index-local, NOT `files.id`),
    //     and the blob table's FK enforces it — so that row comes first, and the
    //     roll's link is later resolved BY SHA through it (`find_by_sha256`).
    runCliMountWrite(
      cli,
      `INSERT INTO doc_mount_files (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt)
       VALUES ('${ROLL_MOUNT_FILE_ID}', '${ROLL_SHA}', ${ROLL_BYTES.length}, 'image/png', 'UPLOAD',
         '${now}', '${now}');`,
    );
    runCliMountWrite(
      cli,
      `INSERT INTO doc_mount_blobs (id, fileId, sha256, sizeBytes, storedMimeType, data, createdAt, updatedAt)
       VALUES ('${ROLL_BLOB_ID}', '${ROLL_MOUNT_FILE_ID}', '${ROLL_SHA}', ${ROLL_BYTES.length},
         'image/png', x'${ROLL_BYTES.toString('hex')}', '${now}', '${now}');`,
    );
    // (3) the vault link under images/history/ — what gives the tile a URL.
    runCliMountWrite(
      cli,
      `INSERT INTO doc_mount_file_links (id, fileId, mountPointId, relativePath, fileName,
         originalFileName, originalMimeType, lastModified, createdAt, updatedAt)
       VALUES ('${ROLL_LINK_ID}', '${ROLL_MOUNT_FILE_ID}', '${vaultId}',
         'images/history/${ROLL_NAME}', '${ROLL_NAME}', '${ROLL_NAME}', 'image/png',
         '${now}', '${now}', '${now}');`,
    );

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
  test('expand → keep → set as avatar → discard, and the section disappears with the last plate', async ({
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
    await expect(header).toContainText('1 plate');
    await expect(
      page.getByText('Portraits the house has already developed for Aria', { exact: false }),
    ).toBeVisible();

    const section = page.locator('qt-avatar-rolls-section');
    await expect(section.locator('img')).toHaveCount(0);
    await header.click();
    await expect(section.locator('img')).toHaveCount(1);

    // Keep: the bookmark flips to the done state and the album gains a photo.
    // The album grid's tiles ONLY: the section's tile is a copy of the album's
    // markup with the same class string, so an unscoped `button.aspect-square`
    // counts the roll too and the arithmetic below passes by accident until
    // the roll is discarded.
    const albumTiles = page.locator('button.aspect-square').filter({
      hasNot: page.locator('qt-avatar-rolls-section button.aspect-square'),
    });
    const albumBefore = await albumTiles.count();
    await section.getByTitle('Keep in the photo album').click();
    await expect(page.getByText('Kept in the photo album')).toBeVisible({ timeout: 10_000 });
    await expect(section.getByTitle('Already in the photo album')).toBeDisabled();
    await expect(albumTiles).toHaveCount(albumBefore + 1, { timeout: 10_000 });

    // ...and the tooltip now says the album copy would survive a discard.
    await expect(
      section.getByTitle('Discard this plate (the album copy stays)'),
    ).toBeVisible();

    // Promote: the portrait pointer moves to the ALBUM link the keep minted,
    // and the section badges the plate the server resolved as the portrait.
    await section.getByTitle('Set as avatar').click();
    await expect(page.getByText('Avatar updated!')).toBeVisible({ timeout: 10_000 });
    await expect(section.getByText('Avatar', { exact: true })).toBeVisible();
    await expect(section.getByTitle('Set as avatar')).toHaveCount(0);

    // Discard: two clicks, and the section goes with the last plate.
    await section.getByTitle('Discard this plate (the album copy stays)').click();
    await section.getByTitle('Click again to confirm delete').click();
    await expect(page.getByText('Roll discarded; the album copy stays')).toBeVisible({
      timeout: 10_000,
    });
    // The `return null` arm: no header, no description, nothing at all.
    await expect(page.locator('qt-avatar-rolls-section button[aria-expanded]')).toHaveCount(0, {
      timeout: 10_000,
    });
    await expect(section).toHaveText('');
    // The album copy stayed — that is what the sentence promised.
    await expect(albumTiles).toHaveCount(albumBefore + 1);
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
