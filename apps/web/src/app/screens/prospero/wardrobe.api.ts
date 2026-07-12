/**
 * The project-wardrobe data layer (v4 `app/prospero/[id]/hooks/useProjectWardrobe.ts`):
 * a mutator over the `projectWardrobe*` verbs. Project wardrobe is the project
 * tier of the tri-tier wardrobe model — items stored here are wearable by every
 * character in this project's chats, alongside the character's own vault items
 * and the Quilltap General archetypes. Supports leaf garments and composites
 * (bundling existing project items); cycle rejection is enforced server-side.
 *
 * The create/update payload rides the opaque `item` bag the p4.6k Shared
 * contract pins; blank optional strings are sent as `null` (v4 `handleSave`).
 */

import { type Signal, signal } from '@angular/core';

import type { CoreClient } from '../../core/core-client';
import type { WardrobeItemDto, WardrobeSlotType } from '../../core/core-contract';

/** The four coverage slots, in v4's declaration order. */
export const WARDROBE_SLOT_TYPES: readonly WardrobeSlotType[] = [
  'top',
  'bottom',
  'footwear',
  'accessories',
];

/** The create payload (v4 `CreateProjectWardrobeInput`); blanks arrive as `null`. */
export interface WardrobeCreateInput {
  title: string;
  description?: string | null;
  /** Plain-text image-generation cue; preferred over the title in image prompts. */
  imagePrompt?: string | null;
  types: WardrobeSlotType[];
  appropriateness?: string | null;
  isDefault?: boolean;
  componentItemIds?: string[];
  replace?: boolean;
}

export type WardrobeUpdateInput = Partial<WardrobeCreateInput>;

/** The uniform result the manager branches on. */
export type WardrobeResult<T = unknown> = ({ ok: true } & T) | { ok: false; error: string };

export interface WardrobeMutator {
  readonly items: Signal<WardrobeItemDto[]>;
  readonly loading: Signal<boolean>;
  readonly error: Signal<string | null>;
  refresh(): Promise<void>;
  createItem(
    input: WardrobeCreateInput,
  ): Promise<WardrobeResult<{ item: WardrobeItemDto | undefined }>>;
  updateItem(id: string, patch: WardrobeUpdateInput): Promise<WardrobeResult>;
  deleteItem(id: string): Promise<WardrobeResult>;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** The project-scoped mutator (v4 `useProjectWardrobe`), base `projectWardrobe*`. */
export function projectWardrobeMutator(core: CoreClient, projectId: string): WardrobeMutator {
  const items = signal<WardrobeItemDto[]>([]);
  const loading = signal(true);
  const error = signal<string | null>(null);

  async function refresh(): Promise<void> {
    loading.set(true);
    error.set(null);
    try {
      const data = await core.dispatchData({ type: 'projectWardrobeList', projectId });
      items.set((data['wardrobeItems'] as WardrobeItemDto[]) ?? []);
    } catch (err) {
      error.set(errorMessage(err));
    } finally {
      loading.set(false);
    }
  }

  async function createItem(
    input: WardrobeCreateInput,
  ): Promise<WardrobeResult<{ item: WardrobeItemDto | undefined }>> {
    try {
      const data = await core.dispatchData({
        type: 'projectWardrobeCreate',
        projectId,
        item: input as unknown as Record<string, unknown>,
      });
      // v4 applies the fresh list inline when the create response carries one.
      if (Array.isArray(data['wardrobeItems'])) {
        items.set(data['wardrobeItems'] as WardrobeItemDto[]);
      }
      return { ok: true, item: data['wardrobeItem'] as WardrobeItemDto | undefined };
    } catch (err) {
      return { ok: false, error: errorMessage(err) };
    }
  }

  async function updateItem(id: string, patch: WardrobeUpdateInput): Promise<WardrobeResult> {
    try {
      await core.dispatchData({
        type: 'projectWardrobeUpdate',
        projectId,
        itemId: id,
        item: patch as unknown as Record<string, unknown>,
      });
      // v4 re-lists after update (the PUT echoes only the one item).
      await refresh();
      return { ok: true };
    } catch (err) {
      return { ok: false, error: errorMessage(err) };
    }
  }

  async function deleteItem(id: string): Promise<WardrobeResult> {
    try {
      await core.dispatchData({ type: 'projectWardrobeDelete', projectId, itemId: id });
      await refresh();
      return { ok: true };
    } catch (err) {
      return { ok: false, error: errorMessage(err) };
    }
  }

  return { items, loading, error, refresh, createItem, updateItem, deleteItem };
}
