/**
 * The v4-side recorder behind `src/app/pascal/wardrobe-vantage.oracle.spec.ts`
 * (P4.D320).
 *
 * `5cb0e0190` gave the Workbench a whose-wardrobe lens: `defaultWardrobeVantage`
 * (`lib/pascal/tool-draft.ts`) opens a definition on the store it lives in — a
 * character vault beats a group store, a project store, Quilltap General, and
 * anything else opens on nobody — and `wardrobeReachLabel`
 * (`lib/wardrobe/wardrobe-container.ts`) names who can reach an item from where
 * it lives. Both are pure; this drives v4's REAL functions.
 *
 * The destinations are v4's own test fixture (`__tests__/unit/lib/pascal/
 * wardrobe-vantage.test.ts`, copied verbatim below — a vault also attached to
 * a group, a loose `other` store), plus a second set whose stores collide
 * across the tiers so the find ORDER is pinned and not only the happy rows.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...`.
 *
 * Rows:
 *   - `{ fn: 'defaultWardrobeVantage', id, mountPointId, destinations, out }` —
 *     `fixture` names the destinations (`v4` / `collide` / `no-general`) and
 *     `destinations` carries them whole, `out` the vantage's `JSON.stringify`.
 *   - `{ fn: 'wardrobeReachLabel', id, args, out }` — `args` is the argument
 *     LIST as passed (an absent argument is simply not in the list, so
 *     `undefined` and `null` stay distinct), `out` the label or `null`.
 *
 * Run it from a pinned v4 worktree at `e8e17b2d9` (Node 24):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at e8e17b2d9}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/wardrobe-vantage.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/wardrobe-vantage.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx wardrobe-vantage.recorder.ts > "$OUT")
 * wc -l "$OUT"                                  # expect 40
 * grep -c '"fn":"wardrobeReachLabel"' "$OUT"    # expect 20
 * ```
 *
 * Expect 40 lines (20 `defaultWardrobeVantage` + 20 `wardrobeReachLabel`).
 */

import { defaultWardrobeVantage } from '@/lib/pascal/tool-draft';
import type { CustomToolDestinations } from '@/lib/pascal/workbench';
import { wardrobeReachLabel } from '@/lib/wardrobe/wardrobe-container';

const store = (mountPointId: string) => ({ mountPointId, mountName: mountPointId, existingToolNames: [] });

/** v4's test destinations, verbatim. */
const V4: CustomToolDestinations = {
  general: store('mp-general'),
  characters: [
    { characterId: 'char-imogen', characterName: 'Imogen', ...store('mp-imogen') },
    // A vault that is also linked to a group: the character wins.
    { characterId: 'char-bertie', characterName: 'Bertie', ...store('mp-shared') },
  ],
  groups: [
    {
      groupId: 'group-regulars',
      groupName: 'The Regulars',
      stores: [{ ...store('mp-regulars'), official: true }, { ...store('mp-shared'), official: false }],
    },
  ],
  projects: [{ projectId: 'proj-thornfield', projectName: 'Thornfield', stores: [store('mp-thornfield')] }],
  other: [store('mp-loose')],
};

/** Stores shared across tiers, so every precedence step is observable. */
const COLLIDE: CustomToolDestinations = {
  general: store('mp-gp'),
  characters: [
    { characterId: 'char-first', characterName: 'First', ...store('mp-twice') },
    { characterId: 'char-second', characterName: 'Second', ...store('mp-twice') },
  ],
  groups: [
    { groupId: 'group-a', groupName: 'A', stores: [{ ...store('mp-gp'), official: true }] },
    { groupId: 'group-b', groupName: 'B', stores: [{ ...store('mp-gp'), official: false }, { ...store('mp-gx'), official: true }] },
  ],
  projects: [
    { projectId: 'proj-a', projectName: 'PA', stores: [store('mp-gx'), store('mp-px')] },
    { projectId: 'proj-b', projectName: 'PB', stores: [store('mp-px'), store('mp-general-too')] },
  ],
  other: [store('mp-general-too')],
};

/** No General store at all (an instance whose General mount is missing). */
const NO_GENERAL: CustomToolDestinations = {
  general: null,
  characters: [],
  groups: [],
  projects: [],
  other: [store('mp-loose')],
};

