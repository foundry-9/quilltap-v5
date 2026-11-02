import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { DIALOG_FOCUSABLE, DialogFocus } from './dialog-focus';

/**
 * P4.D358 — `qtDialogFocus`, v4 `hooks/useDialogFocus.ts` at `236c04e35`
 * (`58120a7fe` + `dff76ebef`), transcribed case for case.
 */
@Component({
  imports: [DialogFocus],
  template: `
    <button id="opener" type="button">open</button>
    @if (shown()) {
      <div id="dlg" tabindex="-1" qtDialogFocus [qtDialogTrap]="trap()">
        @if (withControls()) {
          <button id="first" type="button">one</button>
          <button id="disabled" type="button" disabled>off</button>
          @if (autofocus()) {
            <input id="auto" #auto />
          }
          <button id="last" type="button">two</button>
        }
      </div>
    }
  `,
})
class Host {
  readonly shown = signal(false);
  readonly trap = signal(true);
  readonly withControls = signal(true);
  readonly autofocus = signal(false);
}

const frame = (): Promise<void> => new Promise((r) => requestAnimationFrame(() => r()));

function render() {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  document.body.appendChild(fixture.nativeElement);
  fixture.detectChanges();
  const host = fixture.componentInstance;
  const el = fixture.nativeElement as HTMLElement;
  const q = (id: string) => el.querySelector<HTMLElement>('#' + id);
  return { fixture, host, q, detect: () => fixture.detectChanges() };
}

function tab(target: HTMLElement, shiftKey = false): KeyboardEvent {
  const ev = new KeyboardEvent('keydown', { key: 'Tab', shiftKey, bubbles: true, cancelable: true });
  target.dispatchEvent(ev);
  return ev;
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('qtDialogFocus (v4 hooks/useDialogFocus.ts)', () => {
  it('carries v4 FOCUSABLE byte for byte', () => {
    expect(DIALOG_FOCUSABLE).toBe(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    );
  });

  it('moves focus to the host one frame after opening', async () => {
    const { host, q, detect } = render();
    q('opener')!.focus();
    host.shown.set(true);
    detect();
    expect(document.activeElement?.id).toBe('opener');
    await frame();
    expect(document.activeElement?.id).toBe('dlg');
  });

  it('does NOT steal focus from a descendant that already holds it', async () => {
    const { host, q, detect } = render();
    q('opener')!.focus();
    host.autofocus.set(true);
    host.shown.set(true);
    detect();
    q('auto')!.focus();
    await frame();
    expect(document.activeElement?.id).toBe('auto');
  });

  it('wraps Tab from the last focusable to the first', async () => {
    const { host, q, detect } = render();
    host.shown.set(true);
    detect();
    q('last')!.focus();
    const ev = tab(q('last')!);
    expect(ev.defaultPrevented).toBe(true);
    expect(document.activeElement?.id).toBe('first');
  });

  it('wraps Shift+Tab from the first focusable to the last', () => {
    const { host, q, detect } = render();
    host.shown.set(true);
    detect();
    q('first')!.focus();
    const ev = tab(q('first')!, true);
    expect(ev.defaultPrevented).toBe(true);
    expect(document.activeElement?.id).toBe('last');
  });

  it('wraps Shift+Tab from the HOST itself to the last', async () => {
    const { host, q, detect } = render();
    host.shown.set(true);
    detect();
    await frame();
    expect(document.activeElement?.id).toBe('dlg');
    tab(q('dlg')!, true);
    expect(document.activeElement?.id).toBe('last');
  });

  it('skips a disabled button (the last ENABLED control is the wrap target)', () => {
    const { host, q, detect } = render();
    host.shown.set(true);
    detect();
    q('first')!.focus();
    tab(q('first')!, true);
    expect(document.activeElement?.id).not.toBe('disabled');
    expect(document.activeElement?.id).toBe('last');
  });

  it('prevents Tab outright when nothing inside can take focus', () => {
    const { host, q, detect } = render();
    host.withControls.set(false);
    host.shown.set(true);
    detect();
    const ev = tab(q('dlg')!);
    expect(ev.defaultPrevented).toBe(true);
  });

  it('restores focus to the opener on destroy', async () => {
    const { host, q, detect } = render();
    q('opener')!.focus();
    host.shown.set(true);
    detect();
    await frame();
    host.shown.set(false);
    detect();
    expect(document.activeElement?.id).toBe('opener');
  });

  it('does not throw when the opener was removed before close', async () => {
    const { host, q, detect } = render();
    const stray = document.createElement('button');
    document.body.appendChild(stray);
    stray.focus();
    host.shown.set(true);
    detect();
    await frame();
    stray.remove();
    expect(() => {
      host.shown.set(false);
      detect();
    }).not.toThrow();
    expect(q('opener')).not.toBeNull();
  });

  it('leaves Tab alone when qtDialogTrap is false (C9 R2)', () => {
    const { host, q, detect } = render();
    host.trap.set(false);
    host.shown.set(true);
    detect();
    q('last')!.focus();
    const ev = tab(q('last')!);
    expect(ev.defaultPrevented).toBe(false);
    expect(document.activeElement?.id).toBe('last');
  });
});
