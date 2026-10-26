import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { ItemFolderBar, NewFolderInput } from './item-folder-bar';

/** v4 `components/item-browser/ItemFolderBar.tsx` (`4fbdad186`) — the bar + `NewFolderInput`. */

@Component({
  imports: [ItemFolderBar, NewFolderInput],
  template: `
    <qt-item-folder-bar
      rootLabel="Inventory"
      navLabel="Inventory folders"
      testId="inventory-folder-bar"
      [searching]="searching()"
      [tierLabel]="tierLabel()"
      [currentFolder]="folder()"
      [managed]="managed()"
      [newFolderOpen]="newOpen()"
      [selectMode]="select()"
      (navigateRoot)="log.push('root')"
      (navigateTier)="log.push('tier')"
      (navigateFolder)="log.push('folder:' + $event)"
      (newFolder)="log.push('new')"
      (toggleSelect)="log.push('select')"
    />
    <qt-new-folder-input
      [name]="name()"
      (nameChange)="log.push('name:' + $event)"
      (submitted)="log.push('submit')"
      (cancelled)="log.push('cancel')"
    />
  `,
})
class Host {
  readonly searching = signal(false);
  readonly tierLabel = signal<string | null>(null);
  readonly folder = signal('');
  readonly managed = signal(true);
  readonly newOpen = signal(false);
  readonly select = signal(false);
  readonly name = signal('');
  readonly log: string[] = [];
}

function render() {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  const buttons = () => [...el.querySelectorAll('[data-testid="inventory-folder-bar"] button')] as HTMLButtonElement[];
  return { fixture, el, host: fixture.componentInstance, buttons };
}

describe('ItemFolderBar', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('roots the breadcrumb at rootLabel, names the nav, and marks the root current at the top', () => {
    const { el, buttons } = render();
    expect(el.querySelector('nav')!.getAttribute('aria-label')).toBe('Inventory folders');
    expect(buttons().map((b) => b.textContent!.trim())).toEqual(['Inventory', 'New folder', 'Select']);
    expect(buttons()[0].getAttribute('aria-current')).toBe('page');
  });

  it('crumbs each folder segment; the last is current and styled; each navigates to its path', () => {
    const { fixture, host, buttons } = render();
    host.folder.set('Arms/Bows');
    fixture.detectChanges();
    const crumbs = buttons().slice(0, 3);
    expect(crumbs.map((b) => b.textContent!.trim())).toEqual(['Inventory', 'Arms', 'Bows']);
    expect(crumbs[0].getAttribute('aria-current')).toBeNull();
    expect(crumbs[2].className).toBe('text-foreground font-medium');
    expect(crumbs[1].className).toBe('qt-text-secondary hover:text-foreground');
    crumbs[1].click();
    crumbs[0].click();
    expect(host.log).toEqual(['folder:Arms', 'root']);
  });

  it('inside a borrowed tier, the tier crumb follows the root and leads its folders', () => {
    const { fixture, host, buttons } = render();
    host.tierLabel.set('Project: The Quest');
    fixture.detectChanges();
    expect(buttons()[1].textContent!.trim()).toBe('Project: The Quest');
    expect(buttons()[0].getAttribute('aria-current')).toBeNull();
    expect(buttons()[1].getAttribute('aria-current')).toBe('page');
    buttons()[1].click();
    expect(host.log).toEqual(['tier']);
  });

  it('while searching it says so and drops New folder; unmanaged shows no controls', () => {
    const { fixture, host, el, buttons } = render();
    host.searching.set(true);
    fixture.detectChanges();
    expect(el.querySelector('nav')!.textContent!.trim()).toBe('Searching everywhere');
    expect(buttons().map((b) => b.textContent!.trim())).toEqual(['Select']);
    host.managed.set(false);
    fixture.detectChanges();
    expect(buttons()).toHaveLength(0);
  });

  it('New folder disables while the input is open; Select presses with select mode', () => {
    const { fixture, host, buttons } = render();
    host.newOpen.set(true);
    host.select.set(true);
    fixture.detectChanges();
    const [, newFolder, select] = buttons();
    expect(newFolder.disabled).toBe(true);
    expect(select.getAttribute('aria-pressed')).toBe('true');
    expect(select.classList.contains('qt-button-secondary')).toBe(true);
    select.click();
    expect(host.log).toEqual(['select']);
  });
});

describe('NewFolderInput', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('Enter submits, Escape stops the event and cancels, typing reports the name', () => {
    const { el, host } = render();
    const input = el.querySelector('input[aria-label="New folder name"]') as HTMLInputElement;
    expect(input.placeholder).toBe('Folder name — Enter to make it, Esc to think better of it');
    input.value = 'Arms';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true });
    let reachedDocument = false;
    const spy = () => (reachedDocument = true);
    document.addEventListener('keydown', spy);
    input.dispatchEvent(escape);
    document.removeEventListener('keydown', spy);
    expect(reachedDocument).toBe(false);
    expect(host.log).toEqual(['name:Arms', 'submit', 'cancel']);
  });
});
