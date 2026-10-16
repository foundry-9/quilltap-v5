/**
 * Oracle case (P4.D309 — v4 `bbbcd2388`, copy to folder): the `(copy)` title
 * helper folder filing now calls server-side.
 *
 * Drives the REAL `@/lib/wardrobe/next-copy-title` (`nextCopyTitle`) — no
 * mocks, no reimplementation. v4 has no unit suite for it; the corpus is the
 * order's §4.1 list whole: the `(copy)` → `(copy 2)` → `(copy 3)` chain, the
 * suffix strip's JS `\s` (tab, U+00A0, U+FEFF, U+2028, U+3000), its ASCII `\d`
 * (`(copy ٣)` does NOT strip) and ASCII `/i`, the no-space `(copy2)` that does
 * not strip, only-the-LAST-suffix, the empty and whitespace-only titles (the
 * `|| sourceTitle.trim()` fallback), padded and cased existing titles,
 * non-ASCII lower-casing (`İstanbul`, final sigma), astral characters, a long
 * title, case-only collisions, and a long chain.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx <v5>/harness/oracle/cases/next-copy-title.ts \
 *     > /tmp/oracle-next-copy-title.ndjson
 */

import { nextCopyTitle } from '@/lib/wardrobe/next-copy-title';

const BASELINE = 'p4.d309-next-copy-title';

function emit(row: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(row)}\n`);
}

emit({ kind: 'meta', baseline: BASELINE });

const rows: Array<[string, string, string[]]> = [];
const add = (id: string, source: string, existing: string[] = []) => {
  rows.push([id, source, existing]);
};

// ── the chain ───────────────────────────────────────────────────────────────
add('plain', 'Shirt');
add('taken_cased', 'Shirt', ['shirt (COPY)']);
add('chain_to_3', 'Shirt', ['Shirt (copy)', 'Shirt (copy 2)']);
add('copy_of_copy', 'Shirt (copy)', ['Shirt (copy)']);
add('copy4_strips', 'Shirt (copy 4)');
add('long_chain_12', 'Coat', [
  'Coat (copy)',
  ...Array.from({ length: 11 }, (_, i) => `Coat (copy ${i + 2})`),
]);
add('case_only_n1_n2', 'Hat', ['HAT (COPY)', 'hat (Copy 2)']);

// ── the strip's whitespace (JS `\s`) ────────────────────────────────────────
add('double_inner_trailing', 'Shirt  (copy)  ');
add('tab_before_after', 'Shirt\t(copy)\t');
add('nbsp_before_after', 'Shirt (copy) ');
add('feff_before_after', 'Shirt﻿(copy)﻿');
add('ls_before_after', 'Shirt (copy) ');
add('ideographic_before_after', 'Shirt　(copy)　');
add('nbsp_inside_digits', 'Shirt (copy 7)');
add('nel_not_js_ws', 'Shirt\u0085(copy)');
add('no_space_before_paren', 'Shirt(copy)');
add('no_space_before_digits', 'Shirt (copy2)');

// ── the digits (ASCII `\d`) and the letters (ASCII `/i`) ────────────────────
add('copy_0', 'Shirt (copy 0)');
add('copy_007', 'Shirt (copy 007)');
add('copy_13_digits', 'Shirt (copy 1234567890123)');
add('upper_copy_12', 'Shirt (COPY 12)');
add('mixed_case_copy', 'Shirt (CoPy)');
add('arabic_indic_digit', 'Shirt (copy ٣)');
add('fullwidth_digit', 'Shirt (copy ７)');

// ── which suffix strips ─────────────────────────────────────────────────────
add('two_suffixes_last_only', 'Shirt (copy) (copy)');
add('suffix_mid_title', 'Shirt (copy) blue');
add('only_suffix', '(copy)');
add('only_suffix_padded_n', '  (copy 3)  ');
add('only_suffix_taken', '(copy)', ['(copy) (copy)']);

// ── empty and blank ─────────────────────────────────────────────────────────
add('empty', '');
add('empty_taken', '', [' (copy)']);
add('whitespace_only', '   ');
add('tab_only', '\t');

// ── existing titles: padding, case, non-ASCII ───────────────────────────────
add('padded_existing', 'Shirt', ['  SHIRT (COPY)  ']);
add('unicode_title', 'Ünïcode', ['ünïcode (copy)']);
add('dotted_i', 'İstanbul', ['i̇stanbul (copy)']);
add('dotted_i_not_plain_i', 'İstanbul', ['istanbul (copy)']);
add('final_sigma', 'ΑΣ', ['ας (copy)']);
add('sigma_mid', 'ΑΣ', ['ασ (copy)']);
add('astral', '𝒮𝒽𝒾𝓇𝓉 🧥', ['𝒮𝒽𝒾𝓇𝓉 🧥 (copy)']);
add('long_300', 'L'.repeat(300));
add('long_300_taken', 'L'.repeat(300), [`${'l'.repeat(300)} (copy)`]);
add('existing_blank_entries', 'Shirt', ['', '   ', 'Shirt (copy) ']);

for (const [id, source, existing] of rows) {
  emit({ kind: 'case', id, source, existing, out: nextCopyTitle(source, existing) });
}
