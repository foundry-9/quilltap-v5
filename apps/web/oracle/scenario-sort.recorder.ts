/**
 * The v4-side recorder behind `src/app/scenario/scenario-sort.oracle.spec.ts`
 * (P4.D338, v4 `1d0a31132` pinned at `322a5488e`).
 *
 * v4's scenario lists sort by ONE comparator (`lib/scenarios/sort.ts`): the
 * title, then the filename — each trimmed, a leading "The" dropped, compared
 * `localeCompare(…, undefined, { sensitivity: 'base' })` — then the raw title,
 * then the path at default options. The SPA's picker twins it (`scenario-sort.ts`),
 * and this recorder drives v4's REAL module over the SAME grid the Rust family
 * (`harness/oracle/cases/scenario-pure.ts`) replays, so the two twins agree on
 * one corpus: `key` (the sort key), `compare` (the SIGN, -1 / 0 / 1) and `sort`
 * (the full index order of a list).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree at `322a5488e` (Node 24 at
 * `~/.nvm/versions/node/v24.13.1/bin`; `LANG=en_US.UTF-8` — `localeCompare`'s
 * host locale IS v4's semantics, and the spec runs under the same one):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at 322a5488e}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/scenario-sort.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/scenario-sort.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && LANG=en_US.UTF-8 PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx scenario-sort.recorder.ts > "$OUT")
 * wc -l "$OUT"          # the recorder's `rows=` stderr line
 * rm "$PIN/scenario-sort.recorder.ts"
 * ```
 */

import { compareScenarios, scenarioSortKey, sortScenarios, type SortableScenario } from '@/lib/scenarios/sort';

let rows = 0;
const emit = (row: Record<string, unknown>): void => {
  rows += 1;
  console.log(JSON.stringify(row));
};

const SORT_KEYS: string[] = [
  'The Gilded Cage', '  THE Gilded Cage ', 'Theatre Night', 'the-attic', 'the_attic', 'Into the Woods', 'The', 'the', 'THE',
  '', '   ', 'The ', 'The-', 'The_', 'The\u00a0X', 'The\uFEFFX', 'The\u0085X', 'Thé X', '\uFEFFThe Ballroom', 'The  \t Spaced',
  'The The Twice', 'the\u2003Em', 'The\u3000Ideo', 'The\u2028Sep', 'Thex', 'Th e', '😀 The Crown', 'The 😀 Crown', 'ÉCLAT', 'the\nNewline',
];
for (const value of SORT_KEYS) {
  emit({ kind: 'key', value, output: scenarioSortKey(value) });
}

const sc = (title: string, filename: string, path?: string): SortableScenario =>
  path === undefined ? { title, filename } : { title, filename, path };
const sign = (n: number) => (n < 0 ? -1 : n > 0 ? 1 : 0);
const SORT_PAIRS: Array<[string, SortableScenario, SortableScenario]> = [
  ['title_article_ignored', sc('The Ballroom', 'b'), sc('Casino', 'c')],
  ['title_article_ignored_rev', sc('Casino', 'c'), sc('The Ballroom', 'b')],
  ['title_case_insensitive', sc('alpha', 'z'), sc('Beta', 'a')],
  ['title_case_crossing', sc('apple', 'a'), sc('Banana', 'b')],
  ['title_case_crossing_rev', sc('Banana', 'b'), sc('apple', 'a')],
  ['title_tie_filename_decides', sc('same', 'the-zed'), sc('same', 'attic')],
  ['title_tie_filename_article_ignored', sc('same', 'the-attic'), sc('same', 'bell')],
  ['filename_tie_raw_title_equal_base', sc('é', 'x'), sc('e', 'x')],
  ['accent_tie_path_decides', sc('é', 'x', 'Scenarios/a.md'), sc('e', 'x', 'Scenarios/b.md')],
  ['case_tie_path_default_a_before_A', sc('gala', 'gala', 'Scenarios/a.md'), sc('Gala', 'Gala', 'Scenarios/A.md')],
  ['case_tie_path_default_A_after_a', sc('Gala', 'Gala', 'Scenarios/A.md'), sc('gala', 'gala', 'Scenarios/a.md')],
  ['identical_with_path', sc('x', 'x', 'Scenarios/x.md'), sc('x', 'x', 'Scenarios/x.md')],
  ['pathless_vs_path', sc('x', 'x'), sc('x', 'x', 'a')],
  ['pathless_pair', sc('x', 'x'), sc('x', 'x')],
  ['empty_titles', sc('', ''), sc('', '')],
  ['empty_vs_word', sc('', 'a'), sc('Zed', 'a')],
  ['only_the_vs_word', sc('The', 'a'), sc('Thea', 'a')],
  ['bom_led_title', sc('\uFEFFThe Ballroom', 'a'), sc('Ballroom', 'b')],
  ['nel_not_stripped', sc('The\u0085X', 'a'), sc('X', 'a')],
  ['nbsp_stripped', sc('The\u00a0X', 'a'), sc('X', 'a')],
  ['surrogate_pair_titles', sc('😀 Crown', 'a'), sc('😁 Crown', 'a')],
  ['digits_vs_letters', sc('10 Downing', 'a'), sc('2 Downing', 'a')],
  ['punctuation_vs_letters', sc('...And so', 'a'), sc('And so', 'a')],
  ['trailing_space_title', sc('Attic ', 'a', 'p1'), sc('Attic', 'a', 'p2')],
];
for (const [name, a, b] of SORT_PAIRS) {
  emit({ kind: 'compare', name, a, b, output: sign(compareScenarios(a, b)) });
}

const SORT_LISTS: Array<[string, SortableScenario[]]> = [
  ['v4_vector', [sc('The Ballroom', 'the-ballroom', 'Scenarios/the-ballroom.md'), sc('Casino', 'casino', 'Scenarios/casino.md'), sc('untitled-dock', 'untitled-dock', 'Scenarios/untitled-dock.md'), sc('The Zeppelin', 'zeppelin', 'Scenarios/zeppelin.md')]],
  ['v4_vault_shape', [sc('The Cellar', '3-the-cellar', 'Scenarios/3-the-cellar.md'), sc('Attic', 'attic', 'Scenarios/attic.md'), sc('The Parlour', 'parlour', 'Scenarios/parlour.md')]],
  ['mixed_case_and_articles', [sc('zebra', 'z', 'Scenarios/z.md'), sc('The apple', 'a', 'Scenarios/a.md'), sc('Banana', 'b', 'Scenarios/b.md'), sc('the Cherry', 'c', 'Scenarios/c.md'), sc('apple', 'a2', 'Scenarios/a2.md'), sc('Éclair', 'e', 'Scenarios/e.md')]],
  ['ties_everywhere', [sc('Gala', 'Gala', 'Scenarios/b.md'), sc('gala', 'gala', 'Scenarios/B.md'), sc('gala', 'gala', 'Scenarios/a.md'), sc('Gala', 'Gala', 'Scenarios/A.md'), sc('é', 'x', 'p'), sc('e', 'x', 'q'), sc('E', 'x', 'o')]],
  ['pathless_mix', [sc('b', 'b'), sc('a', 'a', 'z'), sc('a', 'a'), sc('The b', 'b')]],
  ['single', [sc('Only', 'only', 'Scenarios/only.md')]],
  ['empty', []],
];
for (const [name, items] of SORT_LISTS) {
  const tagged = items.map((item, index) => ({ item, index }));
  const sorted = sortScenarios(tagged, (t) => t.item).map((t) => t.index);
  emit({ kind: 'sort', name, items, output: sorted });
}

console.error(`rows=${rows}`);
