import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CustomToolDestinations } from '../../core/core-contract';
import {
  GENERAL_ITEMS,
  IMOGEN,
  OWN_ITEMS,
  wardrobeDispatchStub,
  type WardrobeStubState,
} from '../../wardrobe/__fixtures__/wardrobe-reference-pool';
import {
  draftFromDefinition,
  emptyDraftWardrobeGesture,
  newDraft,
  serializeDraft,
  validateDraft,
  type DraftEffect,
  type ToolDraft,
  type WardrobeVantage,
} from '../../pascal/tool-draft';
import { SideEffectsSection, whenChoiceOf } from './side-effects-section';

/**
 * SideEffectsSection — asserted against v4
 * `components/custom-tools/SideEffectsSection.tsx`. The load-bearing claims:
 * 'verbatim' is NEVER an option in the When select (it is a read-only badge,
 * so a condition richer than the form can draw cannot be silently flattened by
 * touching the control), the value-kind switch reseeds the value the way v4
 * does, the Add button stops at the cap, and every issue `validateDraft`
 * anchors to an effect row renders beside THAT row.
 */

function effect(over: Partial<DraftEffect> = {}): DraftEffect {
  return {
    id: 'e1',
    when: { kind: 'always' },
    kind: 'write',
    target: 'state.tally',
    valueKind: 'literal-number',
    value: '1',
    wardrobe: emptyDraftWardrobeGesture(),
    ...over,
  };
}

function draftWith(effects: DraftEffect[]): ToolDraft {
  const draft = newDraft();
  draft.name = 'unlock';
  draft.description = 'Pick the lock.';
  draft.outcomes[0] = { ...draft.outcomes[0], message: 'Open.' };
  return { ...draft, effects };
}

async function render(draft: ToolDraft): Promise<ComponentFixture<SideEffectsSection>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [SideEffectsSection] });
  const fixture = TestBed.createComponent(SideEffectsSection);
  fixture.componentRef.setInput('draft', draft);
  fixture.componentRef.setInput('issues', validateDraft(draft));
  fixture.detectChanges();
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function html(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).innerHTML;
}

describe('whenChoiceOf (v4 SideEffectsSection:34-38)', () => {
  it('flattens the three draft shapes onto the select values, verbatim apart', () => {
    expect(whenChoiceOf(effect({ when: { kind: 'always' } }))).toBe('always');
    expect(whenChoiceOf(effect({ when: { kind: 'outcome-state', state: 'failure' } }))).toBe(
      'failure',
    );
    expect(whenChoiceOf(effect({ when: { kind: 'verbatim', when: { gt: 0.5 } } }))).toBe('verbatim');
  });
});

