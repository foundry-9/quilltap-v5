/**
 * Oracle case #26 (Wave 6 / B17): parseLegacyEmbeddingText.
 *
 * Drives the REAL helper:
 *   parseLegacyEmbeddingText (lib/embedding/float32-conversion.ts)
 *
 * Recovers pre-BLOB JSON-text embeddings. The interesting fidelity point is the
 * index-keyed-object shape, where `Object.values` visits canonical array-index
 * keys in ascending NUMERIC order ("10" after "9", not string-sorted) ahead of
 * any other key. The scrambled-key cases below pin that ordering.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/legacy-embedding.ts \
 *     > /tmp/oracle-legacy-embedding.ndjson
 */

import { parseLegacyEmbeddingText } from '@/lib/embedding/float32-conversion';

const rows: unknown[] = [];

const cases: Array<[string, string]> = [
  // ---- JSON-array shape ----
  ['array-basic', '[0.1,0.2,0.3]'],
  ['array-empty', '[]'],
  ['array-negatives-and-exp', '[-0.5,1.5e-3,0,2.25]'],
  ['array-ints', '[1,2,3,4,5]'],
  ['array-single', '[0.42]'],
  // ---- integer-keyed-object shape (JSON.stringify(Float32Array)) ----
  ['object-ordered', '{"0":0.1,"1":0.2,"2":0.3}'],
  ['object-scrambled', '{"2":0.3,"0":0.1,"1":0.2}'],
  // "10" must land AFTER "9" — numeric order, not string order:
  ['object-two-digit-keys', '{"2":0.3,"0":0.1,"1":0.2,"10":1.1,"9":0.9}'],
  ['object-single', '{"0":0.5}'],
  ['object-empty', '{}'],
  ['object-twelve-dims', '{"0":0.01,"1":0.02,"2":0.03,"3":0.04,"4":0.05,"5":0.06,"6":0.07,"7":0.08,"8":0.09,"9":0.10,"10":0.11,"11":0.12}'],
  // ---- non-embedding inputs → undefined ----
  ['scalar-number', '42'],
  ['scalar-string', '"hello"'],
  ['scalar-bool', 'true'],
  ['literal-null', 'null'],
  ['invalid-bare-word', 'not json'],
  ['invalid-truncated', '{'],
  ['invalid-empty', ''],
];

for (const [id, input] of cases) {
  const out = parseLegacyEmbeddingText(input);
  rows.push({ kind: 'legacy', id, input, out: out ?? null });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
