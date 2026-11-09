import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { AlertModalHost, AlertModalService } from './alert-modal.service';
import { FLOATING_LAYER_ATTR } from './floating-layer';
import { Modal } from './modal';

/**
 * P4.D277 — v4 `01a83539d` `components/ui/BaseModal.tsx`: the new
 * `overlayClassName` prop (`:31-35`, default `''` `:90`) appended to the
 * overlay's `qt-dialog-overlay` (`:107`). v5's `overlayClass` input; no other
 * `Modal` behaviour changes (R-C).
 */
@Component({
  imports: [Modal],
  template: `<qt-modal title="A dialog" [overlayClass]="overlay()"><p>body</p></qt-modal>`,
})
class Host {
  readonly overlay = signal('');
}

function render(overlay: string): HTMLElement {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.overlay.set(overlay);
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

describe('Modal overlayClass (v4 BaseModal.tsx:31-35, :90, :107 at 01a83539d)', () => {
  it('carries exactly qt-dialog-overlay by default — no stray token', () => {
    const overlay = render('').querySelector('qt-modal > div')!;
    expect(overlay.className).toBe('qt-dialog-overlay');
  });

  it('appends a raised z-index class to the overlay', () => {
    const overlay = render('z-[100]').querySelector('qt-modal > div')!;
    expect(overlay.classList.contains('qt-dialog-overlay')).toBe(true);
    expect(overlay.classList.contains('z-[100]')).toBe(true);
    expect(overlay.classList).toHaveLength(2);
  });
});

/**
 * P4.D358 — R1 for every `qt-modal` host and `closeOnEscape` (R-N), v4
 * `BaseModal.tsx:96-133` + `useClickOutside.ts:98-110` + `useDialogFocus.ts`
 * at `236c04e35`.
 */
@Component({
  imports: [Modal],
  template: `
    <button id="opener" type="button">open</button>
    @if (shown()) {
      <qt-modal title="Rename Chat" [closeOnEscape]="esc()" (close)="closes = closes + 1">
        <input id="field" />
      </qt-modal>
    }
    @if (second()) {
      <qt-modal title="Inner" (close)="innerCloses = innerCloses + 1"><p>inner</p></qt-modal>
    }
  `,
})
class EscHost {
  readonly shown = signal(true);
  readonly second = signal(false);
  readonly esc = signal(true);
  closes = 0;
  innerCloses = 0;
}

function renderEsc() {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [EscHost] });
  const fixture = TestBed.createComponent(EscHost);
  document.body.appendChild(fixture.nativeElement);
  fixture.detectChanges();
  return { fixture, host: fixture.componentInstance, el: fixture.nativeElement as HTMLElement };
}

function escape(target: EventTarget = document, prevent = false): KeyboardEvent {
  const ev = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
  if (prevent) ev.preventDefault();
  target.dispatchEvent(ev);
  return ev;
}

const nextFrame = (): Promise<void> => new Promise((r) => requestAnimationFrame(() => r()));

