/**
 * The v4-side recorder behind `src/app/chat/breadcrumb-density.oracle.spec.ts`.
 *
 * `abbreviateName` and `chooseDensity` (`components/chat/ChatHeaderBreadcrumb.tsx`,
 * v4 `3c41cf3da`) are the two pure helpers behind the Salon's compact header
 * breadcrumb: a crowded chat steps its project / character links down from
 * `full` (avatar + name) to `abbrev` (avatar + short form) to `avatar` (avatar
 * alone) until the row fits. v5's `chat/breadcrumb-density.ts` is a
 * transcription; THIS corpus is what proves it.
 *
 * The vectors go past v4's own jest suite (`__tests__/unit/components/chat/
 * ChatHeaderBreadcrumb.test.tsx`, all ten of whose assertions are rows here) to
 * the edges the source actually has: `Array.from` counts CODE POINTS (astral
 * letters, emoji, a combining mark counted as its own point), the `<= 4` single-
 * word boundary, JS `\s+` splitting on NBSP / tab / newline / U+3000, the
 * all-minor fallback, a case-insensitive minor compare, the four-initial cap,
 * and `toUpperCase` on an initial that grows (`ß` → `SS`). `chooseDensity` is
 * driven at each `<=` boundary, with zero widths, `NaN` widths (every compare
 * false → the `?? 'avatar'` floor), infinities, and `available === 0`.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/components/...`
 * and would not compile in the SPA's own tsconfig. tsx loads the `.tsx` module's
 * whole import graph (`Icon`, `Tooltip`, `CopyChatIdButton`, React) without a
 * DOM — none of them touches `window` at import time.
 *
 * Run it from the lane's pinned v4 worktree, which carries its OWN `npm ci`
 * (never a symlink to the live checkout's `node_modules` — the 2026-11-02
 * rulings), with Node 24 at `~/.nvm/versions/node/v24.13.1/bin`:
 *
 * ```bash
 * PIN='/tmp/qt-v4-pin-p4d384-624d0d25c'
 * ls -ld "$PIN" && git -C "$PIN" rev-parse HEAD   # 624d0d25c…
 * F=<V5>/apps/web/src/testing/fixtures/breadcrumb-density.oracle.ndjson
 * rm -f "$F"
 * cp <V5>/apps/web/oracle/breadcrumb-density.recorder.ts "$PIN/"
 * cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx breadcrumb-density.recorder.ts > "$F"
 * ```
 *
 * Expect 52 + 26 = 78 lines (`grep -c '"kind":"abbreviate"'` → 52,
 * `grep -c '"kind":"density"'` → 26); a shorter file means the recorder errored
 * and the redirect already truncated the old one (the empty-file trap).
 *
 * Row shapes:
 *  - `{ id, kind: 'abbreviate', name, out }`
 *  - `{ id, kind: 'density', available, widths: {full, abbrev, avatar}, out }` —
 *    every number goes through `enc`: a finite number stays a number, `NaN` /
 *    `±Infinity` become the strings `"NaN"` / `"Infinity"` / `"-Infinity"` (JSON
 *    has no spelling for them; the spec decodes with `Number(...)`).
 */

import { abbreviateName, chooseDensity } from '@/components/chat/ChatHeaderBreadcrumb';

/** `[label, name]` — the label is the spec's test name, so it must be unique. */
const NAMES: Array<[string, string]> = [
  // --- v4's own jest assertions, verbatim ----------------------------------
  ['jest: Abigail → first three', 'Abigail'],
  ['jest: Revenant → first three', 'Revenant'],
  ['jest: Amy kept whole', 'Amy'],
  ['jest: Riya kept whole (four is the ceiling)', 'Riya'],
  ['jest: Baraka Ilunga → initials', 'Baraka Ilunga'],
  ['jest: Voyages of the Covenant → minor words dropped', 'Voyages of the Covenant'],
  ['jest: five words capped at four initials', 'One Two Three Four Five'],
  ['jest: all minor falls back to every word', 'Of The'],
  ['jest: blank name', '   '],
  // --- empty / whitespace ---------------------------------------------------
  ['empty string', ''],
  ['tab and newline only', '\t\n'],
  ['NBSP only (JS \\s matches U+00A0)', ' '],
  ['ideographic space only (U+3000)', '　'],
  // --- the single-word code-point boundary ---------------------------------
  ['one letter', 'Q'],
  ['four code points kept whole', 'Aria'],
  ['five code points cut to three', 'Arial'],
  ['surrounding whitespace trimmed before counting', '  Riya  '],
  ['lower-case single word keeps its case', 'abigail'],
  ['digits count as letters', '12345'],
  ['punctuation counts', "O'Neil"],
  ['hyphenated single word', 'Jean-Luc'],
  // --- astral code points --------------------------------------------------
  ['astral letter in a long word', '𝒜lice'],
  ['astral letter in a short word (four points, eight units)', '𝒜𝒷𝒸𝒹'],
  ['five emoji cut to three emoji', '😀😀😀😀😀'],
  ['four emoji kept whole', '😀😀😀😀'],
  ['astral initials across words', '𝒜lice 𝒷ob'],
  ['emoji initial', '😀Smile Face'],
  // --- combining marks (each its own code point) ---------------------------
  ['combining acute makes four letters five points', 'Zoéy'],
  ['precomposed é stays four points', 'Zoéy'],
  ['combining mark on a short word', 'Amé'],
  ['combining mark right after the third letter is cut', 'Reńee'],
  ['initial with a combining mark keeps only the base', 'Émile Zola'],
  // --- separators -----------------------------------------------------------
  ['NBSP separates words', 'Baraka Ilunga'],
  ['tab separates words', 'Baraka\tIlunga'],
  ['newline separates words', 'Baraka\nIlunga'],
  ['run of mixed whitespace', 'Baraka \t\n  Ilunga'],
  ['ideographic space separates words', 'Baraka　Ilunga'],
  ['zero-width space does NOT separate (not \\s)', 'Baraka​Ilunga'],
  // --- minor words ----------------------------------------------------------
  ['The Of A — all minor', 'The Of A'],
  ['all ten minor words → first four initials', 'a an and the of in on at to for'],
  ['mixed-case minor words dropped', 'Voyages OF tHe Covenant'],
  ['leading minor word dropped', 'The Brass Owl'],
  ['trailing minor word dropped', 'Brass Owl Of'],
  ['minor-looking word with punctuation is NOT minor', 'Tales of. the Owl'],
  ['five significant words capped at four', 'Alpha Bravo Charlie Delta Echo'],
  ['minors among five significant words', 'Alpha of Bravo the Charlie and Delta in Echo'],
  ['one significant word among minors', 'The Owl'],
  ['lower-case initials upper-cased', 'baraka ilunga'],
  ['toUpperCase can grow an initial (ß → SS)', 'ßtraße Zeit'],
  ['dotted i upper-cases under the default locale', 'istanbul izmir'],
  ['numbers as words', '7 Seas'],
  ['two single letters', 'A B'],
];

