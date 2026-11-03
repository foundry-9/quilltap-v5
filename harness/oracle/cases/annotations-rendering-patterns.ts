/**
 * Oracle case (P4.6p, tier-1 EXACT): `generateRenderingPatterns`.
 *
 * Drives the REAL `generateRenderingPatterns` from the v4 server's
 * `lib/chat/annotations.ts` over a fixed corpus and prints, per case, the
 * exact `{ delimiters, narrationDelimiters }` input alongside the produced
 * `RenderingPattern[]`. The Rust differential deserializes the SAME input from
 * the NDJSON, runs the port, and asserts byte-equal output (object keys
 * order-insensitive, array order preserved).
 *
 * IMPORTANT — imports the actual app code, does not reimplement it.
 *
 *   cd ~/source/quilltap-server
 *   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
 *     ~/source/quilltap-v5/harness/oracle/cases/annotations-rendering-patterns.ts \
 *     > /tmp/oracle-annotations.ndjson
 *
 * The corpus is fixed in code (no randomness, no Date.now()).
 *
 * P4.D351 (v4 `778b97b2b`, the Novel template): the generator takes a third
 * `options` argument (`unmarkedStyle` → the `[\s\S]` `unmarked` tail,
 * `markdownAsterisks === false` → the `strip` / `literal` pre-parse pair), the
 * narration may be `null`, and a `wrap` carries `matchCurlyQuotes` /
 * `closeOptional` (the quote-tolerant / close-optional arm). A row's `options`
 * rides the NDJSON; a `viaTemplate` row drives `generateTemplateRenderingPatterns`
 * over its `template` object instead. `novel-whole` drives v4's REAL
 * `NOVEL_TEMPLATE` authoring fields.
 */

import { generateRenderingPatterns, generateTemplateRenderingPatterns } from '@/lib/chat/annotations';
import type { TemplateDelimiter, NarrationDelimiters, UnmarkedStyle } from '@/lib/schemas/template.types';
import { NOVEL_TEMPLATE } from '@/lib/chat/novel-template';

// A delimiter builder that fills the required common fields (name/buttonName/
// style) the auto-gen doesn't read for pattern shape but the row schema carries.
type Partial = Record<string, unknown>;
function delim(fields: Partial): TemplateDelimiter {
  return {
    name: (fields.name as string) ?? 'D',
    buttonName: (fields.buttonName as string) ?? 'D',
    style: (fields.style as string) ?? 'qt-rp-x',
    ...fields,
  } as unknown as TemplateDelimiter;
}

interface Options {
  unmarkedStyle?: UnmarkedStyle | null;
  markdownAsterisks?: boolean;
}

interface Case {
  name: string;
  delimiters: TemplateDelimiter[];
  narration?: NarrationDelimiters | null;
  options?: Options;
  /** Drive `generateTemplateRenderingPatterns(template)` instead. */
  viaTemplate?: Record<string, unknown>;
}

