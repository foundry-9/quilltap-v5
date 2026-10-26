import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CarriedEntry, ResolvedInventoryItem } from '../core/core-contract';
import { BOW, KIT, LAMP, RING, SWORD } from './__fixtures__/inventory-ui.fixture';
import { CarryingNowPane } from './carrying-now-pane';

/**
 * v4 `inventory-ui.test.tsx` "Carrying now" (case 9) NAME FOR NAME over v4's
 * fixture, with the picker state held by the host as v4's `Harness` holds it;
 * plus the empty sentence, the `silent` note, a kit's Equip by state, and the
 * empty picker.
 */

@Component({
  imports: [CarryingNowPane],
  template: `
    <qt-carrying-now-pane
      characterName="Vivienne"
      [carried]="carried()"
      [pool]="pool()"
      [busy]="false"
      (equip)="onEquip($event)"
      (unequip)="onUnequip($event)"
      (reset)="onReset()"
      [(picking)]="picking"
    />
  `,
})
class Harness {
  readonly carried = signal<CarriedEntry>({ equipped: [RING], kits: { kit: 'some' } });
  readonly pool = signal<ResolvedInventoryItem[]>([SWORD, RING, LAMP, BOW, KIT]);
  readonly picking = signal(false);
  readonly onEquip = vi.fn();
  readonly onUnequip = vi.fn();
  readonly onReset = vi.fn();
}

function render(setup: (h: Harness) => void = () => {}) {
  TestBed.configureTestingModule({ imports: [Harness] });
  const fixture = TestBed.createComponent(Harness);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  const list = (name: string) => el.querySelector(`[aria-label="${name}"]`) as HTMLElement | null;
  const button = (root: ParentNode, name: string) =>
    [...root.querySelectorAll('button')].find((b) => b.textContent!.trim() === name) as HTMLButtonElement;
  return { fixture, el, host: fixture.componentInstance, list, button };
}

describe('Carrying now', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('lists what is carried, the kits with their state, and offers Take up and Reset', () => {
    const { fixture, el, host, list, button } = render();
    const carried = list('Carried objects')!;
    expect(carried.textContent).toContain('signet ring');
    button(carried, 'Unequip').click();
    expect(host.onUnequip).toHaveBeenCalledWith(['ring']);

    const kits = list('Kits')!;
    expect(kits.textContent).toContain('some');
    button(kits, 'Equip').click();
    expect(host.onEquip).toHaveBeenCalledWith(['kit']);

    button(el, 'Take up…').click();
    fixture.detectChanges();
    const picker = list('Objects within reach')!;
    // Pictures show in the picker; archetypes, kits and what is carried do not.
    expect([...picker.querySelectorAll('[role="option"]')].map((o) => o.querySelector('.text-sm')!.textContent!.trim())).toEqual([
      'Excalibur (sword)',
      'lantern',
    ]);
    expect(picker.querySelector('[data-testid="inventory-picture"]')).not.toBeNull();
    // The picker can be dismissed without choosing.
    button(el, 'Cancel').click();
    fixture.detectChanges();
    expect(list('Objects within reach')).toBeNull();
    expect(host.picking()).toBe(false);

    button(el, 'Reset to defaults').click();
    expect(host.onReset).toHaveBeenCalled();
  });

  // ---- v5-added ----

  it('names the character in the heading; the empty sentence when nothing is carried', () => {
    const { el } = render((h) => h.carried.set({ equipped: [], kits: {} }));
    expect(el.querySelector('[data-testid="inventory-carrying-now"] h3')!.textContent).toBe('Carrying now · Vivienne in this chat');
    expect(el.textContent).toContain('Empty-handed. Equip something from the list, or take it up here.');
    expect(el.querySelector('[aria-label="Kits"]')).toBeNull();
  });

  it('a carried object with no description is marked silent; one with a description is not', () => {
    const { list } = render((h) => h.carried.set({ equipped: [RING, BOW], kits: {} }));
    const rows = [...list('Carried objects')!.querySelectorAll('li')];
    const silent = rows[0].querySelector('span[title^="No description"]')!;
    expect(silent.textContent).toBe('silent');
    expect(silent.getAttribute('title')).toBe('No description: the model is told nothing about it');
    expect(rows[1].querySelector('span[title^="No description"]')).toBeNull();
  });

  it('a kit offers Equip unless all is carried, Unequip unless none is', () => {
    const kitRow = (state: 'all' | 'some' | 'none') => {
      TestBed.resetTestingModule();
      const { list } = render((h) => h.carried.set({ equipped: [], kits: { kit: state } }));
      return [...list('Kits')!.querySelectorAll('button')].map((b) => b.textContent!.trim());
    };
    expect(kitRow('all')).toEqual(['Unequip']);
    expect(kitRow('some')).toEqual(['Equip', 'Unequip']);
    expect(kitRow('none')).toEqual(['Equip']);
  });

  it('picking an object equips it and closes the picker; an empty picker says so; Escape closes only the picker', () => {
    const { fixture, el, host, button } = render();
    button(el, 'Take up…').click();
    fixture.detectChanges();
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Take up — Vivienne');
    ([...el.querySelectorAll('[role="option"]')][1] as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(host.onEquip).toHaveBeenCalledWith(['lamp']);
    expect(host.picking()).toBe(false);

    host.pool.set([RING, BOW, KIT]);
    host.picking.set(true);
    fixture.detectChanges();
    expect(el.textContent).toContain('Nothing within reach that is not already carried.');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    fixture.detectChanges();
    expect(host.picking()).toBe(false);
  });
});
