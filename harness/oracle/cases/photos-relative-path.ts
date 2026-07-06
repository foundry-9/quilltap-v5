/**
 * Tier-1 oracle case — `isPhotosRelativePath` (the stale-chat maintenance sweep's
 * album-protection predicate, v4 `lib/photos/photos-paths.ts`).
 *
 * Drives v4's REAL `isPhotosRelativePath` over a fixed corpus of relative paths
 * and emits one NDJSON row per input. The Rust port
 * (`quilltap_core::db::doc_mount_file_links::is_photos_relative_path`) must return
 * the byte-identical boolean for every input (tier-1 exact equality). The corpus
 * exercises the `path.posix.dirname(...).toLowerCase()` === 'photos' /
 * startsWith('photos/') logic: a photos/ file, a nested photos/ file, a
 * mixed-case folder, non-photos siblings, a bare basename (dirname → '.'), a
 * `my-photos/` false-positive guard, a `photosx/` guard, and null/empty.
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/photos-relative-path.ts \
 *     > /tmp/oracle-photos-relative-path.ndjson
 */

import { isPhotosRelativePath } from '@/lib/photos/photos-paths';

const inputs: Array<{ id: string; path: string | null }> = [
  { id: 'photos-file', path: 'photos/a.webp' },
  { id: 'photos-mixed-case', path: 'Photos/a.webp' },
  { id: 'photos-upper', path: 'PHOTOS/a.webp' },
  { id: 'photos-nested', path: 'photos/sub/a.webp' },
  { id: 'photos-deep-nested', path: 'photos/2026/01/a.webp' },
  { id: 'images-sibling', path: 'images/a.webp' },
  { id: 'bare-basename', path: 'a.webp' },
  { id: 'my-photos-guard', path: 'my-photos/a.webp' },
  { id: 'photosx-guard', path: 'photosx/a.webp' },
  { id: 'root-photos', path: '/photos/a.webp' },
  { id: 'a-photos-nested', path: 'a/photos/x.webp' },
  { id: 'empty', path: '' },
  { id: 'null', path: null },
];

for (const { id, path } of inputs) {
  process.stdout.write(
    JSON.stringify({ id, path, out: isPhotosRelativePath(path) }) + '\n'
  );
}
