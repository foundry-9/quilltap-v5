import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError, type ResolvedInventoryItem } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { BOW, LAMP, PROJECT, RING, SWORD, obj } from './__fixtures__/inventory-ui.fixture';
import type { InventoryContainer } from './inventory-container';
import { InventoryFolderPickerDialog } from './inventory-folder-picker-dialog';
import { InventoryInstantiateDialog } from './inventory-instantiate-dialog';
import type { InventoryItemActions } from './inventory-item-actions';
import { InventoryTransferDialog, catalogueGroups, type InventoryCatalogue } from './inventory-transfer-dialog';

/**
 * v4's three small Inventory dialogs (`322a5488e`): the folder picker wrapper
 * (`InventoryFolderPickerDialog.tsx`), the instantiate dialog both ways in
 * (`InventoryInstantiateDialog.tsx`) and the transfer dialog
 * (`InventoryTransferDialog.tsx`) — every title, note, label and toast.
 */

const VIV: InventoryContainer = { scope: 'character', id: 'c-viv' };
const CATALOGUE: InventoryCatalogue = {
  characters: [{ id: 'c-bert', name: 'Bertie' }, { id: 'c-viv', name: 'Vivienne' }],
  projects: [{ id: 'p-1', name: 'The Quest' }],
  groups: [],
};

const seen: Array<Record<string, unknown>> = [];
let answer: (req: Record<string, unknown>) => unknown = () => ({});
const refresh = vi.fn(async () => undefined);
const actions = { refresh } as unknown as InventoryItemActions;

function configure(host: unknown): void {
  seen.length = 0;
  TestBed.configureTestingModule({
    imports: [host as never],
    providers: [
      {
        provide: CoreClient,
        useValue: {
          dispatchData: vi.fn(async (req: Record<string, unknown>) => {
            seen.push(req);
            const out = answer(req);
            if (out instanceof Error) throw out;
            return out;
          }),
        },
      },
    ],
  });
}

async function flush<T>(fixture: ComponentFixture<T>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

const toasts = () => TestBed.inject(ToastService).toasts().map((t) => t.message);
const footer = (el: HTMLElement, text: string) =>
  [...el.querySelectorAll('[qt-modal-footer] button')].find((b) => b.textContent!.trim() === text) as HTMLButtonElement;
const choose = (select: HTMLSelectElement, value: string) => {
  select.value = value;
  select.dispatchEvent(new Event('change'));
};

describe('catalogueGroups', () => {
  it('characters, General, projects, groups — empty groups dropped bar General; the source omitted', () => {
    expect(catalogueGroups(CATALOGUE, 'character:c-viv')).toEqual([
      { label: 'Characters', options: [{ value: 'character:c-bert', name: 'Bertie' }] },
      { label: 'General', options: [{ value: 'general:', name: 'Quilltap General' }] },
      { label: 'Projects', options: [{ value: 'project:p-1', name: 'The Quest' }] },
    ]);
    expect(catalogueGroups({ characters: [], projects: [], groups: [] }, 'general:')).toEqual([
      { label: 'General', options: [] },
    ]);
  });
});

@Component({
  imports: [InventoryFolderPickerDialog],
  template: `<qt-inventory-folder-picker-dialog [container]="container" [items]="items()" [actions]="actions" (closed)="closes = closes + 1" />`,
})
class PickerHost {
  readonly container = VIV;
  readonly items = signal<ResolvedInventoryItem[]>([LAMP]);
  readonly actions = actions;
  closes = 0;
}

describe('InventoryFolderPickerDialog (v4 InventoryFolderPickerDialog.tsx)', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    answer = () => ({});
  });

  it('a move into a folder: the root label, the note, the sits toast, the close', async () => {
    answer = (req) => (req['type'] === 'inventoryFolders' ? { folders: [{ path: 'Lights', name: 'Lights', itemCount: 0 }] } : { items: [] });
    configure(PickerHost);
    const fixture = TestBed.createComponent(PickerHost);
    fixture.detectChanges();
    await flush(fixture);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Move "lantern" to a folder');
    expect(el.textContent).toContain('Folders are for your browsing; what anyone carries stays exactly as it was.');
    const options = [...el.querySelectorAll('[role="option"]')] as HTMLButtonElement[];
    expect(options.map((o) => o.textContent!.replace(/\s+/g, ' ').trim())).toEqual(['Inventory (top level) (here now)', 'Lights']);
    options[1].click();
    fixture.detectChanges();
    footer(el, 'Move').click();
    await flush(fixture);
    expect(seen.at(-1)).toEqual({ type: 'inventoryItemsMove', container: VIV, itemIds: ['lamp'], folder: 'Lights' });
    expect(toasts().at(-1)).toBe('"lantern" now sits in "Lights"');
    expect(fixture.componentInstance.closes).toBe(1);
  });

  it('several objects to the top; a refusal is worded', async () => {
    answer = (req) =>
      req['type'] === 'inventoryFolders'
        ? { folders: [] }
        : new CoreDispatchError({ kind: 'bad-request', message: 'A folder named that is gone' });
    configure(PickerHost);
    const fixture = TestBed.createComponent(PickerHost);
    fixture.componentInstance.items.set([obj('a', { folder: 'X' }), obj('b', { folder: 'Y' })]);
    fixture.detectChanges();
    await flush(fixture);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Move 2 objects to a folder');
    (el.querySelector('[role="option"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    footer(el, 'Move').click();
    await flush(fixture);
    expect(toasts().at(-1)).toBe('A folder named that is gone');
  });
});

