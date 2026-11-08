import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { ItemTransferFooter } from './item-transfer-footer';

/**
 * v4 `components/item-browser/ItemTransferFooter.tsx` at the pin `624d0d25c`
 * (NEW at `600c2906f`; no v4 jest file of its own — the transfer dialogs'
 * tests drive it): Cancel + the submit button, `Moving…` / `Copying…` while
 * working, `submitLabel ?? 'Move' | 'Copy'` at rest.
 */
@Component({
  imports: [ItemTransferFooter],
  template: `
    <qt-item-transfer-footer
      [mode]="mode()"
      [working]="working()"
      [submitLabel]="label()"
      [submitDisabled]="disabled()"
      (cancelled)="log.push('cancel')"
      (submitted)="log.push('submit')"
    />
  `,
})
class Host {
  readonly mode = signal<'move' | 'copy'>('move');
  readonly working = signal(false);
  readonly label = signal<string | undefined>(undefined);
  readonly disabled = signal(false);
  readonly log: string[] = [];
}

describe('ItemTransferFooter', () => {
  afterEach(() => TestBed.resetTestingModule());

  function render() {
    TestBed.configureTestingModule({ imports: [Host] });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const [cancel, submit] = [...el.querySelectorAll('button')] as HTMLButtonElement[];
    return { fixture, el, host: fixture.componentInstance, cancel, submit };
  }

  it('Cancel then the submit button, labelled Move / Copy at rest, or the caller’s label', () => {
    const r = render();
    expect(r.cancel.textContent!.trim()).toBe('Cancel');
    expect(r.cancel.getAttribute('class')).toBe('qt-button-secondary qt-button-sm');
    expect(r.submit.getAttribute('class')).toBe('qt-button-primary qt-button-sm');
    expect(r.submit.textContent!.trim()).toBe('Move');
    r.host.mode.set('copy');
    r.fixture.detectChanges();
    expect(r.submit.textContent!.trim()).toBe('Copy');
    r.host.label.set('Copy item');
    r.fixture.detectChanges();
    expect(r.submit.textContent!.trim()).toBe('Copy item');
    expect(r.el.firstElementChild!.firstElementChild!.getAttribute('class')).toBe('flex justify-end gap-2');
  });

  it('working: Moving… / Copying…, both buttons disabled', () => {
    const r = render();
    r.host.working.set(true);
    r.host.label.set('Move item');
    r.fixture.detectChanges();
    expect(r.submit.textContent!.trim()).toBe('Moving…');
    expect(r.submit.disabled).toBe(true);
    expect(r.cancel.disabled).toBe(true);
    r.host.mode.set('copy');
    r.fixture.detectChanges();
    expect(r.submit.textContent!.trim()).toBe('Copying…');
  });

  it('submitDisabled disables only the submit; the buttons emit', () => {
    const r = render();
    r.host.disabled.set(true);
    r.fixture.detectChanges();
    expect(r.submit.disabled).toBe(true);
    expect(r.cancel.disabled).toBe(false);
    r.cancel.click();
    r.host.disabled.set(false);
    r.fixture.detectChanges();
    r.submit.click();
    expect(r.host.log).toEqual(['cancel', 'submit']);
  });
});
