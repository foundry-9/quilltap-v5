import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ResolvedInventoryItem } from '../core/core-contract';
import { BOW, KIT, LAMP, PROJECT, RING, SWORD, obj } from './__fixtures__/inventory-ui.fixture';
import {
  InventoryItemRow,
  MENU_ITEM,
  menuPlacement,
  type InventoryRowActions,
  type InventoryRowEquip,
} from './inventory-item-row';

/**
 * v4 `components/inventory/inventory-item-row.tsx` (`322a5488e`, `102a23648`'s
 * fixed menu): the chips in order, the ⋮ menu's entries IN ORDER for a
 * manageable instance / an archetype / a borrowed row / an archived row, the
 * capture-phase scroll + resize close (trap 14), and `menuPlacement` as pure
 * arithmetic (R-15).
 */

describe('menuPlacement (v4 :66-78)', () => {
  it('opens below with room to spare', () => {
    expect(menuPlacement({ top: 100, bottom: 120, right: 900 }, 1000, 800)).toEqual({
      position: 'fixed',
      right: 100,
      top: 124,
      maxHeight: 676,
      overflowY: 'auto',
    });
  });

  it('opens upward when the room below is short and the room above is longer', () => {
    expect(menuPlacement({ top: 700, bottom: 720, right: 990 }, 1000, 800)).toEqual({
      position: 'fixed',
      right: 10,
      bottom: 104,
      maxHeight: 696,
      overflowY: 'auto',
    });
  });

  it('stays below when short both ways but below is longer; maxHeight floors at 120', () => {
    expect(menuPlacement({ top: 50, bottom: 70, right: 500 }, 1000, 150)).toEqual({
      position: 'fixed',
      right: 500,
      top: 74,
      maxHeight: 120,
      overflowY: 'auto',
    });
  });

  it('right floors at the 4px gap', () => {
    expect(menuPlacement({ top: 0, bottom: 20, right: 1100 }, 1000, 800).right).toBe(4);
  });
});

function actions(): InventoryRowActions {
  return {
    onEdit: vi.fn(),
    onInstantiate: vi.fn(),
    onDetach: vi.fn(),
    onDuplicate: vi.fn(),
    onMoveToFolder: vi.fn(),
    onTransfer: vi.fn(),
    onToggleArchived: vi.fn(),
    onDelete: vi.fn(),
  };
}

@Component({
  imports: [InventoryItemRow],
  template: `
    <div class="scroller" style="overflow: auto">
      <qt-inventory-item-row
        [item]="item()"
        [manageable]="manageable()"
        [actions]="actions"
        [equip]="equip()"
        [selection]="selection()"
        [folderCrumb]="crumb()"
      />
    </div>
  `,
})
class Host {
  readonly item = signal<ResolvedInventoryItem>(LAMP);
  readonly manageable = signal(true);
  readonly equip = signal<InventoryRowEquip | undefined>(undefined);
  readonly selection = signal<{ selected: boolean; onToggle: (i: ResolvedInventoryItem) => void } | undefined>(undefined);
  readonly crumb = signal<string | null>(null);
  readonly actions = actions();
}

function render(setup: (h: Host) => void = () => {}) {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  const kebab = () => el.querySelector('[aria-label="More actions"]') as HTMLButtonElement;
  const entries = () => [...el.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent!.trim());
  const chips = () => [...el.querySelectorAll('.mt-0\\.5 > span')].map((s) => s.textContent!.trim());
  const open = () => {
    kebab().click();
    fixture.detectChanges();
  };
  return { fixture, el, host: fixture.componentInstance, kebab, entries, chips, open };
}

