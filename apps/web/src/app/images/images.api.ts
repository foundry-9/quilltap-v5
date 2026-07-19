/**
 * The image-detail data layer (P4.9a2): the `imageInfoGet` wire verb plus the
 * v4 `components/images/image-detail/types.ts` interfaces the deep modal
 * family shares.
 *
 * `ImageInfoGetRequest` was FOLDED into `CoreRequest` (`core-contract.ts`) at
 * the round's unification and the `as unknown as CoreRequest` cast retired;
 * the name-for-name wire diff ran there against
 * `crates/quilltap-core/src/api/types.rs`. It is re-exported below so this
 * module stays the images-family entry point.
 *
 * @module images/images.api
 */

import type { CoreClient } from '../core/core-client';
import type { ImageInfoGetRequest } from '../core/core-contract';

/** §1: `imageInfoGet` — v4 `GET /api/v1/images/{id}` (`route.ts:39-128`). */
export type { ImageInfoGetRequest };

/** v4 `image-detail/types.ts:5-9`. */
export interface CharacterGalleryLink {
  characterId: string;
  characterName: string;
  linkId: string;
}

/**
 * v4 `image-detail/types.ts:11-35`. The `linkId` vs `id` split (the comment at
 * `:13-19`, transcribed verbatim):
 *
 * When the image originates from a character/user vault gallery (Phase 3+
 * photos), `id` is a doc_mount_file_links id and `linkId` is set to the
 * same value. The save-to-gallery action sends `{ linkId }` instead of
 * `{ fileId }` so the server re-links from the existing blob rather than
 * looking up a non-existent images-v2 FileEntry.
 */
export interface ImageData {
  id: string;
  linkId?: string;
  filename: string;
  filepath: string;
  url?: string;
  mimeType: string;
  size: number;
  width?: number;
  height?: number;
  createdAt: string;
  tags?: Array<{ id?: string; tagType: string; tagId: string }>;
  characterGalleryLinks?: CharacterGalleryLink[];
}

/** v4 `image-detail/types.ts:37-41` (the roster slice the detail modal reads). */
export interface DetailCharacter {
  id: string;
  name: string;
  defaultImageId?: string | null;
}

/**
 * The `imageInfoGet` success body — v4's `successResponse({ data: {…} })`
 * envelope (`route.ts:101-123`). Both v4 consumers read only
 * `data.characterGalleryLinks` (`ImageDetailModal.tsx:55`,
 * `ChatGalleryImageViewModal.tsx:71`); the full payload is pinned server-side
 * by `photos_routes_equivalence`.
 */
export interface ImageInfoEnvelope {
  data?: {
    id: string;
    userId: string;
    filename: string;
    filepath: string;
    mimeType: string;
    size: number;
    width?: number;
    height?: number;
    source: 'upload' | 'import' | 'generated';
    generationPrompt?: string;
    generationModel?: string;
    createdAt: string;
    updatedAt: string;
    tags: Array<{ tagId: string; tagType: 'CHARACTER' | 'CHAT' | 'THEME' }>;
    characterGalleryLinks: CharacterGalleryLink[];
    _count: { charactersUsingAsDefault: number; chatAvatarOverrides: number };
  };
}

/**
 * Fetch the image info; returns the `characterGalleryLinks` array (the read
 * both v4 modals derive their album state from). A failed fetch resolves to
 * `null` so callers keep v4's fail-soft shape (v4 checks `imageRes.ok` and
 * simply skips the update).
 */
export async function fetchCharacterGalleryLinks(
  core: CoreClient,
  imageId: string,
): Promise<CharacterGalleryLink[] | null> {
  try {
    const request: ImageInfoGetRequest = { type: 'imageInfoGet', id: imageId };
    const data = (await core.dispatchData(
      request,
    )) as unknown as ImageInfoEnvelope;
    return data?.data?.characterGalleryLinks ?? [];
  } catch {
    return null;
  }
}
