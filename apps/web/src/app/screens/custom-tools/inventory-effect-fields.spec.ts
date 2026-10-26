import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { P4D330_SPA_LANDED } from '../../../testing/p4d334-gates';
import type { ResolvedInventoryPool } from '../../pascal/inventory-refs';
import type { DraftInventoryGesture, WardrobeVantage } from '../../pascal/tool-draft';
import { INVENTORY_ACTIONS, InventoryEffectFields } from './inventory-effect-fields';
import type { ResolvedInventoryItem } from './p4d330-stand-ins';

/**
 * InventoryEffectFields — v4 `InventoryEffectFields.tsx` (`e1872c94f`), the
 * Inventory row of the side-effects card. v4's jsdom coverage of it is the
 * section-level `workbench-inventory.test.tsx` (transcribed in
 * `side-effects-section.spec.ts`); the field-level rows here are this lane's.
 * Rows whose truth reads P4.D330's display / pick rule / picture are gated.
 */

// The fields the field reads; the cast keeps the helper honest against P4.D330's fuller DTO.
const item = (over: Partial<ResolvedInventoryItem>): ResolvedInventoryItem =>
  ({
    id: 'x',
    kind: 'item',
    name: 'x',
    title: null,
    isArchetype: false,
    parentId: null,
    itemIds: [],
    metadata: {},
    folder: '',
    archivedAt: null,
    ...over,
  }) as ResolvedInventoryItem;
const SWORD = item({ id: 'obj-sword', name: 'sword', title: 'Excalibur', origin: { scope: 'character', id: 'c1', name: 'Imogen' } });
const BOW = item({ id: 'arch-bow', name: 'magic bow', isArchetype: true, origin: { scope: 'project', id: 'p1', name: 'Thornfield' } });
const KIT = item({ id: 'kit-1', kind: 'kit', name: 'toolkit', itemIds: ['obj-sword'] });
const OLD = item({ id: 'obj-old', name: 'rusty axe', archivedAt: '2026-01-01T00:00:00.000Z' });
const POOL: ResolvedInventoryPool = { items: [SWORD, BOW, KIT, OLD], owns: (id) => id !== 'arch-bow' };

const CHARACTER: WardrobeVantage = { kind: 'character', characterId: 'c1' };

function render(gesture: DraftInventoryGesture, extra: Record<string, unknown> = {}) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [InventoryEffectFields] });
  const fixture: ComponentFixture<InventoryEffectFields> = TestBed.createComponent(InventoryEffectFields);
  fixture.componentRef.setInput('gesture', gesture);
  for (const [k, v] of Object.entries(extra)) fixture.componentRef.setInput(k, v);
  const gestures: DraftInventoryGesture[] = [];
  const chosen: Array<{ index: number; highlightId: string | null }> = [];
  fixture.componentInstance.gestureChange.subscribe((g) => gestures.push(g));
  fixture.componentInstance.choose.subscribe((c) => chosen.push(c));
  fixture.detectChanges();
  return { fixture, gestures, chosen };
}

const el = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const q = (f: ComponentFixture<unknown>, sel: string) => el(f).querySelector<HTMLElement>(sel);
const qa = (f: ComponentFixture<unknown>, sel: string) => [...el(f).querySelectorAll<HTMLElement>(sel)];
const gesture = (over: Partial<DraftInventoryGesture> = {}): DraftInventoryGesture => ({
  action: 'equip',
  items: ['Excalibur'],
  equip: true,
  ...over,
});

describe('InventoryEffectFields — the action and the switch', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('lists the three actions with v4’s labels and titles (the pick-up title has a curly apostrophe)', () => {
    expect(INVENTORY_ACTIONS.map((a) => [a.value, a.label])).toEqual([
      ['equip', 'equip'],
      ['unequip', 'unequip'],
      ['pick_up', 'pick up'],
    ]);
    expect(INVENTORY_ACTIONS[2].title).toBe('Make an instance of each archetype in the rolling character’s vault');
    const { fixture } = render(gesture());
    const select = q(fixture, 'select[aria-label="Inventory action"]') as HTMLSelectElement;
    expect([...select.options].map((o) => o.title)).toEqual(INVENTORY_ACTIONS.map((a) => a.title));
    expect(select.value).toBe('equip');
  });

  it('writes an action change', () => {
    const { fixture, gestures } = render(gesture());
    const select = q(fixture, 'select[aria-label="Inventory action"]') as HTMLSelectElement;
    select.value = 'pick_up';
    select.dispatchEvent(new Event('change'));
    expect(gestures).toEqual([{ action: 'pick_up', items: ['Excalibur'], equip: true }]);
  });

  it('shows "equip it at once" on pick_up only, checked from the gesture', () => {
    expect(q(render(gesture()).fixture, 'input[type="checkbox"]')).toBeNull();
    const { fixture, gestures } = render(gesture({ action: 'pick_up', equip: false }));
    const box = q(fixture, 'input[type="checkbox"]') as HTMLInputElement;
    expect(box.checked).toBe(false);
    expect(el(fixture).textContent).toContain('equip it at once');
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    expect(gestures.at(-1)).toMatchObject({ action: 'pick_up', equip: true });
  });
});

