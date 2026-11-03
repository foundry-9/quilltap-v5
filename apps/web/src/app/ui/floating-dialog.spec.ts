/**
 * The floating dialog (P4.D359 item 2) — v4 `components/ui/FloatingDialog.tsx`
 * at `236c04e35` (`99463fc56` + `dff76ebef`).
 *
 * The first describe TRANSCRIBES v4 `__tests__/unit/components/ui/
 * FloatingDialog-topmost.test.tsx:39-65` (four cases). v4 renders a real
 * `BaseModal` beneath; v5's `qt-modal` gains its document Escape listener in
 * P4.D358's lane (R-N), not in this tree, so the "modal" here is a stand-in
 * host — `role="dialog" aria-modal="true"`, a document `mousedown`
 * click-outside and a document Escape, each behind the `isInFloatingLayer`
 * guard v4's `useClickOutside` carries (the guard P4.D358's `qt-modal` takes as
 * this lane's HANDOFF). The unifier re-runs the four against the real
 * `qt-modal`.
 */
import { ChangeDetectionStrategy, Component, ElementRef, inject, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  DEFAULT_GEOMETRY,
  FloatingDialog,
  constrainToViewport,
  loadGeometry,
  saveGeometry,
} from './floating-dialog';
import { isInFloatingLayer } from './floating-layer';

@Component({
  selector: 'qt-stand-in-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    '(document:mousedown)': 'onDocumentMousedown($event)',
    '(document:keydown.escape)': 'onEscape($event)',
  },
  template: `<div role="dialog" aria-modal="true"><input aria-label="form field" /></div>`,
})
class StandInModal {
  readonly closes = vi.fn();
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  onDocumentMousedown(e: MouseEvent): void {
    if (isInFloatingLayer(e.target)) return;
    if (!this.host.nativeElement.contains(e.target as Node)) this.closes();
  }
  onEscape(e: Event): void {
    if (isInFloatingLayer(e.target)) return;
    this.closes();
  }
}

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FloatingDialog, StandInModal],
  template: `
    @if (withModal()) {
      <qt-stand-in-modal />
    }
    <button type="button" id="opener">opener</button>
    <qt-floating-dialog
      [open]="open()"
      title="Console"
      storageKey="test:geo"
      [topmost]="topmost()"
      [minWidth]="320"
      [minHeight]="300"
      (close)="closes()"
    >
      <button type="button" qt-floating-dialog-actions id="action">act</button>
      <textarea aria-label="console composer"></textarea>
      <button type="button" id="last">last</button>
    </qt-floating-dialog>
  `,
})
class Host {
  readonly open = signal(true);
  readonly topmost = signal(true);
  readonly withModal = signal(true);
  readonly closes = vi.fn();
}

let fixture: ComponentFixture<Host>;
const el = (): HTMLElement => fixture.nativeElement as HTMLElement;
const dialog = (): HTMLElement => el().querySelector('.qt-floating-dialog') as HTMLElement;
const modalCloses = () =>
  (
    fixture.debugElement.query((d) => d.name === 'qt-stand-in-modal')
      ?.componentInstance as StandInModal
  ).closes;
const key = (target: EventTarget, k = 'Escape', init: KeyboardEventInit = {}) =>
  target.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, ...init }));

function mount(opts: { open?: boolean; topmost?: boolean; withModal?: boolean } = {}): void {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  fixture = TestBed.createComponent(Host);
  fixture.componentInstance.open.set(opts.open ?? true);
  fixture.componentInstance.topmost.set(opts.topmost ?? true);
  fixture.componentInstance.withModal.set(opts.withModal ?? true);
  document.body.appendChild(fixture.nativeElement);
  fixture.detectChanges();
}

