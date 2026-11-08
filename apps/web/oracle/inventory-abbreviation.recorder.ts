/**
 * The v4-side recorder behind `src/app/inventory/inventory-glyph.oracle.spec.ts`.
 *
 * v4 `27fb3ec77` taught the Inventory glyph a second fallback: an object with
 * neither SVG nor emoji can show an ABBREVIATION of its name (`Pocket Watch` →
 * `PW`, `lantern` → `LA`) — the Salon's carried-items row under each avatar
 * asks for it. The rule is `inventoryAbbreviation` in
 * `components/inventory/inventory-glyph.tsx`, exported at the pin, and THIS
 * corpus is recorded by importing that REAL module (tsx loads the `.tsx` and its
 * React / icon imports without rendering anything — the import idiom, not the
 * slice idiom).
 *
 * One `abbr` row per label: its input and v4's answer. The corpus crosses
 * empty / whitespace-only / punctuation-only words, a leading-punctuation word,
 * three words, digits, mixed scripts (Latin with diacritics, Cyrillic, CJK, an
 * astral Fraktur pair), precomposed vs decomposed combining marks, one-letter
 * words, `ß` (upper-cases to `SS`), and Turkish dotted / dotless `i`
 * (`toUpperCase` is locale-free on both sides).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/components/...`
 * and would not compile in the SPA's own tsconfig.
 *
 * Run it from a lane-unique pinned v4 worktree at `624d0d25c` (Node 24.13.1):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-<order>-624d0d25c
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" 624d0d25c
 * (cd "$PIN" && npm ci --offline --ignore-scripts --no-audit --no-fund && npm rebuild better-sqlite3)
 * export PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH
 * cp <V5>/apps/web/oracle/inventory-abbreviation.recorder.ts "$PIN/"
 * rm -f <V5>/apps/web/src/testing/fixtures/inventory-abbreviation.oracle.ndjson
 * cd "$PIN" && npx tsx inventory-abbreviation.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/inventory-abbreviation.oracle.ndjson
 * wc -l <V5>/apps/web/src/testing/fixtures/inventory-abbreviation.oracle.ndjson
 * ```
 *
 * Expect the line count recorded in `src/testing/fixtures/README.md`'s
 * `inventory-abbreviation.oracle.ndjson` section; a second run must reproduce
 * the file byte for byte (`cmp`).
 */

import { inventoryAbbreviation } from '@/components/inventory/inventory-glyph';

const LABELS: Array<[string, string]> = [
  ['v4 pocket watch', 'Pocket Watch'],
  ['v4 lantern', 'lantern'],
  ['empty', ''],
  ['whitespace only', '   \t\n '],
  ['punctuation-only words', '— —'],
  ['ampersand word then word', '& Co'],
  ['leading-punctuation word', '"Lucky" coin'],
  ['three words', 'Brass Pocket Watch'],
  ['digits first', '3 coins'],
  ['letter then digit', 'x9'],
  ['digits only', '1925'],
  ['mixed scripts latin diacritic', 'Ñandú feather'],
  ['cyrillic', 'Карманные часы'],
  ['cyrillic one word', 'фонарь'],
  ['cjk one word', '懐中時計'],
  ['cjk two words', '懐中 時計'],
  ['astral fraktur', '𝔄𝔅 Box'],
  ['astral one word', '𝔄𝔅𝔆'],
  ['precomposed e acute', 'éclair'],
  ['decomposed e acute', 'éclair'],
  ['leading combining mark word', '́x'],
  ['one-letter word', 'a'],
  ['one-letter words', 'a b'],
  ['sharp s', 'ßtraße'],
  ['sharp s two words', 'ß x'],
  ['turkish dotted capital', 'İstanbul key'],
  ['turkish dotless', 'ıron'],
  ['lowercase two words', 'brass key'],
  ['leading and trailing spaces', '  Brass key  '],
  ['nbsp separated', 'Brass key'],
  ['tab separated', 'Brass\tkey'],
  ['hyphenated', 'Jack-in-the-box'],
  ['apostrophe word', "Captain's log"],
  ['underscore word', 'snake_case thing'],
  ['emoji word then word', '🎩 hat'],
  ['emoji only', '🎩'],
  ['punctuation then letters in one word', '...abc'],
  ['greek', 'Ωmega pendant'],
];

const seen = new Set<string>();
for (const [id, label] of LABELS) {
  if (seen.has(id)) throw new Error(`duplicate vector label: ${id}`);
  seen.add(id);
  process.stdout.write(`${JSON.stringify({ id, kind: 'abbr', label, out: inventoryAbbreviation(label) })}\n`);
}
