/**
 * Generator for the committed Ollama think-parser case table (P4.D78).
 *
 * The table is the SHARED input corpus of the tier-1 differential
 * (`crates/quilltap-harness/tests/ollama_think_parser_equivalence.rs`): the
 * oracle (`harness/oracle/cases/ollama-think-parser.ts`) drives v4's REAL
 * `plugins/dist/qtap-plugin-ollama/think-parser.ts` over it, and the Rust test
 * drives the port over the SAME rows and diffs per-push.
 *
 * Every row is an explicit `pushes: string[]` — no size-based chopping at read
 * time, so neither side can disagree about where a boundary fell (JS chops by
 * UTF-16 unit, Rust by byte). The adversarial families below are what earn the
 * table its name: every single split point of a text carrying `<think>` /
 * `</think>` is enumerated, so each tag is straddled at every offset.
 *
 * Regenerate (the output is committed; expect a clean tree on no change):
 *   node harness/oracle/fixtures/ollama-think-parser/gen-cases.mjs \
 *     harness/oracle/fixtures/ollama-think-parser/cases.json
 */

import { writeFileSync } from 'node:fs';

const rows = [];
const seen = new Set();

function add(id, pushes) {
  if (seen.has(id)) throw new Error(`duplicate case id ${id}`);
  seen.add(id);
  rows.push({ id, pushes });
}

/** Every single split point of `text` → a two-push row (the tag straddles). */
function addEverySplit(prefix, text) {
  // Split by CODE POINT so the pieces are always valid strings on both sides.
  const cps = Array.from(text);
  for (let i = 1; i < cps.length; i++) {
    add(`${prefix}-split-${i}`, [cps.slice(0, i).join(''), cps.slice(i).join('')]);
  }
}

/** Every ordered pair of split points → a three-push row. */
function addEveryPairSplit(prefix, text) {
  const cps = Array.from(text);
  for (let i = 1; i < cps.length; i++) {
    for (let j = i + 1; j < cps.length; j++) {
      add(`${prefix}-split-${i}-${j}`, [
        cps.slice(0, i).join(''),
        cps.slice(i, j).join(''),
        cps.slice(j).join(''),
      ]);
    }
  }
}

// ---------------------------------------------------------------------------
// 1. v4's own suite, transcribed (the shapes its assertions pin).
// ---------------------------------------------------------------------------
add('v4-whole-split', ['<think>pondering deeply</think>\n\nHello there']);
add('v4-no-think-passthrough', ['  leading space kept\nand newlines too ']);
add('v4-unterminated', ['<think>cut off mid-']);
add('v4-orphan-close', ['Okay, let\'s see. The user wants JSON.\n</think>\n\n["cat", "dog"]']);
add('v4-stray-close-after-block', ['<think>real</think>answer </think> tail']);
add('v4-multiple-blocks', ['<think>first</think>Answer part one <think>second</think>and two']);
add('v4-partial-tag-never-completes', ['half a tag <thi', 'rd of the way']);
for (const size of [1, 2, 3, 5, 7, 11]) {
  const text = 'Intro <think>some\nreasoning</think> and the answer<think>more</think>!';
  const cps = Array.from(text);
  const pushes = [];
  for (let i = 0; i < cps.length; i += size) pushes.push(cps.slice(i, i + size).join(''));
  add(`v4-chop-${size}`, pushes);
}

// ---------------------------------------------------------------------------
// 2. Exhaustive straddles: both tags, every boundary.
// ---------------------------------------------------------------------------
addEverySplit('straddle-basic', 'A<think>R</think>B');
addEverySplit('straddle-orphan', 'pre</think>post');
addEverySplit('straddle-ws-after-close', '<think>r</think>   \n\n  Answer');
addEverySplit('straddle-unterminated', 'vis<think>tail');
addEverySplit('straddle-empty-block', 'a<think></think>b');
addEverySplit('straddle-nothink', 'plain text with no tags at all');
// A close tag AFTER visible output — the orphan rule is off the table.
addEverySplit('straddle-close-after-visible', 'said</think>more');
// Two blocks back to back, so `sawThinkBlock` is already true at the second.
addEverySplit('straddle-two-blocks', '<think>a</think>X<think>b</think>Y');
// Three-push straddles of the two tags in isolation (each tag split twice).
addEveryPairSplit('pair-open', 'q<think>w</think>e');

// ---------------------------------------------------------------------------
// 3. Adversarial one-offs.
// ---------------------------------------------------------------------------
add('empty-input', ['']);
add('no-pushes-at-all', []);
add('empty-pushes-interleaved', ['', '<think>', '', 'r', '', '</think>', '', 'v', '']);
add('lone-open-tag', ['<think>']);
add('lone-close-tag', ['</think>']);
add('close-then-open', ['</think><think>r</think>v']);
add('open-tag-arrives-one-char-at-a-time', [
  '<', 't', 'h', 'i', 'n', 'k', '>', 'r', '<', '/', 't', 'h', 'i', 'n', 'k', '>', 'v',
]);
add('held-prefix-of-close-at-flush', ['<think>r</think>tail</thin']);
add('held-prefix-of-open-at-flush', ['tail<thin']);
add('held-prefix-of-close-orphan-eligible', ['abc</thin']);
add('almost-close-then-real-text', ['abc</thin', 'k more text']);
add('almost-open-then-real-text', ['abc<thin', 'k more text']);
add('whitespace-only-after-block', ['<think>r</think>   ']);
add('whitespace-only-after-block-then-text', ['<think>r</think>   ', '  Hi']);
add('all-whitespace-no-block', ['   \n\t  ']);
// JS `\s` is not Rust's `char::is_whitespace`: U+FEFF is JS whitespace and
// U+00A0 is both; U+0085 is Rust-only and must NOT be stripped.
add('js-ws-after-block-feff', ['<think>r</think>﻿ Hi']);
add('js-ws-after-block-u0085-kept', ['<think>r</think>Hi']);
// Non-ASCII payloads around and inside the tags (byte vs UTF-16 indexing).
add('astral-around-tags', ['\u{1F600}<think>\u{1F914}</think>\u{1F44B}']);
add('astral-split-mid-block', ['\u{1F600}<think>\u{1F914}', '</think>\u{1F44B}']);
add('cjk-payload', ['前置<think>思考</think>答え']);
add('accented-passthrough', ['café naïve résumé']);
// Nested-looking input: an inner `<think>` inside a block is literal reasoning.
add('open-inside-block', ['<think>a<think>b</think>c']);
add('close-close', ['<think>a</think></think>c']);
// The reasoning-only stream: nothing visible ever emitted.
add('reasoning-only', ['<think>all of it</think>']);
add('reasoning-only-whitespace-tail', ['<think>all of it</think>\n']);

writeFileSync(
  process.argv[2] ?? 'cases.json',
  JSON.stringify(rows, null, 2) + '\n'
);
console.error(`wrote ${rows.length} case(s)`);
