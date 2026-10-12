/**
 * What every character in one chat is wearing — a port of v4
 * `lib/hooks/use-outfit.ts` `useChatOutfit` at `4a38222e6` (bug 198's fix,
 * `e23fb095b`, included).
 *
 * A read of `chatOutfitGet` (`{ equippedOutfit: { [characterId]: EquippedSlots
 * } }`). Pass `chatId === null` for chat-less surfaces; the store stays idle.
 *
 * The snapshot seeds staging that never re-seeds (the Live tab and the Outfit
 * Builder are gated so a refetch can't wipe in-progress edits), so a snapshot
 * from an earlier visit must never reach them: an LLM may have dressed or
 * undressed someone since (bug 198). v4 refetches on every mount and answers
 * `slotsFor` undefined until that fetch lands; here the store is created per
 * dialog mount (the inner dialog is destroyed on close), so it holds no
 * earlier visit's snapshot at all — `slotsFor` is undefined until this
 * store's first read lands. The dialog re-reads it on the `chats` realtime
 * topic (a tool-driven outfit change lands as messages) and after its own
 * commits.
 *
 * Equipping is not here: the wardrobe dialog stages its gestures and commits
 * with `chatEquip` (`set_all`) on Done.
 *
 * **Retired with v4's pre-span hook (`3e6e2f590:lib/hooks/use-outfit.ts`,
 * deleted in `9ecebf986`):** the four per-mode equip primitives
 * (`equipItem` / `replaceItem` / `addToSlot` / `removeFromSlot`), the
 * per-character wardrobe fetch with its `(shared)` title suffix, and the
 * `itemsBySlot` title resolution. None had a v5 caller outside this store's
 * own spec.
 *
 * @module wardrobe/outfit-store
 */

import { computed, signal, type Signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
import { EMPTY_EQUIPPED_SLOTS, type EquippedOutfitState, type EquippedSlots } from './equipped-slots';
import { dispatchWardrobe } from './wardrobe.api';

/** The store handle (v4 `UseChatOutfitResult`). */
export interface OutfitStore {
  /** The chat's equipped outfit per character, or undefined until it arrives. */
  readonly equippedOutfit: Signal<EquippedOutfitState | undefined>;
  /**
   * One character's worn slots — empty slots for a character the chat has
   * never dressed — or undefined until this store's first read has landed.
   */
  slotsFor(characterId: string | null): EquippedSlots | undefined;
  readonly loading: Signal<boolean>;
  /** Re-read the snapshot (v4 `refetch`). Resolves to it, or null on failure / no chat. */
  refreshOutfit(): Promise<EquippedOutfitState | null>;
}

/** Create the store for one chat (fixed per dialog mount). */
export function createOutfitStore(core: CoreClient, chatId: string | null): OutfitStore {
  const equippedOutfit = signal<EquippedOutfitState | undefined>(undefined);
  const inFlight = signal(false);
  let seq = 0;

  async function refreshOutfit(): Promise<EquippedOutfitState | null> {
    if (!chatId) return null;
    const ticket = ++seq;
    inFlight.set(true);
    try {
      const data = await dispatchWardrobe(core, { type: 'chatOutfitGet', chatId });
      const next = (data['equippedOutfit'] as EquippedOutfitState | undefined) ?? {};
      if (ticket === seq) equippedOutfit.set(next);
      return next;
    } catch {
      return null;
    } finally {
      if (ticket === seq) inFlight.set(false);
    }
  }

  return {
    equippedOutfit: equippedOutfit.asReadonly(),
    slotsFor(characterId: string | null): EquippedSlots | undefined {
      const outfit = equippedOutfit();
      if (!characterId || !outfit) return undefined;
      return outfit[characterId] ?? (EMPTY_EQUIPPED_SLOTS as EquippedSlots);
    },
    // v4 `loading: chatId !== null && query.isLoading` — the first read only.
    loading: computed(() => chatId !== null && inFlight() && equippedOutfit() === undefined),
    refreshOutfit,
  };
}
