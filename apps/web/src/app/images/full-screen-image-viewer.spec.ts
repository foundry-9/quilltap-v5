import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  FullScreenImageAction,
  FullScreenImageBottomRight,
  FullScreenImageCaption,
  FullScreenImageViewer,
} from './full-screen-image-viewer';

/**
 * P4.D277 — v4 `components/images/FullScreenImageViewer.tsx` (NEW in
 * `01a83539d`, 169 lines, read at that pin). v4 has no jest suite for the
 * frame itself; every case pins a transcribed behaviour by `file:line`.
 * The frame portals to `document.body` (R-B), so every query reads the body.
 */
@Component({
  imports: [FullScreenImageViewer, FullScreenImageCaption, FullScreenImageBottomRight],
  template: `
    @if (open()) {
      <qt-full-screen-image-viewer
        [src]="src()"
        alt="Picture of a gown"
        imageId="file-1"
        filename="gown.webp"
        [actions]="actions()"
        [onPrev]="onPrev()"
        [onNext]="onNext()"
        [keyboardActive]="keyboardActive()"
        [zIndexClass]="zIndexClass()"
        (closeModal)="closes.set(closes() + 1)"
      >
        @if (withCaption()) {
          <ng-container qtFullScreenCaption><div class="cap">The caption</div></ng-container>
        }
        @if (withBottomRight()) {
          <button qtFullScreenBottomRight type="button" class="bin" (click)="bins.set(bins() + 1)">
            Bin
          </button>
        }
      </qt-full-screen-image-viewer>
    }
  `,
})
class Host {
  readonly open = signal(true);
  readonly src = signal('/api/v1/files/file-1');
  readonly actions = signal<FullScreenImageAction[]>([]);
  readonly onPrev = signal<(() => void) | undefined>(undefined);
  readonly onNext = signal<(() => void) | undefined>(undefined);
  readonly keyboardActive = signal(true);
  readonly zIndexClass = signal('z-[60]');
  readonly withCaption = signal(false);
  readonly withBottomRight = signal(false);
  readonly closes = signal(0);
  readonly bins = signal(0);
}

async function flush(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(setup: (host: Host) => void = () => {}): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  await flush(fixture);
  return fixture;
}

const body = (): HTMLElement => document.body;
const dialog = (): HTMLElement | null => body().querySelector('qt-full-screen-image-viewer [role="dialog"]');
const byTitle = (title: string): HTMLButtonElement | null =>
  body().querySelector(`qt-full-screen-image-viewer button[title="${title}"]`);
const key = (k: string): void => {
  document.dispatchEvent(new KeyboardEvent('keydown', { key: k }));
};

async function miss(fixture: ComponentFixture<Host>): Promise<void> {
  body().querySelector('qt-full-screen-image-viewer img')!.dispatchEvent(new Event('error'));
  await flush(fixture);
}

