/**
 * Oracle case #20 (Wave 4 / B11): embedding vector-math hot paths.
 *
 * Drives the REAL pure helpers:
 *   normalizeVector, applyEmbeddingProfile, cosineSimilarity,
 *   assertEmbeddingDimensionsMatch, textSimilarity (lib/embedding/embedding-service.ts)
 *   getLiteralPhrase, containsLiteralPhrase, applyLiteralBoost,
 *     LITERAL_BOOST_* consts (lib/embedding/literal-boost.ts)
 *   blobToFloat32, float32ToBlob (lib/embedding/float32-conversion.ts)
 * No impurity; no injection. Float32Array inputs are emitted as their
 * f32-rounded values (Array.from) so the Rust port starts from identical bits.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/embedding-vector.ts \
 *     > /tmp/oracle-embedding-vector.ndjson
 */

import {
  normalizeVector,
  applyEmbeddingProfile,
  cosineSimilarity,
  assertEmbeddingDimensionsMatch,
  textSimilarity,
  type FallbackSearchResult,
} from '@/lib/embedding/embedding-service';
import {
  getLiteralPhrase,
  containsLiteralPhrase,
  applyLiteralBoost,
  LITERAL_BOOST_MIN_PHRASE_LENGTH,
  LITERAL_BOOST_CHARACTER,
  LITERAL_BOOST_GROUP,
  LITERAL_BOOST_PROJECT,
  LITERAL_BOOST_GLOBAL,
} from '@/lib/embedding/literal-boost';
import { blobToFloat32, float32ToBlob } from '@/lib/embedding/float32-conversion';

const rows: unknown[] = [];

// ---- normalizeVector ------------------------------------------------------
// normalizeVector mutates in place, so pass a fresh copy each time. The input
// is emitted as the f32-rounded values it actually operates on.
const normCases: Array<[string, number[]]> = [
  ['unit-x', [1, 0, 0]],
  ['simple-3-4', [3, 4]],
  ['all-zero', [0, 0, 0]],
  ['negatives', [-1, 2, -2]],
  ['fractional', [0.1, 0.2, 0.3, 0.4]],
  ['already-unit', [0.6, 0.8]],
  ['tiny', [1e-20, 2e-20]],
  ['large', [1000, 2000, 3000]],
  ['single', [5]],
];
for (const [id, raw] of normCases) {
  const input = new Float32Array(raw);
  const out = normalizeVector(new Float32Array(raw));
  rows.push({ kind: 'normalize', id, input: Array.from(input), out: Array.from(out) });
}

// ---- applyEmbeddingProfile ------------------------------------------------
const profCases: Array<[string, number[], number | null, boolean | undefined]> = [
  ['truncate-and-normalize', [3, 4, 5, 6], 2, true],
  ['no-truncate-normalize', [1, 1, 1, 1], null, true],
  ['truncate-no-normalize', [3, 4, 5, 6], 2, false],
  ['target-ge-len-keeps-all', [1, 2, 3], 10, true],
  ['default-normalize-omitted', [3, 4], null, undefined],
  ['raw-magnitudes', [0.1, 0.2, 0.3], null, false],
];
for (const [id, raw, truncate, normalizeL2] of profCases) {
  const input = new Float32Array(raw);
  const profile = { truncateToDimensions: truncate, normalizeL2 } as Parameters<
    typeof applyEmbeddingProfile
  >[1];
  const out = applyEmbeddingProfile(input, profile);
  rows.push({
    kind: 'profile',
    id,
    input: Array.from(input),
    truncate,
    normalizeL2: normalizeL2 ?? null,
    out: Array.from(out),
  });
}

// ---- cosineSimilarity -----------------------------------------------------
const cosCases: Array<[string, number[], number[]]> = [
  ['identical-unit', [1, 0, 0], [1, 0, 0]],
  ['orthogonal', [1, 0], [0, 1]],
  ['opposite', [0.6, 0.8], [-0.6, -0.8]],
  ['arbitrary', [0.1, 0.2, 0.3], [0.4, 0.5, 0.6]],
  ['negatives', [-1, 2, -3], [4, -5, 6]],
];
for (const [id, a, b] of cosCases) {
  const fa = new Float32Array(a);
  const fb = new Float32Array(b);
  rows.push({
    kind: 'cosine',
    id,
    a: Array.from(fa),
    b: Array.from(fb),
    out: cosineSimilarity(fa, fb),
  });
}

