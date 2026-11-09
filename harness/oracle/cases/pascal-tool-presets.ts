/**
 * Oracle case: Pascal custom-tool RUN PRESETS — the naming contract (P4.D379,
 * v4 `fd165fce0`).
 *
 * Drives the REAL code from v4's `lib/pascal/tool-presets.ts`:
 *   parsePresetFromPath, PRESET_NAME_PATTERN, PRESET_FILE_SUFFIX
 *
 * Only what the SERVER calls is compared (ruling P-2): `sanitizePresetNameInput`
 * and `presetSettingsPath` are client-only and stay with the SPA's own twin.
 *
 * Rows:
 * - `constants` — the pattern's `source` and `flags`, and the suffix;
 * - `pattern` — `PRESET_NAME_PATTERN.test(name)` over v4's own accept / reject
 *   lists plus the quirks;
 * - `parse` — `parsePresetFromPath(toolName, relativePath)`, `null` or the
 *   preset name. The two OVERLAP rows (prefix and suffix overlap, so JS's
 *   `slice(a, b)` with `a > b` answers `""`) are the panic hazard of a naive
 *   Rust byte slice.
 *
 * Run from inside the PINNED server checkout:
 *   cd ~/source/quilltap-server     # (the pin: /tmp/qt-v4-pin-p4d379-624d0d25c)
 *   TZ=UTC npx tsx ~/source/quilltap-v5/harness/oracle/cases/pascal-tool-presets.ts \
 *     > /tmp/oracle-pascal-tool-presets.ndjson
 */

import {
  parsePresetFromPath,
  PRESET_FILE_SUFFIX,
  PRESET_NAME_PATTERN,
} from '@/lib/pascal/tool-presets';

const out = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

out({
  kind: 'constants',
  id: 'constants',
  patternSource: PRESET_NAME_PATTERN.source,
  patternFlags: PRESET_NAME_PATTERN.flags,
  fileSuffix: PRESET_FILE_SUFFIX,
});

const patternCases: Array<[string, string]> = [
  // ---- v4's own accept list (`tool-presets.test.ts`) ----
  ['v4-accept-a', 'a'],
  ['v4-accept-hard-mode', 'hard-mode'],
  ['v4-accept-take_2', 'take_2'],
  ['v4-accept-2nd-attempt', '2nd-attempt'],
  ['v4-accept-64', 'a'.repeat(64)],
  // ---- v4's own reject list ----
  ['v4-reject-empty', ''],
  ['v4-reject-space', 'has space'],
  ['v4-reject-dotted', 'dotted.name'],
  ['v4-reject-slash', 'path/name'],
  ['v4-reject-backslash', 'back\\slash'],
  ['v4-reject-uppercase', 'Uppercase'],
  ['v4-reject-leading-dash', '-leading-dash'],
  ['v4-reject-leading-underscore', '_leading-underscore'],
  ['v4-reject-dotdot', '..'],
  ['v4-reject-65', 'a'.repeat(65)],
  // ---- quirks ----
  ['trailing-newline', 'abc\n'],
  ['leading-newline', '\nabc'],
  ['accented', 'café'],
  ['astral', 'a😀'],
  ['digit-only', '0'],
  ['digit-then-64', '9' + 'z'.repeat(63)],
  ['digit-then-65', '9' + 'z'.repeat(64)],
  ['trailing-dash', 'a-'],
  ['trailing-underscore', 'a_'],
];
for (const [id, name] of patternCases) {
  out({ kind: 'pattern', id, name, matches: PRESET_NAME_PATTERN.test(name) });
}

