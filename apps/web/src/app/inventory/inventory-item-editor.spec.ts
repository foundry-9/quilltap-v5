import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ResolvedInventoryItem } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { BOW, KIT, LAMP, RING, SWORD, obj } from './__fixtures__/inventory-ui.fixture';
import type { InventoryContainer } from './inventory-container';
import type { InventoryItemActions } from './inventory-item-actions';
import { IMAGE_DESCRIPTION_HINT, InventoryItemEditor, draftOf } from './inventory-item-editor';

/**
 * v4 `inventory-ui.test.tsx` "the instance editor" (case 3) NAME FOR NAME,
 * plus the v5-added rows: `buildBody`'s create shapes, the name and 16 KB
 * refusals, the metadata refusal, the size counter, the SVG preview gate, the
 * member picker's exclusions, and `d88c1ce39`'s hint byte for byte.
 */

const VIV: InventoryContainer = { scope: 'character', id: 'c-viv' };

@Component({
  imports: [InventoryItemEditor],
  template: `
    <qt-inventory-item-editor
      [container]="container()"
      containerLabel="Vivienne's inventory"
      [item]="item()"
      [createMode]="createMode()"
      initialFolder="Arms"
      [poolItems]="pool()"
      [actions]="actions"
      (closed)="closes.set(closes() + 1)"
      (saved)="saves.set(saves() + 1)"
    />
  `,
})
class Host {
  readonly container = signal<InventoryContainer>(VIV);
  readonly item = signal<ResolvedInventoryItem | null>(null);
  readonly createMode = signal<'item' | 'kit' | 'archetype'>('item');
  readonly pool = signal<ResolvedInventoryItem[]>([BOW]);
  readonly closes = signal(0);
  readonly saves = signal(0);
  readonly actions = { refresh: vi.fn(async () => undefined) } as unknown as InventoryItemActions;
}

const seen: Array<Record<string, unknown>> = [];

async function render(setup: (h: Host) => void = () => {}): Promise<{
  fixture: ComponentFixture<Host>;
  el: HTMLElement;
  host: Host;
  editor: InventoryItemEditor;
}> {
  seen.length = 0;
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      {
        provide: CoreClient,
        useValue: {
          dispatchData: vi.fn(async (req: Record<string, unknown>) => {
            seen.push(req);
            if (req['type'] === 'inventoryList') return { items: [obj('gen-lamp', { name: 'general lamp', origin: { scope: 'general', id: null, name: 'Quilltap General' } }), LAMP] };
            if (req['type'] === 'inventoryItemImagesList') return { current: null, images: [] };
            return { item: {} };
          }),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  const el = fixture.nativeElement as HTMLElement;
  const editor = fixture.debugElement.children[0].componentInstance as InventoryItemEditor;
  return { fixture, el, host: fixture.componentInstance, editor };
}

const byLabel = (el: HTMLElement, label: string): HTMLInputElement | HTMLTextAreaElement => {
  const aria = el.querySelector(`[aria-label="${label}"]`);
  if (aria) return aria as HTMLInputElement;
  const lab = [...el.querySelectorAll('label')].find((l) => l.textContent!.trim() === label)!;
  return el.querySelector(`#${lab.getAttribute('for')}`) as HTMLInputElement;
};
const type = (input: HTMLInputElement | HTMLTextAreaElement, value: string) => {
  input.value = value;
  input.dispatchEvent(new Event('input'));
};
const clickSave = (el: HTMLElement) =>
  ([...el.querySelectorAll('[qt-modal-footer] button')].find((b) => b.textContent!.trim() === 'Save') as HTMLButtonElement).click();
const toasts = () => TestBed.inject(ToastService).toasts().map((t) => t.message);

describe('the instance editor', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows the archetype value greyed under Inherit; unticking makes it an override, re-ticking clears it', async () => {
    const instance = obj('i-1', {
      parentId: 'bow',
      name: 'magic bow',
      description: 'Never misses.',
      overriddenFields: ['title'],
      title: 'Heartseeker',
      parentName: 'magic bow',
    });
    const { fixture, el } = await render((h) => h.item.set(instance));
    const description = byLabel(el, 'What the characters know about it') as HTMLTextAreaElement;
    expect(description.value).toBe('Never misses.');
    expect((byLabel(el, 'Inherit description') as HTMLInputElement).checked).toBe(true);
    expect((byLabel(el, 'Inherit title') as HTMLInputElement).checked).toBe(false);

    // Untick: the archetype's words become this instance's own, to edit.
    (byLabel(el, 'Inherit description') as HTMLInputElement).click();
    fixture.detectChanges();
    type(byLabel(el, 'What the characters know about it'), 'Mine now.');
    fixture.detectChanges();
    // Re-tick the title: its override is cleared on save.
    (byLabel(el, 'Inherit title') as HTMLInputElement).click();
    fixture.detectChanges();
    clickSave(el);
    await fixture.whenStable();

    expect(seen.filter((r) => r['type'] !== 'inventoryItemImagesList')).toEqual([
      { type: 'inventoryUpdate', container: VIV, itemId: 'i-1', body: { folder: '', description: 'Mine now.' } },
      { type: 'inventoryClearOverride', container: VIV, itemId: 'i-1', fields: ['title'] },
    ]);
  });
});

