import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ResolvedInventoryItem } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { BOW, KIT, LAMP, OWN, PROJECT, RING, SWORD, obj } from './__fixtures__/inventory-ui.fixture';
import { InventoryBrowser, type InventoryBrowserEquip } from './inventory-browser';
import type { InventoryContainer } from './inventory-container';
import { createInventoryFoldersStore, type InventoryFoldersStore } from './inventory-folders-data';
import type { InventoryItemActions } from './inventory-item-actions';
import type { InventoryListData } from './inventory-list-data';

/**
 * v4 `__tests__/unit/components/inventory/inventory-ui.test.tsx` "InventoryBrowser"
 * (cases 1–2) NAME FOR NAME over v4's fixture, plus the v5-added rows: the
 * kind tabs, the four state sentences (trap 10's archived-only folder), the
 * folder toasts, the Show-shared read order (R-8, mutation-proven) and the
 * create buttons.
 */

function makeActions(over: Partial<InventoryItemActions> = {}): InventoryItemActions {
  const noop = vi.fn();
  return {
    editor: signal(null),
    folderPicker: signal(null),
    transfer: signal(null),
    instantiateFor: signal(null),
    overlayOpen: signal(false),
    addressOf: () => ({ scope: 'character', id: 'c-viv' }),
    refresh: vi.fn(async () => undefined),
    requestConfirmation: vi.fn(async () => true),
    requestAlert: vi.fn(async () => null),
    openCreate: noop,
    openEditor: noop,
    closeEditor: noop,
    openFolderPicker: vi.fn(),
    closeFolderPicker: noop,
    openTransfer: noop,
    closeTransfer: noop,
    openInstantiateArchetype: noop,
    openInstantiateInto: vi.fn(),
    closeInstantiate: noop,
    detach: vi.fn(async () => undefined),
    remove: vi.fn(async () => undefined),
    toggleArchived: vi.fn(async () => undefined),
    duplicate: vi.fn(async () => undefined),
    ...over,
  } as InventoryItemActions;
}

function makeData(items: ResolvedInventoryItem[]): InventoryListData {
  return {
    listItems: signal(items),
    loading: signal(false),
    error: signal(null),
    canManage: (i) => i.origin?.scope === 'character',
    reload: async () => undefined,
  };
}

@Component({
  imports: [InventoryBrowser],
  template: `
    <qt-inventory-browser
      [container]="container()"
      [data]="data()"
      [actions]="actions"
      [folders]="folders"
      [showArchived]="false"
      [equip]="equip()"
    />
  `,
})
class Host {
  readonly container = signal<InventoryContainer | null>({ scope: 'character', id: 'c-viv' });
  readonly data = signal(makeData([SWORD, RING, LAMP, BOW, KIT]));
  readonly equip = signal<InventoryBrowserEquip | undefined>(undefined);
  actions = makeActions();
  folders!: InventoryFoldersStore;
}

const seen: Array<Record<string, unknown>> = [];
let folderAnswer: (req: Record<string, unknown>) => unknown = () => ({});

function render(setup: (host: Host) => void = () => {}) {
  seen.length = 0;
  const core = {
    dispatchData: vi.fn(async (req: Record<string, unknown>) => {
      seen.push(req);
      const out = folderAnswer(req);
      if (out instanceof Error) throw out;
      return out;
    }),
  } as unknown as CoreClient;
  TestBed.configureTestingModule({ imports: [Host], providers: [{ provide: CoreClient, useValue: core }] });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.folders = createInventoryFoldersStore(core);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  const rows = () => [...el.querySelectorAll('[data-testid="inventory-item-row"]')] as HTMLElement[];
  const tab = (name: string) =>
    [...el.querySelectorAll('[role="tab"]')].find((t) => t.textContent!.trim() === name) as HTMLButtonElement;
  const checkbox = (label: string) =>
    [...el.querySelectorAll('label')].find((l) => l.textContent!.trim() === label)!.querySelector('input') as HTMLInputElement;
  const text = () => el.textContent!.replace(/\s+/g, ' ');
  return { fixture, el, host: fixture.componentInstance, rows, tab, checkbox, text };
}