const CASES: Case[] = [
  { name: 'empty', delimiters: [] },
  {
    name: 'wrap-same',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-emph', delimiters: '*' })],
  },
  {
    name: 'wrap-diff-brackets',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-br', delimiters: ['[', ']'] })],
  },
  {
    name: 'wrap-diff-braces',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-brace', delimiters: ['{', '}'] })],
  },
  {
    name: 'wrap-tuple-angle',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-ang', delimiters: ['<', '>'] })],
  },
  {
    name: 'wrap-empty-suffix-degrades',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-hash', delimiters: ['#', ''] })],
  },
  {
    name: 'wrap-both-empty-null',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-none', delimiters: ['', ''] })],
  },
  {
    name: 'line-prefix',
    delimiters: [delim({ kind: 'linePrefix', style: 'qt-rp-cmt', marker: '// ' })],
  },
  {
    name: 'line-prefix-empty-marker-null',
    delimiters: [delim({ kind: 'linePrefix', style: 'qt-rp-cmt', marker: '' })],
  },
  {
    name: 'tag-prefix-default-token',
    delimiters: [delim({ kind: 'tagPrefix', style: 'qt-rp-tag', open: '[', close: ']' })],
  },
  {
    name: 'tag-prefix-custom-token',
    delimiters: [
      delim({ kind: 'tagPrefix', style: 'qt-rp-tag', open: '<', close: '>', tokenPattern: '[A-Z]+' }),
    ],
  },
  {
    name: 'tag-prefix-blank-token-falls-back',
    delimiters: [
      delim({ kind: 'tagPrefix', style: 'qt-rp-tag', open: '[', close: ']', tokenPattern: '   ' }),
    ],
  },
  {
    name: 'tag-prefix-empty-close-null',
    delimiters: [delim({ kind: 'tagPrefix', style: 'qt-rp-tag', open: '[', close: '' })],
  },
  {
    name: 'addons-bold-italic',
    delimiters: [
      delim({ kind: 'wrap', style: 'qt-rp-a', delimiters: '*', addOns: { bold: true, italic: true } }),
    ],
  },
  {
    name: 'addons-underline-double-border-dashed-font-reverse',
    delimiters: [
      delim({
        kind: 'wrap',
        style: 'qt-rp-a',
        delimiters: '~',
        addOns: { reverse: true, underline: 'double', border: 'dashed', font: 'serif' },
      }),
    ],
  },
  {
    name: 'addons-underline-single-border-solid',
    delimiters: [
      delim({
        kind: 'wrap',
        style: 'qt-rp-a',
        delimiters: '_',
        addOns: { underline: 'single', border: 'solid' },
      }),
    ],
  },
  {
    name: 'hide-delimiter',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-h', delimiters: '`', hideDelimiter: true })],
  },
  {
    name: 'hide-delimiter-line-prefix',
    delimiters: [
      delim({ kind: 'linePrefix', style: 'qt-rp-cmt', marker: '> ', hideDelimiter: true }),
    ],
  },
  {
    name: 'dedupe-same-key',
    delimiters: [
      delim({ kind: 'wrap', style: 'qt-rp-first', delimiters: '*' }),
      delim({ kind: 'wrap', style: 'qt-rp-second', delimiters: '*' }),
    ],
  },
  {
    name: 'dedupe-kind-distinct',
    delimiters: [
      delim({ kind: 'wrap', style: 'qt-rp-wrap-br', delimiters: ['[', ']'] }),
      delim({ kind: 'tagPrefix', style: 'qt-rp-tag-br', open: '[', close: ']' }),
    ],
  },
  {
    name: 'narration-string-uncovered',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-emph', delimiters: '*' })],
    narration: '"',
  },
  {
    name: 'narration-tuple-uncovered',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-emph', delimiters: '*' })],
    narration: ['(', ')'],
  },
  {
    name: 'narration-already-covered',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-emph', delimiters: '*' })],
    narration: '*',
  },
  {
    name: 'narration-only-empty-delimiters',
    delimiters: [],
    narration: ['«', '»'],
  },
  {
    name: 'narration-both-empty-null',
    delimiters: [],
    narration: '',
  },

  // --- P4.D351: the Novel template's generator arms ---
  {
    name: 'narration-null',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-emph', delimiters: '*' })],
    narration: null,
  },
  {
    name: 'unmarked-plain',
    delimiters: [],
    narration: null,
    options: { unmarkedStyle: { style: 'qt-roleplay-plain' } },
  },
  {
    name: 'unmarked-with-addons',
    delimiters: [delim({ kind: 'wrap', style: 'qt-chat-dialogue', delimiters: '"' })],
    narration: null,
    options: { unmarkedStyle: { style: 'qt-roleplay-plain', addOns: { italic: true } as never } },
  },
  {
    name: 'unmarked-empty-style-emits-nothing',
    delimiters: [],
    narration: null,
    options: { unmarkedStyle: { style: '' } },
  },
  {
    name: 'unmarked-null-explicit',
    delimiters: [],
    narration: '*',
    options: { unmarkedStyle: null },
  },
  {
    name: 'markdown-asterisks-false-alone',
    delimiters: [],
    options: { markdownAsterisks: false },
  },
  {
    name: 'markdown-asterisks-false-with-star-wrap',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-emph', delimiters: '*' })],
    narration: null,
    options: { markdownAsterisks: false },
  },
  {
    name: 'markdown-asterisks-true-explicit',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-emph', delimiters: '*' })],
    narration: '*',
    options: { markdownAsterisks: true },
  },
  {
    name: 'wrap-quote-curly',
    delimiters: [delim({ kind: 'wrap', style: 'qt-chat-dialogue', delimiters: '"', matchCurlyQuotes: true })],
  },
  {
    name: 'wrap-quote-close-optional',
    delimiters: [delim({ kind: 'wrap', style: 'qt-chat-dialogue', delimiters: '"', closeOptional: true })],
  },
  {
    name: 'wrap-quote-both',
    delimiters: [
      delim({ kind: 'wrap', style: 'qt-chat-dialogue', delimiters: '"', matchCurlyQuotes: true, closeOptional: true }),
    ],
  },
  {
    name: 'wrap-single-quote-curly',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-thought', delimiters: "'", matchCurlyQuotes: true })],
  },
  {
    name: 'wrap-pair-curly-quotes-mixed',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-x', delimiters: ['<"', '">'], matchCurlyQuotes: true })],
  },
  {
    name: 'wrap-bracket-close-optional-drops-link-exclusion',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-br', delimiters: ['[', ']'], closeOptional: true })],
  },
  {
    name: 'wrap-bracket-curly-keeps-link-exclusion',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-br', delimiters: ['[', ']'], matchCurlyQuotes: true })],
  },
  {
    name: 'wrap-dash-suffix-exclude',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-x', delimiters: ['<', '-'], closeOptional: true })],
  },
  {
    name: 'wrap-non-ascii-suffix-exclude',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-x', delimiters: ['«', '»'], closeOptional: true })],
  },
  {
    name: 'wrap-astral-suffix-exclude',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-x', delimiters: ['🌙', '🌞'], closeOptional: true })],
  },
  {
    name: 'wrap-same-delim-close-optional-no-guard',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-emph', delimiters: '*', closeOptional: true })],
  },
  {
    name: 'wrap-flags-false-explicit',
    delimiters: [
      delim({ kind: 'wrap', style: 'qt-chat-dialogue', delimiters: '"', matchCurlyQuotes: false, closeOptional: false }),
    ],
  },
  {
    name: 'wrap-empty-suffix-with-flags-degrades',
    delimiters: [delim({ kind: 'wrap', style: 'qt-rp-x', delimiters: ['#', ''], closeOptional: true })],
  },
  {
    name: 'novel-whole',
    delimiters: NOVEL_TEMPLATE.delimiters,
    narration: NOVEL_TEMPLATE.narrationDelimiters,
    options: { unmarkedStyle: NOVEL_TEMPLATE.unmarkedStyle, markdownAsterisks: NOVEL_TEMPLATE.markdownAsterisks },
  },
  { name: 'template-fn-empty', delimiters: [], viaTemplate: {} },
  { name: 'template-fn-null-delimiters', delimiters: [], viaTemplate: { delimiters: null } },
  {
    name: 'template-fn-novel',
    delimiters: [],
    viaTemplate: {
      delimiters: NOVEL_TEMPLATE.delimiters,
      narrationDelimiters: NOVEL_TEMPLATE.narrationDelimiters,
      unmarkedStyle: NOVEL_TEMPLATE.unmarkedStyle,
      markdownAsterisks: NOVEL_TEMPLATE.markdownAsterisks,
    },
  },
  {
    name: 'template-fn-markdown-undefined',
    delimiters: [],
    viaTemplate: { delimiters: [delim({ kind: 'wrap', style: 'qt-rp-emph', delimiters: '*' })], narrationDelimiters: '*' },
  },
];

for (const c of CASES) {
  const output = c.viaTemplate
    ? generateTemplateRenderingPatterns(c.viaTemplate as Parameters<typeof generateTemplateRenderingPatterns>[0])
    : generateRenderingPatterns(c.delimiters, c.narration, c.options ?? {});
  const row = {
    name: c.name,
    delimiters: c.delimiters,
    // The generator cannot tell `undefined` from `null` (both skip the arm);
    // keep the collapse.
    narrationDelimiters: c.narration === undefined ? null : c.narration,
    ...(c.options ? { options: c.options } : {}),
    ...(c.viaTemplate ? { viaTemplate: c.viaTemplate } : {}),
    output,
  };
  process.stdout.write(JSON.stringify(row) + '\n');
}
