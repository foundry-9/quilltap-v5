/**
 * Import/Export shared constants + labels (v4 `components/tools/import-export/
 * types.ts` + `utils.ts` + step components). The label map IS the user-facing
 * copy, so it lives here beside the wizards.
 *
 * @module screens/settings/system/import-export.types
 */

/**
 * Entity types offered in the picker (v4 `ExportTypeStep.tsx:20-36`).
 *
 * This list must stay exhaustive over `ExportEntityType`. Every member the
 * writer supports belongs here — three types were once silently missing from
 * this array with no note saying why, and the result was a set of exports
 * nobody could reach from the UI. If a type is ever deliberately withheld,
 * leave it out *with a comment right here* explaining the reason.
 *
 * In v5 the array is also the SOURCE of `ExportEntityType`, so
 * `ENTITY_TYPE_LABELS` below cannot compile with a member missing — the
 * doctrine is enforced rather than merely asked for.
 */
export const EXPORTABLE_TYPES = [
  'characters',
  'chats',
  'roleplay-templates',
  'prompt-templates',
  'connection-profiles',
  'image-profiles',
  'embedding-profiles',
  'tags',
  'projects',
  'groups',
  'document-stores',
  'files',
  'provider-models',
  'plugin-configs',
  'instance-settings',
] as const;

export type ExportEntityType = (typeof EXPORTABLE_TYPES)[number];

/** v4 `types.ts:42-58` — the label map (all fifteen). */
export const ENTITY_TYPE_LABELS: Record<ExportEntityType, string> = {
  characters: 'Characters',
  chats: 'Chats',
  'roleplay-templates': 'Roleplay Templates',
  'prompt-templates': 'Prompt Templates',
  'connection-profiles': 'Connection Profiles',
  'image-profiles': 'Image Profiles',
  'embedding-profiles': 'Embedding Profiles',
  tags: 'Tags',
  projects: 'Projects',
  groups: 'Groups',
  'document-stores': 'Document Stores',
  files: 'Files & Folders',
  'provider-models': 'Provider Models',
  'plugin-configs': 'Plugin Settings',
  'instance-settings': 'Instance Settings',
};

/**
 * v4 `utils.ts:6-25` — camelCase preview key → kebab export type. The fallback
 * is v4's `mapping[key] || key`: an unrecognized key passes through unchanged
 * and then misses the label map. (v5's `keyLabel` caller renders the raw key
 * via `?? key` where v4's `ImportPreviewStep.tsx:104` renders an EMPTY heading
 * — a pre-existing, unreachable-for-known-keys divergence, recorded at the
 * `7189a968` unification rather than silently re-described as v4's shape.)
 */
export function toExportEntityType(key: string): ExportEntityType {
  const map: Record<string, ExportEntityType> = {
    characters: 'characters',
    chats: 'chats',
    tags: 'tags',
    connectionProfiles: 'connection-profiles',
    imageProfiles: 'image-profiles',
    embeddingProfiles: 'embedding-profiles',
    roleplayTemplates: 'roleplay-templates',
    promptTemplates: 'prompt-templates',
    projects: 'projects',
    groups: 'groups',
    documentStores: 'document-stores',
    files: 'files',
    providerModels: 'provider-models',
    pluginConfigs: 'plugin-configs',
    instanceSettings: 'instance-settings',
  };
  return map[key] ?? (key as ExportEntityType);
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
