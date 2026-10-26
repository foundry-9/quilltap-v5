import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { ItemSelectBar } from './item-select-bar';
import { ItemTierRow } from './item-tier-row';

/** v4 `ItemSelectBar.tsx` + `ItemTierRow.tsx` (`4fbdad186`). */

@Component({
  imports: [ItemSelectBar, ItemTierRow],
  template: `
    <qt-item-select-bar [count]="count()" [sticky]="sticky()" testId="inventory-select-bar" (cancelled)="log.push('cancel')">
      <button type="button" class="qt-button-secondary qt-button-sm">Move to folder…</button>
    </qt-item-select-bar>
    <qt-item-tier-row
      [label]="label()"
      [scope]="scope()"
      [itemCount]="itemCount()"
      testId="inventory-tier-row"
      countNoun="object"
      (opened)="log.push('open')"
    />
  `,
})
class Host {
  readonly count = signal(2);
  readonly sticky = signal(true);
  readonly label = signal('General');
  readonly scope = signal<'general' | 'project' | 'group' | 'character'>('general');
  readonly itemCount = signal(1);
  readonly log: string[] = [];
}

function render() {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement, host: fixture.componentInstance };
}

describe('ItemSelectBar', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('counts the selection, projects the caller’s buttons before Cancel, sticks when asked', () => {
    const { fixture, el, host } = render();
    const bar = el.querySelector('[data-testid="inventory-select-bar"]') as HTMLElement;
    expect(bar.className).toBe(
      'sticky bottom-0 mt-2 flex items-center justify-between gap-2 rounded border qt-border-default qt-bg-default px-3 py-2 shadow-sm',
    );
    expect(bar.querySelector('span')!.textContent).toBe('2 selected');
    const buttons = [...bar.querySelectorAll('button')].map((b) => b.textContent!.trim());
    expect(buttons).toEqual(['Move to folder…', 'Cancel']);
    host.sticky.set(false);
    fixture.detectChanges();
    expect(bar.className.startsWith(' mt-2')).toBe(true);
    (bar.querySelectorAll('button')[1] as HTMLButtonElement).click();
    expect(host.log).toEqual(['cancel']);
  });
});

describe('ItemTierRow', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('labels the tier, badges Shared / Project / Group, counts its noun, opens', () => {
    const { fixture, el, host } = render();
    const row = el.querySelector('[data-testid="inventory-tier-row"]') as HTMLButtonElement;
    expect(row.querySelector('.qt-badge')!.textContent).toBe('Shared');
    expect(row.textContent).toContain('General');
    expect(row.textContent).toContain('1 object');
    host.scope.set('project');
    host.itemCount.set(2);
    fixture.detectChanges();
    expect(row.querySelector('.qt-badge')!.textContent).toBe('Project');
    expect(row.textContent).toContain('2 objects');
    host.scope.set('group');
    fixture.detectChanges();
    expect(row.querySelector('.qt-badge')!.textContent).toBe('Group');
    row.click();
    expect(host.log).toEqual(['open']);
  });
});