beforeAll(() => {
  // jsdom has no ResizeObserver; the dialog only uses it to persist geometry.
  if (!('ResizeObserver' in globalThis)) {
    (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  }
});

beforeEach(() => localStorage.clear());
afterEach(() => {
  fixture?.destroy();
  (fixture?.nativeElement as HTMLElement | undefined)?.remove();
});

describe('FloatingDialog topmost (transcribed — v4 FloatingDialog-topmost.test.tsx:39-65)', () => {
  it('carries the topmost class and the floating-layer marker', () => {
    mount();
    const d = el()
      .querySelector('[aria-label="console composer"]')!
      .closest('.qt-floating-dialog')!;
    expect(d.classList.contains('qt-floating-dialog-topmost')).toBe(true);
    expect(d.hasAttribute('data-qt-floating-layer')).toBe(true);
  });

  it('clicking inside it leaves the modal beneath open', () => {
    mount();
    el()
      .querySelector('[aria-label="console composer"]')!
      .dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(modalCloses()).not.toHaveBeenCalled();
    expect(fixture.componentInstance.closes).not.toHaveBeenCalled();
  });

  it('Escape inside it closes only the dialog', () => {
    mount();
    key(el().querySelector('[aria-label="console composer"]')!);
    expect(fixture.componentInstance.closes).toHaveBeenCalledTimes(1);
    expect(modalCloses()).not.toHaveBeenCalled();
  });

  it('Escape in the modal closes only the modal', () => {
    mount();
    key(el().querySelector('[aria-label="form field"]')!);
    expect(modalCloses()).toHaveBeenCalledTimes(1);
    expect(fixture.componentInstance.closes).not.toHaveBeenCalled();
  });
});

describe('FloatingDialog — chrome and semantics (v4 :198-243, dff76ebef R2)', () => {
  it('a non-modal dialog labelled by its title, tabindex -1, in place and position: fixed', () => {
    mount({ topmost: false });
    const d = dialog();
    expect(d.getAttribute('role')).toBe('dialog');
    expect(d.getAttribute('aria-modal')).toBe('false');
    expect(d.getAttribute('tabindex')).toBe('-1');
    expect(document.getElementById(d.getAttribute('aria-labelledby')!)!.textContent!.trim()).toBe(
      'Console',
    );
    // FLOATING_DIALOG_IN_PLACE — a descendant of its host, not portalled.
    expect(el().querySelector('qt-floating-dialog')!.contains(d)).toBe(true);
    // Not topmost: no topmost class, no floating-layer marker.
    expect(d.classList.contains('qt-floating-dialog-topmost')).toBe(false);
    expect(d.hasAttribute('data-qt-floating-layer')).toBe(false);
  });

  it('projects the header actions before the close button, which v4 names Close', () => {
    mount();
    const actions = dialog().querySelector('.qt-floating-dialog-actions')!;
    const buttons = [...actions.querySelectorAll('button')];
    expect(buttons[0].id).toBe('action');
    expect(buttons[1].getAttribute('title')).toBe('Close');
    expect(buttons[1].getAttribute('aria-label')).toBe('Close');
    buttons[1].click();
    expect(fixture.componentInstance.closes).toHaveBeenCalledTimes(1);
  });

  it('renders nothing while closed', () => {
    mount({ open: false });
    expect(dialog()).toBeNull();
  });
});

describe('FloatingDialog — geometry (v4 :48-89, :113-170)', () => {
  it('round-trips through localStorage under the given key', () => {
    saveGeometry('test:geo', { x: 30, y: 40, width: 500, height: 450 });
    expect(loadGeometry('test:geo')).toEqual({ x: 30, y: 40, width: 500, height: 450 });
    expect(loadGeometry('test:none', { width: 560 })).toEqual({ ...DEFAULT_GEOMETRY, width: 560 });
  });

  it('a stored geometry wins over the initial one; a corrupt one falls back to it', () => {
    localStorage.setItem('test:geo', '{"x":5}');
    expect(loadGeometry('test:geo', { width: 560 })).toEqual({ ...DEFAULT_GEOMETRY, x: 5 });
    localStorage.setItem('test:geo', '{not json');
    expect(loadGeometry('test:geo', { width: 560 })).toEqual({ ...DEFAULT_GEOMETRY, width: 560 });
  });

  describe('constrainToViewport', () => {
    beforeEach(() => {
      vi.spyOn(window, 'innerWidth', 'get').mockReturnValue(1000);
      vi.spyOn(window, 'innerHeight', 'get').mockReturnValue(800);
    });
    afterEach(() => vi.restoreAllMocks());

    it('-1 centres', () => {
      expect(constrainToViewport({ x: -1, y: -1, width: 400, height: 300 }, 320, 300)).toEqual({
        x: 300,
        y: 250,
        width: 400,
        height: 300,
      });
    });
    it('caps the width at vw − 20 and the height at vh − 20', () => {
      const geo = constrainToViewport({ x: 0, y: 0, width: 5000, height: 5000 }, 320, 300);
      expect([geo.width, geo.height]).toEqual([980, 780]);
    });
    it('never below the minimums', () => {
      const geo = constrainToViewport({ x: 0, y: 0, width: 10, height: 10 }, 320, 300);
      expect([geo.width, geo.height]).toEqual([320, 300]);
    });
    it('keeps the origin within vw − 40 / vh − 40', () => {
      const geo = constrainToViewport({ x: 5000, y: 5000, width: 400, height: 300 }, 320, 300);
      expect([geo.x, geo.y]).toEqual([960, 760]);
    });
  });

  it('drags from the header, ignoring a mousedown on a header button, and persists', () => {
    mount();
    const d = dialog();
    const start = { x: parseFloat(d.style.left), y: parseFloat(d.style.top) };
    (dialog().querySelector('#action') as HTMLElement).dispatchEvent(
      new MouseEvent('mousedown', { bubbles: true, clientX: 0, clientY: 0 }),
    );
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 50, clientY: 50 }));
    fixture.detectChanges();
    expect([parseFloat(d.style.left), parseFloat(d.style.top)]).toEqual([start.x, start.y]);

    const header = d.querySelector('.qt-floating-dialog-header')!;
    header.dispatchEvent(
      new MouseEvent('mousedown', { bubbles: true, clientX: start.x + 5, clientY: start.y + 5 }),
    );
    document.dispatchEvent(
      new MouseEvent('mousemove', { clientX: start.x + 25, clientY: start.y + 15 }),
    );
    document.dispatchEvent(new MouseEvent('mouseup'));
    fixture.detectChanges();
    expect([parseFloat(d.style.left), parseFloat(d.style.top)]).toEqual([
      start.x + 20,
      start.y + 10,
    ]);
    expect(JSON.parse(localStorage.getItem('test:geo')!)).toMatchObject({
      x: start.x + 20,
      y: start.y + 10,
    });
  });

  it('a resize under 2 px is ignored; a larger one is recorded', async () => {
    let callback!: ResizeObserverCallback;
    const original = globalThis.ResizeObserver;
    globalThis.ResizeObserver = class {
      constructor(cb: ResizeObserverCallback) {
        callback = cb;
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
    try {
      mount();
      await fixture.whenStable();
      const d = dialog();
      const width = parseFloat(d.style.width);
      const height = parseFloat(d.style.height);
      const fire = (w: number, h: number) =>
        callback(
          [{ contentRect: { width: w, height: h } } as unknown as ResizeObserverEntry],
          {} as ResizeObserver,
        );
      fire(width + 2, height);
      fixture.detectChanges();
      expect(localStorage.getItem('test:geo')).toBeNull();
      fire(width + 50, height);
      fixture.detectChanges();
      expect(parseFloat(d.style.width)).toBe(width + 50);
      expect(JSON.parse(localStorage.getItem('test:geo')!).width).toBe(width + 50);
    } finally {
      globalThis.ResizeObserver = original;
    }
  });
});

