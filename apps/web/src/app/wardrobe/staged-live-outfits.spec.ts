/**
 * Bug 61 regression coverage — the Wardrobe dialog's Live tab staging helpers.
 *
 * Ported case-for-case from v4's
 * `__tests__/unit/lib/wardrobe/staged-live-outfits.test.ts` (`07d4ccce`), plus
 * the `equippedSlotsEqual` case that travelled with the function from
 * `equipped-slots.spec.ts`.
 *
 * Two halves of the same silent loss: a gesture made before the worn snapshot
 * arrives must survive the first seed *and* land on the real slots, and the
 * Done flush must not read "we never learned what clean was" as "nothing
 * changed".
 */

import { describe, expect, it } from 'vitest';

import {
  cloneSlots,
  EMPTY_EQUIPPED_SLOTS,
  wearItemIntoSlots,
  type EquippedSlots,
} from './equipped-slots';
import {
  classifyStagedOutfits,
  equippedSlotsEqual,
  rebaseStagedSlots,
} from './staged-live-outfits';

const slots = (partial: Partial<EquippedSlots>): EquippedSlots => ({
  ...cloneSlots(EMPTY_EQUIPPED_SLOTS),
  ...partial,
});

describe('equippedSlotsEqual', () => {
  it('compares each slot array element-wise, in order', () => {
    expect(equippedSlotsEqual(slots({ top: ['a', 'b'] }), slots({ top: ['a', 'b'] }))).toBe(true);
    expect(equippedSlotsEqual(slots({ top: ['a', 'b'] }), slots({ top: ['b', 'a'] }))).toBe(false);
    expect(equippedSlotsEqual(slots({ top: ['a'] }), slots({ top: ['a', 'b'] }))).toBe(false);
    expect(equippedSlotsEqual(slots({ top: ['a'] }), slots({ bottom: ['a'] }))).toBe(false);
  });

  // Carried over with the move from `equipped-slots.spec.ts` (v4 originally had
  // this inline in the dialog).
  it('compares the four slot arrays element-wise, in order', () => {
    const a: EquippedSlots = { top: ['x'], bottom: [], footwear: ['y'], accessories: [], hair: [] };
    expect(equippedSlotsEqual(a, cloneSlots(a))).toBe(true);
    expect(equippedSlotsEqual(a, { ...cloneSlots(a), top: ['z'] })).toBe(false);
  });
});

describe('rebaseStagedSlots', () => {
  it('returns the worn snapshot untouched when nothing was staged early', () => {
    const worn = slots({ top: ['shirt'], bottom: ['trousers'] });
    const seeded = rebaseStagedSlots(worn, []);
    expect(seeded).toEqual(worn);
    expect(seeded).not.toBe(worn);
    expect(seeded.top).not.toBe(worn.top);
  });

  it('replays an early Wear onto the real slots instead of onto empty', () => {
    // The click that lost the race: staged while the snapshot was still in
    // flight, so it had been layered onto EMPTY_EQUIPPED_SLOTS for the paint.
    const worn = slots({ top: ['shirt'], bottom: ['trousers'] });
    const wearHat = (prev: EquippedSlots): EquippedSlots =>
      wearItemIntoSlots(prev, { id: 'hat', types: ['accessories'] });

    expect(rebaseStagedSlots(worn, [wearHat])).toEqual(
      slots({ top: ['shirt'], bottom: ['trousers'], accessories: ['hat'] }),
    );
  });

  it('replays several early gestures in the order they were made', () => {
    const worn = slots({ top: ['shirt'] });
    const wearCoat = (prev: EquippedSlots): EquippedSlots =>
      wearItemIntoSlots(prev, { id: 'coat', types: ['top'] });
    const takeOffShirt = (prev: EquippedSlots): EquippedSlots => ({
      ...prev,
      top: prev.top.filter((id) => id !== 'shirt'),
    });

    expect(rebaseStagedSlots(worn, [wearCoat, takeOffShirt])).toEqual(slots({ top: ['coat'] }));
    expect(rebaseStagedSlots(worn, [takeOffShirt, wearCoat])).toEqual(slots({ top: ['coat'] }));
  });

  it('does not mutate the worn snapshot it rebases onto', () => {
    const worn = slots({ top: ['shirt'] });
    rebaseStagedSlots(worn, [(prev) => ({ ...prev, top: [...prev.top, 'coat'] })]);
    expect(worn).toEqual(slots({ top: ['shirt'] }));
  });
});

describe('classifyStagedOutfits', () => {
  it('reports a character whose staged slots differ from their baseline as dirty', () => {
    const staged = { alice: slots({ top: ['coat'] }) };
    const baselines = { alice: slots({ top: ['shirt'] }) };

    expect(classifyStagedOutfits(staged, baselines)).toEqual({
      dirty: [{ characterId: 'alice', slots: staged.alice }],
      unresolved: [],
    });
  });

  it('reports a character whose staged slots match their baseline as neither', () => {
    const staged = { alice: slots({ top: ['shirt'] }) };
    const baselines = { alice: slots({ top: ['shirt'] }) };

    expect(classifyStagedOutfits(staged, baselines)).toEqual({ dirty: [], unresolved: [] });
  });

  it('separates "no baseline" from "nothing changed" (Bug 61)', () => {
    // Before the fix both fell out of the loop as clean, so Done closed
    // reporting success having sent nothing.
    const staged = { alice: slots({ accessories: ['hat'] }) };

    expect(classifyStagedOutfits(staged, {})).toEqual({ dirty: [], unresolved: ['alice'] });
  });

  it('keys both verdicts per character', () => {
    const staged = {
      alice: slots({ top: ['coat'] }),
      bob: slots({ top: ['shirt'] }),
      carol: slots({ accessories: ['hat'] }),
    };
    const baselines = {
      alice: slots({ top: ['shirt'] }),
      bob: slots({ top: ['shirt'] }),
    };

    const { dirty, unresolved } = classifyStagedOutfits(staged, baselines);
    expect(dirty).toEqual([{ characterId: 'alice', slots: staged.alice }]);
    expect(unresolved).toEqual(['carol']);
  });

  it('finds nothing to do when nothing was staged', () => {
    expect(classifyStagedOutfits({}, { alice: slots({ top: ['shirt'] }) })).toEqual({
      dirty: [],
      unresolved: [],
    });
  });
});
