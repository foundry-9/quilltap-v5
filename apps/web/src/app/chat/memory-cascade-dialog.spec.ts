import { Component } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { beforeEach, describe, expect, it } from 'vitest';

import { MemoryCascadeDialog, type MemoryCascadeChoice } from './memory-cascade-dialog';

/**
 * The delete-message memory-cascade dialog (v4 `components/ui/MemoryCascadeDialog
 * .tsx`). The load-bearing behaviour is the `confirm` PAYLOAD: v4 hands the host
 * `onConfirm(action, rememberChoice)` (`:48-50`), and the host is what writes the
 * preference (bug 134's write side). Landed at the `f4ad2c8d1` unification — the
 * §3 review found the checkbox → payload wiring pinned by nothing: hard-coding
 * `remember: false` in the template left `npm test` green.
 */

@Component({
  imports: [MemoryCascadeDialog],
  template: `
    <qt-memory-cascade-dialog
      [memoryCount]="3"
      (confirm)="choices.push($event)"
      (cancel)="cancelled = cancelled + 1"
    />
  `,
})
class Host {
  readonly choices: MemoryCascadeChoice[] = [];
  cancelled = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  await TestBed.configureTestingModule({ imports: [Host] }).compileComponents();
  const fixture = TestBed.createComponent(Host);
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function checkbox(fixture: ComponentFixture<Host>): HTMLInputElement {
  return fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
}

function deleteButton(fixture: ComponentFixture<Host>): HTMLButtonElement {
  return Array.from(
    fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
  ).find((b) => b.textContent?.trim() === 'Delete Message')!;
}

describe('MemoryCascadeDialog — the confirm payload (v4 onConfirm(action, rememberChoice))', () => {
  let fixture: ComponentFixture<Host>;

  beforeEach(async () => {
    fixture = await render();
  });

  it('opens unchecked and answers remember: false with the default action', () => {
    expect(checkbox(fixture).checked).toBe(false);
    deleteButton(fixture).click();
    expect(fixture.componentInstance.choices).toEqual([
      { action: 'DELETE_MEMORIES', remember: false },
    ]);
  });

  it('ticking "Remember this choice" reaches the payload as remember: true', () => {
    const box = checkbox(fixture);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    deleteButton(fixture).click();
    expect(fixture.componentInstance.choices).toEqual([
      { action: 'DELETE_MEMORIES', remember: true },
    ]);
  });

  it('the chosen action rides the same payload', () => {
    const radios = fixture.nativeElement.querySelectorAll(
      'input[type="radio"]',
    ) as NodeListOf<HTMLInputElement>;
    const keep = Array.from(radios).find((r) => r.value === 'KEEP_MEMORIES')!;
    keep.checked = true;
    keep.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    deleteButton(fixture).click();
    expect(fixture.componentInstance.choices).toEqual([
      { action: 'KEEP_MEMORIES', remember: false },
    ]);
    expect(fixture.componentInstance.cancelled).toBe(0);
  });

  it('carries v4\'s checkbox sentence verbatim', () => {
    expect(fixture.nativeElement.textContent).toContain(
      'Remember this choice (can be changed in Settings)',
    );
  });
});