@Component({
  imports: [InventoryTransferDialog],
  template: `<qt-inventory-transfer-dialog [item]="item()" [mode]="mode()" [from]="from" [catalogue]="catalogue" [actions]="actions" (closed)="closes = closes + 1" />`,
})
class TransferHost {
  readonly item = signal<ResolvedInventoryItem>(SWORD);
  readonly mode = signal<'move' | 'copy'>('move');
  readonly from = VIV;
  readonly catalogue = CATALOGUE;
  readonly actions = actions;
  closes = 0;
}

describe('InventoryTransferDialog (v4 InventoryTransferDialog.tsx)', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    answer = () => ({});
  });

  async function mount(setup: (h: TransferHost) => void = () => {}) {
    configure(TransferHost);
    const fixture = TestBed.createComponent(TransferHost);
    setup(fixture.componentInstance);
    fixture.detectChanges();
    await flush(fixture);
    return { fixture, el: fixture.nativeElement as HTMLElement };
  }

  it('a move: title, note, the source left out, the changed-hands toast', async () => {
    answer = () => ({ item: SWORD });
    const { fixture, el } = await mount();
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Move "Excalibur" to another inventory');
    expect(el.textContent).toContain(
      'The object keeps its identity: chats that carry it, and kits that name it, still do. Its folder comes along.',
    );
    const select = el.querySelector('#inventory-transfer-target') as HTMLSelectElement;
    expect([...select.querySelectorAll('option')].map((o) => o.textContent!.trim())).toEqual([
      'Choose an inventory',
      'Bertie',
      'Quilltap General',
      'The Quest',
    ]);
    expect(footer(el, 'Move').disabled).toBe(true);
    choose(select, 'general:');
    fixture.detectChanges();
    footer(el, 'Move').click();
    await flush(fixture);
    expect(seen).toEqual([{ type: 'inventoryTransfer', action: 'move', itemId: 'sword', from: VIV, to: { scope: 'general', id: null } }]);
    expect(toasts().at(-1)).toBe('"Excalibur" has changed hands');
    expect(fixture.componentInstance.closes).toBe(1);
    expect(refresh).toHaveBeenCalled();
  });

  it('a copy of an instance: its note tail, its toast; a refusal says why in words', async () => {
    const { fixture, el } = await mount((h) => {
      h.mode.set('copy');
      h.item.set(obj('i-1', { name: 'magic bow', parentId: 'bow' }));
    });
    expect(el.textContent).toContain(
      'The copy is a new object with its own identity, filed in the same folder at the destination. An instance stays an instance of its archetype wherever it goes.',
    );
    answer = () => new CoreDispatchError({ kind: 'conflict', message: 'x', code: 'NOT_COPYABLE', details: { itemIds: ['i-1'] } });
    choose(el.querySelector('#inventory-transfer-target') as HTMLSelectElement, 'project:p-1');
    fixture.detectChanges();
    footer(el, 'Copy').click();
    await flush(fixture);
    expect(toasts().at(-1)).toBe(
      'That object is one of a kind and cannot be copied — tick "Can be copied" in its editor first, or move it instead.',
    );
    answer = () => ({ item: {} });
    footer(el, 'Copy').click();
    await flush(fixture);
    expect(toasts().at(-1)).toBe('A copy of "magic bow" is on its way');
  });
});

