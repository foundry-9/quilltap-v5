import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ChatGalleryEntry } from '../core/core-contract';
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

async function flush(fixture: ComponentFixture<ChatGalleryImageViewModal>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/**
 * The chat-gallery detail view — a rewrite of v4
 * `components/chat/ChatGalleryImageViewModal.tsx` at `78b381a96` (P4.D176).
 * v4 has NO jest suite for this file; every case pins a transcribed behavior
 * by `file:line` (this suite is its only fidelity record).
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
    expect(fixture.nativeElement.querySelector("[title*=\"'s photo album\"]")).toBeNull();
    expect(
      fixture.nativeElement.querySelector('button[title="Save to a photo album"]'),
    ).toBeTruthy();
  });

  it('emits save() when the bookmark button is clicked — no network call of its own', async () => {
    const fixture = await render();
    const saves: number[] = [];
    fixture.componentInstance.save.subscribe(() => saves.push(1));
    (
      fixture.nativeElement.querySelector(
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
    const provenance = fixture.nativeElement.querySelector('.text-xs.opacity-80');
    expect(provenance.textContent).toContain(
      `Avatar, repainted during this conversation · Aria · painted ${day} · current`,
    );
  });

  it('drops a placeholder (<= 0 or non-finite) createdAt from the day clause', async () => {
    const fixture = await render({
      entry: entry({ source: 'portrait', createdAt: '1970-01-01T00:00:00Z' }),
    });
    const provenance = fixture.nativeElement.querySelector('.text-xs.opacity-80');
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
    expect(fixture.nativeElement.textContent).toContain(phrase);
  });

  // :241 — the link count, pluralized.
  it('shows the link-summary bracket only when count > 0, pluralized', async () => {
    const one = await render({ entry: entry({ linkSummary: { count: 1 } }) });
    expect(one.nativeElement.textContent).toContain('[1 link]');

    const many = await render({ entry: entry({ linkSummary: { count: 3 } }) });
    expect(many.nativeElement.textContent).toContain('[3 links]');

    const zero = await render({ entry: entry({ linkSummary: { count: 0 } }) });
    expect(zero.nativeElement.textContent).not.toContain('link');
  });

  // :242-256 — the Jump button renders only when the entry carries a messageId.
  it('renders Jump to message only when entry.messageId is present, and emits it', async () => {
    const withMsg = await render({ entry: entry({ messageId: 'm-9' }) });
    const jumps: string[] = [];
    withMsg.componentInstance.jumpToMessage.subscribe((id) => jumps.push(id));
    (
      withMsg.nativeElement.querySelector('button.qt-link.underline') as HTMLButtonElement
    ).click();
    expect(jumps).toEqual(['m-9']);

    const without = await render({ entry: entry({ messageId: undefined }) });
    expect(without.nativeElement.querySelector('button.qt-link.underline')).toBeNull();
  });

  // Delete double-guard: only rendered when `entry.deletable`.
  it('hides the delete button when the entry is not deletable', async () => {
    const fixture = await render({ entry: entry({ deletable: false }) });
    expect(
      fixture.nativeElement.querySelector('button[title="Delete image permanently"]'),
    ).toBeNull();
  });

  it('confirms before emitting deleteFile — the host performs the actual delete', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fixture = await render();
    const deletes: number[] = [];
    fixture.componentInstance.deleteFile.subscribe(() => deletes.push(1));
    (
      fixture.nativeElement.querySelector(
        'button[title="Delete image permanently"]',
      ) as HTMLButtonElement
    ).click();
    expect(confirm).toHaveBeenCalledWith('Permanently delete this photo? This cannot be undone.');
    expect(deletes.length).toBe(1);
  });

  it('does not emit deleteFile when the confirmation is declined', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    const fixture = await render();
    const deletes: number[] = [];
    fixture.componentInstance.deleteFile.subscribe(() => deletes.push(1));
    (
      fixture.nativeElement.querySelector(
        'button[title="Delete image permanently"]',
      ) as HTMLButtonElement
    ).click();
    expect(deletes.length).toBe(0);
  });

  // :208 — the overlay is z-[60]; :214-238 — the conditional arrows.
  it('layers at z-[60] and renders only the provided arrows', async () => {
    let prevs = 0;
    const fixture = await render({
      onPrev: () => {
        prevs += 1;
      },
    });
    expect(fixture.nativeElement.querySelector('.fixed.inset-0.z-\\[60\\]')).toBeTruthy();
    const prev = fixture.nativeElement.querySelector(
      'button[title="Previous image (Left Arrow)"]',
    ) as HTMLButtonElement;
    expect(prev).toBeTruthy();
    expect(fixture.nativeElement.querySelector('button[title="Next image (Right Arrow)"]')).toBeNull();
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

  // :141 / :194 — the toolbar and delete button hide while the image is missing.
  it('hides the toolbar and delete button for a missing image', async () => {
    const fixture = await render();
    (fixture.nativeElement.querySelector('img') as HTMLImageElement).dispatchEvent(
      new Event('error'),
    );
    await flush(fixture);
    expect(fixture.nativeElement.querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('button[title="Download"]')).toBeNull();
    expect(
      fixture.nativeElement.querySelector('button[title="Delete image permanently"]'),
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
      fixture.nativeElement.querySelector('button[title="Download"]') as HTMLButtonElement
    ).click();
    expect(spy).toHaveBeenCalledOnce();
    expect(clicked[0].href).toContain('/api/v1/files/file-1?download=1');
    expect(clicked[0].download).toBe('shot.png');
  });
});