describe('SideEffectsSection (v4 SideEffectsSection.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('says what an effect is for when none are declared', async () => {
    const fixture = await render(draftWith([]));
    expect(text(fixture)).toContain('None declared.');
    expect(text(fixture)).toContain('Add effect (0/16)');
  });

  it('renders a verbatim condition as a read-only badge, never as a select option', async () => {
    const fixture = await render(
      draftWith([effect({ when: { kind: 'verbatim', when: { gt: 0.5, outcome: { neq: 'failure' } } } })]),
    );
    // The badge shows the condition's own JSON, so nothing is lost to a form
    // that cannot draw it.
    expect(text(fixture)).toContain('{"gt":0.5,"outcome":{"neq":"failure"}}');
    // And the control itself is absent: there is no way to overwrite the
    // condition by nudging a select.
    expect(html(fixture)).not.toContain('aria-label="When this effect applies"');
  });

  it('offers Always and the four outcome states for a form-authored condition', async () => {
    const fixture = await render(draftWith([effect()]));
    const select = (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>(
      'select[aria-label="When this effect applies"]',
    );
    expect(select).not.toBeNull();
    expect(Array.from(select!.options).map((o) => o.value)).toEqual([
      'always',
      'success',
      'partial',
      'failure',
      'info',
    ]);
  });

  it('reseeds the value when the kind changes — true for a boolean, blank for a number', async () => {
    const fixture = await render(draftWith([effect({ valueKind: 'expression', value: '{{value}}' })]));
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));

    const kind = (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>(
      'select[aria-label="Value kind"]',
    )!;
    kind.value = 'literal-boolean';
    kind.dispatchEvent(new Event('change'));
    expect(emitted.at(-1)!.effects[0]).toMatchObject({ valueKind: 'literal-boolean', value: 'true' });

    kind.value = 'literal-number';
    kind.dispatchEvent(new Event('change'));
    expect(emitted.at(-1)!.effects[0]).toMatchObject({ valueKind: 'literal-number', value: '' });
  });

  it('keeps the expression text when the kind stays an expression', async () => {
    const fixture = await render(draftWith([effect({ valueKind: 'expression', value: '{{value}} + 1' })]));
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));

    fixture.componentInstance.updateEffect('e1', { target: 'state.other' });
    expect(emitted.at(-1)!.effects[0].value).toBe('{{value}} + 1');
  });

  it('offers the two prefix shortcuts only while the target is blank', async () => {
    const blank = await render(draftWith([effect({ target: '' })]));
    expect(text(blank)).toContain('state.');
    expect(text(blank)).toContain('metadata.');

    const filled = await render(draftWith([effect({ target: 'state.tally' })]));
    const buttons = Array.from(
      (filled.nativeElement as HTMLElement).querySelectorAll('button'),
    ).map((b) => b.textContent?.trim());
    expect(buttons).not.toContain('metadata.');
  });

  it('adds and removes rows, and stops adding at the cap', async () => {
    const fixture = await render(draftWith([]));
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));

    fixture.componentInstance.addEffect();
    expect(emitted.at(-1)!.effects).toHaveLength(1);
    // A fresh row is an expression with nothing in it — v4's defaults.
    expect(emitted.at(-1)!.effects[0]).toMatchObject({
      when: { kind: 'always' },
      target: '',
      valueKind: 'expression',
      value: '',
    });

    const full = await render(draftWith(Array.from({ length: 16 }, (_, i) => effect({ id: `e${i}` }))));
    const add = Array.from(
      (full.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('button'),
    ).find((b) => b.textContent?.includes('Add effect'))!;
    expect(add.disabled).toBe(true);
    expect(add.getAttribute('title')).toBe('At most 16 effects');

    const removing = await render(draftWith([effect({ id: 'e1' }), effect({ id: 'e2' })]));
    const seen: ToolDraft[] = [];
    removing.componentInstance.draftChange.subscribe((d) => seen.push(d));
    removing.componentInstance.removeEffect('e1');
    expect(seen.at(-1)!.effects.map((e) => e.id)).toEqual(['e2']);
  });

  it('renders the blocking errors validateDraft anchors to the row', async () => {
    const fixture = await render(
      draftWith([effect({ target: 'nowhere.x', valueKind: 'expression', value: 'broken pick' })]),
    );
    expect(text(fixture)).toContain(
      'target must start with "state.", "metadata." or "progress."',
    );
    expect(text(fixture)).toContain('the expression does not parse');
  });

  it('renders a warning beside the row rather than as an error', async () => {
    // `{{dice}}` outside the dice form is advisory — the run still deals.
    const draft = draftWith([effect({ valueKind: 'expression', value: "{{dice}} + ''" })]);
    const fixture = await render(draft);
    expect(text(fixture)).toContain('⚠');
    expect(text(fixture)).toContain('{{dice}} is an empty string outside the dice form');
  });
});

// ---------------------------------------------------------------------------
// v4 `f0db8cbb8` (pinned at `25b8fab6a`): the Kind select, the wardrobe
// branch, `newDraftEffect()` on Add, and the two copy changes. The wardrobe
// fields' own visibility rules are `wardrobe-fields.spec.ts`'s.
// ---------------------------------------------------------------------------