// ---- dimension-mismatch error messages ------------------------------------
{
  const fa = new Float32Array([1, 2, 3]);
  const fb = new Float32Array([1, 2]);
  let message = '';
  try {
    cosineSimilarity(fa, fb);
  } catch (e) {
    message = (e as Error).message;
  }
  rows.push({ kind: 'cosineErr', id: 'cos-3-vs-2', aLen: 3, bLen: 2, message });
}
{
  let message = '';
  try {
    assertEmbeddingDimensionsMatch(new Float32Array(384), new Float32Array(256), 'document search');
  } catch (e) {
    message = (e as Error).message;
  }
  rows.push({
    kind: 'assertErr',
    id: 'assert-384-vs-256-ctx',
    queryLen: 384,
    storedLen: 256,
    context: 'document search',
    message,
  });
}
{
  let message = '';
  try {
    assertEmbeddingDimensionsMatch(new Float32Array(10), new Float32Array(20));
  } catch (e) {
    message = (e as Error).message;
  }
  rows.push({
    kind: 'assertErr',
    id: 'assert-10-vs-20-noctx',
    queryLen: 10,
    storedLen: 20,
    context: null,
    message,
  });
}

// ---- textSimilarity -------------------------------------------------------
const st = (keywords: string[], exactPhrases: string[]): FallbackSearchResult => ({
  usedEmbedding: false,
  keywords,
  exactPhrases,
});
const textCases: Array<[string, string[], string[], string]> = [
  ['empty-terms', [], [], 'anything goes here'],
  ['phrase-hit', [], ['quilltap engine'], 'the Quilltap Engine is fast'],
  ['phrase-miss', [], ['nonexistent'], 'the Quilltap Engine is fast'],
  ['keywords-some', ['fast', 'slow', 'engine'], [], 'the engine is fast'],
  ['mixed', ['engine', 'rust'], ['quilltap engine'], 'the quilltap engine in rust'],
  ['all-miss', ['zebra'], ['no match here'], 'completely different text'],
];
for (const [id, keywords, exactPhrases, target] of textCases) {
  rows.push({
    kind: 'textSim',
    id,
    keywords,
    exactPhrases,
    target,
    out: textSimilarity(st(keywords, exactPhrases), target),
  });
}

// ---- literal-boost --------------------------------------------------------
const phraseCases: Array<[string, string | null]> = [
  ['null', null],
  ['empty', ''],
  ['too-short', 'short'],
  ['exactly-8', 'abcdefgh'],
  ['trimmed-then-long', '   Quilltap Engine   '],
  ['mixed-case', 'HELLO World Foo'],
  ['short-after-trim', '   ab   '],
];
for (const [id, query] of phraseCases) {
  rows.push({ kind: 'literalPhrase', id, query, out: getLiteralPhrase(query) });
}

const containsCases: Array<[string, string | null, string]> = [
  ['hit', 'The Quilltap Engine', 'quilltap engine'],
  ['miss', 'The Engine', 'quilltap engine'],
  ['null-text', null, 'anything'],
  ['empty-text', '', 'anything'],
  ['case-insensitive', 'HELLO THERE', 'hello there'],
];
for (const [id, text, lowerPhrase] of containsCases) {
  rows.push({ kind: 'containsPhrase', id, text, lowerPhrase, out: containsLiteralPhrase(text, lowerPhrase) });
}

const boostCases: Array<[string, number, number | undefined]> = [
  ['default-zero', 0.0, undefined],
  ['default-half', 0.5, undefined],
  ['default-high', 0.8, undefined],
  ['character', 0.7, LITERAL_BOOST_CHARACTER],
  ['group', 0.7, LITERAL_BOOST_GROUP],
  ['project', 0.7, LITERAL_BOOST_PROJECT],
  ['global', 0.7, LITERAL_BOOST_GLOBAL],
];
for (const [id, score, fraction] of boostCases) {
  const out = fraction === undefined ? applyLiteralBoost(score) : applyLiteralBoost(score, fraction);
  rows.push({ kind: 'literalBoost', id, score, fraction: fraction ?? null, out });
}

rows.push({
  kind: 'consts',
  id: 'literal-boost-constants',
  minPhraseLength: LITERAL_BOOST_MIN_PHRASE_LENGTH,
  character: LITERAL_BOOST_CHARACTER,
  group: LITERAL_BOOST_GROUP,
  project: LITERAL_BOOST_PROJECT,
  global: LITERAL_BOOST_GLOBAL,
});

// ---- float32 <-> blob ------------------------------------------------------
const blobCases: Array<[string, number[]]> = [
  ['empty', []],
  ['one', [1.5]],
  ['several', [0.1, -0.2, 3.14159, 1000.5]],
  ['zeros', [0, 0, 0]],
];
for (const [id, raw] of blobCases) {
  const vec = new Float32Array(raw);
  const blob = float32ToBlob(vec);
  const roundTrip = blobToFloat32(blob);
  rows.push({
    kind: 'blob',
    id,
    vec: Array.from(vec),
    bytes: Array.from(blob),
    roundTrip: Array.from(roundTrip),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
