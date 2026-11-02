import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

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

  it('MODAL_ESCAPE_NO_STOP_PROPAGATION: two stacked modals BOTH close on one Escape (v4 parity)', () => {
    const { host, fixture } = renderEsc();
    host.second.set(true);
    fixture.detectChanges();
    escape();
    expect(host.closes).toBe(1);
    expect(host.innerCloses).toBe(1);
    document.body.innerHTML = '';
  });
});
