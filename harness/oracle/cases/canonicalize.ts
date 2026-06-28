/**
 * Oracle case #28 (Wave 6 / B19): tool canonicalization.
 *
 * Drives the REAL helpers:
 *   canonicalizeUniversalTool, canonicalizeUniversalTools
 *     (lib/tools/canonicalize.ts)
 *
 * The canonical form's whole purpose is a byte-stable serialization, so the
 * oracle emits `JSON.stringify(result)` as a STRING and the Rust side compares
 * its own serde_json::to_string output against it — key ORDER, not just
 * structure, is what's under test. Tool names stay lowercase snake_case (no
 * digits / case), where code-unit ordering and localeCompare coincide; the
 * scrambled inputs prove the sorts actually run. Parameter values avoid floats
 * so JS and serde number formatting can't differ.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/canonicalize.ts \
 *     > /tmp/oracle-canonicalize.ndjson
 */

import {
  canonicalizeUniversalTool,
  canonicalizeUniversalTools,
  type UniversalTool,
} from '@/lib/tools/canonicalize';

const rows: unknown[] = [];

const tool = (name: string, description: string, parameters: unknown): UniversalTool =>
  ({ type: 'function', function: { name, description, parameters } }) as UniversalTool;

// ---- canonicalizeUniversalTool (single — deep key sort of parameters) -----
const singleCases: Array<[string, UniversalTool]> = [
  [
    'scrambled-top-keys',
    tool('read_file', 'Read a file', {
      type: 'object',
      required: ['path'],
      properties: { path: { type: 'string' }, encoding: { type: 'string' } },
    }),
  ],
  [
    'nested-properties-scrambled',
    tool('search_web', 'Search the web', {
      properties: {
        query: { type: 'string', description: 'the query' },
        limit: { type: 'integer', minimum: 1, maximum: 50 },
      },
      required: ['query'],
      type: 'object',
    }),
  ],
  [
    'array-of-objects',
    tool('doc_grep', 'Grep docs', {
      type: 'object',
      properties: {
        patterns: {
          type: 'array',
          items: { type: 'string', minLength: 1 },
        },
      },
      examples: [
        { zeta: 1, alpha: 2 },
        { beta: 3, gamma: 4 },
      ],
      required: [],
    }),
  ],
  ['empty-parameters', tool('state', 'Get state', {})],
];
for (const [id, t] of singleCases) {
  rows.push({ kind: 'one', id, input: t, out: JSON.stringify(canonicalizeUniversalTool(t)) });
}

// ---- canonicalizeUniversalTools (array — name sort + per-tool canonical) ---
const arrayCases: Array<[string, UniversalTool[]]> = [
  ['empty', []],
  ['single', [tool('search', 'Search', { type: 'object', properties: {}, required: [] })]],
  [
    'reorder-by-name',
    [
      tool('write_file', 'Write', { type: 'object', required: ['path'], properties: { path: { type: 'string' } } }),
      tool('doc_read_file', 'Read', { properties: { id: { type: 'string' } }, type: 'object', required: ['id'] }),
      tool('ask_carina', 'Ask', { type: 'object', properties: {}, required: [] }),
      tool('search_web', 'Search', { type: 'object', properties: {}, required: [] }),
    ],
  ],
  [
    'underscore-vs-letter-ordering',
    [
      tool('doc_read', 'a', { type: 'object', properties: {}, required: [] }),
      tool('docread', 'b', { type: 'object', properties: {}, required: [] }),
      tool('doc_write', 'c', { type: 'object', properties: {}, required: [] }),
    ],
  ],
  [
    'duplicate-names-stable',
    [
      tool('rng', 'first', { type: 'object', properties: { a: { type: 'string' } }, required: [] }),
      tool('rng', 'second', { type: 'object', properties: { b: { type: 'string' } }, required: [] }),
    ],
  ],
];
for (const [id, ts] of arrayCases) {
  rows.push({ kind: 'many', id, input: ts, out: JSON.stringify(canonicalizeUniversalTools(ts)) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
