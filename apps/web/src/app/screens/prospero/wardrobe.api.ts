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

// The canonical slot list moved to the wardrobe slot registry with P4.D88
// (v4 `4423ad10` did the same on its side); re-exported here so this module's
// importers keep their import path.
export { WARDROBE_SLOT_TYPES } from '../../wardrobe/slot-meta';

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

export type WardrobeUpdateInput = Partial<WardrobeCreateInput> & {
  /** Archive (true) or restore (false). Maps to `archivedAt` server-side. */
  archived?: boolean;
};

/** The uniform result the manager branches on. */
export type WardrobeResult<T = unknown> = ({ ok: true } & T) | { ok: false; error: string };

export interface WardrobeMutator {
  readonly items: Signal<WardrobeItemDto[]>;
  readonly loading: Signal<boolean>;
  readonly error: Signal<string | null>;
  /**
   * "Show archived" state (v4 `d25dacc1`). Flipping it re-fetches with
   * `includeArchived` rather than filtering `items` client-side — the project
   * and group lists used to return archived items unconditionally and filter on
   * the client, which is exactly the second place for the rule to drift that
   * v4 removed.
   */
  readonly showArchived: Signal<boolean>;
  setShowArchived(next: boolean): void;
  refresh(): Promise<void>;
  createItem(
    input: WardrobeCreateInput,
  ): Promise<WardrobeResult<{ item: WardrobeItemDto | undefined }>>;
  updateItem(id: string, patch: WardrobeUpdateInput): Promise<WardrobeResult>;
  deleteItem(id: string): Promise<WardrobeResult>;
  /** Archive an active garment, or restore an archived one. */
  setItemArchived(id: string, archived: boolean): Promise<WardrobeResult>;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** The project-scoped mutator (v4 `useProjectWardrobe`), base `projectWardrobe*`. */
export function projectWardrobeMutator(core: CoreClient, projectId: string): WardrobeMutator {
  const items = signal<WardrobeItemDto[]>([]);
  const loading = signal(true);
  const error = signal<string | null>(null);
  const showArchived = signal(false);

  async function refresh(): Promise<void> {
    loading.set(true);
    error.set(null);
    try {
      const data = await core.dispatchData({
        type: 'projectWardrobeList',
        projectId,
        includeArchived: showArchived(),
      });
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

  /** v4 `setItemArchived` — one line over `updateItem`, so the archive flag
   *  travels the same PUT every other field does. */
  async function setItemArchived(id: string, archived: boolean): Promise<WardrobeResult> {
    return updateItem(id, { archived });
  }

  /** v4's checkbox handler: the flip IS a new request, not a client filter. */
  function setShowArchived(next: boolean): void {
    showArchived.set(next);
    void refresh();
  }

  return {
    items,
    loading,
    error,
    showArchived,
    setShowArchived,
    refresh,
    createItem,
    updateItem,
    deleteItem,
    setItemArchived,
  };
}
