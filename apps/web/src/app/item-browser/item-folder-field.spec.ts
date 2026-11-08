import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { ItemFolderField } from './item-folder-field';

/**
 * v4 `components/item-browser/ItemFolderField.tsx` at the pin `624d0d25c`
 * (NEW at `b2b783b47`; v4's `WardrobeFolderField.tsx` shrinks onto it). v4
 * has no jest file for the shared field (the item-editor tests drive it);
 * these rows pin its options, its New folder… swap, Escape and the hint.
 */

@Component({
  imports: [ItemFolderField],
  template: `
    <qt-item-folder-field
      id="inv-folder"
      [folders]="folders()"
      [value]="value()"
      (valueChange)="value.set($event); changes.push($event)"
      rootLabel="Inventory (top level)"
      hint="Where it is filed for your browsing."
      placeholder="e.g., Tools/Garden"
      [selectClassName]="selectClass()"
    />
  `,
})
class Host {
  readonly folders = signal<ReadonlyArray<{ path: string }>>([{ path: 'tools' }, { path: 'Arms/Bows' }, { path: 'Écrins' }]);
  readonly value = signal('');
  readonly selectClass = signal('qt-select');
  readonly changes: string[] = [];
}

function render(setup: (h: Host) => void = () => {}) {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  document.body.appendChild(fixture.nativeElement);
  fixture.detectChanges();
  TestBed.tick();
  const el = fixture.nativeElement as HTMLElement;
  const select = () => el.querySelector('select') as HTMLSelectElement | null;
  const options = () => [...(select()?.options ?? [])].map((o) => [o.value, o.textContent!.trim()]);
  return { fixture, el, host: fixture.componentInstance, select, options };
}

describe('ItemFolderField', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    TestBed.resetTestingModule();
  });

  it('labels itself Folder; the root first, the tree sorted base-insensitively as crumbs, New folder… last; the hint', () => {
    const { el, options } = render();
    const label = el.querySelector('label')!;
    expect(label.textContent!.trim()).toBe('Folder');
    expect(label.getAttribute('for')).toBe('inv-folder');
    expect(label.getAttribute('class')).toBe('qt-label mb-1');
    expect(options()).toEqual([
      ['', 'Inventory (top level)'],
      ['Arms/Bows', 'Arms › Bows'],
      ['Écrins', 'Écrins'],
      ['tools', 'tools'],
      ['\u0000new', 'New folder…'],
    ]);
    const hint = el.querySelector('p')!;
    expect(hint.textContent!.trim()).toBe('Where it is filed for your browsing.');
    expect(hint.getAttribute('class')).toBe('mt-1 text-xs qt-text-small');
  });

  it('a value the tree does not list is offered as chosen; the selection matches case-insensitively', () => {
    const { fixture, host, select, options } = render((h) => h.value.set('Garden/Pots'));
    expect(options().map((o) => o[0])).toContain('Garden/Pots');
    expect(select()!.value).toBe('Garden/Pots');
    host.value.set('TOOLS');
    fixture.detectChanges();
    expect(select()!.value).toBe('tools');
    expect(options().map((o) => o[0])).not.toContain('TOOLS');
  });

  it('choosing a folder emits it; the root emits ""', () => {
    const { fixture, host, select } = render();
    select()!.value = 'Arms/Bows';
    select()!.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    select()!.value = '';
    select()!.dispatchEvent(new Event('change'));
    expect(host.changes).toEqual(['Arms/Bows', '']);
  });

  it('New folder… swaps to a focused typed input (no change emitted); Pick or Escape returns to the select', () => {
    const { fixture, el, host, select } = render();
    select()!.value = '\u0000new';
    select()!.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    TestBed.tick();
    expect(host.changes).toEqual([]);
    const input = el.querySelector('input') as HTMLInputElement;
    expect(input.id).toBe('inv-folder');
    expect(input.placeholder).toBe('e.g., Tools/Garden');
    expect(input.getAttribute('class')).toBe('qt-input');
    expect(document.activeElement).toBe(input);
    input.value = 'Garden';
    input.dispatchEvent(new Event('input'));
    expect(host.changes).toEqual(['Garden']);
    const esc = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true });
    input.dispatchEvent(esc);
    fixture.detectChanges();
    expect(el.querySelector('input')).toBeNull();
    expect(select()).not.toBeNull();
    select()!.value = '\u0000new';
    select()!.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    const pick = [...el.querySelectorAll('button')].find((b) => b.textContent!.trim() === 'Pick')!;
    expect(pick.getAttribute('class')).toBe('qt-button-ghost qt-button-sm');
    pick.click();
    fixture.detectChanges();
    expect(select()).not.toBeNull();
  });

  it('Escape in the typed input stops there (the editor behind stays open)', () => {
    const { fixture, el, select } = render();
    select()!.value = '\u0000new';
    select()!.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    let reached = false;
    const onDoc = () => (reached = true);
    document.addEventListener('keydown', onDoc);
    el.querySelector('input')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    document.removeEventListener('keydown', onDoc);
    expect(reached).toBe(false);
  });

  it('takes the caller’s select class', () => {
    const { select } = render((h) => h.selectClass.set('qt-select qt-select-sm'));
    expect(select()!.getAttribute('class')).toBe('qt-select qt-select-sm');
  });
});
