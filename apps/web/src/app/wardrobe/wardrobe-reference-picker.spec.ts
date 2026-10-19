import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { WardrobeItemDto, WardrobeSlotType } from '../core/core-contract';
import type { WardrobeVantage } from '../pascal/tool-draft';
import {
  GENERAL_ITEMS,
  IMOGEN,
  OWN_ITEMS,
  wardrobeDispatchStub,
  type WardrobeStubState,
} from './__fixtures__/wardrobe-reference-pool';
import { WardrobeDialogService } from './wardrobe-dialog.service';
import {
  REFERENCE_FORM_HINT,
  WardrobeReferencePicker,
  type WardrobeReferenceForm,
} from './wardrobe-reference-picker';

/**
 * The picker alone — v4 `5cb0e0190`
 * `__tests__/unit/components/custom-tools/wardrobe-reference-picker.test.tsx`'s
 * `describe('WardrobeReferencePicker')`, transcribed name for name (the
 * section's describe lives in `side-effects-section.spec.ts`), plus the
 * v5-added cases P4.D320 item 9 names.
 */

const CHARACTER: WardrobeVantage = { kind: 'character', characterId: IMOGEN };

@Component({
  imports: [WardrobeReferencePicker],
  template: `
    <qt-wardrobe-reference-picker
      [open]="open()"
      [vantage]="vantage()"
      [characterName]="characterName()"
      [action]="action()"
      [slot]="slot()"
      [highlightId]="highlightId()"
      [reference]="reference()"
      (closed)="onClosed()"
      (referenceChange)="references.push($event)"
      (picked)="picks.push($event)"
    >
      <p qtPickerHeader>choose a wardrobe</p>
    </qt-wardrobe-reference-picker>
  `,
})
class Host {
  readonly open = signal(true);
  readonly vantage = signal<WardrobeVantage>(CHARACTER);
  readonly characterName = signal<string | null>('Imogen');
  readonly action = signal<'wear' | 'take_off' | undefined>(undefined);
  readonly slot = signal<WardrobeSlotType | null>(null);
  readonly highlightId = signal<string | null>(null);
  readonly reference = signal<WardrobeReferenceForm>('title');
  closes = 0;
  readonly references: WardrobeReferenceForm[] = [];
  readonly picks: Array<{ item: WardrobeItemDto; reference: WardrobeReferenceForm }> = [];
  onClosed(): void {
    this.closes++;
    this.open.set(false);
  }
}

let state: WardrobeStubState;
const dialog = { open: vi.fn(), close: vi.fn() };

beforeEach(() => {
  state = { own: OWN_ITEMS, general: GENERAL_ITEMS, calls: [] };
  dialog.open.mockReset();
});
afterEach(() => TestBed.resetTestingModule());

async function render(set: (host: Host) => void = () => undefined): Promise<ComponentFixture<Host>> {
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: wardrobeDispatchStub(state) },
      { provide: WardrobeDialogService, useValue: dialog },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  set(fixture.componentInstance);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  await fixture.whenStable();
}

const root = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const optionTitles = (f: ComponentFixture<unknown>) =>
  [...root(f).querySelectorAll('[data-testid="wardrobe-reference-option"]')].map((el) =>
    el.getAttribute('aria-label'),
  );
const region = (f: ComponentFixture<unknown>, name: string) =>
  root(f).querySelector<HTMLElement>(`section[aria-label="${name}"]`);
const labelled = (f: ComponentFixture<unknown>, text: string) =>
  [...root(f).querySelectorAll('label')].find((l) => l.textContent?.trim() === text)!.querySelector('input')!;

describe('WardrobeReferencePicker', () => {
  it('sections outfits apart from garments, grouped by slot', async () => {
    const fixture = await render();
    const outfits = region(fixture, 'Outfits')!;
    expect(outfits.textContent).toContain('Garden Party');
    const garments = region(fixture, 'Garments')!;
    const headings = [...garments.querySelectorAll('h4')].map((h) => h.textContent?.trim());
    expect(headings).toContain('Tops');
    expect(headings).toContain('Footwear');
    expect(garments.textContent).not.toContain('Garden Party');
    // Shared garments carry their origin and reach.
    expect(garments.textContent).toContain('Shared · Quilltap General');
    expect(garments.textContent).toMatch(/Reach: everyone/);
  });

  it('filters by title', async () => {
    const fixture = await render();
    const search = root(fixture).querySelector<HTMLInputElement>('input[aria-label="Search the wardrobe"]')!;
    search.value = 'boot';
    search.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(optionTitles(fixture)).toEqual(['Pick Riding Boots']);
  });

  it('hides garments that do not cover the named slot', async () => {
    const fixture = await render((h) => h.slot.set('footwear'));
    expect(root(fixture).textContent).toContain('Riding Boots');
    expect(root(fixture).textContent).not.toContain('Linen Shirt');
    labelled(fixture, 'Covers Footwear only').click();
    fixture.detectChanges();
    expect(root(fixture).textContent).toContain('Linen Shirt');
  });

  it('hides archived items for wear and shows them for take_off', async () => {
    const wear = await render((h) => h.action.set('wear'));
    expect(root(wear).textContent).toContain('Linen Shirt');
    expect(root(wear).textContent).not.toContain('Old Coat');
    TestBed.resetTestingModule();
    const takeOff = await render((h) => h.action.set('take_off'));
    expect(root(takeOff).textContent).toContain('Old Coat');
  });

  it('picks by title, and by id when asked', async () => {
    const titled = await render();
    root(titled).querySelector<HTMLButtonElement>('button[aria-label="Pick Sundress"]')!.click();
    expect(titled.componentInstance.picks[0].item.id).toBe('dress');
    expect(titled.componentInstance.picks[0].reference).toBe('title');
    expect(titled.componentInstance.closes).toBe(1);
    TestBed.resetTestingModule();

    const byId = await render((h) => h.reference.set('id'));
    root(byId).querySelector<HTMLButtonElement>('button[aria-label="Pick Sundress"]')!.click();
    expect(byId.componentInstance.picks[0].item.id).toBe('dress');
    expect(byId.componentInstance.picks[0].reference).toBe('id');
  });

  it('says nothing but the prompt for the none vantage', async () => {
    const fixture = await render((h) => h.vantage.set({ kind: 'none' }));
    expect(root(fixture).textContent).toContain('choose a wardrobe');
    expect(root(fixture).querySelector('input[aria-label="Search the wardrobe"]')).toBeNull();
    expect(state.calls).toEqual([]);
  });

  it('shows the empty state for a wardrobe with nothing in it', async () => {
    state.own = [];
    state.general = [];
    const fixture = await render();
    expect(root(fixture).textContent).toMatch(/Nothing hangs here yet/);
  });
});

