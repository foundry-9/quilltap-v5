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
 */

import { generateRenderingPatterns } from '@/lib/chat/annotations';
import type { TemplateDelimiter, NarrationDelimiters } from '@/lib/schemas/template.types';

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

interface Case {
  name: string;
  delimiters: TemplateDelimiter[];
  narration?: NarrationDelimiters;
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
];

for (const c of CASES) {
  const output = generateRenderingPatterns(c.delimiters, c.narration);
  const row = {
    name: c.name,
    delimiters: c.delimiters,
    narrationDelimiters: c.narration ?? null,
    output,
  };
  process.stdout.write(JSON.stringify(row) + '\n');
}
