import { apiUrl } from '../../../core/api-url';

/**
 * The v5 port of v4 `lib/download-utils.ts` (browser arm): trigger a file
 * download by anchor-click. The Electron/native save arm is out of scope for
 * the SPA/Tauri webview (there, the browser fallback is the honest behaviour).
 *
 * @module screens/settings/system/download-utils
 */

/**
 * Download a server URL (v4 `triggerUrlDownload`) — used for the single-use
 * backup zip stream. The path resolves through {@link apiUrl} (the D14 raw-route
 * rule). No-op outside a DOM.
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
