/**
 * Tier-1 oracle case — `isPhotosRelativePath` (the stale-chat maintenance sweep's
 * album-protection predicate, v4 `lib/photos/photos-paths.ts`).
 *
 * Drives v4's REAL `isPhotosRelativePath` over a fixed corpus of relative paths
 * and emits one NDJSON row per input. The Rust port
 * (`quilltap_core::photos::photos_paths::is_photos_relative_path`) must return
 * the byte-identical boolean for every input (tier-1 exact equality). The corpus
 * exercises the `path.posix.dirname(...).toLowerCase()` === 'photos' /
 * startsWith('photos/') logic: a photos/ file, a nested photos/ file, a
 * mixed-case folder, non-photos siblings, a bare basename (dirname → '.'), a
 * `my-photos/` false-positive guard, a `photosx/` guard, and null/empty.
 *
 * P4.91 grew it with the SLASH shapes, which is where the two v5 homes disagreed
 * and no row could see it. `path.posix.dirname` skips a whole RUN of trailing
 * slashes before looking for the last separator, so `'photos//'` is `'.'` (NOT
 * `'photos'`) and the predicate is false; a SINGLE trailing slash
 * (`'photos/a.webp/'`) is the control both algorithms already agreed on. The
 * interior-run row (`'photos//a.webp'` → dirname `'photos/'`) is the one shape
 * where the run rule makes the predicate TRUE through `startsWith('photos/')`,
 * and the root shapes pin Node's `hasRoot` arms. Measured on Node 24.13.1.
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

  // --- P4.91: the slash shapes -------------------------------------------
  // The control: ONE trailing slash. Node strips it and both v5 homes always
  // agreed here — a row that reds under M2 and nothing else.
  { id: 'single-trailing-slash', path: 'photos/a.webp/' },
  // The RUN shapes. Node skips the whole run, finds no separator left, and
  // answers '.' — false. A one-slash strip answers 'photos'/'photos/' — true.
  { id: 'photos-double-slash', path: 'photos//' },
  { id: 'photos-triple-slash', path: 'photos///' },
  { id: 'photos-double-slash-upper', path: 'PHOTOS//' },
  { id: 'bare-double-slash', path: 'a//' },
  // A run after a real separator: the run is skipped, the separator before it
  // is the answer — both algorithms land on a folder, but not the same one.
  { id: 'photos-sub-double-slash', path: 'photos/sub//' },
  { id: 'root-photos-double-slash', path: '/photos//' },
  // The one shape the run rule makes TRUE: an INTERIOR run leaves a trailing
  // '/' on the dirname, so `startsWith('photos/')` fires.
  { id: 'photos-interior-double-slash', path: 'photos//a.webp' },
  // Folder-only and degenerate inputs.
  { id: 'photos-bare', path: 'photos' },
  { id: 'photos-bare-trailing-slash', path: 'photos/' },
  { id: 'slash-only', path: '/' },
  { id: 'double-slash-only', path: '//' },
  { id: 'dot', path: '.' },
  { id: 'dot-relative-photos', path: './photos/a.webp' },
  // Node's `hasRoot` arms: dirname('//photos') is '//' (not '/'), and
  // dirname('///photos') is '//' as well.
  { id: 'double-root-photos-file', path: '//photos/a.webp' },
  { id: 'double-root-photos', path: '//photos' },
  { id: 'triple-root-photos', path: '///photos' },
];

for (const { id, path } of inputs) {
  process.stdout.write(
    JSON.stringify({ id, path, out: isPhotosRelativePath(path) }) + '\n'
  );
}