describe('InventoryItemRow', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('names the object (title with the name beside it) and lists the chips in v4’s order', () => {
    const { el, chips } = render((h) => {
      h.item.set(obj('inst', { name: 'magic bow', title: 'Heartseeker', parentId: 'bow', parentName: 'magic bow', equippedByDefault: true, archivedAt: '2026-10-01T00:00:00.000Z', origin: PROJECT }));
      h.manageable.set(false);
      h.equip.set({ equipped: true, onEquip: vi.fn(), onUnequip: vi.fn() });
      h.crumb.set('Arms');
    });
    expect(el.querySelector('.truncate')!.getAttribute('title')).toBe('Heartseeker');
    expect(el.querySelector('.truncate')!.textContent!.replace(/\s+/g, ' ').trim()).toBe('Heartseeker (magic bow)');
    expect(chips()).toEqual(['Instance of magic bow', 'Equipped', 'Default', 'Project: The Quest', 'archived', 'Arms']);
    expect(el.querySelector('[data-testid="inventory-item-row"]')!.getAttribute('class')).toBe(
      'qt-card py-2 px-3 flex items-center gap-2 opacity-60',
    );
  });

  it('an archetype, a missing archetype, a kit (count outside a chat, state in one)', () => {
    const arch = render((h) => h.item.set(BOW));
    expect(arch.chips()).toEqual(['Archetype']);
    TestBed.resetTestingModule();
    const missing = render((h) => h.item.set(obj('orphan', { parentId: 'gone', parentMissing: true })));
    expect(missing.chips()).toEqual(['Instance (archetype missing)']);
    expect(missing.el.querySelector('.qt-badge-outline')!.getAttribute('title')).toBe('Its archetype could not be found');
    TestBed.resetTestingModule();
    const kit = render((h) => h.item.set(KIT));
    expect(kit.chips()).toEqual(['Kit · 2']);
    kit.host.equip.set({ equipped: false, kitState: 'all', onEquip: vi.fn(), onUnequip: vi.fn() });
    kit.fixture.detectChanges();
    expect(kit.chips()).toEqual(['Kit · all']);
  });

  it('a manageable copyable instance’s menu, in order', () => {
    const { entries, open } = render((h) => h.item.set(obj('inst', { parentId: 'bow', parentName: 'magic bow', canCopy: true })));
    open();
    expect(entries()).toEqual([
      'Edit',
      'Detach from archetype…',
      'Duplicate',
      'Move to folder…',
      'Move to another inventory…',
      'Copy to another inventory…',
      'Archive',
      'Delete',
    ]);
  });

  it('a manageable archetype’s menu; an archived row offers Restore and no Instantiate', () => {
    const arch = render((h) => h.item.set(obj('bow', { name: 'magic bow', isArchetype: true })));
    arch.open();
    expect(arch.entries()).toEqual(['Edit', 'Instantiate…', 'Move to folder…', 'Move to another inventory…', 'Archive', 'Delete']);
    TestBed.resetTestingModule();
    const archived = render((h) => h.item.set(obj('bow', { isArchetype: true, archivedAt: '2026-10-01T00:00:00.000Z' })));
    archived.open();
    expect(archived.entries()).toEqual(['Edit', 'Move to folder…', 'Move to another inventory…', 'Restore', 'Delete']);
  });

  it('a borrowed row keeps only the cross-tier Copy (and Instantiate for an archetype)', () => {
    const { entries, open } = render((h) => {
      h.item.set(obj('bow', { isArchetype: true, canCopy: true, origin: PROJECT }));
      h.manageable.set(false);
    });
    open();
    expect(entries()).toEqual(['Instantiate…', 'Copy to another inventory…']);
  });

  it('a carried kit in a chat offers Unequip the kit after Edit; the Delete entry wears the destructive class', () => {
    const onUnequip = vi.fn();
    const { el, entries, open } = render((h) => {
      h.item.set(KIT);
      h.equip.set({ equipped: false, kitState: 'some', onEquip: vi.fn(), onUnequip });
    });
    open();
    expect(entries().slice(0, 2)).toEqual(['Edit', 'Unequip the kit']);
    const items = [...el.querySelectorAll('[role="menuitem"]')] as HTMLButtonElement[];
    expect(items[0].getAttribute('class')).toBe(MENU_ITEM);
    expect(items.at(-1)!.getAttribute('class')).toBe(`${MENU_ITEM} qt-text-destructive`);
    items[1].click();
    expect(onUnequip).toHaveBeenCalledWith(KIT);
  });

  it('an entry closes the menu and acts on the row; Move / Copy to another inventory pass their mode', () => {
    const { fixture, el, host, open } = render((h) => h.item.set(obj('ring', { canCopy: true })));
    open();
    ([...el.querySelectorAll('[role="menuitem"]')].find((b) => b.textContent!.trim() === 'Copy to another inventory…') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(el.querySelector('[role="menu"]')).toBeNull();
    expect(host.actions.onTransfer).toHaveBeenCalledWith(host.item(), 'copy');
  });

  it('the menu is fixed to the viewport by inline style', () => {
    const { el, open } = render();
    open();
    const menu = el.querySelector('[role="menu"]') as HTMLElement;
    expect(menu.style.position).toBe('fixed');
    expect(menu.style.overflowY).toBe('auto');
    expect(menu.style.maxHeight).toMatch(/px$/);
    expect(menu.getAttribute('class')).toBe('z-30 min-w-[14rem] rounded border qt-border-default qt-bg-default shadow-md');
  });

  it('a capture-phase scroll outside the menu or a resize closes it; a scroll inside does not (trap 14)', () => {
    const { fixture, el, open } = render();
    open();
    (el.querySelector('[role="menu"]') as HTMLElement).dispatchEvent(new Event('scroll'));
    fixture.detectChanges();
    expect(el.querySelector('[role="menu"]')).not.toBeNull();
    // A scroll on the list's own scroller does not bubble; only a capture listener sees it.
    (el.querySelector('.scroller') as HTMLElement).dispatchEvent(new Event('scroll'));
    fixture.detectChanges();
    expect(el.querySelector('[role="menu"]')).toBeNull();
    open();
    window.dispatchEvent(new Event('resize'));
    fixture.detectChanges();
    expect(el.querySelector('[role="menu"]')).toBeNull();
  });

  it('Escape closes the menu on the capture phase and stops there; a press outside closes it', () => {
    const { fixture, el, open } = render();
    open();
    let reached = false;
    const later = () => (reached = true);
    document.addEventListener('keydown', later);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    document.removeEventListener('keydown', later);
    fixture.detectChanges();
    expect(reached).toBe(false);
    expect(el.querySelector('[role="menu"]')).toBeNull();
    open();
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    fixture.detectChanges();
    expect(el.querySelector('[role="menu"]')).toBeNull();
  });

  it('Equip / Unequip on the row (none on an archetype or an archived object); select mode disables them', () => {
    const onEquip = vi.fn();
    const onUnequip = vi.fn();
    const { fixture, el, host } = render((h) => {
      h.item.set(SWORD);
      h.equip.set({ equipped: false, onEquip, onUnequip });
    });
    const rowButton = () => [...el.querySelectorAll('button')].find((b) => ['Equip', 'Unequip'].includes(b.textContent!.trim()));
    rowButton()!.click();
    expect(onEquip).toHaveBeenCalledWith(SWORD);
    host.equip.set({ equipped: true, onEquip, onUnequip });
    fixture.detectChanges();
    expect(rowButton()!.textContent!.trim()).toBe('Unequip');
    host.selection.set({ selected: false, onToggle: vi.fn() });
    fixture.detectChanges();
    expect(rowButton()!.disabled).toBe(true);
    expect((el.querySelector('[aria-label="More actions"]') as HTMLButtonElement).disabled).toBe(true);
    expect(el.querySelector('input[aria-label="Select Excalibur"]')).not.toBeNull();
    host.item.set(BOW);
    fixture.detectChanges();
    expect(rowButton()).toBeUndefined();
    host.item.set(obj('old', { archivedAt: '2026-10-01T00:00:00.000Z' }));
    fixture.detectChanges();
    expect(rowButton()).toBeUndefined();
    host.item.set(RING);
    fixture.detectChanges();
    expect(rowButton()!.textContent!.trim()).toBe('Unequip');
  });
});
