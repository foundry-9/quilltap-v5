/**
 * Import/Export shared constants + labels (v4 `components/tools/import-export/
 * types.ts` + `utils.ts` + step components). The label map IS the user-facing
 * copy, so it lives here beside the wizards.
 *
 * @module screens/settings/system/import-export.types
 */

/** v4 `ExportTypeStep.tsx:12-20` — the exportable types, in order. */
export const EXPORTABLE_TYPES = [
  'characters',
  'chats',
  'roleplay-templates',
  'connection-profiles',
  'image-profiles',
  'embedding-profiles',
  'tags',
] as const;

export type ExportEntityType = (typeof EXPORTABLE_TYPES)[number];

/** v4 `types.ts:42-53` — the label map (all ten). */
export const ENTITY_TYPE_LABELS: Record<string, string> = {
  characters: 'Characters',
  chats: 'Chats',
  'roleplay-templates': 'Roleplay Templates',
  'connection-profiles': 'Connection Profiles',
  'image-profiles': 'Image Profiles',
  'embedding-profiles': 'Embedding Profiles',
  tags: 'Tags',
  projects: 'Projects',
  groups: 'Groups',
  'document-stores': 'Document Stores',
};

/** v4 `utils.ts:6-17` — camelCase preview key → kebab export type. */
export function toExportEntityType(key: string): string {
  const map: Record<string, string> = {
    characters: 'characters',
    chats: 'chats',
    tags: 'tags',
    connectionProfiles: 'connection-profiles',
    imageProfiles: 'image-profiles',
    embeddingProfiles: 'embedding-profiles',
    roleplayTemplates: 'roleplay-templates',
  };
  return map[key] ?? key;
}

/** One entity in the picker (v4 `AvailableEntity`). */
export interface AvailableEntity {
  id: string;
  name: string;
  memoryCount?: number;
}

/** The import conflict strategies the UI offers (v4 `ConflictStrategy`). */
export type ConflictStrategy = 'skip' | 'overwrite' | 'duplicate';

/** v4 `lib/utils/format-bytes.ts` — 1024-based, `0 → "0 B"`, `<KB` whole. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return '';
  const sign = bytes < 0 ? '-' : '';
  const n = Math.abs(bytes);
  if (n === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(n) / Math.log(1024));
  const value = n / Math.pow(1024, i);
  return `${sign}${i === 0 ? Math.round(value) : value.toFixed(1)} ${units[i]}`;
}
