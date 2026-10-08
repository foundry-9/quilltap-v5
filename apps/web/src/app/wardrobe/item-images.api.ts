/**
 * Client-side contract for wardrobe item pictures — a port of v4
 * `lib/wardrobe/item-images-client.ts` (`7c8572869`, read at the pin
 * `f5e953a3f`): the URLs of the images route (`/api/v1/wardrobe/[itemId]/
 * images`), the shapes it answers with, and thin call helpers.
 *
 * Every surface that shows or changes an item's picture (the editor's Picture
 * section, the row thumbnail and its "Generate image" menu entry, the picker
 * thumbnails) goes through here, so the URL shape lives in one place.
 *
 * **Recorded mechanism divergences (P4.D261).**
 *  - `list` / `set-current` / `delete-image` ride the C2 §6 dispatch verbs
 *    (`wardrobeItemImagesList` / `…SetCurrent` / `…Delete`), naming the
 *    container instead of building a URL.
 *  - `upload` has NO verb (C2 §6): it POSTs multipart to the binary route, as
 *    v4 does and as `chat/chat-files.api.ts` does for chat files.
 *  - `generate` ALSO POSTs the REST route (`?action=generate`, C2 §6) rather
 *    than the `wardrobeItemImageGenerate` verb: the Picture section's refusal
 *    notice reads the 422's `details.trail` and its status, and the dispatch
 *    error envelope (`CoreError`) carries neither — the REST body is v4's
 *    exactly. (The verb's request type stays in the contract, as C2 names it.)
 *  - The byte URLs go through the origin-aware `apiUrl` (identity in a
 *    browser; the qtap origin in the Tauri dev loop), where v4 returns the
 *    bare path.
 *
 * @module wardrobe/item-images.api
 */

import { apiUrl } from '../core/api-url';
import type { CoreClient } from '../core/core-client';
import type {
  WardrobeItemImageGenerateResponse,
  WardrobeItemImageRefusal,
  WardrobeItemImagesResponse,
  WardrobeItemImageSummary,
} from '../core/core-contract';
import type { WardrobeContainer } from './wardrobe-container';

export type {
  WardrobeItemImageGenerateResponse,
  WardrobeItemImageRefusal,
  WardrobeItemImagesResponse,
  WardrobeItemImageSummary,
} from '../core/core-contract';

export type WardrobeItemImageAction = 'generate' | 'upload' | 'set-current' | 'delete-image';

/** `/api/v1/wardrobe/<itemId>/images?scope=…&id=…[&action=…]` (v4 `:51-62`). */
export function wardrobeItemImagesUrl(
  itemId: string,
  container: WardrobeContainer,
  action?: WardrobeItemImageAction,
): string {
  const params = new URLSearchParams({ scope: container.scope });
  if (container.id) params.set('id', container.id);
  if (action) params.set('action', action);
  return `/api/v1/wardrobe/${encodeURIComponent(itemId)}/images?${params.toString()}`;
}

/** The thumbnail URL for a picture's file id (v4 `:64-67`). */
export function wardrobeImageThumbnailUrl(fileId: string): string {
  return apiUrl(`/api/v1/files/${fileId}?action=thumbnail`);
}

/** The full-size URL for a picture's file id (v4 `:69-72`). */
export function wardrobeImageUrl(fileId: string): string {
  return apiUrl(`/api/v1/files/${fileId}`);
}

/** An error from the images route, carrying the parsed body (v4 `:79-90`). */
export class WardrobeImageRequestError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly refusal: WardrobeItemImageRefusal | null,
  ) {
    super(message);
    this.name = 'WardrobeImageRequestError';
  }
}

/** v4 `:92-100`. */
async function readJsonOrThrow<T>(res: Response): Promise<T> {
  const body = (await res.json().catch(() => null)) as {
    error?: unknown;
    details?: unknown;
  } | null;
  if (!res.ok) {
    const message =
      (body && typeof body.error === 'string' && body.error) || `Request failed (${res.status})`;
    const details = body?.details;
    const refusal =
      details && typeof details === 'object' && 'trail' in details
        ? (details as WardrobeItemImageRefusal)
        : null;
    throw new WardrobeImageRequestError(message, res.status, refusal);
  }
  return body as T;
}

/** The C2 §6 container half of every verb — General carries no `containerId`. */
function containerFields(container: WardrobeContainer): {
  scope: WardrobeContainer['scope'];
  containerId?: string;
} {
  return container.id
    ? { scope: container.scope, containerId: container.id }
    : { scope: container.scope };
}

/** The item's pictures, newest first, and which one is current. */
export async function listWardrobeItemImages(
  core: CoreClient,
  itemId: string,
  container: WardrobeContainer,
): Promise<WardrobeItemImagesResponse> {
  const data = await core.dispatchData({
    type: 'wardrobeItemImagesList',
    ...containerFields(container),
    itemId,
  });
  return data as unknown as WardrobeItemImagesResponse;
}

/**
 * Generate a picture with the designated profile unless `imageProfileId`
 * overrides it once (v4 `:102-114`).
 */
export async function generateWardrobeItemImage(
  itemId: string,
  container: WardrobeContainer,
  imageProfileId?: string | null,
): Promise<WardrobeItemImageGenerateResponse> {
  const res = await fetch(apiUrl(wardrobeItemImagesUrl(itemId, container, 'generate')), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(imageProfileId ? { imageProfileId } : {}),
  });
  return readJsonOrThrow<WardrobeItemImageGenerateResponse>(res);
}

/**
 * Attach a picture by hand (`uploaded`) or from Import from image (`imported`
 * — the modal is a named deferral, R-A(2); the parameter lands now so it has
 * nothing to widen). v4 `:116-131`.
 */
export async function uploadWardrobeItemImage(
  itemId: string,
  container: WardrobeContainer,
  file: File,
  kind: 'uploaded' | 'imported' = 'uploaded',
): Promise<{ image: WardrobeItemImageSummary; current: string }> {
  const form = new FormData();
  form.append('file', file);
  form.append('kind', kind);
  const res = await fetch(apiUrl(wardrobeItemImagesUrl(itemId, container, 'upload')), {
    method: 'POST',
    body: form,
  });
  return readJsonOrThrow(res);
}

/** Make one of the item's pictures current (v4 `:133-145`). */
export async function setCurrentWardrobeItemImage(
  core: CoreClient,
  itemId: string,
  container: WardrobeContainer,
  fileId: string,
): Promise<{ current: string | null }> {
  const data = await core.dispatchData({
    type: 'wardrobeItemImageSetCurrent',
    ...containerFields(container),
    itemId,
    fileId,
  });
  return data as unknown as { current: string | null };
}

/** Delete one of the item's pictures; the next-newest becomes current (v4 `:147-160`). */
export async function deleteWardrobeItemImage(
  core: CoreClient,
  itemId: string,
  container: WardrobeContainer,
  fileId: string,
): Promise<{ current: string | null }> {
  const data = await core.dispatchData({
    type: 'wardrobeItemImageDelete',
    ...containerFields(container),
    itemId,
    fileId,
  });
  return data as unknown as { current: string | null };
}
