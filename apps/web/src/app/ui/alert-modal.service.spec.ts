import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { AlertModalHost, AlertModalService } from './alert-modal.service';

/** v4 `lib/alert.tsx` `showAlert` + `components/alert-dialog.tsx` (P4.D330 Tier 2 item 24). */

@Component({ imports: [AlertModalHost], template: `<qt-alert-modal-host />` })
class Host {}

function render() {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement, alerts: TestBed.inject(AlertModalService) };
}

describe('AlertModalService + AlertModalHost', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows the message and the labelled buttons, the LAST primary; a click resolves its label', async () => {
    const { fixture, el, alerts } = render();
    const choice = alerts.show('2 objects derive from "magic bow"…', ['Detach all and delete', 'Cancel']);
    fixture.detectChanges();
    expect(el.querySelector('[data-testid="alert-modal-message"]')!.textContent).toBe('2 objects derive from "magic bow"…');
    const buttons = [...el.querySelectorAll('.qt-dialog button')] as HTMLButtonElement[];
    expect(buttons.map((b) => [b.textContent!.trim(), b.getAttribute('class')])).toEqual([
      ['Detach all and delete', 'qt-button qt-button-secondary'],
      ['Cancel', 'qt-button qt-button-primary'],
    ]);
    buttons[0].click();
    expect(await choice).toBe('Detach all and delete');
    fixture.detectChanges();
    expect(el.querySelector('.qt-dialog')).toBeNull();
  });

  it('Escape and the backdrop dismiss to null; no buttons → v4’s one Close', async () => {
    const { fixture, el, alerts } = render();
    const escaped = alerts.show('sealed', ['Understood']);
    fixture.detectChanges();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(await escaped).toBeNull();
    const backdrop = alerts.show('plain');
    fixture.detectChanges();
    expect([...el.querySelectorAll('.qt-dialog button')].map((b) => b.textContent!.trim())).toEqual(['Close']);
    (el.querySelector('button[aria-label="Close dialog"]') as HTMLButtonElement).click();
    expect(await backdrop).toBeNull();
  });

  it('a second alert dismisses the first', async () => {
    const { alerts } = render();
    const first = alerts.show('one', ['A']);
    const second = alerts.show('two', ['B']);
    expect(await first).toBeNull();
    alerts.answer('B');
    expect(await second).toBe('B');
  });
});