describe('FullScreenImageViewer (v4 FullScreenImageViewer.tsx at 01a83539d)', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    document.body.style.overflow = '';
  });

  it('portals its host to document.body after render and removes it on destroy (:98-166, R-B)', async () => {
    const fixture = await render();
    const host = body().querySelector('qt-full-screen-image-viewer')!;
    expect(host.parentElement).toBe(document.body);
    expect((fixture.nativeElement as HTMLElement).querySelector('qt-full-screen-image-viewer')).toBeNull();
    fixture.componentInstance.open.set(false);
    await flush(fixture);
    expect(body().querySelector('qt-full-screen-image-viewer')).toBeNull();
  });

  it('names the dialog by alt (:99-105 aria-label={alt})', async () => {
    await render();
    const d = dialog()!;
    expect(d.getAttribute('aria-modal')).toBe('true');
    expect(d.getAttribute('aria-label')).toBe('Picture of a gown');
  });

  it('layers at z-[60] by default and at a passed z-[90] (:76, :100)', async () => {
    await render();
    expect(dialog()!.className).toBe(
      'fixed inset-0 z-[60] flex items-center justify-center qt-bg-overlay backdrop-blur-sm',
    );
    await render((h) => h.zIndexClass.set('z-[90]'));
    expect(dialog()!.className).toBe(
      'fixed inset-0 z-[90] flex items-center justify-center qt-bg-overlay backdrop-blur-sm',
    );
  });

  it('renders the actions in order, named by label, firing without closing (:117-131)', async () => {
    const fired: string[] = [];
    const fixture = await render((h) =>
      h.actions.set([
        { icon: 'bookmark', label: 'Save it', onClick: () => fired.push('save') },
        { icon: 'download', label: 'Download', onClick: () => fired.push('download') },
      ]),
    );
    const buttons = [...body().querySelectorAll<HTMLButtonElement>('qt-full-screen-image-viewer .top-4 button')];
    expect(buttons.map((b) => [b.title, b.getAttribute('aria-label')])).toEqual([
      ['Save it', 'Save it'],
      ['Download', 'Download'],
      ['Close (Escape)', 'Close'],
    ]);
    expect(buttons[0].className).toBe(
      'p-2 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors cursor-pointer disabled:opacity-50',
    );
    buttons[0].click();
    buttons[1].click();
    expect(fired).toEqual(['save', 'download']);
    expect(fixture.componentInstance.closes()).toBe(0);
  });

  it('disables a disabled action (:125)', async () => {
    await render((h) =>
      h.actions.set([{ icon: 'copy', label: 'Copy to clipboard', onClick: () => {}, disabled: true }]),
    );
    expect(byTitle('Copy to clipboard')!.disabled).toBe(true);
    expect(byTitle('Close (Escape)')!.disabled).toBe(false);
  });

  it('keeps Close with and without a miss; a miss hides the actions and bottom-right and shows the placeholder (:118, :132, :136, :143-145)', async () => {
    const fixture = await render((h) => {
      h.actions.set([{ icon: 'download', label: 'Download', onClick: () => {} }]);
      h.withBottomRight.set(true);
    });
    expect(byTitle('Close (Escape)')).toBeTruthy();
    expect(body().querySelector('qt-full-screen-image-viewer .bottom-4.right-4 button.bin')).toBeTruthy();
    await miss(fixture);
    expect(body().querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    expect(byTitle('Download')).toBeNull();
    expect(body().querySelector('qt-full-screen-image-viewer .bottom-4.right-4')).toBeNull();
    expect(byTitle('Close (Escape)')).toBeTruthy();
  });

  it('clears the miss when src changes — keyed by src (:79-81)', async () => {
    const fixture = await render();
    await miss(fixture);
    expect(body().querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    fixture.componentInstance.src.set('/api/v1/files/file-2');
    await flush(fixture);
    expect(body().querySelector('qt-deleted-image-placeholder')).toBeNull();
    expect(body().querySelector('qt-full-screen-image-viewer img')!.getAttribute('src')).toBe(
      '/api/v1/files/file-2',
    );
  });

  it('renders only the given arrows, with v4 titles and classes (:106-115)', async () => {
    let prevs = 0;
    await render((h) => h.onPrev.set(() => (prevs += 1)));
    const prev = byTitle('Previous image (Left Arrow)')!;
    expect(prev.className).toBe(
      'absolute top-1/2 -translate-y-1/2 p-3 qt-bg-overlay-btn hover:qt-bg-overlay-btn rounded-full qt-text-overlay transition-colors z-10 cursor-pointer left-4',
    );
    expect(byTitle('Next image (Right Arrow)')).toBeNull();
    prev.click();
    expect(prevs).toBe(1);
  });

  it('handles ArrowLeft / ArrowRight / Escape while keyboardActive (:83-89)', async () => {
    const seen: string[] = [];
    const fixture = await render((h) => {
      h.onPrev.set(() => seen.push('prev'));
      h.onNext.set(() => seen.push('next'));
    });
    key('ArrowLeft');
    key('ArrowRight');
    key('Escape');
    expect(seen).toEqual(['prev', 'next']);
    expect(fixture.componentInstance.closes()).toBe(1);
  });

  it('with keyboardActive false, keys do nothing but the arrow BUTTONS still work (:83-89)', async () => {
    const seen: string[] = [];
    const fixture = await render((h) => {
      h.onPrev.set(() => seen.push('prev'));
      h.onNext.set(() => seen.push('next'));
      h.keyboardActive.set(false);
    });
    key('ArrowLeft');
    key('ArrowRight');
    key('Escape');
    expect(seen).toEqual([]);
    expect(fixture.componentInstance.closes()).toBe(0);
    byTitle('Next image (Right Arrow)')!.click();
    expect(seen).toEqual(['next']);
    // Turning the keyboard back on restores the keys.
    fixture.componentInstance.keyboardActive.set(true);
    await flush(fixture);
    key('Escape');
    expect(fixture.componentInstance.closes()).toBe(1);
  });

  it('renders the caption box only with projected content (:156-163)', async () => {
    await render();
    expect(body().querySelector('qt-full-screen-image-viewer .qt-bg-overlay-caption')).toBeNull();
    await render((h) => h.withCaption.set(true));
    const box = body().querySelector('qt-full-screen-image-viewer .qt-bg-overlay-caption')!;
    expect(box.className).toBe(
      'absolute bottom-4 left-1/2 -translate-x-1/2 qt-text-overlay-muted text-sm qt-bg-overlay-caption px-3 py-1 rounded text-center',
    );
    expect(box.querySelector('.cap')!.textContent).toBe('The caption');
  });

  it('a backdrop click closes; the image box, caption and bottom-right do not (:103, :140, :159)', async () => {
    const fixture = await render((h) => {
      h.withCaption.set(true);
      h.withBottomRight.set(true);
    });
    body().querySelector<HTMLElement>('qt-full-screen-image-viewer .max-w-\\[90vw\\]')!.click();
    body().querySelector<HTMLElement>('qt-full-screen-image-viewer .qt-bg-overlay-caption')!.click();
    body().querySelector<HTMLButtonElement>('qt-full-screen-image-viewer button.bin')!.click();
    expect(fixture.componentInstance.closes()).toBe(0);
    expect(fixture.componentInstance.bins()).toBe(1);
    dialog()!.click();
    expect(fixture.componentInstance.closes()).toBe(1);
  });

  it('locks body scroll while open and restores it on destroy (useImageNavigation.ts:70-79)', async () => {
    const fixture = await render();
    expect(document.body.style.overflow).toBe('hidden');
    fixture.componentInstance.open.set(false);
    await flush(fixture);
    expect(document.body.style.overflow).toBe('');
  });
});