describe('Modal dialog semantics + Escape (P4.D358, v4 BaseModal at 236c04e35)', () => {
  it('is named by its title through aria-labelledby, and takes focus (tabindex=-1)', () => {
    const { el } = renderEsc();
    const card = el.querySelector<HTMLElement>('[role="dialog"]')!;
    expect(card.getAttribute('aria-modal')).toBe('true');
    expect(card.getAttribute('tabindex')).toBe('-1');
    const id = card.getAttribute('aria-labelledby')!;
    expect(id).toMatch(/^qt-dialog-\d+$/);
    expect(document.getElementById(id)?.textContent?.trim()).toBe('Rename Chat');
    document.body.innerHTML = '';
  });

  it('mints a different title id per instance', () => {
    const { el, host, fixture } = renderEsc();
    host.second.set(true);
    fixture.detectChanges();
    const ids = Array.from(el.querySelectorAll('[role="dialog"]')).map((c) =>
      c.getAttribute('aria-labelledby'),
    );
    expect(ids).toHaveLength(2);
    expect(ids[0]).not.toBe(ids[1]);
    document.body.innerHTML = '';
  });

  it('moves focus onto the card one frame after opening, and Tab wraps inside it', async () => {
    const { el } = renderEsc();
    await nextFrame();
    const card = el.querySelector<HTMLElement>('[role="dialog"]')!;
    expect(document.activeElement).toBe(card);
    const field = el.querySelector<HTMLElement>('#field')!;
    field.focus();
    const ev = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true });
    field.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(true);
    // the close button is the first focusable in the card
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Close');
    document.body.innerHTML = '';
  });

  it('emits close once on a document Escape', () => {
    const { host } = renderEsc();
    escape();
    expect(host.closes).toBe(1);
    document.body.innerHTML = '';
  });

  it('does not emit when [closeOnEscape] is false', () => {
    const { host, fixture } = renderEsc();
    host.esc.set(false);
    fixture.detectChanges();
    escape();
    expect(host.closes).toBe(0);
    document.body.innerHTML = '';
  });

  it('does not emit when the Escape was already consumed (defaultPrevented)', () => {
    const { host } = renderEsc();
    escape(document, true);
    expect(host.closes).toBe(0);
    document.body.innerHTML = '';
  });

  it('ignores an Escape pressed inside the topmost floating layer (v4 useClickOutside, 99463fc56)', () => {
    const { host } = renderEsc();
    const layer = document.createElement('div');
    layer.setAttribute(FLOATING_LAYER_ATTR, '');
    const inner = document.createElement('button');
    layer.appendChild(inner);
    document.body.appendChild(layer);
    escape(inner);
    expect(host.closes).toBe(0);
    escape();
    expect(host.closes).toBe(1);
    document.body.innerHTML = '';
  });

  it('two stacked modals: one Escape closes only the top one (v4 5efe53ea7 anotherDialogOwns)', async () => {
    // BY-DESIGN flip (§R.7) of `MODAL_ESCAPE_NO_STOP_PROPAGATION`: before v4
    // `5efe53ea7` one Escape closed both stacked BaseModals. The top dialog
    // takes focus on open (qtDialogFocus, one frame later), so it owns Escape.
    const { host, fixture } = renderEsc();
    host.second.set(true);
    fixture.detectChanges();
    await nextFrame();
    escape(document.activeElement ?? document);
    expect(host.innerCloses).toBe(1);
    expect(host.closes).toBe(0);
    document.body.innerHTML = '';
  });
});

/**
 * P4.D387 item 5 — v4 `5efe53ea7` `components/ui/BaseModal.tsx`
 * `anotherDialogOwns`: only the dialog on top answers an outside click or
 * Escape. v4's `__tests__/unit/components/ui/BaseModal-layered.test.tsx`
 * (four cases) translated case for case, each against SIBLING stacking (v4's
 * portaled shape) and, for cases 1–2, NESTED stacking (a `qt-modal` rendered
 * in place inside another — `MODAL_IN_PLACE_LAYERING`).
 */
@Component({
  imports: [Modal],
  template: `
    <qt-modal title="Picker" (close)="outer = outer + 1">
      <button type="button" id="outer-button">Outer button</button>
      @if (nested() && inner()) {
        <qt-modal title="Move to" (close)="innerCount = innerCount + 1">
          <button type="button" id="inner-option">Inner option</button>
        </qt-modal>
      }
    </qt-modal>
    @if (!nested() && inner()) {
      <qt-modal title="Move to" (close)="innerCount = innerCount + 1">
        <button type="button" id="inner-option">Inner option</button>
      </qt-modal>
    }
    @if (alert()) {
      <div role="alertdialog" id="alert">Delete it?</div>
    }
  `,
})
class LayerHost {
  readonly nested = signal(false);
  readonly inner = signal(true);
  readonly alert = signal(false);
  outer = 0;
  innerCount = 0;
}