describe('SideEffectsSection — the Kind select and wardrobe rows (v4 f0db8cbb8)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const root = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
  const kindSelect = (f: ComponentFixture<unknown>) =>
    root(f).querySelector<HTMLSelectElement>('select[aria-label="Effect kind"]');

  function chooseKind(fixture: ComponentFixture<SideEffectsSection>, value: string): void {
    const select = kindSelect(fixture)!;
    select.value = value;
    select.dispatchEvent(new Event('change'));
  }

  it('puts the Kind select FIRST in the row’s top line, with v4’s two options', async () => {
    const fixture = await render(draftWith([effect()]));
    const select = kindSelect(fixture);
    expect(select).not.toBeNull();
    expect(Array.from(select!.options).map((o) => [o.value, o.textContent?.trim()])).toEqual([
      ['write', 'Write a value'],
      ['wardrobe', 'Change the outfit'],
    ]);
    const topLine = select!.closest('div');
    expect(topLine?.querySelector('select')).toBe(select);
    expect(select!.closest('label')?.textContent?.trim().startsWith('Kind')).toBe(true);
  });

  it('switching Write → Outfit → Write keeps BOTH halves (the target survives)', async () => {
    const fixture = await render(draftWith([effect({ target: 'state.tally' })]));
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));

    chooseKind(fixture, 'wardrobe');
    expect(emitted.at(-1)!.effects[0]).toMatchObject({ kind: 'wardrobe', target: 'state.tally' });

    fixture.componentRef.setInput('draft', emitted.at(-1)!);
    fixture.componentRef.setInput('issues', validateDraft(emitted.at(-1)!));
    fixture.detectChanges();
    chooseKind(fixture, 'write');
    expect(emitted.at(-1)!.effects[0]).toMatchObject({ kind: 'write', target: 'state.tally', value: '1' });
    expect(emitted.at(-1)!.effects[0].wardrobe).toEqual(emptyDraftWardrobeGesture());
  });

  it('shows the wardrobe fields INSTEAD of Target + Value on a wardrobe row', async () => {
    const fixture = await render(draftWith([effect({ kind: 'wardrobe' })]));
    expect(root(fixture).querySelector('qt-wardrobe-fields')).not.toBeNull();
    expect(root(fixture).querySelector('[aria-label="Effect target"]')).toBeNull();
    expect(root(fixture).querySelector('[aria-label="Value kind"]')).toBeNull();
    const write = await render(draftWith([effect()]));
    expect(root(write).querySelector('qt-wardrobe-fields')).toBeNull();
    expect(root(write).querySelector('[aria-label="Effect target"]')).not.toBeNull();
  });

  it('routes a gesture edit onto the row’s wardrobe half, and marks items on error', async () => {
    const fixture = await render(draftWith([effect({ kind: 'wardrobe' })]));
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));
    // The blank item row is an error (`wear needs at least one item`), so the
    // row's issue line renders and the item input carries the error class.
    expect(text(fixture)).toContain('wear needs at least one item');
    const item = root(fixture).querySelector<HTMLInputElement>('input[aria-label="Item 1"]')!;
    expect(item.classList.contains('qt-input-error')).toBe(true);
    item.value = 'Cloak of Night';
    item.dispatchEvent(new Event('input'));
    expect(emitted.at(-1)!.effects[0].wardrobe.items).toEqual(['Cloak of Night']);
    expect(emitted.at(-1)!.effects[0].target).toBe('state.tally');
  });

  it('adds a fresh write row through newDraftEffect()', async () => {
    const fixture = await render(draftWith([]));
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));
    fixture.componentInstance.addEffect();
    const added = emitted.at(-1)!.effects[0];
    expect(added).toMatchObject({
      when: { kind: 'always' },
      kind: 'write',
      target: '',
      valueKind: 'expression',
      value: '',
      wardrobe: emptyDraftWardrobeGesture(),
    });
    expect(added.id).toMatch(/^effect-\d+$/);
  });

  it('says a roll may dress or undress the character, in the empty sentence and the hint', async () => {
    const fixture = await render(draftWith([]));
    const rendered = text(fixture).replace(/\s+/g, ' ');
    expect(rendered).toContain(
      'None declared. An effect lets a roll record its consequences — tick a counter in the story’s state, note a fact on the rolling character’s sheet, put a garment on them or take one off — applied server-side as part of the deal, beyond any model’s power to fudge.',
    );
    expect(rendered).toContain(
      'An outfit change dresses or undresses the rolling character and nobody else; an item it cannot find is skipped and the rest still go on.',
    );
  });
});

