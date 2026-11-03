// v5-authored from v4 `lib/clipboard-utils.ts:111-133` (`9faed0c7e`, at 236c04e35):
// `copyPngBlobToClipboard`'s two-step failure order and its promised blob.
import { afterEach, describe, expect, it, vi } from 'vitest';

import { copyPngBlobToClipboard } from './clipboard-utils';

class FakeClipboardItem {
  constructor(readonly items: Record<string, unknown>) {}
}

function stubClipboard(write: (items: FakeClipboardItem[]) => Promise<void>): void {
  vi.stubGlobal('ClipboardItem', FakeClipboardItem);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, writable: true, value: { write } });
}

describe('copyPngBlobToClipboard', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('v5: hands ClipboardItem the PROMISE, not an awaited blob', async () => {
    const write = vi.fn(async () => undefined);
    stubClipboard(write);
    const png = Promise.resolve(new Blob(['png'], { type: 'image/png' }));
    await copyPngBlobToClipboard(png);
    const [[items]] = write.mock.calls as unknown as [[FakeClipboardItem[]]];
    expect(items[0].items['image/png']).toBeInstanceOf(Promise);
    expect(items[0].items['image/png']).toBe(png);
  });

  it("v5: a rejecting picture rejects with the picture's error, even when the clipboard also throws", async () => {
    stubClipboard(async () => {
      throw new Error('clipboard refused');
    });
    const png = Promise.reject(new Error('picture failed'));
    png.catch(() => undefined);
    await expect(copyPngBlobToClipboard(png)).rejects.toThrow('picture failed');
  });

  it("v5: a clipboard failure with a good picture rejects with the clipboard's error", async () => {
    stubClipboard(async () => {
      throw new Error('clipboard refused');
    });
    await expect(copyPngBlobToClipboard(Promise.resolve(new Blob(['png'])))).rejects.toThrow('clipboard refused');
  });
});
