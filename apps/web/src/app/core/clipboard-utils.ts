/**
 * The v5 port of v4 `lib/clipboard-utils.ts` (browser arm) — copy an image to
 * the clipboard from a fetch-able URL.
 *
 * @module core/clipboard-utils
 *
 * v4 always prefers the standard Clipboard API (`navigator.clipboard.write()`)
 * for image copies: it works in modern browsers and Electron ≥ 25, and — the
 * reason it comes first rather than second — it ensures the copied image is
 * paste-able back into the SAME renderer process (fullscreen viewer → the chat
 * composer). The Clipboard API only accepts `image/png` for `ClipboardItem`
 * writes, so anything else (a WebP-transcoded upload, say) is converted through
 * an offscreen canvas first.
 *
 * **Recorded non-goal, same class as `core/download-utils`:** v4 falls back to
 * Electron's native `clipboard.writeImage()` over `window.quilltap` IPC when the
 * browser API throws. The SPA/Tauri webview has no such bridge, so that arm has
 * no v5 counterpart and the browser failure is the honest terminal state — v4's
 * own last line (`No clipboard write method available`) is what a v4 browser
 * build reaches too.
 */

/** v4 `blobToDataUrl` — used only for the canvas load below (see `convertToPngBlob`). */
function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onloadend = () => resolve(reader.result as string);
    reader.onerror = reject;
    reader.readAsDataURL(blob);
  });
}

/**
 * v4 `convertToPngBlob` — an image Blob through an offscreen canvas to PNG.
 *
 * The image is loaded from a **data URL, not a `blob:` URL**: the CSP `img-src`
 * directive allows `data:` and does not allow `blob:`.
 */
async function convertToPngBlob(blob: Blob): Promise<Blob> {
  const dataUrl = await blobToDataUrl(blob);
  return new Promise<Blob>((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const canvas = document.createElement('canvas');
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;
      const ctx = canvas.getContext('2d');
      if (!ctx) {
        reject(new Error('Failed to get canvas 2d context'));
        return;
      }
      ctx.drawImage(img, 0, 0);
      canvas.toBlob((pngBlob) => {
        if (pngBlob) {
          resolve(pngBlob);
        } else {
          reject(new Error('Canvas toBlob returned null'));
        }
      }, 'image/png');
    };
    img.onerror = () => {
      reject(new Error('Failed to load image for PNG conversion'));
    };
    img.src = dataUrl;
  });
}

/**
 * v4 `copyImageToClipboard(src)` — fetch the bytes, write them to the clipboard
 * as `image/png`. Resolves `true` on success; **throws** when no clipboard write
 * method is available (v4's terminal arm once its Electron bridge is absent).
 */
export async function copyImageToClipboard(src: string): Promise<boolean> {
  const response = await fetch(src);
  const blob = await response.blob();

  try {
    const pngBlob = blob.type === 'image/png' ? blob : await convertToPngBlob(blob);
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': pngBlob })]);
    return true;
  } catch {
    // v4 falls through to its Electron IPC path here; v5 has none.
  }

  throw new Error('No clipboard write method available');
}
