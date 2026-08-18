/**
 * The wardrobe slot registry — one place that describes a slot.
 *
 * A port of v4's `lib/schemas/wardrobe.types.ts:24-104` as consolidated by
 * `4423ad10` ("add the 'hair' slot"): one ordered slot list plus a
 * `WARDROBE_SLOT_META` table (labels, badge class, clothing flag, the
 * empty-reporting rule). v4's commit deleted five local label/badge maps in
 * favour of this registry; v5 had already collapsed those five into two
 * (`SLOT_LABEL`, `TYPE_BADGE_CLASS`), and this module replaces both.
 *
 * The canonical slot list lived in `screens/prospero/wardrobe.api.ts` until
 * this lane (it re-exports from here now, so its importers are undisturbed).
 */

import type { EquippedSlots, WardrobeSlotType } from '../core/core-contract';

/**
 * All valid wardrobe slot types, in canonical order (types arrays,
 * frontmatter, bundle sorting, and UI slot rows all follow this order). Append
 * new slots at the END — inserting mid-list rewrites every serialized `types`
 * array and reorders existing UI (v4 `WARDROBE_SLOT_TYPES`).
 */
export const WARDROBE_SLOT_TYPES: readonly WardrobeSlotType[] = [
  'top',
  'bottom',
  'footwear',
  'accessories',
  'hair',
];

/** Per-slot presentation and semantics metadata (v4 `WardrobeSlotMeta`). */
export interface WardrobeSlotMeta {
  /** Singular display label ("Hair"). */
  label: string;
  /** Group-header label in the item editor ("Tops", "Hair"). */
  groupLabel: string;
  /** qt-* badge class for this slot's chip. */
  badgeClass: string;
  /**
   * True for garment slots that participate in nudity semantics; false for
   * styling slots (hair). Drives naked collapses and deliberate-unclothed
   * detection — all of which live server-side in v5 (the core's own registry).
   */
  isClothing: boolean;
  /**
   * Whether an EMPTY slot is reported at all — to an LLM, to an image model, or
   * to the user.
   *
   * True for garment slots, where emptiness is real information: an empty top
   * means "topless", an empty footwear slot means "barefoot".
   *
   * False for styling slots ("unreported-if-blank"). An empty `hair` slot does
   * NOT mean the character has no hair — it means their hair is in its natural,
   * unstyled state, which the physical description already covers.
   *
   * NOTE the boundary, measured against v4's real client 2026-08-18: v4
   * consumes this rule ONLY in `lib/` (the wardrobe tool handlers and
   * `outfit-description.ts`) — the surfaces that narrate an outfit to a model
   * or into prose. No v4 *component* reads it: the editing and preview UIs
   * render every slot unconditionally, including an empty "Hair" row (the
   * dialog's `Empty`) and an empty hair card (the Green Room's `nothing`),
   * because a slot you cannot see is a slot you cannot fill. The registry
   * carries the flag for parity and for any future SPA narration site; today
   * nothing in the SPA reads it.
   */
  reportWhenEmpty: boolean;
  /**
   * Phrase rendered when a *reported* slot is empty. Non-null exactly when
   * `reportWhenEmpty` is true; null for unreported-if-blank slots, which render
   * nothing.
   */
  emptyFallback: string | null;
}

/** v4 `WARDROBE_SLOT_META` (`wardrobe.types.ts:77-83`), verbatim. */
export const WARDROBE_SLOT_META: Record<WardrobeSlotType, WardrobeSlotMeta> = {
  top: {
    label: 'Top',
    groupLabel: 'Tops',
    badgeClass: 'qt-badge-wardrobe-top',
    isClothing: true,
    reportWhenEmpty: true,
    emptyFallback: 'topless',
  },
  bottom: {
    label: 'Bottom',
    groupLabel: 'Bottoms',
    badgeClass: 'qt-badge-wardrobe-bottom',
    isClothing: true,
    reportWhenEmpty: true,
    emptyFallback: 'bottomless',
  },
  footwear: {
    label: 'Footwear',
    groupLabel: 'Footwear',
    badgeClass: 'qt-badge-wardrobe-footwear',
    isClothing: true,
    reportWhenEmpty: true,
    emptyFallback: 'barefoot',
  },
  accessories: {
    label: 'Accessories',
    groupLabel: 'Accessories',
    badgeClass: 'qt-badge-wardrobe-accessories',
    isClothing: true,
    reportWhenEmpty: true,
    emptyFallback: 'no accessories',
  },
  hair: {
    label: 'Hair',
    groupLabel: 'Hair',
    badgeClass: 'qt-badge-wardrobe-hair',
    isClothing: false,
    reportWhenEmpty: false,
    emptyFallback: null,
  },
};

/** Slots that count as clothing for nudity/undress semantics (v4 `CLOTHING_SLOT_TYPES`). */
export const CLOTHING_SLOT_TYPES: readonly WardrobeSlotType[] = WARDROBE_SLOT_TYPES.filter(
  (s) => WARDROBE_SLOT_META[s].isClothing,
);

/**
 * True when an empty `slot` should still be reported (as a phrase, a label, or
 * an "(empty)" marker). False for unreported-if-blank slots — skip them.
 * (v4 `isSlotReportedWhenEmpty`.)
 */
export function isSlotReportedWhenEmpty(slot: WardrobeSlotType): boolean {
  return WARDROBE_SLOT_META[slot].reportWhenEmpty;
}

/** Slots that vanish from every report when empty — today: `hair` (v4 `UNREPORTED_IF_BLANK_SLOT_TYPES`). */
export const UNREPORTED_IF_BLANK_SLOT_TYPES: readonly WardrobeSlotType[] =
  WARDROBE_SLOT_TYPES.filter((s) => !WARDROBE_SLOT_META[s].reportWhenEmpty);

/** A brand-new all-empty snapshot — v5's twin of v4's `makeEmptyEquippedSlots`
 *  (`wardrobe.types.ts:231-234`, which replaced the spread-a-shared-constant
 *  idiom this function already avoided). Registry-driven, so a new slot arrives
 *  here for free. */
export function freshSlots(): EquippedSlots {
  return Object.fromEntries(WARDROBE_SLOT_TYPES.map((s) => [s, []])) as unknown as EquippedSlots;
}

/** Deep copy — v5's twin of v4's `cloneEquippedSlots`
 *  (`wardrobe.types.ts:240-244`). Registry-driven, and tolerant of a missing
 *  key on raw JSON: a payload minted before a slot existed clones as `[]`. */
export function cloneSlots(slots: EquippedSlots): EquippedSlots {
  return Object.fromEntries(
    WARDROBE_SLOT_TYPES.map((s) => [s, [...(slots[s] ?? [])]]),
  ) as unknown as EquippedSlots;
}