describe('FloatingDialog — focus and Escape (v4 useDialogFocus + :172-196)', () => {
  const frame = () => new Promise((r) => requestAnimationFrame(() => r(undefined)));

  it('moves focus to the dialog after a frame, and back to the opener on close', async () => {
    mount({ open: false, withModal: false });
    const opener = el().querySelector('#opener') as HTMLButtonElement;
    opener.focus();
    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    await fixture.whenStable();
    await frame();
    expect(document.activeElement).toBe(dialog());
    fixture.componentInstance.open.set(false);
    fixture.detectChanges();
    expect(document.activeElement).toBe(opener);
  });

  it('leaves focus alone when a descendant already holds it', async () => {
    mount({ open: false, withModal: false });
    fixture.componentInstance.open.set(true);
    fixture.detectChanges();
    const composer = el().querySelector('[aria-label="console composer"]') as HTMLElement;
    composer.focus();
    await fixture.whenStable();
    await frame();
    expect(document.activeElement).toBe(composer);
  });

  it('traps nothing — Tab from the last focusable is never prevented', () => {
    mount({ withModal: false });
    const last = el().querySelector('#last') as HTMLElement;
    const tab = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true });
    last.dispatchEvent(tab);
    expect(tab.defaultPrevented).toBe(false);
  });

  it('a document Escape closes it with no modal open, and not with one', () => {
    mount({ withModal: false });
    key(document.body);
    expect(fixture.componentInstance.closes).toHaveBeenCalledTimes(1);
    mount({ withModal: true });
    key(document.body);
    expect(fixture.componentInstance.closes).not.toHaveBeenCalled();
  });

  it('skips an Escape already handled (defaultPrevented), inside and out', () => {
    mount({ withModal: false });
    key(el().querySelector('[aria-label="console composer"]')!, 'Escape', { cancelable: true });
    const handled = new KeyboardEvent('keydown', {
      key: 'Escape',
      bubbles: true,
      cancelable: true,
    });
    handled.preventDefault();
    el().querySelector('[aria-label="console composer"]')!.dispatchEvent(handled);
    const outside = new KeyboardEvent('keydown', {
      key: 'Escape',
      bubbles: true,
      cancelable: true,
    });
    outside.preventDefault();
    document.body.dispatchEvent(outside);
    expect(fixture.componentInstance.closes).toHaveBeenCalledTimes(1);
  });

  it('a non-topmost dialog lets an inside Escape bubble on', () => {
    mount({ topmost: false, withModal: false });
    const spy = vi.fn();
    document.addEventListener('keydown', spy);
    try {
      key(el().querySelector('[aria-label="console composer"]')!);
      expect(spy).toHaveBeenCalledTimes(1);
      expect(fixture.componentInstance.closes).toHaveBeenCalledTimes(1);
    } finally {
      document.removeEventListener('keydown', spy);
    }
  });
});