// ---------------------------------------------------------------------------
// The wardrobe lens (v4 `5cb0e0190`)
// ---------------------------------------------------------------------------

const lensDestinations: CustomToolDestinations = {
  general: { mountPointId: 'mp-general', mountName: 'General', existingToolNames: [] },
  characters: [
    {
      characterId: IMOGEN,
      characterName: 'Imogen',
      mountPointId: 'mp-imogen',
      mountName: 'Imogen',
      existingToolNames: [],
    },
  ],
  groups: [],
  projects: [],
  other: [],
};
const CHARACTER: WardrobeVantage = { kind: 'character', characterId: IMOGEN };

function wardrobeDraft(wardrobeGesture: Record<string, unknown>): ToolDraft {
  const draft = draftFromDefinition({
    name: 'dresser',
    description: 'x',
    parameters: { garment: { type: 'string', default: 'Boots' } },
    outcomes: [{ when: true, message: 'done', state: 'info' }],
    effects: [{ wardrobe: wardrobeGesture }],
  });
  if (!draft) throw new Error('fixture failed to load');
  return draft;
}

let lensState: WardrobeStubState;

async function renderLens(
  draft: ToolDraft,
  vantage: WardrobeVantage = CHARACTER,
): Promise<{ fixture: ComponentFixture<SideEffectsSection>; changes: ToolDraft[] }> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [SideEffectsSection],
    providers: [{ provide: CoreClient, useValue: wardrobeDispatchStub(lensState) }],
  });
  const fixture = TestBed.createComponent(SideEffectsSection);
  fixture.componentRef.setInput('draft', draft);
  fixture.componentRef.setInput('issues', validateDraft(draft));
  fixture.componentRef.setInput('vantage', vantage);
  fixture.componentRef.setInput('showVantage', true);
  fixture.componentRef.setInput('destinations', lensDestinations);
  const changes: ToolDraft[] = [];
  fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => changes.push(d));
  fixture.detectChanges();
  await settleLens(fixture);
  return { fixture, changes };
}