describe('the editor (v5-added)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('an instance field edited and then re-ticked to Inherit is NOT sent; nothing to clear when it was never overridden', async () => {
    const instance = obj('i-1', { parentId: 'bow', name: 'magic bow', parentName: 'magic bow', overriddenFields: [] });
    const { fixture, el } = await render((h) => h.item.set(instance));
    expect((byLabel(el, 'Inherit emoji') as HTMLInputElement).checked).toBe(true);
    type(byLabel(el, 'Emoji glyph'), '🏹');
    fixture.detectChanges();
    expect((byLabel(el, 'Inherit emoji') as HTMLInputElement).checked).toBe(false);
    (byLabel(el, 'Inherit emoji') as HTMLInputElement).click();
    fixture.detectChanges();
    clickSave(el);
    await fixture.whenStable();
    expect(seen.filter((r) => r['type'] !== 'inventoryItemImagesList')).toEqual([
      { type: 'inventoryUpdate', container: VIV, itemId: 'i-1', body: { folder: '' } },
    ]);
  });

  it('buildBody: a new item, an archetype and a kit — the whole create shape', async () => {
    const item = await render();
    item.editor['draft'].set({ ...draftOf(null, 'item', 'Arms'), name: ' sword ', title: '  ', emoji: '🗡️', maxEquipped: 2.6 });
    expect(item.editor.buildBody()).toEqual({
      name: 'sword',
      title: null,
      description: null,
      imageDescription: null,
      equippedByDefault: false,
      maxEquipped: 3,
      canCopy: false,
      emoji: '🗡️',
      svg: null,
      metadata: {},
      folder: 'Arms',
      kind: 'item',
      isArchetype: false,
      itemIds: [],
    });
    TestBed.resetTestingModule();

    const arch = await render((h) => h.createMode.set('archetype'));
    arch.editor['draft'].update((d) => ({ ...d, name: 'bow', maxEquipped: 0 }));
    expect(arch.editor.buildBody()).toMatchObject({ kind: 'item', isArchetype: true, itemIds: [], maxEquipped: 1 });
    TestBed.resetTestingModule();

    const kit = await render((h) => h.createMode.set('kit'));
    kit.editor['draft'].update((d) => ({ ...d, name: 'kit', isArchetype: true, itemIds: ['bow'], maxEquipped: 5000 }));
    expect(kit.editor.buildBody()).toMatchObject({ kind: 'kit', isArchetype: false, itemIds: ['bow'], maxEquipped: 999 });
  });

  it('an edit of a standalone sends the full body; a kit adds its members, an item its archetype flag', async () => {
    const plain = await render((h) => h.item.set(SWORD));
    expect(plain.editor.buildBody()).toMatchObject({ name: 'sword', title: 'Excalibur', isArchetype: false });
    expect(plain.editor.buildBody()).not.toHaveProperty('itemIds');
    TestBed.resetTestingModule();
    const kit = await render((h) => h.item.set(KIT));
    expect(kit.editor.buildBody()).toMatchObject({ itemIds: ['bow', 'lamp'] });
    expect(kit.editor.buildBody()).not.toHaveProperty('isArchetype');
  });

  it('refuses a nameless object, an SVG over 16 KB, and metadata that is not a JSON object', async () => {
    const { fixture, el, editor } = await render();
    clickSave(el);
    expect(toasts().at(-1)).toBe('An object needs a name');
    type(byLabel(el, 'Name'), 'sword');
    type(byLabel(el, 'SVG glyph'), `<svg>${'x'.repeat(16 * 1024)}</svg>`);
    fixture.detectChanges();
    clickSave(el);
    expect(toasts().at(-1)).toBe('An SVG glyph may be at most 16 KB');
    type(byLabel(el, 'SVG glyph'), '');
    for (const bad of ['[1, 2]', '"text"', '{ nope']) {
      type(byLabel(el, 'Metadata'), bad);
      expect(editor.buildBody()).toBeNull();
      expect(toasts().at(-1)).toBe('Metadata must be a JSON object');
    }
    expect(seen.filter((r) => r['type'] === 'inventoryCreate')).toEqual([]);
  });

  it('a create files where it was opened and toasts "is laid on the table"', async () => {
    const { fixture, el, host } = await render();
    type(byLabel(el, 'Name'), 'compass');
    type(byLabel(el, 'Title'), 'Brass Compass');
    fixture.detectChanges();
    clickSave(el);
    await fixture.whenStable();
    expect(seen.at(-1)).toMatchObject({ type: 'inventoryCreate', container: VIV, body: { name: 'compass', folder: 'Arms' } });
    expect(toasts().at(-1)).toBe('"Brass Compass" is laid on the table');
    expect(host.saves()).toBe(1);
  });

  it('the SVG counter reads x.x / 16 KB and turns destructive when over; the preview only for an <svg', async () => {
    const { fixture, el } = await render();
    const counter = () => el.querySelector('#inventory-svg')!.parentElement!.querySelector('span.qt-text-xs') as HTMLElement;
    expect(counter().textContent).toBe('0.0 / 16 KB');
    expect(counter().getAttribute('class')).toBe('qt-text-xs qt-text-secondary');
    type(byLabel(el, 'SVG glyph'), 'not an svg');
    fixture.detectChanges();
    expect(el.querySelector('#inventory-svg')!.parentElement!.querySelector('img')).toBeNull();
    type(byLabel(el, 'SVG glyph'), ' <svg viewBox="0 0 1 1"/>');
    fixture.detectChanges();
    expect(el.querySelector('#inventory-svg')!.parentElement!.querySelector('img')).not.toBeNull();
    type(byLabel(el, 'SVG glyph'), `<svg>${'x'.repeat(17 * 1024)}</svg>`);
    fixture.detectChanges();
    expect(counter().textContent).toBe('17.0 / 16 KB');
    expect(counter().getAttribute('class')).toBe('qt-text-xs qt-text-destructive');
  });

  it('the kit member picker excludes kits, archived objects and duplicates (first wins); a project kit adds General’s', async () => {
    const archived = obj('old', { archivedAt: '2026-10-01T00:00:00.000Z' });
    const dupe = obj('lamp', { name: 'second lamp' });
    const { el } = await render((h) => {
      h.createMode.set('kit');
      h.container.set({ scope: 'project', id: 'p-1' });
      h.pool.set([LAMP, KIT, archived, dupe, BOW]);
    });
    const names = [...el.querySelectorAll('[aria-label="Kit members"] li span.truncate')].map((s) => s.textContent);
    expect(names).toEqual(['lantern', 'magic bow', 'general lamp']);
    expect(seen.find((r) => r['type'] === 'inventoryList')).toEqual({
      type: 'inventoryList',
      container: { scope: 'general', id: null },
    });
  });

  it('a character kit reads no General extra; an empty pool says so', async () => {
    const { el } = await render((h) => {
      h.createMode.set('kit');
      h.pool.set([]);
    });
    expect(seen.filter((r) => r['type'] === 'inventoryList')).toEqual([]);
    expect(el.querySelector('[aria-label="Kit members"]')!.textContent).toContain('Nothing here to put in a kit yet.');
  });

  it('carries every label, placeholder and hint — d88c1ce39’s drawing hint byte for byte', async () => {
    const { el } = await render();
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('New object');
    expect(el.textContent).toContain('Will be kept in Vivienne\'s inventory');
    const hints = [...el.querySelectorAll('p.qt-text-xs.qt-text-secondary')].map((p) => p.textContent!.trim());
    expect(hints).toContain(IMAGE_DESCRIPTION_HINT);
    expect(IMAGE_DESCRIPTION_HINT).toBe(
      "Rendering hints joined to the character's appearance while carried, and the subject of the object's own picture. Leave empty and the object is invisible in avatars, story backgrounds and every other picture of the character (its own picture falls back to the description above).",
    );
    expect((byLabel(el, 'Name') as HTMLInputElement).placeholder).toBe('sword');
    expect((byLabel(el, 'Title') as HTMLInputElement).placeholder).toBe('Excalibur');
    expect((byLabel(el, 'Emoji glyph') as HTMLInputElement).placeholder).toBe('🗡️');
    expect((byLabel(el, 'Folder') as HTMLInputElement).placeholder).toBe('Weapons/Edged');
    expect(el.textContent).toContain('at a time, per chat, across objects of the same name');
  });

  it('a kit’s placeholders; an edited instance’s title and lineage line', async () => {
    const kit = await render((h) => h.createMode.set('kit'));
    expect(kit.el.querySelector('.qt-dialog-title')!.textContent).toBe('New kit');
    expect((byLabel(kit.el, 'Name') as HTMLInputElement).placeholder).toBe("burglar's kit");
    expect((byLabel(kit.el, 'Title') as HTMLInputElement).placeholder).toBe('The Night Shift');
    TestBed.resetTestingModule();
    const inst = await render((h) => h.item.set(obj('i-1', { parentId: 'bow', name: 'magic bow', parentName: 'magic bow' })));
    expect(inst.el.querySelector('.qt-dialog-title')!.textContent).toBe('Edit magic bow');
    expect(inst.el.textContent!.replace(/\s+/g, ' ')).toContain('Kept in Vivienne\'s inventory · an instance of magic bow');
    TestBed.resetTestingModule();
    const orphan = await render((h) => {
      h.item.set(obj('i-2', { parentId: 'gone' }));
      h.pool.set([]);
    });
    expect(orphan.el.textContent).toContain('an instance whose archetype is out of view');
  });

  it('Escape closes unless saving', async () => {
    const { host } = await render((h) => h.item.set(RING));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(host.closes()).toBe(1);
  });
});
