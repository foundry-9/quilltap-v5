import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ItemFolderRow, type ItemFolderRowView } from './item-folder-row';

/** v4 `components/item-browser/ItemFolderRow.tsx` (`4fbdad186`). */

@Component({
  imports: [ItemFolderRow],
  template: `
    <qt-item-folder-row
      [row]="row()"
      [manageable]="manageable()"
      [rename]="rename"
      testId="inventory-folder-row"
      countNoun="object"
      (opened)="log.push('open')"
      (deleted)="log.push('delete')"
    />
  `,
})
class Host {
  readonly row = signal<ItemFolderRowView>({ path: 'Arms', name: 'Arms', itemCount: 1 });
  readonly manageable = signal(true);
  readonly rename = vi.fn(async (_name: string) => true);
  readonly log: string[] = [];
}

function render() {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  return { fixture, el, host: fixture.componentInstance };
}

async function startRename(fixture: ReturnType<typeof render>['fixture'], el: HTMLElement): Promise<HTMLInputElement> {
  (el.querySelector('[aria-label="Folder actions for Arms"]') as HTMLButtonElement).click();
  fixture.detectChanges();
  (el.querySelector('[role="menuitem"]') as HTMLButtonElement).click();
  fixture.detectChanges();
  return el.querySelector('input[aria-label="Rename folder Arms"]') as HTMLInputElement;
}

describe('ItemFolderRow', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('carries the caller’s test id and counts its noun; the name opens it', () => {
    const { fixture, el, host } = render();
    const row = el.querySelector('[data-testid="inventory-folder-row"]')!;
    expect(row.className).toBe('qt-card-interactive py-2 px-3 flex items-center gap-2');
    expect(row.textContent).toContain('1 object');
    host.row.set({ path: 'Arms', name: 'Arms', itemCount: 3 });
    fixture.detectChanges();
    expect(row.textContent).toContain('3 objects');
    (row.querySelector('button.truncate') as HTMLButtonElement).click();
    expect(host.log).toEqual(['open']);
  });

  it('the ⋮ menu offers Rename then Delete; Delete closes the menu and emits', () => {
    const { fixture, el, host } = render();
    const kebab = el.querySelector('[aria-label="Folder actions for Arms"]') as HTMLButtonElement;
    expect(kebab.getAttribute('aria-haspopup')).toBe('menu');
    kebab.click();
    fixture.detectChanges();
    expect(kebab.getAttribute('aria-expanded')).toBe('true');
    const items = [...el.querySelectorAll('[role="menuitem"]')] as HTMLButtonElement[];
    expect(items.map((b) => b.textContent!.trim())).toEqual(['Rename', 'Delete']);
    items[1].click();
    fixture.detectChanges();
    expect(el.querySelector('[role="menu"]')).toBeNull();
    expect(host.log).toEqual(['delete']);
  });

  it('an unmanaged row has no menu', () => {
    const { fixture, el, host } = render();
    host.manageable.set(false);
    fixture.detectChanges();
    expect(el.querySelector('[aria-label="Folder actions for Arms"]')).toBeNull();
  });

  it('Enter renames; the input closes only when the rename stuck', async () => {
    const { fixture, el, host } = render();
    const input = await startRename(fixture, el);
    input.value = 'Weapons';
    input.dispatchEvent(new Event('input'));
    host.rename.mockResolvedValueOnce(false);
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(host.rename).toHaveBeenCalledWith('Weapons');
    expect(el.querySelector('input[aria-label="Rename folder Arms"]')).not.toBeNull();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(el.querySelector('input[aria-label="Rename folder Arms"]')).toBeNull();
  });

  it('a blank or unchanged name just closes; Escape restores; blur submits', async () => {
    const { fixture, el, host } = render();
    let input = await startRename(fixture, el);
    input.value = '  ';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(new Event('blur'));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(host.rename).not.toHaveBeenCalled();
    expect(el.querySelector('input[aria-label="Rename folder Arms"]')).toBeNull();

    input = await startRename(fixture, el);
    input.value = 'Other';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    fixture.detectChanges();
    expect(el.querySelector('input[aria-label="Rename folder Arms"]')).toBeNull();
    expect(host.rename).not.toHaveBeenCalled();

    input = await startRename(fixture, el);
    expect(input.value).toBe('Arms');
    input.value = 'Blades';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(new Event('blur'));
    await fixture.whenStable();
    expect(host.rename).toHaveBeenCalledWith('Blades');
  });
});
