import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';

import { E2E_PASSPHRASE, INSTANCE_DIR } from './env';

/**
 * The P4.9a wire: seed two `photos/` gallery entries into the SHARED e2e
 * instance so `photos-flow.spec.ts` has data of its OWN to walk.
 *
 * Why seed at all, when the shared instance already carries Aria's vault
 * avatar as a photos-folder link: because the characters walk DELETES gallery
 * tiles, and the beats run against one shared server. A walk that asserted on
 * whatever happened to be there would be reading a moving target — the standing
 * "never assert absolute counts a sibling spec can grow" rule, applied one step
 * earlier, at the data.
 *
 * The two rows carry deliberately distinctive captions ("Zeppelin over the
 * Ironworks" / "The Ironworks at dusk") so the walk can find, filter, and
 * delete exclusively its own rows no matter what else the instance holds. Ids
 * are e2e-only (`9a…`), well outside every fixture family's scheme, so nothing
 * can collide.
 *
 * Mechanics mirror `seed-courier-fixture`: raw SQL through the CLI against the
 * shared instance, one statement per invocation (the CLI unwraps the PBKDF2
 * `.dbkey` each time). The rows go into whichever ENABLED database mount the
 * instance already has — discovered here rather than pinned, because the
 * salon fixture's mount ids are not this lane's to assume.
 *
 * The `extractedText` is real kept-image markdown (the frontmatter block plus
 * an `## Original prompt` section), because the gallery projection PARSES it:
 * the caption, the tags, and the prompt excerpt the cards and modal render all
 * come out of that text, not out of columns.
 */

const MOUNT_FILE_A = '9a000000-0000-4000-8000-0000000000f1';
const MOUNT_FILE_B = '9a000000-0000-4000-8000-0000000000f2';
const LINK_A = '9a000000-0000-4000-8000-0000000000e1';
const LINK_B = '9a000000-0000-4000-8000-0000000000e2';

/** The captions the walk searches for and deletes — its own, and only its own. */
export const PHOTOS_E2E_CAPTION_A = 'Zeppelin over the Ironworks';
export const PHOTOS_E2E_CAPTION_B = 'The Ironworks at dusk';

const TS = '2026-03-01T00:00:00.000Z';

function write(cli: string, statement: string, mount = false): void {
  const args = [
    'db',
    '--data-dir',
    INSTANCE_DIR,
    ...(mount ? ['--mount-points'] : []),
    '--write',
    statement,
  ];
  const res = spawnSync(cli, args, {
    env: { ...process.env, QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  });
  if (res.status !== 0) {
    throw new Error(`photos-fixture write failed (${statement}):\n${res.stdout}\n${res.stderr}`);
  }
}

function read(cli: string, statement: string, mount = false): Array<Record<string, unknown>> {
  const args = [
    'db',
    '--data-dir',
    INSTANCE_DIR,
    ...(mount ? ['--mount-points'] : []),
    '--json',
    statement,
  ];
  const res = spawnSync(cli, args, {
    env: { ...process.env, QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  });
  if (res.status !== 0) {
    throw new Error(`photos-fixture read failed (${statement}):\n${res.stdout}\n${res.stderr}`);
  }
  return JSON.parse(res.stdout) as Array<Record<string, unknown>>;
}

/** Real kept-image markdown — the projection parses this, it does not read columns. */
function markdown(caption: string, prompt: string, tags: string[]): string {
  const tagLines = tags.length
    ? tags.map((t) => `  - ${t}`).join('\n')
    : '  []';
  return (
    `---\ntags:\n${tagLines}\nlinkedBy: You\nlinkedById: ffffffff-ffff-ffff-ffff-ffffffffffff\n` +
    `linkedByRole: user\ngenerationModel: e2e-model\n---\n\n` +
    `## Original prompt\n\n${prompt}\n\n` +
    `You saved this image at ${TS} with this caption: ${caption}\n`
  );
}

export function seedPhotosFixture(cli: string): void {
  // The destination mount: whichever enabled DATABASE store the instance has.
  const mounts = read(
    cli,
    "SELECT id FROM doc_mount_points WHERE enabled = 1 AND mountType = 'database' LIMIT 1",
    true,
  );
  const mountPointId = mounts[0]?.['id'] as string | undefined;
  if (!mountPointId) {
    throw new Error('photos-fixture: the shared instance has no enabled database mount');
  }

  const rows: Array<{
    fileId: string;
    linkId: string;
    caption: string;
    prompt: string;
    tags: string[];
    path: string;
    sha: string;
  }> = [
    {
      fileId: MOUNT_FILE_A,
      linkId: LINK_A,
      caption: PHOTOS_E2E_CAPTION_A,
      prompt: 'A zeppelin drifting above the ironworks at first light.',
      tags: ['zeppelin', 'ironworks'],
      path: 'photos/e2e-zeppelin.webp',
      sha: '9a11111111111111111111111111111111111111111111111111111111111111',
    },
    {
      fileId: MOUNT_FILE_B,
      linkId: LINK_B,
      caption: PHOTOS_E2E_CAPTION_B,
      prompt: 'The ironworks at dusk, chimneys banked and quiet.',
      tags: ['ironworks'],
      path: 'photos/e2e-ironworks.webp',
      sha: '9a22222222222222222222222222222222222222222222222222222222222222',
    },
  ];

  for (const row of rows) {
    const raw = markdown(row.caption, row.prompt, row.tags);
    const textSha = createHash('sha256').update(raw, 'utf-8').digest('hex');
    const text = raw.replace(/'/g, "''");
    write(
      cli,
      `INSERT OR REPLACE INTO doc_mount_files ` +
        `(id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt) ` +
        `VALUES ('${row.fileId}', '${row.sha}', 64, 'blob', 'database', '${TS}', '${TS}')`,
      true,
    );
    write(
      cli,
      `INSERT OR REPLACE INTO doc_mount_file_links ` +
        `(id, fileId, mountPointId, relativePath, fileName, folderId, ` +
        `originalFileName, originalMimeType, description, conversionStatus, ` +
        `extractedText, extractedTextSha256, extractionStatus, chunkCount, ` +
        `allowEmbed, allowCharacterRead, allowCharacterWrite, ` +
        `lastModified, createdAt, updatedAt) ` +
        `VALUES ('${row.linkId}', '${row.fileId}', '${mountPointId}', '${row.path}', ` +
        `'${row.path.split('/').pop()}', NULL, ` +
        `'${row.path.split('/').pop()}', 'image/webp', '${row.caption}', 'converted', ` +
        `'${text}', '${textSha}', 'converted', 0, ` +
        `1, 1, 1, ` +
        `'${TS}', '${TS}', '${TS}')`,
      true,
    );
  }

  process.stderr.write(
    `[e2e] seeded ${rows.length} photos/ gallery entries into mount ${mountPointId}\n`,
  );
}
