import { describe, expect, it } from 'vitest';

import {
  ENTITY_TYPE_LABELS,
  EXPORTABLE_TYPES,
  toExportEntityType,
  type ExportEntityType,
} from './import-export.types';

/**
 * P4.D47 unit 1 — the fifteen export types, their picker order, their labels,
 * and the camelCase→kebab map, pinned against the round contract (C1), which is
 * v4 `ExportTypeStep.tsx:20-36` + `types.ts:42-58` + `utils.ts:6-25` at
 * `7189a968`.
 *
 * The list is transcribed here as a literal rather than derived from the module
 * under test — a spec that iterates the same array it is checking asserts
 * nothing. Swap two rows in either place and the order assertion goes red.
 */

/** Contract C1, in order: `[id, label]`. */
const CONTRACT: ReadonlyArray<readonly [ExportEntityType, string]> = [
  ['characters', 'Characters'],
  ['chats', 'Chats'],
  ['roleplay-templates', 'Roleplay Templates'],
  ['prompt-templates', 'Prompt Templates'],
  ['connection-profiles', 'Connection Profiles'],
  ['image-profiles', 'Image Profiles'],
  ['embedding-profiles', 'Embedding Profiles'],
  ['tags', 'Tags'],
  ['projects', 'Projects'],
  ['groups', 'Groups'],
  ['document-stores', 'Document Stores'],
  ['files', 'Files & Folders'],
  ['provider-models', 'Provider Models'],
  ['plugin-configs', 'Plugin Settings'],
  ['instance-settings', 'Instance Settings'],
];

/** Contract C1's camelCase preview keys → kebab types (v4's whole mapping). */
const PREVIEW_KEY_MAP: ReadonlyArray<readonly [string, ExportEntityType]> = [
  ['characters', 'characters'],
  ['chats', 'chats'],
  ['tags', 'tags'],
  ['connectionProfiles', 'connection-profiles'],
  ['imageProfiles', 'image-profiles'],
  ['embeddingProfiles', 'embedding-profiles'],
  ['roleplayTemplates', 'roleplay-templates'],
  ['promptTemplates', 'prompt-templates'],
  ['projects', 'projects'],
  ['groups', 'groups'],
  ['documentStores', 'document-stores'],
  ['files', 'files'],
  ['providerModels', 'provider-models'],
  ['pluginConfigs', 'plugin-configs'],
  ['instanceSettings', 'instance-settings'],
];

describe('the export types table (contract C1)', () => {
  it('offers the fifteen types in the contract order', () => {
    expect([...EXPORTABLE_TYPES]).toEqual(CONTRACT.map(([id]) => id));
  });

  it('labels every offered type with v4’s copy', () => {
    expect(CONTRACT.map(([id]) => [id, ENTITY_TYPE_LABELS[id]])).toEqual(
      CONTRACT.map(([id, label]) => [id, label]),
    );
  });

  it('carries no label the picker cannot reach, and no type without a label', () => {
    // The map is `Record<ExportEntityType, string>`, so a MISSING label is a
    // compile error; this catches the other direction — a label left behind
    // after a type is withdrawn.
    expect(Object.keys(ENTITY_TYPE_LABELS).sort()).toEqual([...EXPORTABLE_TYPES].sort());
  });
});

describe('toExportEntityType (contract C1)', () => {
  it('maps every camelCase preview key to its kebab type', () => {
    expect(PREVIEW_KEY_MAP.map(([key]) => toExportEntityType(key))).toEqual(
      PREVIEW_KEY_MAP.map(([, type]) => type),
    );
  });

  it('reaches a label for every mapped preview key', () => {
    for (const [key] of PREVIEW_KEY_MAP) {
      expect(ENTITY_TYPE_LABELS[toExportEntityType(key)]).toBeTypeOf('string');
    }
  });

  it("passes an unknown key through unchanged (v4's `mapping[key] || key`)", () => {
    expect(toExportEntityType('somethingElse')).toBe('somethingElse');
  });
});
