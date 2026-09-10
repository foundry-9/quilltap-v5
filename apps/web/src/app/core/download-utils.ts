import { apiUrl } from './api-url';

/**
 * The v5 port of v4 `lib/download-utils.ts` (browser arm): trigger a file
 * download by anchor-click. The Electron/native save arm is out of scope for
 * the SPA/Tauri webview (there, the browser fallback is the honest behaviour).
 *
 * @module core/download-utils
 *
 * Re-homed here by P4.d28 when the Organize drawer's Export Markdown entry
 * became its third consumer from a different feature — v4 keeps it in `lib/`
 * for the same reason.
 */

/**
 * Download a server URL (v4 `triggerUrlDownload`) — the single-use backup zip
 * stream and the Organize drawer's Markdown transcript. The path resolves
 * through {@link apiUrl} (the D14 raw-route rule). No-op outside a DOM.
 */
export function triggerUrlDownload(path: string, filename: string): void {
  if (typeof document === 'undefined') return;
  const anchor = document.createElement('a');
  anchor.href = apiUrl(path);
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
}

/**
 * Download a Blob (v4 `triggerDownload`) — used for the streamed `.qtap` export.
 * No-op outside a DOM / where object URLs are unavailable.
 */
export function triggerBlobDownload(blob: Blob, filename: string): void {
  if (typeof document === 'undefined' || typeof URL === 'undefined' || !URL.createObjectURL) {
    return;
  }
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}

// ===========================================================================
// P4.D176: the Salon chat gallery — v4 `lib/download-utils.ts`'s
// `withDownloadFlag`/`downloadImageUrl`/`downloadGalleryEntry` trio.
//
// v4 buys `withDownloadFlag` + `triggerUrlDownload` for Electron's
// `will-download` streaming; that mechanism is v5's DEFAULT already (see
// {@link triggerUrlDownload} above) — what this port actually gains is the
// server-side `Content-Disposition: attachment` header (P4.D174's `?download=1`
// arm) and one helper three hand-rolled anchors converge on, rather than each
// building its own `<a download>` element and fetch-to-blob dance.
// ===========================================================================

/**
 * Append `?download=1` to a URL the app serves inline, so the response comes
 * back as an `attachment` (v4 `lib/download-utils.ts:89-94`, transcribed
 * byte-for-byte — including its one real quirk: the idempotence guard tests
 * the WHOLE url, hash included, so `?download=1#frag` is NOT recognised as
 * already flagged and gets the flag appended a second time. Pinned by the
 * `download-utils.ndjson` oracle recorded from v4's real module.
 *
 * Preserves whatever query the URL already carries, and is a no-op if the flag
 * is already there in the `?`/`&`-anchored `download=1` or `download=true`
 * spelling.
 */
export function withDownloadFlag(url: string): string {
  if (/[?&]download=(1|true)(&|$)/.test(url)) return url;
  const [withoutHash, hash] = splitHash(url);
  const separator = withoutHash.includes('?') ? '&' : '?';
  return `${withoutHash}${separator}download=1${hash}`;
}

function splitHash(url: string): [string, string] {
  const index = url.indexOf('#');
  return index === -1 ? [url, ''] : [url.slice(0, index), url.slice(index)];
}

/**
 * Download an image the app is serving inline (v4 `downloadImageUrl`). Asks
 * the route for an `attachment` disposition via {@link withDownloadFlag} and
 * anchor-clicks the result — the server's own `Content-Disposition` names the
 * file.
 *
 * `url` must already be the FINAL resolved URL (v4 has no separate host to
 * resolve — the browser just uses the server path directly): every caller
 * resolves through {@link apiUrl} up front, exactly as `image-urls.ts`'s
 * `fileUrl`/`thumbnailUrl` do, so this function does not resolve a second
 * time (calling {@link apiUrl} twice would double-prefix the Tauri
 * cross-origin dev loop).
 */
export function downloadImageUrl(url: string, filename: string): void {
  if (typeof document === 'undefined') return;
  const anchor = document.createElement('a');
  anchor.href = withDownloadFlag(url);
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
}

/**
 * {@link downloadImageUrl} for a chat-gallery entry, which already carries the
 * URL its bytes are served from and the name they should be saved under (v4
 * `downloadGalleryEntry`).
 */
export function downloadGalleryEntry(entry: { url: string; filename: string }): void {
  downloadImageUrl(entry.url, entry.filename);
}
