/**
 * Staged Live-tab outfit edits — a port of v4 `lib/wardrobe/staged-live-outfits.ts`
 * (new at 4.8.2, commit `07d4ccce`, "bug 61").
 *
 * The Wardrobe dialog's "Wearing now" tab stages every gesture locally and
 * commits once, on Done. Two moments in that lifecycle have to reckon with a
 * worn snapshot that has not arrived yet, and both live here so they can be
 * reasoned about — and tested — away from the dialog's render logic.
 *
 *  - `rebaseStagedSlots` replays the gestures made during the window before the
 *    snapshot landed onto the true worn slots, so a fast click is neither
 *    overwritten by the first seed nor committed against an empty base.
 *  - `classifyStagedOutfits` separates "nothing changed" from "we never learned
 *    what clean was", which the flush used to treat identically — reporting
 *    success for an outfit it never sent (Bug 61).
 *
 * `equippedSlotsEqual` moved here with them (v4 lifted it out of the dialog in
 * the same commit); it lived in `equipped-slots.ts` before.
 *
 * This is the race v5 measured first (a 3 ms margin — dogfood finding #78,
 * filed upstream as v4 Bug 61 and deflaked gesture-level in
 * `e2e/wardrobe-flow.spec.ts`); v4 has now fixed it, so v5 ports the fix.
 */

import {
  addIdToSlot,
  addItemToSlot,
  breakApartBundleInSlots,
  cloneSlots,
  removeIdFromSlot,
  takeOffBundleFromSlots,
  wearItemIntoSlots,
  WARDROBE_SLOT_TYPES,
  type EquippedBundle,
  type EquippedSlots,
  type WardrobeItemDto,
  type WardrobeSlotType,
} from './equipped-slots';

/** A staging gesture: pure, and safe to replay against a different base. */
export type SlotsMutator = (prev: EquippedSlots) => EquippedSlots;

// ---------------------------------------------------------------------------
// The wear ledger's staged claims (v4 `3ee3b1342`, `staged-live-outfits.ts:
// 16-22, 33-100`). The dialog dissolves a bundle to its leaves client-side, so
// the slots it flushes cannot say an outfit was worn; the ids travel beside
// them as `wornBundleIds` on the `set_all`, and the server expands and
// intersects them with what actually went on. They live and die with the
// staged slots — a rebase recomputes them from the replayed gestures, and a
// commit or reset clears them.
// ---------------------------------------------------------------------------

/**
 * A recorded gesture: the slot mutation, plus the bundles it put on (empty for
 * anything that is not wearing an outfit). Recorded while the worn snapshot is
 * still in flight so both halves can be replayed onto it together.
 */
export interface StagedGesture {
  mutate: SlotsMutator;
  wornBundleIds: readonly string[];
}

/**
 * The bundle ids a put-on gesture for `item` claims: the item's own id when it
 * is a bundle (has components), nothing otherwise. A claim, not a fact — the
 * server credits it only if one of the bundle's leaves actually went on.
 */
export function wornBundleIdsFor(item: {
  id: string;
  componentItemIds?: readonly string[] | null;
}): string[] {
  return (item.componentItemIds?.length ?? 0) > 0 ? [item.id] : [];
}

/** Append bundle ids to an accumulated list, de-duplicated, first-seen order. */
export function appendWornBundleIds(
  prev: readonly string[] | undefined,
  ids: readonly string[],
): string[] {
  const out = [...(prev ?? [])];
  for (const id of ids) {
    if (!out.includes(id)) out.push(id);
  }
  return out;
}

/**
 * {@link rebaseStagedSlots} for recorded gestures: replay the slot mutations
 * onto the worn snapshot and rebuild the accumulated bundle ids from exactly
 * the gestures replayed. Whatever was accumulated against the empty fallback
 * is discarded with it — the staged state and its bundle claims reset together.
 */
export function rebaseStagedGestures(
  wornSlots: EquippedSlots,
  pending: readonly StagedGesture[],
): { slots: EquippedSlots; wornBundleIds: string[] } {
  return {
    slots: rebaseStagedSlots(
      wornSlots,
      pending.map((g) => g.mutate),
    ),
    wornBundleIds: pending.reduce<string[]>(
      (ids, g) => appendWornBundleIds(ids, g.wornBundleIds),
      [],
    ),
  };
}

