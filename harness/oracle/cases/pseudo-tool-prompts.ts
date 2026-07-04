/**
 * Oracle case: pseudo-tool prompt builders + option determination
 * (chat-orchestration wave 4 / W4.1b.1 — prompts half).
 *
 * Drives the REAL functions from v4's:
 *   lib/tools/simple-json-prompt.ts (buildSimpleJsonToolInstructions — renders
 *     from the b.3 tool definitions), lib/tools/legacy/text-block-prompt.ts,
 *     lib/tools/native-tool-prompt.ts, and the service wrappers
 *     lib/services/chat-message/pseudo-tool.service.ts.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/pseudo-tool-prompts.ts \
 *     > /tmp/oracle-pseudo-tool-prompts.ndjson
 */

import { buildSimpleJsonToolInstructions } from '@/lib/tools/simple-json-prompt';
import { buildTextBlockInstructions } from '@/lib/tools/legacy/text-block-prompt';
import { buildNativeToolInstructions } from '@/lib/tools/native-tool-prompt';
import {
  buildNativeToolSystemInstructions,
  determineEnabledToolOptions,
  determineTextBlockToolOptions,
  buildTextBlockSystemInstructions,
  buildSimpleJsonSystemInstructions,
  type TextBlockEnabledToolOptions,
} from '@/lib/services/chat-message/pseudo-tool.service';

const rows: unknown[] = [];
function emit(row: unknown): void {
  rows.push(row);
}

// ---- native ----
emit({ fn: 'build_native', id: 'native-true', hasTools: true, out: buildNativeToolInstructions(true) });
emit({ fn: 'build_native', id: 'native-false', hasTools: false, out: buildNativeToolInstructions(false) });
emit({ fn: 'build_native', id: 'native-default', hasTools: true, out: buildNativeToolInstructions() });
emit({ fn: 'build_native_system', id: 'native-system', out: buildNativeToolSystemInstructions() });

// ---- prompt flag combos (shared shape between simple-json + text-block) ----
const combos: Array<[string, Record<string, boolean>]> = [
  ['empty', {}],
  ['search-suppressed', { search: false }],
  ['search-only', { search: true }],
  ['rng-plus-default-search', { rng: true }],
  ['whisper-and-image', { whisper: true, imageGeneration: true }],
  ['help-trio', { helpSearch: true, helpSettings: true, helpNavigate: true }],
  ['wardrobe-subset', { wardrobeList: true, wardrobeWear: true, wardrobeCreate: true }],
  ['create-note-only', { search: false, createNote: true }],
  [
    'all-on',
    {
      whisper: true,
      search: true,
      imageGeneration: true,
      webSearch: true,
      state: true,
      rng: true,
      projectInfo: true,
      helpSearch: true,
      helpSettings: true,
      helpNavigate: true,
      createNote: true,
      wardrobeList: true,
      wardrobeRead: true,
      wardrobeWear: true,
      wardrobeTakeOff: true,
      wardrobeCreate: true,
      wardrobeUpdate: true,
      wardrobeArchive: true,
    },
  ],
];

for (const [id, options] of combos) {
  emit({ fn: 'build_text_block', id, options, out: buildTextBlockInstructions(options) });
  emit({ fn: 'build_simple_json', id, options, out: buildSimpleJsonToolInstructions(options) });
}

// ---- determineEnabledToolOptions ----
for (const [id, imageProfileId, allowWebSearch] of [
  ['det-enabled-both', 'img-1', true],
  ['det-enabled-neither', null, false],
  ['det-enabled-image-only', 'img-2', false],
] as Array<[string, string | null, boolean]>) {
  emit({
    fn: 'determine_enabled',
    id,
    imageProfileId,
    allowWebSearch,
    out: determineEnabledToolOptions(imageProfileId, allowWebSearch),
  });
}

// ---- determineTextBlockToolOptions ----
type DetInput = {
  imageProfileId: string | null;
  allowWebSearch: boolean;
  isMultiCharacter: boolean;
  hasProject: boolean;
  helpToolsEnabled?: boolean;
  canDressThemselves?: boolean;
  canCreateOutfits?: boolean;
};
const detCases: Array<[string, DetInput]> = [
  ['det-tb-defaults', { imageProfileId: null, allowWebSearch: false, isMultiCharacter: false, hasProject: false }],
  ['det-tb-full', { imageProfileId: 'img', allowWebSearch: true, isMultiCharacter: true, hasProject: true, helpToolsEnabled: true, canDressThemselves: true, canCreateOutfits: true }],
  ['det-tb-dress-off', { imageProfileId: null, allowWebSearch: false, isMultiCharacter: true, hasProject: false, canDressThemselves: false }],
  ['det-tb-create-off', { imageProfileId: null, allowWebSearch: false, isMultiCharacter: false, hasProject: true, canCreateOutfits: false }],
  ['det-tb-help-undefined', { imageProfileId: null, allowWebSearch: true, isMultiCharacter: false, hasProject: false }],
];
for (const [id, c] of detCases) {
  const opts = determineTextBlockToolOptions(
    c.imageProfileId,
    c.allowWebSearch,
    c.isMultiCharacter,
    c.hasProject,
    c.helpToolsEnabled,
    c.canDressThemselves,
    c.canCreateOutfits,
  );
  emit({ fn: 'determine_text_block', id, input: c, out: opts });
  // And feed the materialized options into both system-instruction wrappers.
  emit({ fn: 'build_text_block_system', id, enabledOptions: opts, out: buildTextBlockSystemInstructions(opts) });
  emit({ fn: 'build_simple_json_system', id, enabledOptions: opts, out: buildSimpleJsonSystemInstructions(opts as TextBlockEnabledToolOptions) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
