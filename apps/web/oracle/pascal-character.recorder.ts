/**
 * The v4-side recorder behind `src/app/pascal/character-arms.oracle.spec.ts`
 * (P4.D320, R-5 (b)).
 *
 * `a05c6f755` gave Pascal's format a `{{character}}` placeholder (the rolling
 * character's name — exact key, so `character.x` names nothing) and a
 * top-level `announceAsCharacter` boolean between `defaultVisibility` and
 * `parameters`. The SPA's hand-ported twins (`placeholders.ts`,
 * `custom-tool-types.ts`) must classify the one and parse — or refuse — the
 * other exactly as v4's REAL `classifyPlaceholder` / `QtapCustomToolSchema` /
 * `formatDefinitionIssues` / `collectUnknownKeys` do: the Workbench renders the
 * rejection sentence verbatim for a draft the author is repairing.
 *
 * The SHARED corpus (`harness/oracle/cases/pascal-custom-tool-definition.ts`)
 * is the Pascal SERVER lane's (P4.D319) to grow; these are the rows that did
 * not exist before, recorded here so this lane's hunks are pinned without
 * editing another lane's generator — the `pascal-progress.recorder.ts`
 * precedent. The definition row ids are the ones survey §4 (1) names for the
 * shared generator, so at the union a same-id row whose bytes differ between
 * the two files is a FINDING; once the unifier's one regen of the grown shared
 * corpus carries them, these become the redundant half and may retire (the
 * unifier's call).
 *
 * Run it from a pinned v4 worktree at `e8e17b2d9` (Node 24):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at e8e17b2d9}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/pascal-character.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/pascal-character.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx pascal-character.recorder.ts > "$OUT")
 * wc -l "$OUT"                                     # expect 18
 * grep -c '"id":"announce-as-character-' "$OUT"    # expect 4
 * grep -c '"kind":"character"' "$OUT"              # > 0
 * ```
 *
 * Expect 18 lines (12 `definition` + 6 `placeholder`).
 */

import {
  QtapCustomToolSchema,
  collectUnknownKeys,
  formatDefinitionIssues,
} from '@/lib/pascal/custom-tool.types';
import { classifyPlaceholder } from '@/lib/pascal/placeholders';

/** A definition that parses, so a row only ever fails on the arm it means to test. */
function definition(extra: string): string {
  return `{"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}]${extra}}`;
}

const DEFINITIONS: Array<[string, string]> = [
  ['announce-as-character-true', definition(`,"announceAsCharacter":true`)],
  ['announce-as-character-false', definition(`,"announceAsCharacter":false`)],
  ['announce-as-character-string', definition(`,"announceAsCharacter":"yes"`)],
  ['announce-as-character-null', definition(`,"announceAsCharacter":null`)],
  [
    'effects-value-character-concat',
    definition(`,"effects":[{"target":"state.winner","value":"{{character}} + ' wins'"}]`),
  ],
  [
    'effects-wardrobe-character-ref',
    definition(`,"effects":[{"wardrobe":{"action":"wear","items":["{{character}}'s armour"]}}]`),
  ],
  [
    'unknown-keys-announce-as-character-known',
    definition(`,"announceAsCharacter":true,"persist":true`),
  ],
  // Issue ORDER: a bad defaultVisibility is reported before a bad
  // announceAsCharacter, which is reported before a bad parameters block.
  [
    'announce-as-character-issue-order',
    definition(`,"parameters":7,"announceAsCharacter":0,"defaultVisibility":"loud"`),
  ],
  // Key ORDER on accept: written last, parsed into schema position.
  [
    'announce-as-character-key-order',
    `{"announceAsCharacter":true,"outcomes":[{"when":true,"message":"-","state":"info"}],"defaultVisibility":"whisper","description":"A probe.","name":"probe","parameters":{"n":{"type":"number","description":"d","default":1}}}`,
  ],
  [
    'character-message-chip',
    `{"name":"probe","description":"A probe.","chipLabel":"For {{character}}","outcomes":[{"when":true,"message":"{{character}} rolls {{value}}","state":"info"}],"announceAsCharacter":true}`,
  ],
  ['effects-value-character-dotted', definition(`,"effects":[{"target":"state.x","value":"{{character.name}}"}]`)],
  ['effects-value-character-bare', definition(`,"effects":[{"target":"state.x","value":"{{character}}"}]`)],
];

for (const [id, inputJson] of DEFINITIONS) {
  const doc = JSON.parse(inputJson);
  const result = QtapCustomToolSchema.safeParse(doc);
  console.log(
    JSON.stringify({
      kind: 'definition',
      id,
      inputJson,
      success: result.success,
      reason: result.success ? null : formatDefinitionIssues(result.error),
      data: result.success ? JSON.stringify(result.data) : null,
      unknownKeys: collectUnknownKeys(doc),
    }),
  );
}

const KEYS: Array<[string, string]> = [
  ['classify-character', 'character'],
  ['classify-character-dotted', 'character.name'],
  ['classify-character-padded', ' character '],
  ['classify-character-capital', 'Character'],
  ['classify-characters', 'characters'],
  ['classify-character-trailing-dot', 'character.'],
];
for (const [id, key] of KEYS) {
  console.log(JSON.stringify({ kind: 'placeholder', id, key, out: JSON.stringify(classifyPlaceholder(key)) }));
}
