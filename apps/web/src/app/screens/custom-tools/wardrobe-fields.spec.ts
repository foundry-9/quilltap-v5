import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { emptyDraftWardrobeGesture, type DraftWardrobeGesture } from '../../pascal/tool-draft';
import { WARDROBE_SLOT_TYPES } from '../../wardrobe/slot-meta';
import { GENERAL_ITEMS, OWN_ITEMS } from '../../wardrobe/__fixtures__/wardrobe-reference-pool';
import { WARDROBE_ACTIONS, WARDROBE_MODES, WardrobeFields, slotIsCoupled } from './wardrobe-fields';

/**
 * WardrobeFields — asserted against v4 `SideEffectsSection.tsx:308-426`
 * (`f0db8cbb8`, pinned at `25b8fab6a`): Mode only on `wear`; Slot on
 * `clear_slot`, `take_off`, and `wear` + `add_to_slot`, its blank option worded
 * by whether the slot is optional; one input per item with `Item N` labels
 * and v4's two placeholders; Add item dead at eight; remove dead at one row and
 * never below `['']`; every edit the spread of the PREVIOUS gesture.
 */

async function render(
  over: Partial<DraftWardrobeGesture> = {},
  opts: { hasErrors?: boolean; disabled?: boolean } = {},
): Promise<{ fixture: ComponentFixture<WardrobeFields>; emitted: DraftWardrobeGesture[] }> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [WardrobeFields] });
  const fixture = TestBed.createComponent(WardrobeFields);
  fixture.componentRef.setInput('gesture', { ...emptyDraftWardrobeGesture(), ...over });
  fixture.componentRef.setInput('hasErrors', opts.hasErrors ?? false);
  fixture.componentRef.setInput('disabled', opts.disabled ?? false);
  const emitted: DraftWardrobeGesture[] = [];
  fixture.componentInstance.gestureChange.subscribe((g) => emitted.push(g));
  fixture.detectChanges();
  return { fixture, emitted };
}

const root = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const select = (f: ComponentFixture<unknown>, label: string) =>
  root(f).querySelector<HTMLSelectElement>(`select[aria-label="${label}"]`);
const items = (f: ComponentFixture<unknown>) =>
  [...root(f).querySelectorAll<HTMLInputElement>('input[type="text"]')];
const button = (f: ComponentFixture<unknown>, label: string) =>
  [...root(f).querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent?.trim() === label);
const removes = (f: ComponentFixture<unknown>) =>
  [...root(f).querySelectorAll<HTMLButtonElement>('button[title="Remove this item"]')];

