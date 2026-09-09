/**
 * The v4-side recorder behind `src/app/core/download-utils.oracle.spec.ts`.
 *
 * `withDownloadFlag` (`lib/download-utils.ts:89-94`) is the one piece of v4's
 * gallery download machinery with real branching logic worth an equivalence
 * test: idempotence against BOTH `download=1` and `download=true`, and
 * preservation of an existing query string and/or URL fragment. The rest of
 * v4's `download-utils.ts` (`triggerDownload`/`triggerUrlDownload`/
 * `downloadFetchedFile`) is DOM-driving glue with no v5 counterpart worth a
 * byte-for-byte oracle (`core/download-utils.ts`'s module doc already records
 * the Electron-arm NO-COUNTERPART).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d176-78b381a96
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" 78b381a96
 * ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
 * cp <V5>/apps/web/oracle/download-utils.recorder.ts "$PIN/"
 * cd "$PIN" && npx tsx download-utils.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/download-utils.ndjson
 * ```
 *
 * Expect 14 lines; a shorter file means the recorder errored and the redirect
 * already truncated the old one (the empty-file trap).
 */

import { withDownloadFlag } from '@/lib/download-utils';

const URLS: string[] = [
  // A plain inline-served path, no query, no hash.
  '/api/v1/files/f-1',
  // An existing query string is PRESERVED, `&`-joined.
  '/api/v1/files/f-1?action=thumbnail&size=80',
  // A bare hash, no query — the flag lands BEFORE the hash.
  '/api/v1/files/f-1#frag',
  // Query AND hash together.
  '/api/v1/files/f-1?action=thumbnail#frag',
  // Idempotent against `download=1` already present, mid-query.
  '/api/v1/files/f-1?download=1&action=thumbnail',
  // Idempotent against `download=1` as the ONLY param.
  '/api/v1/files/f-1?download=1',
  // Idempotent against the `download=true` spelling.
  '/api/v1/files/f-1?download=true',
  // NOT idempotent when a hash trails the existing flag: the guard regex's
  // `(&|$)` alternation matches neither `#` nor end-of-string before it, so
  // the flag is appended a second time — a real v4 quirk, pinned as-is.
  '/api/v1/files/f-1?download=1#frag',
  // `download=1` appearing INSIDE a `&`-joined pair is still idempotent (the
  // `(&|$)` alternation matches the trailing `&`).
  '/api/v1/files/f-1?download=1&foo=bar',
  // A value that merely CONTAINS the substring "download=1" without it
  // starting right after `?`/`&` does NOT count — the regex needs a boundary.
  '/api/v1/files/f-1?note=download=1',
  // An invalid `download=` value (neither `1` nor `true`) is NOT idempotent —
  // the flag is appended a second time, verbatim, byte for byte.
  '/api/v1/files/f-1?download=2',
  // A trailing bare `?` still counts as "has a query" for the separator
  // choice — the literal `?&download=1` v4 produces is pinned as-is.
  '/api/v1/files/f-1?',
  // A mount-point blob route with a URL-encoded path segment.
  '/api/v1/mount-points/mp-1/blobs/photos%2Fyacht.png',
  // The proxy-key route, which already carries its own query.
  '/api/v1/files/proxy/abc123?ext=.png',
];

for (const url of URLS) {
  console.log(JSON.stringify({ id: url, out: withDownloadFlag(url) }));
}
