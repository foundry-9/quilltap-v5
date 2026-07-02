/**
 * Oracle case: Carina inline-query parser (chat-orchestration wave 1.5).
 *
 * Drives the REAL pure function from v4's lib/chat/carina-parser.ts:
 *   parseCarinaQuery
 *
 * Corpus covers: no markup, markup at start/middle/end of a multiline
 * message, multiple @-lines in one text (first-non-empty wins), straight and
 * smart quote pairing (with and without a matching close), names with
 * interior spaces/underscores/digits, non-ASCII names (accents, CJK, an
 * astral emoji — none of which are JS `\w`, so they land in the "no match"
 * or "@ not at line start relative to the name" bucket as appropriate),
 * unterminated markup, empty/whitespace-only question bodies, CRLF/LF/mixed
 * line endings, and assorted punctuation edge cases.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/carina-parser.ts \
 *     > /tmp/oracle-carina-parser.ndjson
 */

import { parseCarinaQuery } from '@/lib/chat/carina-parser';

interface Row {
  id: string;
  content: string | null;
  out: { characterName: string; whisper: boolean; question: string } | null;
}

const cases: Array<[string, string | null]> = [
  // ---- null / empty / whitespace-only ----
  ['null-input', null],
  ['empty-string', ''],
  ['whitespace-only', '   \n  \n  '],
  ['no-at-sign-plain-text', 'just some regular text\nwith no carina queries'],

  // ---- basic public / whisper ----
  ['basic-public-colon', '@Alice: hello'],
  ['basic-whisper-question', '@Bob? are you there'],
  ['brahma-ordinary-name-colon', '@Brahma: what tables exist'],
  ['brahma-ordinary-name-whisper', '@Brahma? how many chats reference Aria'],

  // ---- name shapes ----
  ['single-char-name-rejected', '@A: hi'],
  ['two-char-name', '@Al: hi'],
  ['name-with-interior-space', '@Earl Grey: tea or coffee'],
  ['name-with-multiple-interior-spaces', '@Lord Peter Wimsey? a question'],
  ['name-leading-underscore', '@_Alice: hi'],
  ['name-trailing-underscore', '@Alice_: hi'],
  ['name-with-digits', '@Bot2: hello'],
  ['name-with-interior-underscore', '@Alice_Bob: hi'],
  ['name-digits-and-spaces', '@Bot 2 Version 3: question'],
  ['space-before-separator-rejected', '@Alice : question'],

  // ---- non-ASCII names (JS \w is ASCII-only, so these never match LINE_RE) ----
  ['name-accented-rejected', '@Renée: bonjour'],
  ['name-cjk-rejected', '@田中: こんにちは'],
  ['name-astral-emoji-rejected', '@Al😀ce: hi'],
  // A non-ASCII char adjacent to an otherwise-valid name still breaks the
  // trailing \w boundary the regex requires.
  ['name-trailing-accent-rejected', '@José: hola'],

  // ---- quoting: straight double ----
  ['straight-double-quotes-matched', '@Bob: "What was the capital?"'],
  [
    'straight-double-quotes-interior-punctuation',
    '@Bob: "What was the capital? And who ruled?"',
  ],
  ['straight-double-unterminated', '@Bob: "no closing quote here'],
  ['straight-double-empty', '@Bob: ""'],

  // ---- quoting: straight single ----
  ["straight-single-quotes-matched", "@Bob? 'two sentences here. yes?'"],
  ["straight-single-unterminated", "@Bob: 'no closing quote here"],

  // ---- quoting: smart quotes ----
  ['smart-double-quotes-matched', '@Bob: “fancy question”'],
  ['smart-single-quotes-matched', '@Bob: ‘fancy question’'],
  ['smart-double-unterminated', '@Bob: “no closing smart quote'],
  // Smart-open must close with smart-close, not a straight quote — falls
  // through to the unquoted form (no straight " found to pair with it... but
  // one is present later, proving pairing is by exact character, not "any
  // quote-like char").
  ['smart-open-straight-close-no-pair', '@Bob: “text" more'],
  ['smart-quote-followed-by-more-text', '@Bob: “text” more'],

  // ---- unquoted ----
  ['unquoted-to-end-of-line', '@Bob: everything after separator to end'],
  ['unquoted-leading-spaces-trimmed', '@Bob:    question with leading spaces'],
  ['unquoted-trailing-spaces-trimmed', '@Bob: question with trailing spaces   '],
  ['unquoted-interior-whitespace-preserved', '@Bob: what   is   this'],
  ['tabs-after-separator', '@Bob:\t\tquestion'],

  // ---- empty question bodies ----
  ['empty-after-colon', '@Alice:'],
  ['whitespace-only-after-colon', '@Alice:    '],
  ['quoted-empty-question', '@Alice: ""'],

  // ---- multiple lines / first-non-empty wins ----
  ['markup-at-start-of-multiline', '@Alice: first\n@Bob: second'],
  [
    'markup-in-middle-of-multiline',
    'preamble text\n@Alice: the real question\nfollowup text',
  ],
  [
    'markup-at-end-of-multiline',
    'first line of text\nsecond line of text\n@Charlie: final question',
  ],
  ['first-empty-second-real', '@Alice:\n@Bob: real question'],
  ['multiple-empty-then-real', '@Alice:\n@Bob:\n@Charlie: found it'],
  ['non-at-lines-skipped-then-match', 'some text\n@Charlie: the answer'],
  [
    'multiple-queries-first-wins',
    '@Alice: first question\n@Bob: second question\n@Charlie: third question',
  ],

  // ---- @-detection rules ----
  ['at-mid-line-not-start-rejected', 'email me@host: what is this'],
  ['at-preceded-by-text-same-line-rejected', 'some text @Alice: question'],
  ['at-after-newline-matches', 'first line\n@Alice: question'],

  // ---- line endings ----
  ['lf-line-endings', '@Alice: first\n@Bob: second'],
  ['crlf-line-endings', '@Alice: first\r\n@Bob: second'],
  ['crlf-strips-trailing-cr-from-question', '@Alice: question\r\nmore text'],
  ['mixed-line-endings', '@Alice: first\n@Bob: second\r\n@Charlie: third'],
  // A lone \r with no following \n is NOT a JS line separator (/\r?\n/), so
  // this whole thing is one "line" to the regex — and the trailing content
  // after the embedded \r is folded into the unquoted question body via `.`
  // (which doesn't match \n but DOES match a bare \r).
  ['lone-cr-not-a-separator', '@Alice: question\rmore text on same line'],

  // ---- cross-line quote behavior ----
  [
    'quoted-does-not-span-lines',
    '@Bob: "first line\nmore text"',
  ],

  // ---- real-world composite examples ----
  [
    'real-world-public-question-mark-in-question',
    '@Jeeves: What is the capital of Wessex?',
  ],
  [
    'real-world-whisper-no-colon-in-name',
    '@PeterWimsey? Is this a coded message?',
  ],
  [
    'real-world-quoted-multi-sentence',
    '@Alice: "Do you know the answer? What about alternatives? Which is best?"',
  ],
  [
    'text-before-at-line-ignored',
    'I have a question for someone.\n@Bob: What is this?',
  ],
  [
    'text-after-at-line-ignored',
    '@Bob: What?\n\nSome follow-up text that is ignored.',
  ],

  // ---- quote pairing specificity ----
  ['straight-double-closes-straight-double', '@Bob: "question"'],
  ["straight-single-closes-straight-single", "@Bob: 'question'"],
  [
    'smart-double-closes-smart-double',
    '@Bob: “question”',
  ],
  [
    'smart-single-closes-smart-single',
    '@Bob: ‘question’',
  ],
  [
    'whitespace-trimmed-inside-quotes',
    '@Bob: "  text inside  "',
  ],
];

const rows: Row[] = cases.map(([id, content]) => ({
  id,
  content,
  out: parseCarinaQuery(content as string),
}));

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