/**
 * The body of one `set_all` equip (less the dispatch envelope — the caller
 * adds `type` + `chatId`). `wornBundleIds` is omitted when empty so a plain
 * slot edit sends exactly what it always did.
 */
export function buildSetAllEquipBody(
  characterId: string,
  slots: EquippedSlots,
  wornBundleIds?: readonly string[],
): { characterId: string; mode: 'set_all'; slots: EquippedSlots; wornBundleIds?: string[] } {
  return {
    characterId,
    mode: 'set_all',
    slots,
    ...(wornBundleIds && wornBundleIds.length > 0 ? { wornBundleIds: [...wornBundleIds] } : {}),
  };
}

/** Deep array equality on the four EquippedSlots arrays, in order. */
export function equippedSlotsEqual(a: EquippedSlots, b: EquippedSlots): boolean {
  for (const slot of WARDROBE_SLOT_TYPES) {
    const av = a[slot];
    const bv = b[slot];
    if (av.length !== bv.length) return false;
    for (let i = 0; i < av.length; i++) {
      if (av[i] !== bv[i]) return false;
    }
  }
  return true;
}

/**
 * Replay gestures staged before the worn snapshot arrived onto that snapshot.
 *
 * With no snapshot to build on, the dialog stages onto an empty fallback purely
 * so the click paints. Committing that would clear every slot the user never
 * touched, so the gestures are recorded and replayed here the moment the real
 * slots land. Mutators are pure, so applying them twice (once for the optimistic
 * paint, once here) is safe.
 */
export function rebaseStagedSlots(
  wornSlots: EquippedSlots,
  pending: readonly SlotsMutator[],
): EquippedSlots {
  return pending.reduce<EquippedSlots>((slots, mutate) => mutate(slots), cloneSlots(wornSlots));
}

/** What the Done flush should do with each character's staged slots. */
export interface StagedOutfitClassification {
  /**
   * Characters whose staged slots differ from their captured baseline, with
   * the bundles their staged gestures put on (present only when non-empty).
   */
  dirty: Array<{ characterId: string; slots: EquippedSlots; wornBundleIds?: string[] }>;
  /**
   * Characters staged against no baseline at all — their worn snapshot never
   * arrived, so the edit can be neither confirmed as a change nor safely sent
   * (the staged slots were built on an empty fallback). The caller must surface
   * these rather than counting them as clean.
   */
  unresolved: string[];
}

/**
 * Split staged outfits into the ones to commit and the ones we cannot judge.
 *
 * A character with a baseline and matching slots is simply clean and appears in
 * neither list.
 */
export function classifyStagedOutfits(
  staged: Record<string, EquippedSlots>,
  baselines: Record<string, EquippedSlots>,
  wornBundleIdsByChar: Record<string, readonly string[]> = {},
): StagedOutfitClassification {
  const dirty: StagedOutfitClassification['dirty'] = [];
  const unresolved: string[] = [];

  for (const [characterId, slots] of Object.entries(staged)) {
    const baseline = baselines[characterId];
    if (!baseline) {
      unresolved.push(characterId);
      continue;
    }
    if (!equippedSlotsEqual(slots, baseline)) {
      const wornBundleIds = wornBundleIdsByChar[characterId] ?? [];
      dirty.push(
        wornBundleIds.length > 0
          ? { characterId, slots, wornBundleIds: [...wornBundleIds] }
          : { characterId, slots },
      );
    }
  }

  return { dirty, unresolved };
}

// ============================================================================
// COMPOSER GESTURES (v4 `4a38222e6` `staged-live-outfits.ts:166-229`)
//
// The six things an outfit composer can do to a slot bag, as replayable
// gestures. Every surface that stages an outfit (the dialog's Live tab and
// Outfit Builder; v4's chat-start composer too — v5's new-chat outfit selector
// mounts no composer) builds its handlers from these via
// `createComposerHandlers`, so the surfaces cannot drift.
// ============================================================================

const plain = (mutate: SlotsMutator): StagedGesture => ({ mutate, wornBundleIds: [] });

/**
 * Wear an item across every slot it covers, honouring its `replace` flag
 * (`wearItemIntoSlots`). An id the pool doesn't know is appended to the slot
 * the gesture started in. Wearing an outfit claims its id for the wear ledger.
 */
