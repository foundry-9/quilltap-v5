/**
 * What a chat's cast carries, and the gestures that change it — v4
 * `useChatInventory` (`lib/hooks/use-inventory.ts:281-324`, `322a5488e`) as a
 * signal store (R-2).
 *
 * Every gesture AWAITS its dispatch and then, success or failure, reloads the
 * chat's read AND the browser's list — v4's `onSettled` invalidates both
 * `equipped(chatId)` and `inventory.lists`, because a kit's archetype members
 * are instantiated into the vault (survey trap 12). A refusal still rejects
 * to the caller, who words it.
 *
 * @module inventory/chat-inventory-data
 */

import { computed, effect, inject, signal, untracked, type Signal } from '@angular/core';
import { QueryClient, injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import { chatKeys } from '../chat/chat-keys';

import type { CarriedEntry, InventoryEquipResult, InventoryUnequipResult, ResolvedInventoryItem } from '../core/core-contract';
import {
  equipChatInventory,
  getChatInventory,
  resetChatInventoryDefaults,
  unequipChatInventory,
} from './inventory.api';

export interface ChatInventoryData {
  readonly inventory: Signal<Record<string, CarriedEntry>>;
  /** The chat's project, for the project tier of a character's pool. */
  readonly projectId: Signal<string | null>;
  readonly loading: Signal<boolean>;
  /** One cast member's carried objects (none for no character, or no entry). */
  carriedBy(characterId: string | null): CarriedEntry;
  equip(characterId: string, ids: string[]): Promise<InventoryEquipResult>;
  unequip(characterId: string, ids: string[]): Promise<InventoryUnequipResult>;
  resetToDefaults(characterId: string): Promise<{ equipped: ResolvedInventoryItem[] }>;
  /** Re-read the chat's carried objects (the `chats` topic's refetch). */
  reload(): Promise<void>;
}

export interface ChatInventoryDataOptions {
  chatId: () => string | null;
  /** The browser list's reload — run after every gesture (v4's `inventory.lists`). */
  reloadList?: () => Promise<void> | void;
}

const NONE: CarriedEntry = { equipped: [], kits: {} };

export function createChatInventoryData(core: CoreClient, opts: ChatInventoryDataOptions): ChatInventoryData {
  const inventory = signal<Record<string, CarriedEntry>>({});
  const projectId = signal<string | null>(null);
  const loaded = signal<string | null>(null);
  const inFlight = signal(false);

  let seq = 0;
  const reload = async (): Promise<void> => {
    const chatId = opts.chatId();
    const ticket = ++seq;
    if (!chatId) {
      inventory.set({});
      projectId.set(null);
      loaded.set(null);
      inFlight.set(false);
      return;
    }
    inFlight.set(true);
    try {
      const data = await getChatInventory(core, chatId);
      if (ticket !== seq) return;
      inventory.set(data.inventory);
      projectId.set(data.projectId);
      loaded.set(chatId);
    } catch {
      if (ticket !== seq) return;
      loaded.set(chatId);
    } finally {
      if (ticket === seq) inFlight.set(false);
    }
  };

  effect(() => {
    opts.chatId();
    untracked(() => void reload());
  });

  /** v4's `onSettled`: the chat read and the lists, whatever happened. */
  const settled = async <T>(run: () => Promise<T>): Promise<T> => {
    try {
      return await run();
    } finally {
      await Promise.all([reload(), opts.reloadList?.()]);
    }
  };
  const chatIdOrThrow = (): string => {
    const chatId = opts.chatId();
    if (!chatId) throw new Error('No chat to carry objects in');
    return chatId;
  };

  return {
    inventory: inventory.asReadonly(),
    projectId: projectId.asReadonly(),
    loading: computed(() => inFlight() && loaded() !== opts.chatId()),
    carriedBy: (characterId) => (characterId && inventory()[characterId]) || NONE,
    equip: (characterId, ids) => settled(() => equipChatInventory(core, chatIdOrThrow(), characterId, ids)),
    unequip: (characterId, ids) => settled(() => unequipChatInventory(core, chatIdOrThrow(), characterId, ids)),
    resetToDefaults: (characterId) => settled(() => resetChatInventoryDefaults(core, chatIdOrThrow(), characterId)),
    reload,
  };
}

// === P4.D385 (27fb3ec77) ===
/** The frozen empty answer — reference-stable, so an avatar with nothing never re-renders. */
const NOTHING_CARRIED: readonly ResolvedInventoryItem[] = Object.freeze([]);

/**
 * What one character carries in a chat, read-only — the Salon's avatar column
 * shows it (v4 `useChatCarriedItems`, `lib/hooks/use-inventory.ts:226-239`,
 * `27fb3ec77`). ONE TanStack entry per chat under {@link chatKeys.inventory},
 * so "every avatar in the transcript costs one request" — `select` picks the
 * character's `equipped` list out of the shared answer. The key sits under
 * {@link chatKeys.detail}'s prefix, so the `chats` realtime topic (published on
 * every equip / unequip, `inventory/equip.rs`) refetches it with no row of its
 * own. Enabled only with both ids, as v4's is.
 *
 * {@link createChatInventoryData} (the Inventory control dialog's store, P4.D386's)
 * is untouched; it still reads its own copy (HANDOFF H5 offers it this key).
 */
export function injectChatCarriedItems(
  chatId: () => string | null,
  characterId: () => string | null,
): Signal<readonly ResolvedInventoryItem[]> {
  // A host with no TanStack client (a component spec that renders a message
  // row without one) shows no glyphs rather than failing the row; every SPA
  // root provides one (`app.config.ts`).
  if (!inject(QueryClient, { optional: true })) return computed(() => NOTHING_CARRIED);
  const core = inject(CoreClient);
  const query = injectQuery(() => {
    const chat = chatId();
    const character = characterId();
    return {
      queryKey: chatKeys.inventory(chat ?? 'none'),
      queryFn: () => getChatInventory(core, chat!),
      enabled: !!chat && !!character,
      select: (data: { inventory: Record<string, CarriedEntry> }) =>
        (character && data.inventory[character]?.equipped) || NOTHING_CARRIED,
    };
  });
  return computed(() => query.data() ?? NOTHING_CARRIED);
}
// === end P4.D385 ===
