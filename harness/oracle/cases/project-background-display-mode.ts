/**
 * Tier-1 oracle case — the retired project background display modes
 * (v4 `70505745a`, `lib/schemas/project.types.ts`).
 *
 * Drives v4's REAL `normalizeBackgroundDisplayMode` + `RETIRED_BACKGROUND_
 * DISPLAY_MODES` + `ProjectPropertiesSchema`, one row per assertion in v4's own
 * `__tests__/unit/lib/schemas/project-background-display-mode.test.ts`, plus the
 * two shapes that test does not state and the port has to get right anyway:
 * an explicit JSON `null` on the property bag (v4's `.default()` short-circuits
 * only on `undefined`, so `null` reaches the preprocess, becomes `undefined`,
 * and then FAILS the enum) and the empty string (unrecognised → 'theme').
 *
 * `normalizeBackgroundDisplayMode` returns `unknown`; `undefined` is emitted as
 * the JSON string `"<undefined>"` so the NDJSON can carry the distinction from
 * `null` at all.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/project-background-display-mode.ts \
 *     > /tmp/oracle-project-background-mode.ndjson
 */

import {
  ProjectPropertiesSchema,
  normalizeBackgroundDisplayMode,
  RETIRED_BACKGROUND_DISPLAY_MODES,
} from '@/lib/schemas/project.types';

const rows: unknown[] = [];

/** `undefined` is not JSON; give it a name the harness can compare on. */
const enc = (v: unknown): unknown => (v === undefined ? '<undefined>' : v);

// ---- normalizeBackgroundDisplayMode ---------------------------------------
// v4's four `it()` blocks, expanded to one row per assertion, in their order.
const normalizeCases: Array<[string, unknown]> = [
  // "passes the surviving modes through untouched"
  ['surviving-latest-chat', 'latest_chat'],
  ['surviving-theme', 'theme'],
  // "coerces every retired mode to theme" — driven off v4's own constant.
  ...RETIRED_BACKGROUND_DISPLAY_MODES.map(
    (m) => [`retired-${m}`, m] as [string, unknown],
  ),
  // "coerces an unrecognised value to theme"
  ['unrecognised-string', 'wallpaper'],
  ['unrecognised-number', 7],
  // "leaves absent values to the schema default"
  ['absent-undefined', undefined],
  ['absent-null', null],
  // Not in v4's test, but the port has to agree: the empty string is
  // unrecognised like any other, NOT a synonym for absent.
  ['empty-string', ''],
];
for (const [id, input] of normalizeCases) {
  rows.push({
    kind: 'normalize',
    id,
    input: enc(input),
    out: enc(normalizeBackgroundDisplayMode(input)),
  });
}

rows.push({
  kind: 'retiredList',
  id: 'RETIRED_BACKGROUND_DISPLAY_MODES',
  out: [...RETIRED_BACKGROUND_DISPLAY_MODES],
});

// ---- ProjectPropertiesSchema.backgroundDisplayMode ------------------------
// The schema is `.parse`d (not `safeParse`d) on every project read, so a stored
// retired value that merely narrowing the enum would have THROWN — losing the
// whole project — must come back as 'theme' instead. Recorded as
// `{ ok, mode }` so a throwing shape is a comparand rather than a crash.
const parseCases: Array<[string, Record<string, unknown>]> = [
  ...RETIRED_BACKGROUND_DISPLAY_MODES.map(
    (m) => [`parse-retired-${m}`, { backgroundDisplayMode: m }] as [string, Record<string, unknown>],
  ),
  ['parse-latest-chat', { backgroundDisplayMode: 'latest_chat' }],
  ['parse-theme', { backgroundDisplayMode: 'theme' }],
  ['parse-absent', {}],
  ['parse-unrecognised', { backgroundDisplayMode: 'wallpaper' }],
  ['parse-empty-string', { backgroundDisplayMode: '' }],
  ['parse-explicit-null', { backgroundDisplayMode: null }],
];
for (const [id, input] of parseCases) {
  const parsed = ProjectPropertiesSchema.safeParse(input);
  rows.push({
    kind: 'parse',
    id,
    input,
    ok: parsed.success,
    mode: parsed.success ? parsed.data.backgroundDisplayMode : null,
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
