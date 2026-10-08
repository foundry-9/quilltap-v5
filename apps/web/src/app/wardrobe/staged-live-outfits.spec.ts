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
  appendWornBundleIds,
  buildSetAllEquipBody,
  classifyStagedOutfits,
  equippedSlotsEqual,
  rebaseStagedGestures,
  rebaseStagedSlots,
  wornBundleIdsFor,
  type StagedGesture,
} from './staged-live-outfits';
import type { WardrobeItemDto } from '../core/core-contract';

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

/**
 * P4.D261 — v4 `3ee3b1342` `__tests__/unit/lib/wardrobe/staged-worn-bundles.
 * test.ts` at the pin `f5e953a3f`, transcribed case for case: the dialog
 * dissolves an outfit to its leaves client-side, so the slots it flushes
 * cannot say an outfit was worn — the ids ride beside them on the `set_all`,
 * accumulate per character, and reset whenever the staged state is rebased.
 */
describe('staged worn-bundle claims (v4 staged-worn-bundles.test.ts)', () => {
  const witem = (id: string, types: WardrobeItemDto['types'], componentItemIds: string[] = []) =>
    ({ id, title: id, types, replace: false, componentItemIds }) as unknown as WardrobeItemDto;
  const hat = witem('hat', ['accessories']);
  const boots = witem('boots', ['footwear']);
  const walkingSet = witem('walking-set', ['accessories', 'footwear'], ['hat', 'boots']);
  const itemsById = new Map([hat, boots, walkingSet].map((i) => [i.id, i]));
  const wearGesture = (it: WardrobeItemDto): StagedGesture => ({
    mutate: (prev) => wearItemIntoSlots(prev, it, itemsById),
    wornBundleIds: wornBundleIdsFor(it),
  });

  describe('wornBundleIdsFor', () => {
    it('claims a bundle by its own id and claims nothing for a garment', () => {
      expect(wornBundleIdsFor(walkingSet)).toEqual(['walking-set']);
      expect(wornBundleIdsFor(hat)).toEqual([]);
      expect(wornBundleIdsFor({ id: 'x', componentItemIds: null })).toEqual([]);
    });
  });

  describe('appendWornBundleIds', () => {
    it('accumulates in first-seen order without duplicates', () => {
      let acc = appendWornBundleIds(undefined, ['a']);
      acc = appendWornBundleIds(acc, ['b', 'a']);
      acc = appendWornBundleIds(acc, []);
      expect(acc).toEqual(['a', 'b']);
    });
  });

  describe('staged bundle ids travel with set_all', () => {
    it('a dirty character carries its accumulated bundle ids into the flush body', () => {
      const baseline = slots({ top: ['shirt'] });
      const staged = wearItemIntoSlots(baseline, walkingSet, itemsById);
      const acc = appendWornBundleIds(undefined, wornBundleIdsFor(walkingSet));

      const { dirty } = classifyStagedOutfits({ alice: staged }, { alice: baseline }, { alice: acc });
      expect(dirty).toEqual([
        { characterId: 'alice', slots: staged, wornBundleIds: ['walking-set'] },
      ]);

      const body = buildSetAllEquipBody(dirty[0].characterId, dirty[0].slots, dirty[0].wornBundleIds);
      expect(body).toEqual({
        characterId: 'alice',
        mode: 'set_all',
        slots: slots({ top: ['shirt'], accessories: ['hat'], footwear: ['boots'] }),
        wornBundleIds: ['walking-set'],
      });
    });

    it('a plain edit sends exactly the old body — no empty wornBundleIds', () => {
      const body = buildSetAllEquipBody('alice', slots({ top: ['shirt'] }), []);
      expect(body).toEqual({ characterId: 'alice', mode: 'set_all', slots: slots({ top: ['shirt'] }) });
      expect('wornBundleIds' in body).toBe(false);
      expect('wornBundleIds' in buildSetAllEquipBody('alice', slots({}))).toBe(false);
    });

    it('a clean character sends nothing, whatever it accumulated', () => {
      const baseline = slots({ top: ['shirt'] });
      expect(
        classifyStagedOutfits({ alice: baseline }, { alice: baseline }, { alice: ['walking-set'] })
          .dirty,
      ).toEqual([]);
    });

    it('ids belong to their own character', () => {
      const baseline = slots({});
      const { dirty } = classifyStagedOutfits(
        { alice: slots({ top: ['shirt'] }), bob: slots({ top: ['vest'] }) },
        { alice: baseline, bob: baseline },
        { alice: ['walking-set'] },
      );
      expect(dirty.find((d) => d.characterId === 'alice')?.wornBundleIds).toEqual(['walking-set']);
      expect(dirty.find((d) => d.characterId === 'bob')?.wornBundleIds).toBeUndefined();
      expect('wornBundleIds' in dirty.find((d) => d.characterId === 'bob')!).toBe(false);
    });
  });

  describe('rebaseStagedGestures', () => {
    it('replays the gestures onto the snapshot and rebuilds the ids from them alone', () => {
      const worn = slots({ top: ['shirt'] });
      const rebased = rebaseStagedGestures(worn, [wearGesture(walkingSet), wearGesture(hat)]);
      expect(rebased.slots).toEqual(slots({ top: ['shirt'], accessories: ['hat'], footwear: ['boots'] }));
      expect(rebased.wornBundleIds).toEqual(['walking-set']);
    });

    it('resets the ids when nothing is replayed — no claim outlives its staged state', () => {
      const worn = slots({ top: ['shirt'] });
      const rebased = rebaseStagedGestures(worn, []);
      expect(rebased).toEqual({ slots: worn, wornBundleIds: [] });
    });

    it('does not mutate the snapshot it rebases onto', () => {
      const worn = slots({ top: ['shirt'] });
      rebaseStagedGestures(worn, [wearGesture(walkingSet)]);
      expect(worn).toEqual(slots({ top: ['shirt'] }));
    });
  });
});
