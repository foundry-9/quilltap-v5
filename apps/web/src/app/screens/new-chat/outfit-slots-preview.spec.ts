import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { OutfitPreviewSlots } from '../../core/core-contract';
import { OutfitSlotsPreview } from './outfit-slots-preview';

function render(slots: OutfitPreviewSlots): ComponentFixture<OutfitSlotsPreview> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [OutfitSlotsPreview] });
  const fixture = TestBed.createComponent(OutfitSlotsPreview);
  fixture.componentRef.setInput('slots', slots);
  fixture.detectChanges();
  return fixture;
}

const EMPTY: OutfitPreviewSlots = {
  top: [],
  bottom: [],
  footwear: [],
  accessories: [],
  hair: [],
};

const entry = (id: string, title: string) => ({ id, title, isComposite: false });

/**
 * The Green Room's read-only outfit preview (v4 `OutfitSlotsPreview.tsx`).
 *
 * Measured against v4's real component 2026-08-18: it renders a card for
 * EVERY slot, empty or not — `reportWhenEmpty` is a `lib/`-side narration rule
 * (the tool handlers and `outfit-description.ts`), and no v4 *component* reads
 * it. So an empty hair slot shows its label and v4's "nothing" vacancy here,
 * exactly like the four garment slots.
 */
describe('OutfitSlotsPreview (v4 OutfitSlotsPreview.tsx)', () => {
  it('renders one card per slot, in canonical order with Hair last', () => {
    const fixture = render(EMPTY);
    const labels = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-text-secondary'),
    ).map((el) => el.textContent?.trim());
    expect(labels).toEqual(['Top', 'Bottom', 'Footwear', 'Accessories', 'Hair']);
  });

  it('badges a worn hairdo with the rose wardrobe-hair class (v4 WARDROBE_SLOT_META)', () => {
    const fixture = render({ ...EMPTY, hair: [entry('h1', 'Marcel Waves')] });
    const badge = (fixture.nativeElement as HTMLElement).querySelector(
      '.qt-badge-wardrobe-hair',
    );
    expect(badge).not.toBeNull();
    expect(badge?.textContent?.trim()).toContain('Marcel Waves');
    // The rose badge is hair's alone — the garment slots keep their own.
    expect(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-badge-wardrobe-hair'),
    ).toHaveLength(1);
  });

  it('shows v4’s "nothing" vacancy for an EMPTY hair slot, like every other slot', () => {
    const fixture = render(EMPTY);
    const cards = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('.qt-card'));
    expect(cards).toHaveLength(5);
    for (const card of cards) expect(card.textContent).toContain('nothing');
  });

  it('renders a frame minted WITHOUT the hair key — v4’s `slots[key] ?? []` guard', () => {
    // A progress frame from a server older than the hair slot. The cast is the
    // point: frames arrive as raw JSON. Drop the `?? []` in
    // `OutfitSlotsPreview.rows` and this reads `.length` off undefined.
    const legacy = {
      top: [entry('t1', 'Waistcoat')],
      bottom: [],
      footwear: [],
      accessories: [],
    } as unknown as OutfitPreviewSlots;
    const fixture = render(legacy);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Waistcoat');
    expect(text).toContain('Hair');
    expect(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-badge-wardrobe-hair'),
    ).toHaveLength(0);
  });
});