describe('WardrobeFields (v4 SideEffectsSection.tsx WardrobeFields)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('offers v4’s three actions, lowercase', async () => {
    const { fixture } = await render();
    expect(Array.from(select(fixture, 'Wardrobe action')!.options).map((o) => [o.value, o.textContent?.trim()])).toEqual([
      ['wear', 'wear'],
      ['take_off', 'take off'],
      ['clear_slot', 'clear a slot'],
    ]);
    expect(WARDROBE_ACTIONS.map((a) => a.value)).toEqual(['wear', 'take_off', 'clear_slot']);
  });

  it('shows Mode only on wear, each option carrying v4’s title', async () => {
    const wear = await render({ action: 'wear' });
    const mode = select(wear.fixture, 'Wear mode');
    expect(Array.from(mode!.options).map((o) => [o.value, o.textContent?.trim(), o.title])).toEqual([
      ['wear', 'as the item says', "Honour each item's own replace flag"],
      ['replace', 'replace', 'Swap out whatever is in the slots the item covers'],
      ['add_to_slot', 'layer into one slot', 'Add the item to one named slot'],
    ]);
    expect(WARDROBE_MODES.map((m) => m.title)).toEqual([
      "Honour each item's own replace flag",
      'Swap out whatever is in the slots the item covers',
      'Add the item to one named slot',
    ]);
    expect(select((await render({ action: 'take_off' })).fixture, 'Wear mode')).toBeNull();
    expect(select((await render({ action: 'clear_slot' })).fixture, 'Wear mode')).toBeNull();
  });

  it('shows Slot on clear_slot / take_off / wear+add_to_slot, worded by whether it is optional', async () => {
    const blank = (f: ComponentFixture<unknown>) => select(f, 'Wardrobe slot')?.options[0]?.textContent?.trim();
    const clear = await render({ action: 'clear_slot' });
    expect(blank(clear.fixture)).toBe('choose a slot');
    const takeOff = await render({ action: 'take_off' });
    expect(blank(takeOff.fixture)).toBe('every slot it covers');
    const layer = await render({ action: 'wear', mode: 'add_to_slot' });
    expect(blank(layer.fixture)).toBe('choose a slot');
    expect(select((await render({ action: 'wear', mode: 'wear' })).fixture, 'Wardrobe slot')).toBeNull();
    expect(select((await render({ action: 'wear', mode: 'replace' })).fixture, 'Wardrobe slot')).toBeNull();
    // Then the registry's five slots by their labels.
    expect(
      Array.from(select(clear.fixture, 'Wardrobe slot')!.options)
        .slice(1)
        .map((o) => [o.value, o.textContent?.trim()]),
    ).toEqual([
      ['top', 'Top'],
      ['bottom', 'Bottom'],
      ['footwear', 'Footwear'],
      ['accessories', 'Accessories'],
      ['hair', 'Hair'],
    ]);
  });

  it('reads the slot list from the wardrobe registry, v4’s five in v4’s order (L-B’s tripwire)', () => {
    expect(WARDROBE_SLOT_TYPES).toEqual(['top', 'bottom', 'footwear', 'accessories', 'hair']);
  });

  it('shows no item rows on clear_slot, and one labelled row per item otherwise', async () => {
    expect(items((await render({ action: 'clear_slot', items: ['Wig'] })).fixture)).toHaveLength(0);
    const { fixture } = await render({ items: ['Cloak of Night', 'Boots'] });
    const rows = items(fixture);
    expect(rows.map((r) => r.getAttribute('aria-label'))).toEqual(['Item 1', 'Item 2']);
    expect(rows.map((r) => r.value)).toEqual(['Cloak of Night', 'Boots']);
    expect(rows.map((r) => r.placeholder)).toEqual(['Cloak of Night', '{{params.garment}}']);
    expect(rows[0].getAttribute('maxlength')).toBe('200');
    // An empty list still shows one blank row.
    expect(items((await render({ items: [] })).fixture).map((r) => r.value)).toEqual(['']);
  });

  it('pins the item hint bytes', async () => {
    const { fixture } = await render();
    expect(root(fixture).querySelector('.qt-hint')?.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'a title or id — {{params.x}} lets a parameter choose',
    );
  });

  it('kills Add item at eight, with v4’s title', async () => {
    const seven = await render({ items: Array.from({ length: 7 }, (_, i) => `Item ${i}`) });
    expect(button(seven.fixture, 'Add item')!.disabled).toBe(false);
    expect(button(seven.fixture, 'Add item')!.hasAttribute('title')).toBe(false);
    button(seven.fixture, 'Add item')!.click();
    expect(seven.emitted.at(-1)!.items).toHaveLength(8);
    expect(seven.emitted.at(-1)!.items.at(-1)).toBe('');
    const eight = await render({ items: Array.from({ length: 8 }, (_, i) => `Item ${i}`) });
    expect(button(eight.fixture, 'Add item')!.disabled).toBe(true);
    expect(button(eight.fixture, 'Add item')!.title).toBe('At most 8 items');
  });

  it('kills remove at one row, and removing never leaves fewer than one blank row', async () => {
    const one = await render({ items: ['Hat'] });
    expect(removes(one.fixture)[0].disabled).toBe(true);
    one.fixture.componentInstance.removeItem(0);
    expect(one.emitted.at(-1)!.items).toEqual(['']);
    const two = await render({ items: ['Hat', 'Boots'] });
    removes(two.fixture)[0].click();
    expect(two.emitted.at(-1)!.items).toEqual(['Boots']);
  });

  it('edits spread the PREVIOUS gesture (the other fields survive)', async () => {
    const { fixture, emitted } = await render({ action: 'take_off', items: ['Sundress'], slot: 'top', mode: 'replace' });
    const row = items(fixture)[0];
    row.value = 'Gown';
    row.dispatchEvent(new Event('input'));
    expect(emitted.at(-1)).toEqual({ action: 'take_off', items: ['Gown'], slot: 'top', mode: 'replace' });
    const slot = select(fixture, 'Wardrobe slot')!;
    slot.value = 'hair';
    slot.dispatchEvent(new Event('change'));
    expect(emitted.at(-1)).toEqual({ action: 'take_off', items: ['Sundress'], slot: 'hair', mode: 'replace' });
    const action = select(fixture, 'Wardrobe action')!;
    action.value = 'clear_slot';
    action.dispatchEvent(new Event('change'));
    expect(emitted.at(-1)).toEqual({ action: 'clear_slot', items: ['Sundress'], slot: 'top', mode: 'replace' });
  });

  it('marks the item inputs on error, and disables everything when disabled', async () => {
    const errored = await render({ items: [''] }, { hasErrors: true });
    expect(items(errored.fixture)[0].classList.contains('qt-input-error')).toBe(true);
    const clean = await render({ items: [''] });
    expect(items(clean.fixture)[0].classList.contains('qt-input-error')).toBe(false);
    const off = await render({ action: 'wear', mode: 'add_to_slot', items: ['Hat', 'Boots'] }, { disabled: true });
    expect(select(off.fixture, 'Wardrobe action')!.disabled).toBe(true);
    expect(select(off.fixture, 'Wear mode')!.disabled).toBe(true);
    expect(select(off.fixture, 'Wardrobe slot')!.disabled).toBe(true);
    expect(items(off.fixture).every((r) => r.disabled)).toBe(true);
    expect(button(off.fixture, 'Add item')!.disabled).toBe(true);
    expect(removes(off.fixture).every((b) => b.disabled)).toBe(true);
  });
});

