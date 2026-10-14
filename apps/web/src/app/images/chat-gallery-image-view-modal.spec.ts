import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ChatGalleryEntry } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { ChatGalleryImageViewModal } from './chat-gallery-image-view-modal';

function entry(over: Partial<ChatGalleryEntry> = {}): ChatGalleryEntry {
  return {
    id: 'file-1',
    idKind: 'file',
    url: '/api/v1/files/file-1',
    filename: 'shot.png',
    mimeType: 'image/png',
    size: 1024,
    createdAt: '2026-03-05T00:00:00Z',
    source: 'attachment',
    isCurrent: false,
    deletable: true,
    ...over,
  };
}

async function render(
  inputs: Record<string, unknown> = {},
): Promise<ComponentFixture<ChatGalleryImageViewModal>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ChatGalleryImageViewModal] });
  const fixture = TestBed.createComponent(ChatGalleryImageViewModal);
  fixture.componentRef.setInput('entry', entry());
  for (const [key, value] of Object.entries(inputs)) {
    fixture.componentRef.setInput(key, value);
  }
  fixture.detectChanges();
  await flush(fixture);
  return fixture;
}

/**
 * The frame portals to `document.body` (P4.D277 R-B), so the viewer's DOM
 * leaves the fixture — every query reads the body. `render` resets the
 * module first, which destroys the previous fixture and removes its frame.
 */
const body = (): HTMLElement => document.body;