const FIXTURES: Record<string, CustomToolDestinations> = { v4: V4, collide: COLLIDE, 'no-general': NO_GENERAL };

const VANTAGE: Array<[string, string | null, string]> = [
  // v4's six tests
  ['v4-character-vault', 'mp-imogen', 'v4'],
  ['v4-group-store', 'mp-regulars', 'v4'],
  ['v4-project-store', 'mp-thornfield', 'v4'],
  ['v4-general-store', 'mp-general', 'v4'],
  ['v4-unattached-store', 'mp-loose', 'v4'],
  ['v4-unsaved-file', null, 'v4'],
  ['v4-character-beats-group', 'mp-shared', 'v4'],
  // the falsy mount ids
  ['empty-mount-id', '', 'v4'],
  ['unknown-mount-id', 'mp-nowhere', 'v4'],
  ['other-only', 'mp-loose', 'collide'],
  // precedence across tiers
  ['first-character-of-two', 'mp-twice', 'collide'],
  ['group-beats-general', 'mp-gp', 'collide'],
  ['group-beats-project', 'mp-gx', 'collide'],
  ['first-project-of-two', 'mp-px', 'collide'],
  ['project-beats-other', 'mp-general-too', 'collide'],
  // no General at all
  ['no-general-null-mount', null, 'no-general'],
  ['no-general-loose', 'mp-loose', 'no-general'],
  ['no-general-empty', '', 'no-general'],
  // case sensitivity: mount ids compare exactly
  ['mount-id-case', 'MP-IMOGEN', 'v4'],
  ['mount-id-padded', ' mp-imogen', 'v4'],
];

const REACH: Array<[string, unknown[]]> = [
  ['character-named', [{ scope: 'character', id: 'char-1', name: 'Imogen' }, 'Imogen']],
  ['character-name-given-overrides-origin', [{ scope: 'character', id: 'char-1', name: 'Imogen' }, 'Bertie']],
  ['character-name-padded', [{ scope: 'character', id: 'char-1', name: 'Imogen' }, '  Bertie  ']],
  ['character-name-blank', [{ scope: 'character', id: 'char-1', name: 'Imogen' }, '']],
  ['character-name-whitespace', [{ scope: 'character', id: 'char-1', name: 'Imogen' }, '   ']],
  ['character-name-null', [{ scope: 'character', id: 'char-1', name: 'Imogen' }, null]],
  ['character-name-absent', [{ scope: 'character', id: 'char-1', name: 'Imogen' }]],
  ['character-origin-padded', [{ scope: 'character', id: 'char-1', name: '  Imogen  ' }]],
  ['character-origin-empty', [{ scope: 'character', id: 'char-1', name: '' }]],
  ['character-origin-whitespace-name-blank', [{ scope: 'character', id: 'char-1', name: '   ' }, '  ']],
  ['group', [{ scope: 'group', id: 'g-1', name: 'The Regulars' }]],
  ['group-padded-untrimmed', [{ scope: 'group', id: 'g-1', name: '  The Regulars ' }]],
  ['group-with-character-name', [{ scope: 'group', id: 'g-1', name: 'The Regulars' }, 'Imogen']],
  ['project', [{ scope: 'project', id: 'p-1', name: 'Thornfield' }]],
  ['project-padded-untrimmed', [{ scope: 'project', id: 'p-1', name: ' Thornfield  ' }]],
  ['project-empty-name', [{ scope: 'project', id: 'p-1', name: '' }]],
  ['general', [{ scope: 'general', id: null, name: 'Quilltap General' }]],
  ['general-with-character-name', [{ scope: 'general', id: null, name: 'Quilltap General' }, 'Imogen']],
  ['null-origin', [null, 'Imogen']],
  ['undefined-origin', []],
];

for (const [id, mountPointId, fixture] of VANTAGE) {
  console.log(
    JSON.stringify({
      fn: 'defaultWardrobeVantage',
      id,
      mountPointId,
      fixture,
      destinations: FIXTURES[fixture],
      out: JSON.stringify(defaultWardrobeVantage(mountPointId, FIXTURES[fixture])),
    }),
  );
}
for (const [id, args] of REACH) {
  const out = (wardrobeReachLabel as (...a: unknown[]) => string | null)(...args);
  console.log(JSON.stringify({ fn: 'wardrobeReachLabel', id, args, out }));
}