/**
 * P4.D286 — bug 194's second v4 case (`wardrobe-image-viewer.missing-cleanup.
 * test.tsx:76` at the pin `4a38222e6`): with no `missingCleanup` override the
 * frame's placeholder still uses the generic image delete, and closes after
 * it; with one, the override runs INSTEAD (v4 `FullScreenImageViewer.tsx:151-158`).
 */
describe('FullScreenImageViewer — the missing-picture cleanup (bug 194)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
    document.body.style.overflow = '';
  });

  const removeButton = (): HTMLButtonElement =>
    [...body().querySelectorAll('qt-deleted-image-placeholder button')].find(
      (b) => (b.textContent ?? '').trim() === 'Remove',
    ) as HTMLButtonElement;

  it('still uses the generic image delete when no override is given', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fetchMock = vi.fn(async () => ({ ok: true, json: async () => ({}) }));
    vi.stubGlobal('fetch', fetchMock);
    const fixture = await render();
    await miss(fixture);
    removeButton().click();
    await flush(fixture);
    expect(fetchMock).toHaveBeenCalledWith('/api/v1/images/file-1', { method: 'DELETE' });
    expect(fixture.componentInstance.closes()).toBe(1);
  });

  it('runs the override instead of the generic delete, then closes', async () => {
    @Component({
      imports: [FullScreenImageViewer],
      template: `
        <qt-full-screen-image-viewer
          src="/api/v1/files/file-2"
          alt="A coat"
          imageId="file-2"
          filename="coat.webp"
          [missingCleanup]="cleanup"
          (closeModal)="closes.set(closes() + 1)"
        />
      `,
    })
    class OverrideHost {
      readonly closes = signal(0);
      readonly calls: number[] = [];
      readonly cleanup = async (): Promise<void> => {
        this.calls.push(this.closes());
      };
    }
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [OverrideHost] });
    const fixture = TestBed.createComponent(OverrideHost);
    fixture.detectChanges();
    await flush(fixture as unknown as ComponentFixture<Host>);
    body().querySelector('qt-full-screen-image-viewer img')!.dispatchEvent(new Event('error'));
    await flush(fixture as unknown as ComponentFixture<Host>);
    removeButton().click();
    await flush(fixture as unknown as ComponentFixture<Host>);
    expect(fixture.componentInstance.calls).toEqual([0]);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(fixture.componentInstance.closes()).toBe(1);
  });
});