describe('WardrobeReferencePicker — the v5-added cases (P4.D320 item 9)', () => {
  it("falls back to the ASCII 's own / their own for an origin-less item", async () => {
    state.own = OWN_ITEMS.map((i) => ({ ...i, origin: undefined }));
    const named = await render();
    expect(region(named, 'Garments')!.textContent).toContain("Imogen's own");
    TestBed.resetTestingModule();
    const nameless = await render((h) => h.characterName.set(null));
    expect(region(nameless, 'Garments')!.textContent).toContain('their own');
  });

  it('stars a default outfit', async () => {
    state.own = OWN_ITEMS.map((i) => (i.id === 'ensemble' ? { ...i, isDefault: true } : i));
    const fixture = await render();
    const star = region(fixture, 'Outfits')!.querySelector('span[title="Default outfit"]')!;
    expect(star.getAttribute('aria-label')).toBe('Default outfit');
    expect(star.textContent).toBe('★ ');
  });

  it('marks the referenced item current', async () => {
    const fixture = await render((h) => h.highlightId.set('dress'));
    const current = root(fixture).querySelectorAll('[aria-current="true"]');
    expect(current).toHaveLength(1);
    expect(current[0].getAttribute('aria-label')).toBe('Pick Sundress');
    expect(current[0].closest('li')!.className).toBe('qt-bg-muted rounded');
  });

  it('expands an outfit into its pieces, naming one out of reach', async () => {
    state.general = [];
    const fixture = await render();
    const expander = root(fixture).querySelector<HTMLButtonElement>(
      'button[aria-label="Show what Garden Party is made of"]',
    )!;
    expander.click();
    fixture.detectChanges();
    const list = root(fixture).querySelector('ul[aria-label="Garden Party components"]')!;
    const lines = [...list.querySelectorAll('li')].map((li) => li.textContent?.trim());
    expect(lines).toEqual(['Linen Shirt — Top', 'a piece no longer in reach']);
    expect(
      root(fixture).querySelector('button[aria-label="Hide what Garden Party is made of"]'),
    ).not.toBeNull();
  });

  it('opens the Wardrobe on the character, or unscoped for a shared wardrobe, then closes', async () => {
    state.own = [];
    state.general = [];
    const character = await render();
    [...root(character).querySelectorAll('button')]
      .find((b) => b.textContent?.trim() === 'Open the Wardrobe')!
      .click();
    expect(dialog.open).toHaveBeenCalledWith({ characterId: IMOGEN });
    expect(character.componentInstance.closes).toBe(1);
    TestBed.resetTestingModule();

    const shared = await render((h) =>
      h.vantage.set({ kind: 'container', container: { scope: 'general', id: null } }),
    );
    [...root(shared).querySelectorAll('button')]
      .find((b) => b.textContent?.trim() === 'Open the Wardrobe')!
      .click();
    expect(dialog.open).toHaveBeenLastCalledWith(undefined);
  });

  it('resets its filters each time it opens', async () => {
    const fixture = await render();
    const search = () =>
      root(fixture).querySelector<HTMLInputElement>('input[aria-label="Search the wardrobe"]')!;
    search().value = 'boot';
    search().dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(optionTitles(fixture)).toEqual(['Pick Riding Boots']);
    fixture.componentInstance.open.set(false);
    fixture.detectChanges();
    fixture.componentInstance.open.set(true);
    await settle(fixture);
    expect(search().value).toBe('');
    expect(optionTitles(fixture).length).toBeGreaterThan(1);
  });

  it('shows the id hint only when referencing by id, and reports a switch', async () => {
    const fixture = await render();
    expect(root(fixture).textContent).not.toContain(REFERENCE_FORM_HINT);
    const idRadio = labelled(fixture, 'id');
    idRadio.click();
    expect(fixture.componentInstance.references).toEqual(['id']);
    fixture.componentInstance.reference.set('id');
    fixture.detectChanges();
    expect(root(fixture).querySelector('p.qt-hint')!.textContent).toBe(REFERENCE_FORM_HINT);
  });

  it('closes the picker alone on Escape', async () => {
    const fixture = await render();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(fixture.componentInstance.closes).toBe(1);
  });
});
