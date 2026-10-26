import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ResolvedInventoryItem } from '../core/core-contract';
import type { WardrobeVantage } from '../pascal/tool-draft';
import { BOW, KIT, LAMP, PROJECT, RING, SWORD, obj } from './__fixtures__/inventory-ui.fixture';
import {
  InventoryReferencePicker,
  inventoryPickRule,
  type InventoryPickAction,
  type InventoryReferenceForm,
} from './inventory-reference-picker';
import { InventoryVantageItemsLoader } from './inventory-vantage-items';

/**
 * P4.D330 Tier 2 item 27 — the reference picker + vantage loader C10 hands
 * P4.D334. v4's `workbench-inventory.test.tsx` has NO case that drives the
 * picker alone at the pin (its eight `it`s are the Workbench sections' —
 * P4.D334's), so every case here is v5-authored against v4's
 * `inventory-reference-picker.tsx` / `use-inventory-vantage-items.ts`.
 */

const VIV: WardrobeVantage = { kind: 'character', characterId: 'c-viv' };
const ARCHIVED = obj('old', { name: 'old map', archivedAt: '2026-10-01T00:00:00.000Z' });

@Component({
  imports: [InventoryReferencePicker],
  template: `
    <qt-inventory-reference-picker
      [open]="open()"
      [vantage]="vantage()"
      [action]="action()"
      [highlightId]="highlight()"
      [reference]="reference()"
      (referenceChange)="reference.set($event)"
      (picked)="picks.push($event)"
      (closed)="open.set(false)"
    >
      <span qtPickerHeader class="header">whose?</span>
    </qt-inventory-reference-picker>
  `,
})
class Host {
  readonly open = signal(true);
  readonly vantage = signal<WardrobeVantage>(VIV);
  readonly action = signal<InventoryPickAction>('equip');
  readonly highlight = signal<string | null>(null);
  readonly reference = signal<InventoryReferenceForm>('label');
  readonly picks: Array<{ item: ResolvedInventoryItem; reference: InventoryReferenceForm }> = [];
}

const seen: Array<Record<string, unknown>> = [];
let items: ResolvedInventoryItem[] = [SWORD, BOW, RING, KIT, LAMP, ARCHIVED];

async function render(setup: (h: Host) => void = () => {}): Promise<{ fixture: ComponentFixture<Host>; el: HTMLElement; host: Host }> {
  seen.length = 0;
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      {
        provide: CoreClient,
        useValue: {
          dispatchData: vi.fn(async (req: Record<string, unknown>) => {
            seen.push(req);
            return { items };
          }),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  setup(fixture.componentInstance);
  fixture.detectChanges();
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return { fixture, el: fixture.nativeElement as HTMLElement, host: fixture.componentInstance };
}

const options = (el: HTMLElement) =>
  [...el.querySelectorAll('[data-testid="inventory-reference-picker-list"] button')] as HTMLButtonElement[];
const names = (el: HTMLElement) => options(el).map((b) => b.querySelector('.text-sm')!.textContent!.trim());

describe('inventoryPickRule (v4 :49-53, spec-pinned)', () => {
  it('pick up names archetypes; equip and unequip refuse them', () => {
    expect(inventoryPickRule(BOW, 'pick_up')).toBeNull();
    expect(inventoryPickRule(SWORD, 'pick_up')).toBe('Pick up names an archetype');
    expect(inventoryPickRule(BOW, 'equip')).toBe('An archetype is picked up, not equipped');
    expect(inventoryPickRule(BOW, 'unequip')).toBe('An archetype is picked up, not equipped');
    expect(inventoryPickRule(SWORD, 'equip')).toBeNull();
  });
});

describe('InventoryReferencePicker', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    items = [SWORD, BOW, RING, KIT, LAMP, ARCHIVED];
  });

  it('titles itself, projects the header, reads the character pool with archived and shared', async () => {
    const { el } = await render();
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Choose an object');
    expect(el.querySelector('.header')!.textContent).toBe('whose?');
    expect(seen).toEqual([{ type: 'inventoryList', container: { scope: 'character', id: 'c-viv' }, includeArchived: true, shared: true }]);
  });

  it('for equip: the eligible first (stable), archetypes greyed with their refusal, archived hidden', async () => {
    const { el } = await render();
    expect(names(el)).toEqual(['Excalibur (sword)', 'signet ring', "ranger's loadout", 'lantern', 'magic bow']);
    const bow = options(el).at(-1)!;
    expect(bow.disabled).toBe(true);
    expect(bow.title).toBe('An archetype is picked up, not equipped');
    expect(bow.getAttribute('class')).toContain('opacity-50');
    expect(bow.textContent).toContain('archetype');
    expect(bow.textContent).toContain('Project: The Quest');
    expect(options(el)[2].textContent).toContain('kit');
  });

  it('for pick up: the archetypes first, everything else greyed', async () => {
    const { el } = await render((h) => h.action.set('pick_up'));
    expect(names(el)[0]).toBe('magic bow');
    expect(options(el)[1].title).toBe('Pick up names an archetype');
  });

  it('the search reads the display name; Show archived reveals the archived, flagged', async () => {
    const { fixture, el } = await render();
    const search = el.querySelector('input[aria-label="Find an object"]') as HTMLInputElement;
    expect(search.placeholder).toBe('Find an object');
    search.value = 'excal';
    search.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(names(el)).toEqual(['Excalibur (sword)']);
    search.value = '';
    search.dispatchEvent(new Event('input'));
    ([...el.querySelectorAll('label')].find((l) => l.textContent!.trim() === 'Show archived')!.querySelector('input') as HTMLInputElement).click();
    fixture.detectChanges();
    expect(names(el)).toContain('old map');
    expect(options(el).find((b) => b.textContent!.includes('old map'))!.textContent).toContain('archived');
  });

  it('a pick hands back the object and the reference form, then closes; Write it down by flips the form', async () => {
    const { fixture, el, host } = await render((h) => h.highlight.set('ring'));
    expect(options(el)[1].getAttribute('class')).toContain('qt-border-primary');
    const form = el.querySelector('select[aria-label="Reference form"]') as HTMLSelectElement;
    expect([...form.options].map((o) => [o.value, o.textContent])).toEqual([
      ['label', 'title or name'],
      ['id', 'id'],
    ]);
    form.value = 'id';
    form.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    options(el)[0].click();
    fixture.detectChanges();
    expect(host.picks).toEqual([{ item: SWORD, reference: 'id' }]);
    expect(host.open()).toBe(false);
  });

  it('the three empty texts: no vantage, nothing kept; nothing rendered while closed', async () => {
    const none = await render((h) => h.vantage.set({ kind: 'none' }));
    expect(none.el.textContent).toContain('Choose whose inventory to browse above.');
    expect(seen).toEqual([]);
    TestBed.resetTestingModule();
    items = [];
    const empty = await render();
    expect(empty.el.textContent).toContain('Nothing is kept here yet. Objects are added from the Inventory.');
    TestBed.resetTestingModule();
    const closed = await render((h) => h.open.set(false));
    expect(closed.el.querySelector('.qt-dialog')).toBeNull();
    expect(seen).toEqual([]);
  });

  it('Escape closes the picker', async () => {
    const { fixture, host } = await render();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    fixture.detectChanges();
    expect(host.open()).toBe(false);
  });
});

