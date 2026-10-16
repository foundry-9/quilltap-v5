/**
 * Oracle case: Pascal TRIGGER PHRASES — the pure scanner (P4.D305, v4
 * `362426c8f`).
 *
 * Drives the REAL code from v4's lib/pascal/trigger-phrases.ts:
 *   scanTriggerPhrases, triggersContainedIn, MAX_TRIGGER_RUNS_PER_MESSAGE
 *
 * Every `scan` row emits `{ hits, dropped }` WHOLE — `tool`, `phrase` (as
 * written) and `index`, which is a UTF-16 offset into the FOLDED, STRIPPED
 * haystack ("for ordering only", but exact here: the astral, CRLF and Carina
 * rows are where a byte offset or a strip-after-fold would show). `contained`
 * rows drive `triggersContainedIn`.
 *
 * A tool travels as `{ name, triggers? }` — the `Pick<QtapCustomTool, 'name' |
 * 'triggers'>` the scanner accepts — so an ABSENT `triggers` and a
 * zero-length phrase (which the schema refuses, but the Pick admits) are both
 * reachable.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   TZ=UTC npx tsx ~/source/quilltap-v5/harness/oracle/cases/pascal-trigger-phrases.ts \
 *     > /tmp/oracle-pascal-trigger-phrases.ndjson
 */

import {
  scanTriggerPhrases,
  triggersContainedIn,
  MAX_TRIGGER_RUNS_PER_MESSAGE,
} from '@/lib/pascal/trigger-phrases';

type Tool = { name: string; triggers?: string[] };
const tool = (name: string, triggers?: string[]): Tool => (triggers === undefined ? { name } : { name, triggers });

const capTools = Array.from({ length: MAX_TRIGGER_RUNS_PER_MESSAGE + 1 }, (_, i) => tool(`t${i}`, [`word${i}`]));

const scanCases: Array<[string, string, Tool[]]> = [
  // ---- v4's own ten (`trigger-phrases.test.ts`) ----
  ['v4-folds-both-sides', 'She cried ABRACADABRA!', [tool('spell', ['AbraCadabra'])]],
  ['v4-substring-not-word', 'warm by the fireplace', [tool('blaze', ['fire'])]],
  ['v4-one-hit-earliest-phrase', 'open sesame, then abracadabra', [tool('door', ['abracadabra', 'open sesame'])]],
  ['v4-order-by-appearance', 'I draw my sword and consult the oracle', [tool('oracle', ['consult the oracle']), tool('initiative', ['draw my sword'])]],
  ['v4-substring-of-other-phrase', 'the firestorm', [tool('a', ['fire']), tool('b', ['firestorm'])]],
  ['v4-cap-and-dropped', capTools.map((_, i) => `word${i}`).join(' '), capTools],
  ['v4-carina-public-line', '@Alice: abracadabra', [tool('spell', ['abracadabra'])]],
  ['v4-carina-whisper-line', '@Alice? “abracadabra”', [tool('spell', ['abracadabra'])]],
  ['v4-spoken-lines-around-carina', 'Hold on.\n@Alice: what is a spell?\nAbracadabra!', [tool('spell', ['abracadabra'])]],
  ['v4-empty-content', '', [tool('spell', ['abracadabra'])]],
  ['v4-tool-without-triggers', 'abracadabra', [tool('plain')]],
  // ---- offsets ----
  // The strip shifts every later offset: the spoken line is now at 0.
  ['offset-after-stripped-carina-line', '@Alice: q\nabracadabra', [tool('spell', ['abracadabra'])]],
  // An astral character is TWO UTF-16 units: 3, not 2.
  ['offset-past-astral', '😀 abracadabra', [tool('spell', ['abracadabra'])]],
  // CRLF is normalised to LF by the strip: index 2, not 3.
  ['offset-crlf-normalised', 'a\r\nabracadabra', [tool('spell', ['abracadabra'])]],
  // `İ` folds to TWO units (i + U+0307): the offset after it is in the FOLDED text.
  ['offset-after-dotted-capital-i', 'İ abracadabra', [tool('spell', ['abracadabra'])]],
  // ---- ties ----
  ['tie-two-tools-same-offset-roster-order', 'abracadabra', [tool('second', ['abra']), tool('first', ['abracadabra'])]],
  ['tie-one-tool-two-phrases-same-offset', 'abracadabra', [tool('spell', ['abracadabra', 'abra'])]],
  // ---- as written ----
  ['phrase-returned-as-written', 'say open sesame now', [tool('door', ['Open SESAME'])]],
  // A zero-length phrase is skipped (reachable only through the Pick shape).
  ['zero-length-phrase-skipped', 'abracadabra', [tool('spell', ['', 'cadabra'])]],
  ['only-zero-length-phrase', 'abracadabra', [tool('spell', [''])]],
  ['empty-triggers-array', 'abracadabra', [tool('spell', [])]],
  // ---- empties ----
  ['whitespace-only-after-strip', '   \n@Alice: abracadabra\n  ', [tool('spell', ['abracadabra'])]],
  ['whitespace-only-content', ' \t ', [tool('spell', ['abracadabra'])]],
  ['triggers-undefined-and-present', 'abracadabra', [tool('plain'), tool('spell', ['abracadabra'])]],
  ['no-match', 'nothing to see', [tool('spell', ['abracadabra'])]],
  // ---- the cap: the DROPPED one is the LATEST by offset, not by roster ----
  ['cap-drops-latest-by-offset', 'word1 word2 word3 word4 word0', capTools],
  ['cap-exactly-four', 'word0 word1 word2 word3', capTools],
  // ---- fold identities (the definition corpus's rows, at the scanner) ----
  ['fold-sharp-s-not-case-folded', 'STRASSE', [tool('road', ['Straße'])]],
  ['fold-final-sigma', 'ΟΔΥΣΣΕΥΣ', [tool('hero', ['οδυσσευς'])]],
];

for (const [id, content, tools] of scanCases) {
  process.stdout.write(
    JSON.stringify({ kind: 'scan', id, content, tools, out: scanTriggerPhrases(content, tools) }) + '\n',
  );
}

const containedCases: Array<[string, string, string[] | undefined]> = [
  ['v4-contained-as-written', 'The word ABRACADABRA rings out.', ['Abracadabra', 'open sesame']],
  ['v4-contained-none', 'nothing here', ['abracadabra']],
  ['v4-contained-undefined', 'abracadabra', undefined],
  ['contained-empty-text', '', ['abracadabra']],
  ['contained-empty-list', 'abracadabra', []],
  ['contained-zero-length-skipped', 'abracadabra', ['', 'abra']],
  ['contained-definition-order', 'cadabra then abra', ['cadabra', 'abra', 'zzz']],
  // Unlike the scan, `triggersContainedIn` does NOT strip Carina lines.
  ['contained-no-carina-strip', '@Alice: abracadabra', ['abracadabra']],
];

for (const [id, text, triggers] of containedCases) {
  process.stdout.write(
    JSON.stringify({
      kind: 'contained',
      id,
      text,
      ...(triggers === undefined ? {} : { triggers }),
      out: triggersContainedIn(text, triggers),
    }) + '\n',
  );
}

process.stdout.write(JSON.stringify({ kind: 'constant', id: 'max-trigger-runs', out: MAX_TRIGGER_RUNS_PER_MESSAGE }) + '\n');