export function wearGesture(
  slot: WardrobeSlotType,
  itemId: string,
  itemsById: ReadonlyMap<string, WardrobeItemDto>,
): StagedGesture {
  const item = itemsById.get(itemId);
  if (!item) return plain((prev) => addIdToSlot(prev, slot, itemId));
  return {
    mutate: (prev) => wearItemIntoSlots(prev, item, itemsById),
    wornBundleIds: wornBundleIdsFor(item),
  };
}

/** Layer an item into one slot only (`addItemToSlot`). */
export function addToSlotGesture(
  slot: WardrobeSlotType,
  item: WardrobeItemDto,
  itemsById: ReadonlyMap<string, WardrobeItemDto>,
): StagedGesture {
  return {
    mutate: (prev) => addItemToSlot(prev, slot, item, itemsById),
    wornBundleIds: wornBundleIdsFor(item),
  };
}

/** Take one id out of one slot. */
export function removeFromSlotGesture(slot: WardrobeSlotType, itemId: string): StagedGesture {
  return plain((prev) => removeIdFromSlot(prev, slot, itemId));
}

/** Empty one slot. */
export function clearSlotGesture(slot: WardrobeSlotType): StagedGesture {
  return plain((prev) => removeIdFromSlot(prev, slot));
}

/** Take a legacy whole-composite card off every slot it occupies. */
export function takeOffBundleGesture(bundle: EquippedBundle): StagedGesture {
  return plain((prev) => takeOffBundleFromSlots(prev, bundle));
}

/** Dissolve a legacy whole-composite card into its leaves. */
export function breakApartBundleGesture(
  bundle: EquippedBundle,
  itemsById: ReadonlyMap<string, WardrobeItemDto>,
): StagedGesture {
  return plain((prev) => breakApartBundleInSlots(prev, bundle, itemsById));
}

/** The composer's five handlers (v4 `useComposerHandlers.ts` `ComposerHandlers`). */
export interface ComposerHandlers {
  onAddToSlot: (slot: WardrobeSlotType, itemId: string) => void;
  onRemoveFromSlot: (slot: WardrobeSlotType, itemId: string) => void;
  onClearSlot: (slot: WardrobeSlotType) => void;
  onTakeOffBundle: (bundle: EquippedBundle) => void;
  onBreakApartBundle: (bundle: EquippedBundle) => void;
}

/**
 * v4 `components/wardrobe/hooks/useComposerHandlers.ts` as a plain factory —
 * the handlers read `itemsById()` at call time, so one set serves the dialog's
 * whole life. `refuse` returns true to refuse putting `item` on (bug 191's
 * archived refusal); the guard says why itself.
 */
export function createComposerHandlers(opts: {
  itemsById: () => ReadonlyMap<string, WardrobeItemDto>;
  /** Land a gesture on this surface's slots (and its worn-bundle claims). */
  apply: (gesture: StagedGesture) => void;
  refuse?: (item: WardrobeItemDto) => boolean;
}): ComposerHandlers {
  const { itemsById, apply, refuse } = opts;
  return {
    // Picking an item wears it across every slot it covers (layering or
    // replacing per its flag) rather than dropping it into the one row the
    // picker was opened from. The outfit pull-down arrives here too.
    onAddToSlot: (slot, itemId) => {
      const pool = itemsById();
      const item = pool.get(itemId);
      if (item && refuse?.(item)) return;
      apply(wearGesture(slot, itemId, pool));
    },
    onRemoveFromSlot: (slot, itemId) => apply(removeFromSlotGesture(slot, itemId)),
    onClearSlot: (slot) => apply(clearSlotGesture(slot)),
    onTakeOffBundle: (bundle) => apply(takeOffBundleGesture(bundle)),
    onBreakApartBundle: (bundle) => apply(breakApartBundleGesture(bundle, itemsById())),
  };
}

// ============================================================================
// THE WEAR REFUSAL (v4 `4a38222e6` `lib/wardrobe/wearable.ts:15-25`, bug 191)
//
// Item lookups include archived items on purpose, so a garment archived after
// a chat dressed in it can still be named. Putting one *on* is another matter:
// an archived item is retired, and every wear gesture refuses it with the same
// words the server's `?action=equip` and the `wardrobe_wear` tool use.
// ============================================================================

/** The refusal every wear path gives for an archived item. */
export function archivedWearMessage(title: string): string {
  return `Item "${title}" is archived and cannot be worn`;
}

/** Why `item` may not be put on, or null when it may. */
export function wearRefusal(item: { title: string; archivedAt?: string | null }): string | null {
  return item.archivedAt ? archivedWearMessage(item.title) : null;
}