const parseCases: Array<[string, string, string]> = [
  // ---- v4's own suite ----
  ['v4-round-trip', 'coin_toss', 'Tools/coin_toss.hard-mode.settings.json'],
  ['v4-other-tool-prefix-sharing-1', 'coin', 'Tools/coin_toss.hard.settings.json'],
  ['v4-other-tool-prefix-sharing-2', 'coin_toss', 'Tools/coin.hard.settings.json'],
  ['v4-definition-file', 'coin_toss', 'Tools/coin_toss.tool.json'],
  ['v4-nested', 'coin_toss', 'Tools/deep/coin_toss.hard.settings.json'],
  ['v4-outside-tools', 'coin_toss', 'coin_toss.hard.settings.json'],
  ['v4-bad-name-space', 'coin_toss', 'Tools/coin_toss.Hard Mode.settings.json'],
  ['v4-empty-segment', 'coin_toss', 'Tools/coin_toss..settings.json'],
  ['v4-dotted-segment', 'coin_toss', 'Tools/coin_toss.a.b.settings.json'],
  // ---- the overlap (JS slice(a, b) with a > b → "") ----
  ['overlap-coin', 'coin', 'Tools/coin.settings.json'],
  ['overlap-dotted-tool', 'a.settings', 'Tools/a.settings.json'],
  ['overlap-exact-prefix-only', 'coin', 'Tools/coin.'],
  ['overlap-suffix-only', 'coin', '.settings.json'],
  ['bare-double-dot', 'coin', 'Tools/coin..settings.json'],
  // ---- case sensitivity (contrast the `.tool.json` filter, which lowercases) ----
  ['suffix-case', 'coin', 'Tools/coin.fine.Settings.JSON'],
  ['suffix-upper', 'coin', 'Tools/coin.fine.SETTINGS.JSON'],
  ['prefix-folder-case', 'coin', 'tools/coin.fine.settings.json'],
  ['prefix-tool-case', 'coin', 'Tools/Coin.fine.settings.json'],
  ['segment-upper', 'coin', 'Tools/coin.Fine.settings.json'],
  // ---- shapes ----
  ['fine', 'coin', 'Tools/coin.fine.settings.json'],
  ['nested-old', 'coin', 'Tools/old/coin.y.settings.json'],
  ['another-tool', 'coin', 'Tools/lockpick.x.settings.json'],
  ['definition-coin', 'coin', 'Tools/coin.tool.json'],
  ['leading-underscore', 'coin', 'Tools/coin._x.settings.json'],
  ['leading-dash', 'coin', 'Tools/coin.-x.settings.json'],
  ['digit-first', 'coin', 'Tools/coin.2nd.settings.json'],
  ['seg-64', 'coin', `Tools/coin.${'a'.repeat(64)}.settings.json`],
  ['seg-65', 'coin', `Tools/coin.${'a'.repeat(65)}.settings.json`],
  ['dot-inside', 'coin', 'Tools/coin.a.b.settings.json'],
  ['trailing-newline-segment', 'coin', 'Tools/coin.abc\n.settings.json'],
  ['trailing-newline-path', 'coin', 'Tools/coin.abc.settings.json\n'],
  ['accented-segment', 'coin', 'Tools/coin.café.settings.json'],
  ['astral-segment', 'coin', 'Tools/coin.a😀.settings.json'],
  ['astral-tool', 'c😀', 'Tools/c😀.fine.settings.json'],
  ['empty-tool-name', '', 'Tools/.x.settings.json'],
  ['empty-tool-name-bad', '', 'Tools/x.settings.json'],
  ['empty-path', 'coin', ''],
  ['double-suffix', 'coin', 'Tools/coin.x.settings.json.settings.json'],
  ['suffix-in-segment', 'coin', 'Tools/coin.settings.settings.json'],
  ['hyphen-tool', 'coin-toss', 'Tools/coin-toss.quick_1.settings.json'],
  ['leading-slash', 'coin', '/Tools/coin.fine.settings.json'],
  ['backslash-segment', 'coin', 'Tools/coin.a\\b.settings.json'],
  ['slash-segment', 'coin', 'Tools/coin.a/b.settings.json'],
  ['space-in-tool', 'co in', 'Tools/co in.fine.settings.json'],
  ['tool-with-slash', 'old/coin', 'Tools/old/coin.y.settings.json'],
  ['only-suffix-after-tools', 'coin', 'Tools/.settings.json'],
];
for (const [id, toolName, relativePath] of parseCases) {
  out({ kind: 'parse', id, toolName, relativePath, preset: parsePresetFromPath(toolName, relativePath) });
}
