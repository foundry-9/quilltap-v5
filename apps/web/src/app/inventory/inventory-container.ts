/**
 * The Inventory's client-side addressing — a port of v4 `lib/inventory/
 * container.ts` (`322a5488e`): which tier a browser is looking at. A
 * container is the wardrobe's `{ scope, id }` shape (one encoder names a tier
 * for both), so the dialogs' container selects read the same.
 *
 * **Recorded mechanism divergence (R-1 / R-M).** v4's module also exports the
 * REST URL builders (`inventoryCollectionUrl`, `inventoryActionUrl`,
 * `inventoryItemUrl`); v5's reads and writes ride C7's dispatch verbs, which
 * carry the container itself, so only `inventoryImagesUrl` survives — the
 * multipart upload is REST (`inventory.api.ts`).
 *
 * Client-safe: pure. `containerOfOrigin` and `inventoryOriginLabel` are
 * pinned row for row against v4 by `inventory-display.oracle.spec.ts`.
 *
 * @module inventory/inventory-container
 */

import type { InventoryOrigin } from '../core/core-contract';
import {
  GENERAL_CONTAINER,
  decodeWardrobeContainer,
  encodeWardrobeContainer,
  type WardrobeContainer,
} from '../wardrobe/wardrobe-container';

/** v4 `container.ts:20` — the wardrobe's container shape. */
export type InventoryContainer = WardrobeContainer;

/** v4 `container.ts:22`. */
export const GENERAL_INVENTORY_CONTAINER: InventoryContainer = GENERAL_CONTAINER;
/** v4 `container.ts:23`. */
export const GENERAL_INVENTORY_NAME = 'Quilltap General';

/** v4 `container.ts:25-26` — the wardrobe's encoder and decoder. */
export const encodeInventoryContainer = encodeWardrobeContainer;
export const decodeInventoryContainer = decodeWardrobeContainer;

/**
 * The images URL of one object, with its container in the query (v4
 * `container.ts:59-64`) — the multipart upload's REST path.
 */
export function inventoryImagesUrl(container: InventoryContainer, itemId: string, action?: string): string {
  const query = new URLSearchParams({ scope: container.scope });
  if (container.id) query.set('id', container.id);
  if (action) query.set('action', action);
  return `/api/v1/inventory/${itemId}/images?${query.toString()}`;
}

/** The tier an object lives in, from the origin a read tagged it with (v4 `:67-71`). */
export function containerOfOrigin(
  origin: InventoryOrigin | null | undefined,
  fallback: InventoryContainer,
): InventoryContainer {
  if (!origin) return fallback;
  if (origin.scope === 'general') return GENERAL_INVENTORY_CONTAINER;
  return origin.id ? { scope: origin.scope, id: origin.id } : fallback;
}

/**
 * A short label for the tier an object came from (v4 `:74-79`). NOT the
 * wardrobe's `Shared · …` / `Project · X` spelling — the two stay apart
 * (survey trap 4).
 */
export function inventoryOriginLabel(origin: InventoryOrigin | null | undefined): string | null {
  if (!origin) return null;
  if (origin.scope === 'general') return 'General';
  if (origin.scope === 'project') return `Project: ${origin.name}`;
  if (origin.scope === 'group') return `Group: ${origin.name}`;
  return null;
}

/** The tier key a container's reads are keyed by (v4 `use-inventory.ts:76-78`). */
export function inventoryTierKey(container: InventoryContainer, shared = false): string {
  return `${encodeInventoryContainer(container)}${shared ? '|shared' : ''}`;
}
