import { Component, signal } from '@angular/core';
import { type ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { tooltipContentOf } from '../../testing/tooltip-content';
import type { RaisedHandStatus } from './raised-hand';
import { RaisedHandChip, describeRaisedHand } from './raised-hand-chip';

@Component({
  imports: [RaisedHandChip],
  template: `<qt-raised-hand-chip
    [status]="status()"
    (lower)="lowered.set(lowered() + 1)"
    (dismiss)="dismissed.set(dismissed() + 1)"
  />`,
})
class Host {
  readonly status = signal<RaisedHandStatus | null>(null);
  readonly lowered = signal(0);
  readonly dismissed = signal(0);
}

function render(status: RaisedHandStatus | null): ComponentFixture<Host> {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.status.set(status);
  fixture.detectChanges();
  return fixture;
}

const el = (f: ComponentFixture<Host>) => f.nativeElement as HTMLElement;

/** v4 `RaisedHandChip.tsx`'s five lines and × roles (`9eab8d2da`), verbatim. */
const TABLE: Array<[RaisedHandStatus, string, boolean]> = [
  [{ kind: 'raised' }, 'Hand raised — the next speaker will be asked to yield', true],
  [{ kind: 'asked', characterName: 'Abigail' }, 'Abigail is weighing your raised hand…', false],
  [{ kind: 'yielded', characterName: 'Abigail' }, 'Abigail yields the floor — it is yours', false],
  [{ kind: 'declined', characterName: 'Abigail' }, 'Abigail carried on regardless; your hand is lowered', false],
  [{ kind: 'unasked' }, 'The room fell quiet before anyone was asked — the floor is yours', false],
];

describe('describeRaisedHand', () => {
  it.each(TABLE)('%o', (status, label, lowers) => {
    expect(describeRaisedHand(status)).toEqual({ label, lowers });
  });
});

describe('RaisedHandChip', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('keeps the live region mounted with no status, and draws no chip', () => {
    const f = render(null);
    const live = el(f).querySelector('[aria-live="polite"]');
    expect(live).not.toBeNull();
    expect(live!.querySelector('.qt-chat-tool-result-chip')).toBeNull();
  });

  it.each(TABLE)('renders %o with its line and state', (status, label) => {
    const f = render(status);
    const chip = el(f).querySelector('[aria-live="polite"] .qt-chat-tool-result-chip')!;
    expect(chip.getAttribute('data-hand-state')).toBe(status.kind);
    expect(chip.querySelector('qt-icon span')!.getAttribute('data-icon')).toBe('hand-raised');
    expect(chip.querySelector('.text-foreground')!.textContent!.trim()).toBe(label);
  });

  it('has no × while the hand is being weighed', () => {
    const f = render({ kind: 'asked', characterName: 'Abigail' });
    expect(el(f).querySelector('.qt-chat-attachment-chip-remove')).toBeNull();
  });

  it('lowers a raised hand from its ×', () => {
    const f = render({ kind: 'raised' });
    const x = el(f).querySelector<HTMLButtonElement>('.qt-chat-attachment-chip-remove')!;
    expect(x.getAttribute('aria-label')).toBe('Lower your hand');
    expect(tooltipContentOf(f, x)).toBe('Lower your hand');
    expect(x.hasAttribute('title')).toBe(false);
    x.click();
    expect(f.componentInstance.lowered()).toBe(1);
    expect(f.componentInstance.dismissed()).toBe(0);
  });

  it.each([
    { kind: 'yielded', characterName: 'Abigail' },
    { kind: 'declined', characterName: 'Abigail' },
    { kind: 'unasked' },
  ] as RaisedHandStatus[])('dismisses the %o note from its ×', (status) => {
    const f = render(status);
    const x = el(f).querySelector<HTMLButtonElement>('.qt-chat-attachment-chip-remove')!;
    expect(x.getAttribute('aria-label')).toBe('Dismiss the raised-hand note');
    expect(tooltipContentOf(f, x)).toBe('Dismiss');
    x.click();
    expect(f.componentInstance.dismissed()).toBe(1);
    expect(f.componentInstance.lowered()).toBe(0);
  });
});