async function flush(fixture: ComponentFixture<ChatGalleryImageViewModal>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/**
 * The chat-gallery detail view — a rewrite of v4
 * `components/chat/ChatGalleryImageViewModal.tsx` at `78b381a96` (P4.D176),
 * moved onto the shared `FullScreenImageViewer` frame at `01a83539d`
 * (P4.D277). v4 has NO jest suite for this file; every case pins a
 * transcribed behavior by `file:line` (this suite is its only fidelity
 * record).
 */
describe('ChatGalleryImageViewModal', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  // The two hard-wired album buttons v4 DELETED (module doc) — a v5-only
  // divergence this rewrite retires along with v4.
  it('carries NO character-album toggle buttons — Save opens the shared dialog instead', async () => {
    const fixture = await render();
    // v4's deleted buttons read "Save to <name>'s photo album" — distinct from
    // the ONE shared-dialog trigger's fixed "Save to a photo album".
    expect(body().querySelector("[title*=\"'s photo album\"]")).toBeNull();
    expect(
      body().querySelector('button[title="Save to a photo album"]'),
    ).toBeTruthy();
  });

  it('emits save() when the bookmark button is clicked — no network call of its own', async () => {
    const fixture = await render();
    const saves: number[] = [];
    fixture.componentInstance.save.subscribe(() => saves.push(1));
    (
      body().querySelector(
        'button[title="Save to a photo album"]',
      ) as HTMLButtonElement
    ).click();
    expect(saves.length).toBe(1);
  });

  // :94-102 — the provenance line's four clauses. The day itself is computed
  // the SAME way `formatDay` does (`toLocaleDateString(undefined, {day, month}`)
  // rather than hardcoded, since the runtime's default locale AND timezone both
  // shape it (v4's own behavior — this is not a divergence to paper over).
  it('joins the provenance line: source phrase, character, painted day, current', async () => {
    // Noon UTC avoids a timezone rollover shifting the calendar day.
    const createdAt = '2026-03-05T12:00:00Z';
    const day = new Date(createdAt).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
    const fixture = await render({
      entry: entry({
        source: 'avatar',
        characterId: 'c-1',
        characterName: 'Aria',
        createdAt,
        isCurrent: true,
      }),
    });
    const provenance = body().querySelector('.text-xs.opacity-80')!;
    expect(provenance.textContent).toContain(
      `Avatar, repainted during this conversation · Aria · painted ${day} · current`,
    );
  });

  it('drops a placeholder (<= 0 or non-finite) createdAt from the day clause', async () => {
    const fixture = await render({
      entry: entry({ source: 'portrait', createdAt: '1970-01-01T00:00:00Z' }),
    });
    const provenance = body().querySelector('.text-xs.opacity-80')!;
    expect(provenance.textContent).toContain('Standing portrait');
    expect(provenance.textContent).not.toContain('painted');
  });

  it.each([
    ['story-background', 'Story background'],
    ['avatar', 'Avatar, repainted during this conversation'],
    ['portrait', 'Standing portrait'],
    ['generated', 'Generated in this conversation'],
    ['attachment', 'Attached beneath a message'],
    ['kept', 'Brought out of a photo album'],
    ['inline', 'Woven into the prose'],
  ] as const)('SOURCE_PHRASE[%s] = %s', async (source, phrase) => {
    const fixture = await render({ entry: entry({ source }) });
    expect(body().textContent).toContain(phrase);
  });

  // :241 — the link count, pluralized.
  it('shows the link-summary bracket only when count > 0, pluralized', async () => {
    const one = await render({ entry: entry({ linkSummary: { count: 1 } }) });
    expect(body().textContent).toContain('[1 link]');

    const many = await render({ entry: entry({ linkSummary: { count: 3 } }) });
    expect(body().textContent).toContain('[3 links]');

    const zero = await render({ entry: entry({ linkSummary: { count: 0 } }) });
    expect(body().textContent).not.toContain('link');
  });

  // :242-256 — the Jump button renders only when the entry carries a messageId.
  it('renders Jump to message only when entry.messageId is present, and emits it', async () => {
    const withMsg = await render({ entry: entry({ messageId: 'm-9' }) });
    const jumps: string[] = [];
    withMsg.componentInstance.jumpToMessage.subscribe((id) => jumps.push(id));
    (
      body().querySelector('button.qt-link.underline') as HTMLButtonElement
    ).click();
    expect(jumps).toEqual(['m-9']);

    const without = await render({ entry: entry({ messageId: undefined }) });
    expect(body().querySelector('button.qt-link.underline')).toBeNull();
  });

  // Delete double-guard: only rendered when `entry.deletable`.
  it('hides the delete button when the entry is not deletable', async () => {
    const fixture = await render({ entry: entry({ deletable: false }) });
    expect(
      body().querySelector('button[title="Delete image permanently"]'),
    ).toBeNull();
  });

  // v4 `:195-197` — the bin calls `onDelete()` with NO confirmation; the ONE
  // `showConfirmation` is the host's (`PhotoGalleryModal.tsx:258`). The §3
  // unification review of the `78b381a96` round found this modal confirming
  // too — two identical dialogs for one delete. `window.confirm` is NOT
  // stubbed here on purpose: jsdom's answers `undefined` (falsy), so a
  // re-introduced confirm would swallow the emit and redden this case.
  it('emits deleteFile WITHOUT confirming — the host confirms and performs the delete', async () => {
    const confirm = vi.spyOn(window, 'confirm');
    const fixture = await render();
    const deletes: number[] = [];
    fixture.componentInstance.deleteFile.subscribe(() => deletes.push(1));
    (
      body().querySelector(
        'button[title="Delete image permanently"]',
      ) as HTMLButtonElement
    ).click();
    expect(confirm).not.toHaveBeenCalled();
    expect(deletes.length).toBe(1);
  });

  // :208 — the overlay is z-[60]; :214-238 — the conditional arrows.
  it('layers at z-[60] and renders only the provided arrows', async () => {
    let prevs = 0;
    const fixture = await render({
      onPrev: () => {
        prevs += 1;
      },
    });
    expect(body().querySelector('.fixed.inset-0.z-\\[60\\]')).toBeTruthy();
    const prev = body().querySelector(
      'button[title="Previous image (Left Arrow)"]',
    ) as HTMLButtonElement;
    expect(prev).toBeTruthy();
    expect(body().querySelector('button[title="Next image (Right Arrow)"]')).toBeNull();
    prev.click();
    expect(prevs).toBe(1);
  });

  // Escape closes through the navigation helper.
  it('closes on Escape', async () => {
    const fixture = await render();
    const closes: number[] = [];
    fixture.componentInstance.closeModal.subscribe(() => closes.push(1));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(closes.length).toBe(1);
  });

  // 01a83539d (FullScreenImageViewer.tsx:117-136): a missing image hides the
  // actions and the bottom-right delete — but Close now STAYS (the old viewer
  // hid the whole cluster, Close included). Re-pinned by P4.D277.
  it('hides the actions and delete button for a missing image, keeping Close', async () => {
    const fixture = await render();
    (body().querySelector('img') as HTMLImageElement).dispatchEvent(
      new Event('error'),
    );
    await flush(fixture);
    expect(body().querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    expect(body().querySelector('button[title="Download"]')).toBeNull();
    expect(body().querySelector('button[title="Save to a photo album"]')).toBeNull();
    expect(
      body().querySelector('button[title="Delete image permanently"]'),
    ).toBeNull();
    expect(body().querySelector('button[title="Close (Escape)"]')).toBeTruthy();
  });

  // 01a83539d — the frame names the dialog by `alt`, which this view passes as
  // the entry's filename (ChatGalleryImageViewModal.tsx hunk; frame :104).
  it("names the dialog by the entry's filename", async () => {
    await render({ entry: entry({ filename: 'yacht.webp' }) });
    expect(body().querySelector('[role="dialog"]')!.getAttribute('aria-label')).toBe('yacht.webp');
  });

  // 01a83539d (frame :79-81) — the miss is keyed by `src`, so walking to the
  // next entry clears a previous miss (the old viewer's flag never reset).
  it('clears a miss on entry A once the host switches to entry B', async () => {
    const fixture = await render();
    (body().querySelector('img') as HTMLImageElement).dispatchEvent(new Event('error'));
    await flush(fixture);
    expect(body().querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    fixture.componentRef.setInput(
      'entry',
      entry({ id: 'file-2', url: '/api/v1/files/file-2', filename: 'next.png' }),
    );
    await flush(fixture);
    expect(body().querySelector('qt-deleted-image-placeholder')).toBeNull();
    expect(body().querySelector('img')!.getAttribute('src')).toBe('/api/v1/files/file-2');
    expect(body().querySelector('button[title="Download"]')).toBeTruthy();
  });

  // 01a83539d (frame :98-166, bug 99) — the overlay is portalled to the body,
  // leaving this view's own host empty (R-B).
  it('portals the frame to document.body', async () => {
    const fixture = await render();
    const frame = body().querySelector('qt-full-screen-image-viewer');
    expect(frame?.parentElement).toBe(document.body);
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('[role="dialog"]'),
    ).toBeNull();
  });

  // v4 :83-90 — the download goes through `downloadGalleryEntry` (the URL, an
  // attachment-disposition anchor click), NOT a fetch-to-blob dance.
  it('downloads via an anchor carrying ?download=1, not a fetch-to-blob', async () => {
    const fixture = await render({ entry: entry({ url: '/api/v1/files/file-1', filename: 'shot.png' }) });
    const clicked: Array<{ href: string; download: string }> = [];
    const spy = vi
      .spyOn(HTMLAnchorElement.prototype, 'click')
      .mockImplementation(function (this: HTMLAnchorElement) {
        clicked.push({ href: this.href, download: this.download });
      });
    (
      body().querySelector('button[title="Download"]') as HTMLButtonElement
    ).click();
    expect(spy).toHaveBeenCalledOnce();
    expect(clicked[0].href).toContain('/api/v1/files/file-1?download=1');
    expect(clicked[0].download).toBe('shot.png');
  });

  // v4 :67-75 — a Copy that throws (no clipboard write method) toasts v4's
  // failure sentence and logs v4's line; a bare boolean branch left the
  // rejection unhandled and the user untold (the `01a83539d` unification).
  it("a failed Copy toasts v4's failure sentence and logs its line (:67-75)", async () => {
    const fixture = await render();
    vi.stubGlobal('fetch', vi.fn(async () => ({ blob: async () => new Blob(['x'], { type: 'image/png' }) })));
    vi.stubGlobal('ClipboardItem', class {
      constructor(readonly items: unknown) {}
    });
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      // P4.D301 OUT-OF-MANDATE: `writable` — without it a later spec in the same
      // vitest worker that `Object.assign`s the clipboard (courier-bubble) throws.
      writable: true,
      // `writable` too: without it a later spec file in the same vitest worker
      // that ASSIGNS `navigator.clipboard` (`courier-bubble.spec.ts`) fails
      // 8/8 with "Cannot assign to read only property" (memory
      // `spa-component-harness-gotchas`; surfaced by P4.D302's new spec files
      // moving the worker allocation).
      writable: true,
      value: { write: vi.fn(async () => Promise.reject(new Error('denied'))) },
    });
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
    (body().querySelector('button[title="Copy to clipboard"]') as HTMLButtonElement).click();
    await flush(fixture);
    const toasts = TestBed.inject(ToastService).toasts().map((t) => t.message);
    expect(toasts).toContain('Failed to copy image to clipboard');
    expect(errors).toHaveBeenCalledWith('Failed to copy image to clipboard:', {
      error: 'No clipboard write method available',
    });
  });
});