describe('InventoryVantageItemsLoader (v4 use-inventory-vantage-items.ts)', () => {
  function loader(answer: (req: Record<string, unknown>) => Promise<unknown>) {
    const calls: Array<Record<string, unknown>> = [];
    const core = {
      dispatchData: async (req: Record<string, unknown>) => {
        calls.push(req);
        return answer(req);
      },
    } as unknown as CoreClient;
    return { loader: TestBed.runInInjectionContext(() => new InventoryVantageItemsLoader(core)), calls };
  }

  it('a character vantage reads its pool with NO project tier; a container vantage its tier', async () => {
    const { loader: l, calls } = loader(async () => ({ items: [SWORD] }));
    await l.load(VIV);
    await l.load({ kind: 'container', container: { scope: 'project', id: 'p-1' } });
    expect(calls).toEqual([
      { type: 'inventoryList', container: { scope: 'character', id: 'c-viv' }, includeArchived: true, shared: true },
      { type: 'inventoryList', container: { scope: 'project', id: 'p-1' }, includeArchived: true },
    ]);
    expect(l.fetched()).toBe(true);
  });

  it('owns: the vantage’s own tier, an origin-less object counted as own', async () => {
    const loose = obj('loose', { origin: undefined });
    const { loader: l } = loader(async () => ({ items: [SWORD, BOW, loose] }));
    await l.load(VIV);
    expect(l.pool().owns('sword')).toBe(true);
    expect(l.pool().owns('bow')).toBe(false);
    expect(l.pool().owns('loose')).toBe(true);
    expect(l.pool().items).toHaveLength(3);
    void PROJECT;
  });

  it('none reads nothing and is not fetched; a stale answer is dropped', async () => {
    let release!: (v: unknown) => void;
    const { loader: l, calls } = loader((req) =>
      (req['container'] as { scope: string }).scope === 'character' ? new Promise((r) => (release = r)) : Promise.resolve({ items: [LAMP] }),
    );
    await l.load({ kind: 'none' });
    expect(calls).toEqual([]);
    expect(l.fetched()).toBe(false);
    const slow = l.load(VIV);
    await l.load({ kind: 'container', container: { scope: 'general', id: null } });
    release({ items: [SWORD] });
    await slow;
    expect(l.items()).toEqual([LAMP]);
  });
});