/** The wardrobe lens on the fields themselves (v4 `5cb0e0190`, P4.D320 item 11). */
describe('WardrobeFields — the lens', () => {
  afterEach(() => TestBed.resetTestingModule());
  const POOL = [...OWN_ITEMS, ...GENERAL_ITEMS];

  async function lens(over: Partial<DraftWardrobeGesture>, vantageNone = false) {
    const r = await render(over);
    r.fixture.componentRef.setInput('pool', POOL);
    r.fixture.componentRef.setInput('poolFetched', true);
    r.fixture.componentRef.setInput(
      'vantage',
      vantageNone ? { kind: 'none' } : { kind: 'character', characterId: 'char-imogen' },
    );
    r.fixture.componentRef.setInput('vantageName', 'Imogen');
    r.fixture.componentRef.setInput('characterName', 'Imogen');
    r.fixture.detectChanges();
    return r;
  }
  const slotOptions = (f: ComponentFixture<unknown>) =>
    [...(select(f, 'Wardrobe slot')?.options ?? [])].map((o) => o.textContent);

  it('couples the slot under add_to_slot and take_off only', () => {
    const g = emptyDraftWardrobeGesture();
    expect(slotIsCoupled({ ...g, action: 'wear', mode: 'add_to_slot' })).toBe(true);
    expect(slotIsCoupled({ ...g, action: 'take_off' })).toBe(true);
    expect(slotIsCoupled({ ...g, action: 'wear', mode: 'replace' })).toBe(false);
    expect(slotIsCoupled({ ...g, action: 'clear_slot' })).toBe(false);
  });

  it('judges coverage on the concrete rows alone, a templated row left out', async () => {
    // v4's code (not its comment): templated rows are filtered out of the
    // concrete set, so a resolved Sundress beside one still couples the slot.
    const { fixture } = await lens({
      action: 'wear',
      mode: 'add_to_slot',
      slot: 'footwear',
      items: ['Sundress', '{{params.garment}}'],
    });
    expect(slotOptions(fixture)).toContain('Footwear (not covered)');
    expect(root(fixture).querySelectorAll('[data-testid="wardrobe-slot-advisory"]')).toHaveLength(1);
  });

  it('flags nothing while a concrete row is unresolved', async () => {
    const { fixture } = await lens({
      action: 'take_off',
      slot: 'footwear',
      items: ['Sundress', 'Cloak of Nite'],
    });
    expect(slotOptions(fixture).some((o) => o?.includes('(not covered)'))).toBe(false);
  });

  it('flags the uncovered slots for a take_off whose items all resolved', async () => {
    const { fixture } = await lens({ action: 'take_off', slot: 'footwear', items: ['Sundress'] });
    expect(slotOptions(fixture)).toContain('Footwear (not covered)');
    expect(slotOptions(fixture)).toContain('Bottom');
    expect(root(fixture).querySelector('[data-testid="wardrobe-slot-advisory"]')!.textContent).toBe(
      '⚠ “Sundress” covers Top, Bottom — not Footwear',
    );
  });

  it('resolves nothing under the none vantage', async () => {
    const { fixture } = await lens({ action: 'wear', items: ['Sundress'] }, true);
    expect(root(fixture).querySelector('[data-testid="wardrobe-reference-chip"]')).toBeNull();
    expect(root(fixture).querySelector('[data-testid="wardrobe-reference-unresolved"]')).toBeNull();
    expect(items(fixture)[0].value).toBe('Sundress');
  });

  it('keeps the archived warning to wear alone', async () => {
    const worn = await lens({ action: 'wear', items: ['Old Coat'] });
    expect(root(worn.fixture).textContent).toContain('⚠ Item "Old Coat" is archived and cannot be worn');
    const removed = await lens({ action: 'take_off', items: ['Old Coat'] });
    expect(root(removed.fixture).textContent).not.toContain('is archived and cannot be worn');
    expect(root(removed.fixture).textContent).toContain('archived');
  });

  it('asks the host to open the picker, with the chip\'s item highlighted', async () => {
    const { fixture } = await lens({ action: 'wear', items: ['Sundress', ''] });
    const asks: Array<{ index: number; highlightId: string | null }> = [];
    fixture.componentInstance.choose.subscribe((a) => asks.push(a));
    root(fixture).querySelector<HTMLButtonElement>('[data-testid="wardrobe-reference-chip"]')!.click();
    button(fixture, 'Choose…')!.click();
    expect(asks).toEqual([
      { index: 0, highlightId: 'dress' },
      { index: 1, highlightId: null },
    ]);
    expect(
      root(fixture)
        .querySelector('[data-testid="wardrobe-reference-chip"]')!
        .getAttribute('aria-label'),
    ).toBe('Item 1: Sundress — choose another');
  });

  it('titles the toggle with the reference hint', async () => {
    const { fixture } = await lens({ action: 'wear', items: ['dress'] });
    const toggle = root(fixture).querySelector<HTMLButtonElement>(
      'button[aria-label="Reference by title instead"]',
    )!;
    expect(toggle.textContent?.trim()).toBe('by id');
    expect(toggle.title).toBe(
      'Referenced by id — click to switch. An id names this exact garment and no other; a title names whichever garment of that name is nearest when the tool runs.',
    );
  });
});