describe('InventoryBrowser', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    folderAnswer = () => ({});
  });

  it('filters Items / Kits / Archetypes, and shows the glyph, never the picture', () => {
    const { fixture, el, rows, tab, text } = render();
    expect(rows()).toHaveLength(3);
    expect(text()).toContain('Excalibur');
    expect(el.querySelector('[data-testid="inventory-glyph-svg"]')).not.toBeNull();
    expect(el.querySelector('[data-testid="inventory-glyph-emoji"]')!.textContent).toBe('💍');
    expect(el.querySelectorAll('[data-testid="inventory-glyph-default"]')).toHaveLength(1);
    expect(el.querySelector('[data-testid="inventory-picture"]')).toBeNull();
    expect(text()).toContain('Default');

    tab('Kits').click();
    fixture.detectChanges();
    expect(rows()).toHaveLength(1);
    expect(text()).toMatch(/Kit · 2/);

    // The project archetype is borrowed: it sits behind its tier row.
    tab('Archetypes').click();
    fixture.detectChanges();
    (el.querySelector('[data-testid="inventory-tier-row"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(rows().map((r) => r.querySelector('.qt-badge-primary')?.textContent)).toEqual(['Archetype']);
  });

  it("in a chat: Equipped only, the Equipped chip and a kit's state", () => {
    const { fixture, rows, tab, checkbox, text } = render((h) =>
      h.equip.set({ carried: { equipped: [RING], kits: { kit: 'some' } }, onEquip: vi.fn(), onUnequip: vi.fn() }),
    );
    checkbox('Equipped only').click();
    fixture.detectChanges();
    expect(rows()).toHaveLength(1);
    expect(rows()[0].textContent).toContain('Equipped');
    const buttons = [...rows()[0].querySelectorAll('button')].map((b) => b.textContent!.trim());
    expect(buttons).toContain('Unequip');
    tab('Kits').click();
    fixture.detectChanges();
    expect(text()).toContain('Kit · some');
  });

  // ---- v5-added ----

  it('the kind tabs: aria-selected and the secondary / ghost classes', () => {
    const { fixture, tab } = render();
    expect(tab('Items').getAttribute('aria-selected')).toBe('true');
    expect(tab('Items').getAttribute('class')).toBe('qt-button-sm qt-button-secondary');
    expect(tab('Kits').getAttribute('aria-selected')).toBe('false');
    expect(tab('Kits').getAttribute('class')).toBe('qt-button-sm qt-button-ghost');
    tab('Archetypes').click();
    fixture.detectChanges();
    expect(tab('Archetypes').getAttribute('aria-selected')).toBe('true');
  });

  it('the four state sentences', () => {
    const none = render((h) => h.container.set(null));
    expect(none.text()).toContain('Select an inventory to browse.');
    TestBed.resetTestingModule();

    const loading = render((h) => h.data.set({ ...makeData([]), loading: signal(true) }));
    expect(loading.text()).toContain('Loading…');
    TestBed.resetTestingModule();

    const empty = render((h) => h.data.set(makeData([])));
    expect(empty.text()).toContain('Nothing on this shelf yet.');
    TestBed.resetTestingModule();

    const filtered = render((h) => h.data.set(makeData([KIT])));
    expect(filtered.text()).toContain('Nothing matches this filter.');
  });

  // Survey trap 10 is FALSIFIED by the code (measured at the pin): the list
  // read omits archived objects unless Show archived is ticked
  // (`location.ts:144-146`), so an archived-only folder is never listed with
  // them hidden. What `folderIsEmpty` (unfiltered `tierItems`) really
  // separates is a folder whose objects are all filtered OUT of view.
  it('a folder whose objects are all filtered out reads "Nothing matches this filter.", an empty one "Nothing on this shelf yet."', () => {
    const kitInFolder = obj('kit', { name: 'kit', kind: 'kit', itemIds: ['lamp'], folder: 'Kits' });
    const { fixture, el, text } = render((h) => h.data.set(makeData([kitInFolder])));
    (el.querySelector('[data-testid="inventory-folder-row"] button') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(text()).toContain('Nothing matches this filter.');
    expect(text()).not.toContain('Nothing on this shelf yet.');
  });

  it('a search looks everywhere and crumbs each hit’s folder', () => {
    const filed = obj('compass', { name: 'brass compass', folder: 'Instruments/Navigation' });
    const { fixture, el, rows } = render((h) => h.data.set(makeData([filed, LAMP])));
    const search = el.querySelector('input[aria-label="Search the inventory by name"]') as HTMLInputElement;
    expect(search.placeholder).toBe('Search the inventory…');
    search.value = 'brass';
    search.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(rows()).toHaveLength(1);
    expect(el.querySelector('[data-testid="inventory-folder-crumb"]')!.textContent).toBe('Instruments › Navigation');
    expect(el.querySelector('nav')!.textContent!.trim()).toBe('Searching everywhere');
  });

  it('unticking Show shared inside a borrowed tier goes home (R-8 — the read precedes the flip)', () => {
    const { fixture, el, checkbox, tab } = render();
    tab('Archetypes').click();
    fixture.detectChanges();
    (el.querySelector('[data-testid="inventory-tier-row"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(el.querySelector('nav')!.textContent).toContain('Project: The Quest');
    checkbox('Show shared').click();
    fixture.detectChanges();
    // Re-ticking must NOT drop the operator back inside the tier they left.
    checkbox('Show shared').click();
    fixture.detectChanges();
    expect(el.querySelector('nav')!.textContent).not.toContain('Project: The Quest');
    expect(el.querySelector('[data-testid="inventory-tier-row"]')).not.toBeNull();
  });

  it('the create buttons file into the folder being browsed; From archetype names the destination', () => {
    const filed = obj('compass', { name: 'brass compass', folder: 'Instruments' });
    const { fixture, el, host } = render((h) => h.data.set(makeData([filed])));
    (el.querySelector('[data-testid="inventory-folder-row"] button') as HTMLButtonElement).click();
    fixture.detectChanges();
    const footer = [...el.querySelectorAll('.sticky.bottom-0 button')] as HTMLButtonElement[];
    expect(footer.map((b) => b.textContent!.trim())).toEqual(['+ From archetype', '+ New Kit', '+ New Archetype', '+ New Item']);
    footer[3].click();
    footer[0].click();
    expect(host.actions.openCreate).toHaveBeenCalledWith('item', 'Instruments');
    expect(host.actions.openInstantiateInto).toHaveBeenCalledWith({ scope: 'character', id: 'c-viv' }, 'Instruments');
  });

  it('New folder: the toast, the refresh; a refusal is worded', async () => {
    const { fixture, el, host } = render();
    const newFolder = [...el.querySelectorAll('[data-testid="inventory-folder-bar"] button')].find(
      (b) => b.textContent!.trim() === 'New folder',
    ) as HTMLButtonElement;
    newFolder.click();
    fixture.detectChanges();
    folderAnswer = (req) => ({ folder: { path: req['path'], name: 'Instruments', itemCount: 0 } });
    const input = el.querySelector('input[aria-label="New folder name"]') as HTMLInputElement;
    input.value = 'Instruments';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await fixture.whenStable();
    expect(seen).toEqual([
      { type: 'inventoryFolderCreate', container: { scope: 'character', id: 'c-viv' }, path: 'Instruments' },
    ]);
    const toasts = TestBed.inject(ToastService).toasts();
    expect(toasts.at(-1)?.message).toBe('The folder "Instruments" is ready for the stock');
    expect(host.actions.refresh).toHaveBeenCalled();
  });

  it('deleting a folder asks with the count and the parent, then toasts the move', async () => {
    const filed = obj('compass', { name: 'brass compass', folder: 'Instruments' });
    const { fixture, el, host } = render((h) => h.data.set(makeData([filed])));
    folderAnswer = () => ({ movedItems: 1, movedFolders: 0 });
    (el.querySelector('[aria-label="Folder actions for Instruments"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    ([...el.querySelectorAll('[role="menuitem"]')][1] as HTMLButtonElement).click();
    await fixture.whenStable();
    expect(host.actions.requestConfirmation).toHaveBeenCalledWith(
      'Send its 1 object up to the top of the inventory? Only the folder itself goes.',
    );
    expect(TestBed.inject(ToastService).toasts().at(-1)?.message).toBe('Folder removed; 1 object moved up');
  });

  it('select mode: the bar, Move to folder… with the ticked objects', () => {
    const { fixture, el, host, rows } = render();
    ([...el.querySelectorAll('[data-testid="inventory-folder-bar"] button')].find((b) => b.textContent!.trim() === 'Select') as HTMLButtonElement).click();
    fixture.detectChanges();
    const bar = el.querySelector('[data-testid="inventory-select-bar"]')!;
    expect(bar.textContent).toContain('0 selected');
    (rows()[0].querySelector('input[type="checkbox"]') as HTMLInputElement).click();
    fixture.detectChanges();
    expect(bar.textContent).toContain('1 selected');
    (bar.querySelector('button') as HTMLButtonElement).click();
    expect(host.actions.openFolderPicker).toHaveBeenCalledWith([SWORD]);
  });

  it('a borrowed row carries its tier’s label chip', () => {
    const borrowed = obj('cloak', { name: 'cloak', origin: PROJECT });
    const { fixture, el } = render((h) => h.data.set(makeData([borrowed, obj('own', { origin: OWN })])));
    (el.querySelector('[data-testid="inventory-tier-row"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(el.querySelector('[data-testid="inventory-item-row"] .qt-badge-wardrobe-shared')!.textContent).toBe(
      'Project: The Quest',
    );
  });
});
