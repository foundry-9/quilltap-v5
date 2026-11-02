/**
 * Oracle case (P4.D354, v4 `faeae2b71`): the persistent-target check — the
 * guard that stops sync (and, in v4, deconvert) from writing a store into the
 * container's own layer.
 *
 * Drives the REAL exports of v4's `lib/mount-index/persistent-target.ts`:
 * `parseMountInfo`, `findContainingMount`, `checkPersistentTarget(target,
 * readMountInfo)`. The case runs with `DOCKER_CONTAINER=true` in its own env
 * (set below, before the import) so v4's SERVER probe `isDockerEnvironment()`
 * answers true and the check runs; the mount table is injected through the
 * second parameter (v5's `MountInfoSource` seam — contract C7).
 *
 * Texts: v4's own test fixture `DOCKER_DESKTOP_MOUNTINFO` (byte-for-byte) and
 * a second table with a `\040`-escaped mount, an optional-fields line (the
 * `shared:1` tags before the `-`), a malformed line, a `-` in field 0 … The
 * symlink row builds a real directory + link under a FIXED temp path the row
 * names (`setup`), so the Rust side can rebuild it identically.
 *
 * Run (from the v4 checkout or a pin worktree):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/persistent-target.ts \
 *     > /tmp/oracle-persistent-target.ndjson
 */

process.env.DOCKER_CONTAINER = 'true';

import { promises as realFs } from 'fs';
import os from 'os';
import path from 'path';

/** Trimmed from a real Docker Desktop (macOS) container — v4's test fixture. */
const DOCKER_DESKTOP_MOUNTINFO = [
  '134 94 0:53 / / rw,relatime - overlay overlay rw,lowerdir=/x,upperdir=/y,workdir=/z',
  '136 134 0:62 / /proc rw,nosuid,nodev,noexec,relatime - proc proc rw',
  '137 134 0:63 / /dev rw,nosuid - tmpfs tmpfs rw,size=65536k,mode=755',
  '142 137 0:66 / /dev/shm rw,nosuid,nodev,noexec,relatime - tmpfs shm rw,size=65536k',
  '143 134 0:43 /csebold/Library/Mobile\\040Documents/Quilltap/V4test /app/quilltap rw,nosuid,nodev,relatime - virtiofs virtiofs0 rw',
  '150 134 0:43 /csebold/Vaults/My\\040Notes /Users/csebold/Vaults/My\\040Notes rw,relatime - virtiofs virtiofs0 rw',
  '144 134 254:1 /docker/containers/abc/hostname /etc/hostname rw,relatime - ext4 /dev/vda1 rw,discard',
  '160 134 0:70 / /scratch rw,relatime - tmpfs tmpfs rw',
  '170 134 254:1 /docker/srv/qtvol/_data /srv/qtvol rw,relatime shared:1 - ext4 /dev/vda1 rw',
  '',
].join('\n');

/** Edge table: ties, escapes, optional fields, malformed and odd lines. */
const EDGE_MOUNTINFO = [
  '1 0 0:1 / / rw - overlay overlay rw',
  '2 1 0:2 / /data rw shared:2 master:3 - ext4 /dev/sda1 rw',
  '3 1 0:3 / /data rw - tmpfs tmpfs rw',
  '4 1 0:4 / /data/ rw - cgroup2 cgroup2 rw',
  '5 1 0:5 / /odd\\011tab\\134slash rw - virtiofs v rw',
  '6 1 0:6 / /mq rw - mqueue mqueue rw',
  'garbage',
  '7 1 0:7 / /nofs rw -',
  '8 1 - / /dashfirst rw - ext4 x rw',
  '9 1',
  '   ',
  '10 1 0:9 / /sys rw - sysfs sysfs rw',
  '11 1 0:10 / /vol rw - xfs /dev/x rw',
].join('\n');

const TEXTS: Record<string, string> = {
  docker_desktop: DOCKER_DESKTOP_MOUNTINFO,
  edge: EDGE_MOUNTINFO,
  malformed: 'garbage\n\n1 2 3',
  empty: '',
};

const FIND_PATHS = [
  '/app/quilltap/stores/x', '/app/quilltap-other/x', '/app/quilltap', '/', '/data', '/data/',
  '/data/x', '/datax', '/odd\ttab\\slash/y', '/Users/csebold/Vaults/My Notes', '/srv/qtvol',
  '/proc/1', '/dev/shm/x', '',
];

const CHECK_TARGETS = [
  '/tmp/qt-deconvert-test', '/scratch/out', '/app/quilltap/exports/store',
  '/Users/csebold/Vaults/My Notes/out', '/srv/qtvol/store', '/data/out', '/mq/x', '/vol/a/b',
  'relative/target', '/sys/x', '/',
];

async function main(): Promise<void> {
  const { parseMountInfo, findContainingMount, checkPersistentTarget } = await import(
    '@/lib/mount-index/persistent-target'
  );
  const rows: unknown[] = [];
  for (const [name, text] of Object.entries(TEXTS)) {
    const mounts = parseMountInfo(text);
    rows.push({ kind: 'parse', text: name, mounts });
    for (const p of FIND_PATHS) {
      rows.push({ kind: 'find', text: name, path: p, mount: findContainingMount(mounts, p) });
    }
    for (const t of CHECK_TARGETS) {
      rows.push({
        kind: 'check',
        text: name,
        target: t,
        check: await checkPersistentTarget(t, () => Promise.resolve(text)),
      });
    }
  }
  rows.push({
    kind: 'check',
    text: 'unreadable',
    target: '/tmp/x',
    check: await checkPersistentTarget('/tmp/x', () => Promise.reject(new Error('EACCES'))),
  });

  // A link that lives "on the bind" but points into the overlay must be refused.
  const dir = path.join(os.tmpdir(), 'qt-persist-oracle-fixed');
  await realFs.rm(dir, { recursive: true, force: true });
  await realFs.mkdir(dir, { recursive: true });
  try {
    const realDir = await realFs.realpath(dir);
    const target = path.join(realDir, 'real');
    await realFs.mkdir(target);
    const link = path.join(realDir, 'link');
    await realFs.symlink(target, link);
    const mountinfo = ['1 0 0:1 / / rw - overlay overlay rw', `2 1 0:2 / ${link} rw - virtiofs virtiofs0 rw`].join('\n');
    rows.push({
      kind: 'check',
      text: 'symlink',
      setup: { dir: realDir, target, link, mountinfo },
      target: path.join(link, 'new-store'),
      check: await checkPersistentTarget(path.join(link, 'new-store'), () => Promise.resolve(mountinfo)),
    });
  } finally {
    await realFs.rm(dir, { recursive: true, force: true });
  }
  process.stdout.write(rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
}

main().catch((err) => {
  process.stderr.write(`persistent-target oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