type Widths = { full: number; abbrev: number; avatar: number };

const W: Widths = { full: 600, abbrev: 400, avatar: 250 };

/** `[label, available, widths]`. */
const DENSITIES: Array<[string, number, Widths]> = [
  // --- v4's own jest assertions, verbatim ----------------------------------
  ['jest: full fits', 700, W],
  ['jest: only abbrev and avatar fit', 450, W],
  ['jest: only avatar fits', 300, W],
  ['jest: nothing fits → avatar', 100, W],
  // --- each `<=` boundary ---------------------------------------------------
  ['full exactly equal', 600, W],
  ['one under full', 599, W],
  ['abbrev exactly equal', 400, W],
  ['one under abbrev', 399, W],
  ['avatar exactly equal', 250, W],
  ['one under avatar', 249, W],
  ['fractional just under full', 599.5, W],
  // --- zero ------------------------------------------------------------------
  ['available 0 with real widths', 0, W],
  ['available 0 with zero widths', 0, { full: 0, abbrev: 0, avatar: 0 }],
  ['zero widths fit any width', 500, { full: 0, abbrev: 0, avatar: 0 }],
  ['only avatar zero, available 0', 0, { full: 10, abbrev: 5, avatar: 0 }],
  // --- NaN -------------------------------------------------------------------
  ['NaN full skips to abbrev', 500, { full: NaN, abbrev: 400, avatar: 250 }],
  ['NaN everywhere → avatar floor', 500, { full: NaN, abbrev: NaN, avatar: NaN }],
  ['NaN available → avatar floor', NaN, W],
  ['NaN full and abbrev, avatar fits', 300, { full: NaN, abbrev: NaN, avatar: 250 }],
  // --- non-monotone widths (the first fit wins, in order) ---------------------
  ['abbrev wider than full still picks full first', 500, { full: 300, abbrev: 450, avatar: 100 }],
  ['avatar wider than abbrev', 350, { full: 600, abbrev: 300, avatar: 400 }],
  // --- infinities and negatives -------------------------------------------------
  ['Infinity available fits full', Infinity, W],
  ['-Infinity available → avatar floor', -Infinity, W],
  ['Infinity widths never fit', 1000, { full: Infinity, abbrev: Infinity, avatar: Infinity }],
  ['negative available', -5, W],
  ['negative widths fit available 0', 0, { full: -1, abbrev: -1, avatar: -1 }],
];

/** JSON has no NaN / Infinity; spell them as strings the spec feeds to `Number`. */
function enc(n: number): number | string {
  return Number.isFinite(n) ? n : String(n);
}

const out: string[] = [];
const seen = new Set<string>();
function id(label: string): string {
  if (seen.has(label)) throw new Error(`duplicate label: ${label}`);
  seen.add(label);
  return label;
}

for (const [label, name] of NAMES) {
  out.push(JSON.stringify({ id: id(label), kind: 'abbreviate', name, out: abbreviateName(name) }));
}
for (const [label, available, widths] of DENSITIES) {
  out.push(
    JSON.stringify({
      id: id(label),
      kind: 'density',
      available: enc(available),
      widths: { full: enc(widths.full), abbrev: enc(widths.abbrev), avatar: enc(widths.avatar) },
      out: chooseDensity(available, widths),
    }),
  );
}

process.stdout.write(out.join('\n') + '\n');