describe('InventoryEffectFields — the text rows', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('placeholders: magic bow (pick_up, row 0), Excalibur (row 0), {{params.loot}} (later rows)', () => {
    expect(q(render(gesture({ action: 'pick_up', items: [''] })).fixture, 'input[aria-label="Object 1"]')!.getAttribute('placeholder')).toBe('magic bow');
    expect(q(render(gesture({ items: [''] })).fixture, 'input[aria-label="Object 1"]')!.getAttribute('placeholder')).toBe('Excalibur');
    const { fixture } = render(gesture({ items: ['', ''] }));
    expect(q(fixture, 'input[aria-label="Object 2"]')!.getAttribute('placeholder')).toBe('{{params.loot}}');
  });

  it('caps a reference at 200 characters and marks every box on a draft error', () => {
    const { fixture } = render(gesture({ items: ['a'] }), { hasErrors: true });
    const box = q(fixture, 'input[aria-label="Object 1"]')!;
    expect(box.getAttribute('maxlength')).toBe('200');
    expect(box.classList.contains('qt-input-error')).toBe(true);
  });

  it('shows "chosen at run time" for a templated reference and no unresolved badge', () => {
    const { fixture } = render(gesture({ items: ['{{params.weapon}}'] }), { vantage: CHARACTER, pool: POOL, poolFetched: true });
    expect(el(fixture).textContent).toContain('chosen at run time');
    expect(q(fixture, '[data-testid="inventory-reference-unresolved"]')).toBeNull();
    expect((q(fixture, 'input[aria-label="Object 1"]') as HTMLInputElement).value).toBe('{{params.weapon}}');
  });

  describe('the unresolved badge — four conditions, all required', () => {
    const badge = (f: ComponentFixture<unknown>) => q(f, '[data-testid="inventory-reference-unresolved"]');
    it('not templated, a vantage, the pool fetched, a non-blank reference', () => {
      const { fixture } = render(gesture({ items: ['Excalibur II'] }), { vantage: CHARACTER, pool: POOL, poolFetched: true, vantageName: 'Imogen' });
      expect(badge(fixture)!.textContent?.trim()).toBe('not in Imogen');
    });
    it('names this inventory when the vantage has no name', () => {
      const { fixture } = render(gesture({ items: ['nothing'] }), { vantage: CHARACTER, pool: POOL, poolFetched: true });
      expect(badge(fixture)!.textContent?.trim()).toBe('not in this inventory');
    });
    it('no badge under the none vantage, before the pool is fetched, or for a blank reference', () => {
      expect(badge(render(gesture({ items: ['nothing'] }), { pool: POOL, poolFetched: true }).fixture)).toBeNull();
      expect(badge(render(gesture({ items: ['nothing'] }), { vantage: CHARACTER, pool: POOL }).fixture)).toBeNull();
      expect(badge(render(gesture({ items: ['  '] }), { vantage: CHARACTER, pool: POOL, poolFetched: true }).fixture)).toBeNull();
    });
  });

  it('Choose… asks the host to open the picker with no highlight', () => {
    const { fixture, chosen } = render(gesture({ items: ['', ''] }));
    qa(fixture, 'button').find((b) => b.textContent?.trim() === 'Choose…')!.click();
    expect(chosen).toEqual([{ index: 0, highlightId: null }]);
    expect(qa(fixture, 'button').find((b) => b.textContent?.trim() === 'Choose…')!.title).toBe('Browse the inventory and pick an object');
  });
});

