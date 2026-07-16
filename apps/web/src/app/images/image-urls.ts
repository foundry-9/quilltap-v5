/**
 * Byte-route helpers for in-chat images (v5's id-keyed file routes). v4's
 * MessageRow served thumbnails straight from `attachment.filepath`; v5 serves
 * them through the existing `GET /api/v1/files/{id}` route (+ `?action=thumbnail`
 * for the cached WebP thumbnail), which is the store-backed byte path.
 */

import { apiUrl } from '../core/api-url';

/** The 80px thumbnail default (v4 `DEFAULT_THUMBNAIL_SIZE` — the message-row size). */
export const CHAT_THUMBNAIL_SIZE = 80;

/** Full-resolution bytes for a file id. */
export function fileUrl(fileId: string): string {
  return apiUrl(`/api/v1/files/${fileId}`);
}

/** Cached WebP thumbnail bytes for a file id at `size` (square). */
export function thumbnailUrl(fileId: string, size = CHAT_THUMBNAIL_SIZE): string {
  return apiUrl(`/api/v1/files/${fileId}?action=thumbnail&size=${size}`);
}
