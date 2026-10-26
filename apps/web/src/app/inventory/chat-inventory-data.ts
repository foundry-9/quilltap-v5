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

import { computed, effect, signal, untracked, type Signal } from '@angular/core';

import type { CoreClient } from '../core/core-client';
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