@Component({
  imports: [InventoryInstantiateDialog],
  template: `
    <qt-inventory-instantiate-dialog
      [destination]="destination()"
      [archetype]="archetype()"
      [projectId]="projectId()"
      [catalogue]="catalogue"
      [actions]="actions"
      (closed)="closes = closes + 1"
    />
  `,
})
class InstantiateHost {
  readonly destination = signal<{ container: InventoryContainer; folder: string } | null>(null);
  readonly archetype = signal<ResolvedInventoryItem | null>(null);
  readonly projectId = signal<string | null>(null);
  readonly catalogue = CATALOGUE;
  readonly actions = actions;
  closes = 0;
}

describe('InventoryInstantiateDialog (v4 InventoryInstantiateDialog.tsx)', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    answer = () => ({});
  });

  async function mount(setup: (h: InstantiateHost) => void) {
    configure(InstantiateHost);
    const fixture = TestBed.createComponent(InstantiateHost);
    setup(fixture.componentInstance);
    fixture.detectChanges();
    await flush(fixture);
    return { fixture, el: fixture.nativeElement as HTMLElement };
  }

  it('archetype-first: Into a tier, the project named from the archetype’s origin, the made toast', async () => {
    answer = () => ({ item: obj('new', { name: 'magic bow', title: 'Heartseeker' }) });
    const { fixture, el } = await mount((h) => h.archetype.set(BOW));
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Instantiate "magic bow"');
    expect(el.textContent).toContain(
      'The instance follows its archetype — later edits reach it — except for whatever you override on it.',
    );
    expect(el.textContent).toContain(
      'The destination must be able to reach the archetype (a character reaches their own vault, their groups, a named project and General).',
    );
    expect(footer(el, 'Instantiate').disabled).toBe(true);
    choose(el.querySelector('#inventory-instantiate-target') as HTMLSelectElement, 'character:c-viv');
    fixture.detectChanges();
    footer(el, 'Instantiate').click();
    await flush(fixture);
    expect(seen).toEqual([{ type: 'inventoryInstantiate', container: VIV, archetypeId: 'bow', projectId: PROJECT.id }]);
    expect(toasts().at(-1)).toBe('"Heartseeker" is made from its archetype');
    expect(fixture.componentInstance.closes).toBe(1);
  });

  it('destination-first: the archetypes this tier reaches, chosen, filed in the folder', async () => {
    answer = (req) => (req['type'] === 'inventoryArchetypes' ? { items: [BOW, obj('arch2', { name: 'cloak', isArchetype: true, description: 'Warm.' })] } : { item: RING });
    const { fixture, el } = await mount((h) => h.destination.set({ container: VIV, folder: 'Arms' }));
    expect(seen[0]).toEqual({ type: 'inventoryArchetypes', container: VIV });
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Make an object from an archetype');
    const options = [...el.querySelectorAll('[aria-label="Archetypes"] [role="option"]')] as HTMLButtonElement[];
    expect(options.map((o) => o.querySelector('.text-sm')!.textContent)).toEqual(['magic bow', 'cloak']);
    options[1].click();
    fixture.detectChanges();
    expect(options[1].getAttribute('aria-selected')).toBe('true');
    footer(el, 'Instantiate').click();
    await flush(fixture);
    expect(seen.at(-1)).toEqual({ type: 'inventoryInstantiate', container: VIV, archetypeId: 'arch2', folder: 'Arms' });
  });

  it('Looking for archetypes… while loading; none within reach', async () => {
    let release!: (v: unknown) => void;
    answer = () => new Promise((r) => (release = r));
    const { fixture, el } = await mount((h) => h.destination.set({ container: VIV, folder: '' }));
    expect(el.textContent).toContain('Looking for archetypes…');
    release({ items: [] });
    await flush(fixture);
    expect(el.textContent).toContain('No archetypes are within reach of this inventory yet.');
  });
});