@Component({ imports: [AlertModalHost], template: `<qt-alert-modal-host />` })
class AlertWrap {}

function renderLayers(opts: { nested?: boolean; inner?: boolean; alert?: boolean }) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [LayerHost] });
  const fixture = TestBed.createComponent(LayerHost);
  const host = fixture.componentInstance;
  host.nested.set(opts.nested ?? false);
  host.inner.set(opts.inner ?? true);
  host.alert.set(opts.alert ?? false);
  document.body.appendChild(fixture.nativeElement);
  fixture.detectChanges();
  return { fixture, host, el: fixture.nativeElement as HTMLElement };
}

describe('Modal layering (v4 5efe53ea7 BaseModal-layered.test.tsx)', () => {
  for (const nested of [false, true]) {
    const shape = nested ? 'nested' : 'sibling';

    it(`case 1 (${shape}): a click inside a dialog over it does not close the modal beneath`, () => {
      const { host, el } = renderLayers({ nested });
      el.querySelector<HTMLElement>('#inner-option')!.click();
      expect(host.outer).toBe(0);
      expect(host.innerCount).toBe(0);
      document.body.innerHTML = '';
    });

    it(`case 2 (${shape}): Escape with focus in the dialog on top closes only that one`, () => {
      const { host, el } = renderLayers({ nested });
      const option = el.querySelector<HTMLElement>('#inner-option')!;
      option.focus();
      escape(option);
      expect(host.innerCount).toBe(1);
      expect(host.outer).toBe(0);
      document.body.innerHTML = '';
    });
  }

  it('case 3: an open confirmation owns Escape even without focus', () => {
    const { host, el } = renderLayers({ inner: false, alert: true });
    escape(el.querySelector<HTMLElement>('#outer-button')!);
    expect(host.outer).toBe(0);
    document.body.innerHTML = '';
  });

  it('case 3 (the real AlertModalService confirmation): the modal beneath stays open', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [LayerHost, AlertWrap] });
    const fixture = TestBed.createComponent(LayerHost);
    const host = fixture.componentInstance;
    host.inner.set(false);
    document.body.appendChild(fixture.nativeElement);
    fixture.detectChanges();
    // A second createComponent detaches the first fixture's root, so the
    // alert host is created (and attached) last.
    const alertFixture = TestBed.createComponent(AlertWrap);
    document.body.appendChild(alertFixture.nativeElement);
    alertFixture.detectChanges();
    void TestBed.inject(AlertModalService).show('Delete it?', ['Cancel', 'Delete']);
    alertFixture.detectChanges();
    expect(document.querySelector('[role="alertdialog"]')).not.toBeNull();
    escape(fixture.nativeElement.querySelector('#outer-button'));
    expect(host.outer).toBe(0);
    document.body.innerHTML = '';
  });

  it('case 4: alone, it still closes on Escape and on an outside click', () => {
    const { host, el } = renderLayers({ inner: false });
    escape(document.body);
    el.querySelector<HTMLElement>('.qt-dialog-overlay')!.click();
    expect(host.outer).toBe(2);
    document.body.innerHTML = '';
  });

  it('nested: the top modal still closes on its own backdrop', () => {
    const { host, el } = renderLayers({ nested: true });
    const overlays = el.querySelectorAll<HTMLElement>('.qt-dialog-overlay');
    overlays[1].click();
    expect(host.innerCount).toBe(1);
    expect(host.outer).toBe(0);
    document.body.innerHTML = '';
  });

  it('a non-modal popover inside the modal does not own its Escape (v4 !modal.contains)', () => {
    const { host, el } = renderLayers({ inner: false });
    const pop = document.createElement('div');
    pop.setAttribute('role', 'dialog');
    const btn = document.createElement('button');
    pop.appendChild(btn);
    el.querySelector('#outer-button')!.parentElement!.appendChild(pop);
    btn.focus();
    escape(btn);
    expect(host.outer).toBe(1);
    document.body.innerHTML = '';
  });
});
