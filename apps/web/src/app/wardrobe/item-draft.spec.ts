import { describe, expect, it } from 'vitest';

import type { WardrobeItemDto } from '../core/core-contract';
import {
  draftFromItem,
  draftToPayload,
  emptyDraft,
  payloadForEdit,
  validateDraft,
} from './item-draft';

/**
 * P4.D286 — v4 `__tests__/unit/lib/wardrobe/item-draft.test.ts` at the pin
 * `4a38222e6`, transcribed case for case (5), plus the zod-free validator's
 * third rule (the folder cap, zod 4.6.5's sentence measured at the pin).
 */
const coat = {
  id: '00000000-0000-4000-8000-000000000001',
  title: 'Charcoal Coat',
  description: null,
  imagePrompt: 'charcoal wool greatcoat',
  types: ['top'],
  appropriateness: 'formal',
  isDefault: true,
  componentItemIds: [],
  replace: true,
} as unknown as WardrobeItemDto;

describe('item drafts', () => {
  it('round-trips an item into the body the routes accept', () => {
    expect(draftToPayload(draftFromItem(coat))).toEqual({
      title: 'Charcoal Coat',
      description: null,
      imagePrompt: 'charcoal wool greatcoat',
      types: ['top'],
      appropriateness: 'formal',
      isDefault: true,
      componentItemIds: [],
      // A leaf always replaces its slots; the flag is composite-only.
      replace: false,
      // An item read without a folder sits at the top level.
      folder: '',
    });
  });

  it('keeps replace for a composite and sends blank text as null', () => {
    const payload = draftToPayload(
      emptyDraft({
        title: '  Naked  ',
        types: ['top', 'bottom'],
        componentItemIds: ['ring'],
        replace: true,
        description: '   ',
      }),
    );
    expect(payload.title).toBe('Naked');
    expect(payload.description).toBeNull();
    expect(payload.replace).toBe(true);
  });

  it('validates through the routes’ own schema', () => {
    expect(validateDraft(emptyDraft({ types: ['top'] }))).toEqual({
      ok: false,
      error: 'Title is required',
    });
    expect(validateDraft(emptyDraft({ title: 'Hat' })).ok).toBe(false);
    const ok = validateDraft(emptyDraft({ title: 'Hat', types: ['accessories'] }));
    expect(ok.ok).toBe(true);
  });

  it('reports the schema’s first issue in key order, and the folder cap (v5)', () => {
    expect(validateDraft(emptyDraft())).toEqual({ ok: false, error: 'Title is required' });
    expect(validateDraft(emptyDraft({ title: 'Hat' }))).toEqual({
      ok: false,
      error: 'At least one type is required',
    });
    expect(
      validateDraft(emptyDraft({ title: 'Hat', types: ['accessories'], folder: 'x'.repeat(901) })),
    ).toEqual({ ok: false, error: 'Too big: expected string to have <=900 characters' });
    expect(
      validateDraft(emptyDraft({ title: 'Hat', types: ['accessories'], folder: 'x'.repeat(900) }))
        .ok,
    ).toBe(true);
  });

  it('carries the item’s folder into the draft (v5)', () => {
    expect(draftFromItem({ ...coat, folder: 'Formal/Evening' }).folder).toBe('Formal/Evening');
  });
});

describe('payloadForEdit', () => {
  const base = draftToPayload(
    emptyDraft({ title: 'Boater', types: ['accessories'], folder: 'Hats: Summer' }),
  );

  it('drops an unchanged folder (any casing) so a hand-made one is never re-resolved', () => {
    expect(payloadForEdit(base, 'hats: summer')).not.toHaveProperty('folder');
  });

  it('keeps a folder that changed', () => {
    expect(payloadForEdit(base, '').folder).toBe('Hats: Summer');
  });
});