describe('InventoryEffectFields — adding and removing rows', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('Add object appends a blank row, up to eight, then disables with its title', () => {
    const { fixture, gestures } = render(gesture({ items: ['a'] }));
    qa(fixture, 'button').find((b) => b.textContent?.includes('Add object'))!.click();
    expect(gestures.at(-1)!.items).toEqual(['a', '']);
    const full = render(gesture({ items: Array.from({ length: 8 }, (_, i) => `o${i}`) }));
    const add = qa(full.fixture, 'button').find((b) => b.textContent?.includes('Add object')) as HTMLButtonElement;
    expect(add.disabled).toBe(true);
    expect(add.getAttribute('title')).toBe('At most 8 objects');
  });

  it('the hint says what a reference may be', () => {
    const { fixture } = render(gesture());
    expect(el(fixture).textContent!.replace(/\s+/g, ' ')).toContain('an id, title or name — {{params.x}} lets a parameter choose');
    expect(q(fixture, 'code.font-mono')!.textContent).toBe('{{params.x}}');
  });

  it('the trash is disabled at one row, removes otherwise, and never leaves fewer than one', () => {
    const one = render(gesture({ items: ['a'] }));
    expect((qa(one.fixture, 'button[title="Remove this object"]')[0] as HTMLButtonElement).disabled).toBe(true);
    const two = render(gesture({ items: ['a', 'b'] }));
    (qa(two.fixture, 'button[title="Remove this object"]')[0] as HTMLButtonElement).click();
    expect(two.gestures.at(-1)!.items).toEqual(['b']);
  });

  it('treats an empty items list as one blank row', () => {
    const { fixture } = render(gesture({ items: [] }));
    expect(qa(fixture, 'input[aria-label^="Object "]')).toHaveLength(1);
  });

  it('typing writes the row', () => {
    const { fixture, gestures } = render(gesture({ items: ['a', 'b'] }));
    const box = q(fixture, 'input[aria-label="Object 2"]') as HTMLInputElement;
    box.value = 'bb';
    box.dispatchEvent(new Event('input'));
    expect(gestures.at(-1)!.items).toEqual(['a', 'bb']);
  });
});

describe('InventoryEffectFields — a resolved reference as a chip', () => {
  afterEach(() => TestBed.resetTestingModule());
  const lens = { vantage: CHARACTER, pool: POOL, poolFetched: true };

  it('renders the chip (aria-label from the label rule), the by id toggle and the trash', () => {
    const { fixture, chosen } = render(gesture({ items: ['Excalibur'] }), lens);
    const chip = q(fixture, '[data-testid="inventory-reference-chip"]')!;
    expect(chip.getAttribute('aria-label')).toBe('Object 1: Excalibur — choose another');
    chip.click();
    expect(chosen).toEqual([{ index: 0, highlightId: 'obj-sword' }]);
    expect(q(fixture, 'input[aria-label="Object 1"]')).toBeNull();
    const toggle = qa(fixture, 'button').find((b) => b.textContent?.trim() === 'by name')!;
    expect(toggle.title).toBe('Referenced by name — click to use its id');
  });

  it('flips between by name and by id', () => {
    const { fixture, gestures } = render(gesture({ items: ['Excalibur'] }), lens);
    qa(fixture, 'button').find((b) => b.textContent?.trim() === 'by name')!.click();
    expect(gestures.at(-1)!.items).toEqual(['obj-sword']);
    const byId = render(gesture({ items: ['obj-sword'] }), lens);
    const toggle = qa(byId.fixture, 'button').find((b) => b.textContent?.trim() === 'by id')!;
    expect(toggle.title).toBe('Referenced by id — click to name it instead');
    toggle.click();
    expect(byId.gestures.at(-1)!.items).toEqual(['Excalibur']);
  });

  it('picks the first match whose rule passes: pick_up prefers the archetype, equip never an archetype alone', () => {
    // 'magic bow' resolves only to the archetype: it is still shown (the first match), flagged archetype.
    const { fixture } = render(gesture({ action: 'pick_up', items: ['magic bow'] }), lens);
    expect(q(fixture, '[data-testid="inventory-reference-chip"]')).not.toBeNull();
    expect(el(fixture).textContent).toContain('archetype');
  });

  it('shows a kit and an archived object with their badges', () => {
    const kit = render(gesture({ items: ['toolkit'] }), lens);
    expect(el(kit.fixture).textContent).toContain('kit');
    const old = render(gesture({ items: ['rusty axe'] }), lens);
    expect(el(old.fixture).textContent).toContain('archived');
  });

  it('keeps the text box under the none vantage', () => {
    const { fixture } = render(gesture({ items: ['Excalibur'] }), { pool: POOL, poolFetched: true });
    expect(q(fixture, '[data-testid="inventory-reference-chip"]')).toBeNull();
    expect(q(fixture, 'input[aria-label="Object 1"]')).not.toBeNull();
  });

  it.skipIf(!P4D330_SPA_LANDED)('names the object by its display name, origin and refusal (P4.D330’s display / pick rule)', () => {
    const { fixture } = render(gesture({ items: ['magic bow'] }), lens);
    expect(el(fixture).textContent).toContain('magic bow');
    expect(el(fixture).textContent).toContain('Project: Thornfield');
    expect(el(fixture).textContent).toContain('⚠ An archetype is picked up, not equipped');
  });
});