async function settleLens(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

const q = (f: ComponentFixture<unknown>, sel: string) =>
  (f.nativeElement as HTMLElement).querySelector<HTMLElement>(sel);
const chip = (f: ComponentFixture<unknown>) => q(f, '[data-testid="wardrobe-reference-chip"]');

/** v4 `wardrobe-reference-picker.test.tsx:203-280`, transcribed name for name. */
describe('SideEffectsSection — wardrobe references', () => {
  afterEach(() => TestBed.resetTestingModule());
  const fresh = () => {
    lensState = { own: OWN_ITEMS, general: GENERAL_ITEMS, calls: [] };
  };

  it('renders a resolved reference as a chip with its slots and origin', async () => {
    fresh();
    const { fixture } = await renderLens(wardrobeDraft({ action: 'wear', items: ['riding boots'] }));
    const c = chip(fixture)!;
    expect(c.textContent).toContain('Riding Boots');
    expect(c.textContent).toContain('Footwear');
    expect(c.textContent).toContain('Shared · Quilltap General');
    expect(c.textContent).toContain('everyone');
    expect(text(fixture)).toContain('Browsing Imogen’s wardrobe');
  });

  it('names a character-owned garment as theirs', async () => {
    fresh();
    const { fixture } = await renderLens(wardrobeDraft({ action: 'wear', items: ['Linen Shirt'] }));
    const c = chip(fixture)!;
    expect(c.textContent).toContain('Imogen’s own');
    expect(c.textContent).toContain('Imogen only');
  });

  it('keeps a templated reference as text, chosen at run time', async () => {
    fresh();
    const { fixture } = await renderLens(
      wardrobeDraft({ action: 'wear', items: ['{{params.garment}}'] }),
    );
    expect((q(fixture, 'input[aria-label="Item 1"]') as HTMLInputElement).value).toBe(
      '{{params.garment}}',
    );
    expect(text(fixture)).toContain('chosen at run time');
    expect(lensState.calls.length).toBeGreaterThan(0);
    expect(q(fixture, '[data-testid="wardrobe-reference-unresolved"]')).toBeNull();
  });

  it('advises on an unresolved reference without making it a draft error', async () => {
    fresh();
    const draft = wardrobeDraft({ action: 'wear', items: ['Cloak of Nite'] });
    const { fixture } = await renderLens(draft);
    expect(q(fixture, '[data-testid="wardrobe-reference-unresolved"]')!.textContent).toBe(
      'not in Imogen’s reach',
    );
    expect(validateDraft(draft).filter((issue) => issue.severity === 'error')).toEqual([]);
  });

  it('warns when an archived garment is to be worn', async () => {
    fresh();
    const { fixture } = await renderLens(wardrobeDraft({ action: 'wear', items: ['Old Coat'] }));
    expect(text(fixture)).toContain('⚠ Item "Old Coat" is archived and cannot be worn');
  });

  it('flags slots the chosen items do not cover under add_to_slot', async () => {
    fresh();
    const { fixture } = await renderLens(
      wardrobeDraft({ action: 'wear', items: ['Sundress'], mode: 'add_to_slot', slot: 'footwear' }),
    );
    expect(q(fixture, '[data-testid="wardrobe-slot-advisory"]')!.textContent).toBe(
      '⚠ “Sundress” covers Top, Bottom — not Footwear',
    );
    const options = [
      ...(fixture.nativeElement as HTMLElement).querySelectorAll('select[aria-label="Wardrobe slot"] option'),
    ].map((o) => o.textContent);
    expect(options).toContain('Footwear (not covered)');
    expect(options).toContain('Top');
  });

  it('switches a chip between title and id', async () => {
    fresh();
    const { fixture, changes } = await renderLens(wardrobeDraft({ action: 'wear', items: ['Sundress'] }));
    q(fixture, 'button[aria-label="Reference by id instead"]')!.click();
    expect(changes[0].effects[0].wardrobe.items).toEqual(['dress']);
  });

  it('fills a reference from the picker, and the vantage never reaches the file', async () => {
    fresh();
    const loaded = wardrobeDraft({ action: 'wear', items: ['placeholder'] });
    const draft = {
      ...loaded,
      effects: loaded.effects.map((e) => ({ ...e, wardrobe: { ...e.wardrobe, items: [''] } })),
    };
    const { fixture, changes } = await renderLens(draft);
    [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')]
      .find((b) => b.textContent?.trim() === 'Choose…')!
      .click();
    fixture.detectChanges();
    await settleLens(fixture);
    q(fixture, 'button[aria-label="Pick Riding Boots"]')!.click();
    const next = changes[0];
    expect(next.effects[0].wardrobe.items).toEqual(['Riding Boots']);
    const saved = JSON.parse(serializeDraft(next));
    expect(saved.effects).toEqual([{ wardrobe: { action: 'wear', items: ['Riding Boots'] } }]);
    expect(serializeDraft(next)).not.toContain(IMOGEN);
  });

  it('serializes byte-identically whatever the vantage', async () => {
    fresh();
    const draft = wardrobeDraft({ action: 'wear', items: ['Sundress'] });
    const before = serializeDraft(draft);
    const { fixture } = await renderLens(draft);
    expect(chip(fixture)).not.toBeNull();
    expect(serializeDraft(draft)).toBe(before);
  });
});

/** The v5-added lens cases (P4.D320 item 11). */
describe('SideEffectsSection — the wardrobe lens, v5 cases', () => {
  afterEach(() => TestBed.resetTestingModule());
  const fresh = () => {
    lensState = { own: OWN_ITEMS, general: GENERAL_ITEMS, calls: [] };
  };

  it('spells an origin-less own garment Their own, with a curly apostrophe when named', async () => {
    fresh();
    lensState.own = OWN_ITEMS.map((i) => ({ ...i, origin: undefined }));
    const named = await renderLens(wardrobeDraft({ action: 'wear', items: ['Linen Shirt'] }));
    expect(chip(named.fixture)!.textContent).toContain('Imogen’s own');
    fresh();
    lensState.general = GENERAL_ITEMS.map((i) => ({ ...i, origin: undefined }));
    const unnamed = await renderLens(wardrobeDraft({ action: 'wear', items: ['Riding Boots'] }), {
      kind: 'container',
      container: { scope: 'general', id: null },
    });
    // A shared vantage names no character: v4's `Their own`.
    expect(chip(unnamed.fixture)!.textContent).toContain('Their own');
  });

  it('names a shared wardrobe in the advisory', async () => {
    fresh();
    const { fixture } = await renderLens(wardrobeDraft({ action: 'wear', items: ['Cloak of Nite'] }), {
      kind: 'container',
      container: { scope: 'general', id: null },
    });
    expect(q(fixture, '[data-testid="wardrobe-reference-unresolved"]')!.textContent).toBe(
      'not in Quilltap General',
    );
  });

  it('says nothing about a reference before the pool has answered', async () => {
    fresh();
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SideEffectsSection],
      providers: [{ provide: CoreClient, useValue: { dispatchData: () => new Promise(() => undefined) } }],
    });
    const draft = wardrobeDraft({ action: 'wear', items: ['Cloak of Nite'] });
    const fixture = TestBed.createComponent(SideEffectsSection);
    fixture.componentRef.setInput('draft', draft);
    fixture.componentRef.setInput('issues', validateDraft(draft));
    fixture.componentRef.setInput('vantage', CHARACTER);
    fixture.detectChanges();
    await settleLens(fixture);
    expect(q(fixture, '[data-testid="wardrobe-reference-unresolved"]')).toBeNull();
  });

  it('reads nothing without a wardrobe row, and offers no vantage select', async () => {
    fresh();
    const { fixture } = await renderLens(
      draftWith([effect({ id: 'w', kind: 'write', target: 'state.x', value: '1' })]),
    );
    expect(lensState.calls).toEqual([]);
    expect(q(fixture, 'qt-wardrobe-vantage-select')).toBeNull();
  });

  it('opens the picker from Choose… with nothing highlighted, and from a chip on its item', async () => {
    fresh();
    const blank = await renderLens(wardrobeDraft({ action: 'wear', items: ['Cloak of Nite'] }));
    [...(blank.fixture.nativeElement as HTMLElement).querySelectorAll('button')]
      .find((b) => b.textContent?.trim() === 'Choose…')!
      .click();
    blank.fixture.detectChanges();
    await settleLens(blank.fixture);
    expect(q(blank.fixture, '[aria-current="true"]')).toBeNull();
    expect(text(blank.fixture)).toContain('Choose a garment or outfit');

    fresh();
    const chipped = await renderLens(wardrobeDraft({ action: 'wear', items: ['Sundress'] }));
    chip(chipped.fixture)!.click();
    chipped.fixture.detectChanges();
    await settleLens(chipped.fixture);
    expect(q(chipped.fixture, '[aria-current="true"]')!.getAttribute('aria-label')).toBe(
      'Pick Sundress',
    );
  });

  it('closes the picker on Escape and returns focus to the control that opened it', async () => {
    fresh();
    const { fixture } = await renderLens(wardrobeDraft({ action: 'wear', items: ['Cloak of Nite'] }));
    const choose = [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Choose…',
    )!;
    document.body.appendChild(fixture.nativeElement);
    choose.focus();
    choose.click();
    fixture.detectChanges();
    await settleLens(fixture);
    expect(text(fixture)).toContain('Choose a garment or outfit');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    fixture.detectChanges();
    await settleLens(fixture);
    expect(text(fixture)).not.toContain('Choose a garment or outfit');
    expect(document.activeElement).toBe(choose);
    (fixture.nativeElement as HTMLElement).remove();
  });
});

