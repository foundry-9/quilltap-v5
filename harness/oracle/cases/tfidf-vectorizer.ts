/**
 * Oracle case (W4.7e2): the BUILTIN TF-IDF/BM25 embedding provider.
 *
 * Drives the REAL exported functions from the builtin-embeddings plugin:
 *   stem / tokenize / generateBigrams  (porter-stemmer.ts)
 *   TfIdfVectorizer                    (tfidf-vectorizer.ts)
 * over a committed corpus, emitting one NDJSON row per case for a tier-1 diff.
 *
 * Numeric fields (idf, transformed vectors) carry the `Math.log` libm seam and
 * are compared at 1e-12 by the Rust harness (a 1-ULP `ln` divergence is ≈1e-16).
 * The stemmer / tokenizer / vocabulary are exact. `fittedAt` (v4's internal
 * `new Date().toISOString()`) is captured here and injected into the Rust fit so
 * the state round-trip matches.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/tfidf-vectorizer.ts \
 *     > /tmp/oracle-tfidf-vectorizer.ndjson
 */

import { stem, tokenize, generateBigrams } from '@/plugins/dist/qtap-plugin-builtin-embeddings/porter-stemmer';
import { TfIdfVectorizer } from '@/plugins/dist/qtap-plugin-builtin-embeddings/tfidf-vectorizer';

const rows: unknown[] = [];

// ---------------------------------------------------------------------------
// 1. Stemmer — dozens of words per suffix family (a one-word divergence shifts
//    every vocabulary index).
// ---------------------------------------------------------------------------
const stemWords: string[] = [
  // Short words (<= 2 chars) — returned unchanged, un-lowercased.
  'a', 'ab', 'AB', 'x', 'Go', 'is',
  // Plurals (step 1a).
  'caresses', 'ponies', 'ties', 'caress', 'cats', 'dogs', 'buses', 'glasses',
  'classes', 'kisses', 'series', 'skies', 'boss', 'mass', 'process', 'axes',
  'gas', 'bus', 'as',
  // -ed / -ing (step 1b) + at/bl/iz + double-consonant + CVC.
  'agreed', 'feed', 'plastered', 'bled', 'motoring', 'sing', 'conflated',
  'troubled', 'sized', 'hopping', 'tanned', 'falling', 'hissing', 'fizzed',
  'failing', 'filing', 'happening', 'running', 'jumped', 'walked', 'played',
  'freed', 'agreed', 'guaranteed', 'controlled', 'batted', 'fitting', 'stopping',
  // -y -> -i (step 1c).
  'happy', 'sky', 'try', 'apply', 'quickly', 'gray', 'toy', 'yy', 'body',
  // step 2 suffix chains.
  'relational', 'conditional', 'rational', 'valenci', 'hesitanci', 'digitizer',
  'conformabli', 'radicalli', 'differentli', 'vileli', 'analogousli',
  'vietnamization', 'predication', 'operator', 'feudalism', 'decisiveness',
  'hopefulness', 'callousness', 'formaliti', 'sensitiviti', 'sensibiliti',
  // step 3.
  'triplicate', 'formative', 'formalize', 'electriciti', 'electrical', 'hopeful',
  'goodness',
  // step 4.
  'revival', 'allowance', 'inference', 'airliner', 'gyroscopic', 'adjustable',
  'defensible', 'irritant', 'replacement', 'adjustment', 'dependent', 'adoption',
  'homologou', 'communism', 'activate', 'angulariti', 'homologous', 'effective',
  'bowdlerize', 'adoption', 'condition', 'digitization',
  // step 5.
  'probate', 'rate', 'cease', 'controll', 'roll', 'able', 'ate',
  // Words with digits (tokenizer domain includes digits).
  '12', '123', 'abc123', '007',
  // Misc real words.
  'organization', 'organizational', 'nationalization', 'international',
  'generalization', 'systematically', 'characterization', 'reproducibility',
];
for (const w of stemWords) {
  rows.push({ kind: 'stem', word: w, stemmed: stem(w) });
}

// ---------------------------------------------------------------------------
// 2. Tokenizer — stop-word removal on/off, punctuation, digits, non-ASCII,
//    whitespace variety, casing.
// ---------------------------------------------------------------------------
const tokenizeCases: Array<{ id: string; text: string; removeStopWords?: boolean }> = [
  { id: 'plain', text: 'The quick brown foxes jumped over the lazy dogs' },
  { id: 'no-stop-removal', text: 'The quick brown foxes', removeStopWords: false },
  { id: 'punctuation', text: 'Hello, world! This is a test... (of tokenizing).' },
  { id: 'digits', text: 'room 101 has 42 chairs and 3 tables' },
  { id: 'mixed-case', text: 'RUNNING Runners RAN quickly' },
  { id: 'non-ascii', text: 'café naïve résumé Zoë' },
  { id: 'non-ascii-mixed', text: 'José rolls 2d6 and éats' },
  { id: 'whitespace', text: 'tabs\tand\nnewlines\r\nand   spaces' },
  { id: 'single-chars', text: 'a b c d e f g' },
  { id: 'empty', text: '' },
  { id: 'only-stop', text: 'the and or but if' },
  { id: 'apostrophes', text: "don't can't won't it's" },
  { id: 'nbsp', text: 'foo bar baz' },
];
for (const c of tokenizeCases) {
  const removeStopWords = c.removeStopWords ?? true;
  rows.push({
    kind: 'tokenize',
    id: c.id,
    text: c.text,
    removeStopWords,
    tokens: tokenize(c.text, removeStopWords),
  });
}

// ---------------------------------------------------------------------------
// 3. Bigrams.
// ---------------------------------------------------------------------------
const bigramCases: Array<{ id: string; tokens: string[] }> = [
  { id: 'empty', tokens: [] },
  { id: 'single', tokens: ['alpha'] },
  { id: 'pair', tokens: ['alpha', 'beta'] },
  { id: 'triple', tokens: ['run', 'fast', 'jump'] },
  { id: 'repeat', tokens: ['a', 'a', 'a'] },
];
for (const c of bigramCases) {
  rows.push({ kind: 'bigrams', id: c.id, tokens: c.tokens, bigrams: generateBigrams(c.tokens) });
}

// ---------------------------------------------------------------------------
// 4. Fit → getState + transform. Multiple corpora × includeBigrams; transform a
//    set of queries. Captures fittedAt for injection into the Rust fit.
// ---------------------------------------------------------------------------
const corpora: Array<{ id: string; includeBigrams: boolean; docs: string[]; queries: string[] }> = [
  {
    id: 'memories-bigrams',
    includeBigrams: true,
    docs: [
      'The dragon guards the ancient treasure in the mountain cave.',
      'A brave knight rides toward the mountain to slay the dragon.',
      'The village celebrates the harvest festival every autumn.',
      'Merchants trade spices and silk along the coastal roads.',
      'The wizard studies forbidden magic in his lonely tower.',
    ],
    queries: [
      'dragon in the mountain cave',
      'the knight slays the dragon',
      'a completely unrelated query about pancakes',
      '',
      'harvest festival autumn village',
    ],
  },
  {
    id: 'unigrams-only',
    includeBigrams: false,
    docs: [
      'running runners ran quickly through the forest',
      'the forest was dark and full of running water',
      'quick foxes jump over lazy sleeping dogs',
    ],
    queries: ['running through the forest', 'lazy dogs', 'running running running'],
  },
  {
    id: 'single-doc',
    includeBigrams: true,
    docs: ['organization organizational nationalize international generalization'],
    queries: ['organization', 'international generalization', 'nothing matches here'],
  },
  {
    id: 'repeated-terms',
    includeBigrams: true,
    docs: [
      'apple apple apple banana',
      'banana banana cherry',
      'cherry apple date',
    ],
    queries: ['apple banana cherry', 'date date date', 'apple apple apple apple'],
  },
];

for (const c of corpora) {
  const v = new TfIdfVectorizer(c.includeBigrams);
  v.fitCorpus(c.docs);
  const state = v.getState()!;
  const transforms = c.queries.map((q) => ({ query: q, vector: v.transform(q) }));
  rows.push({
    kind: 'fit',
    id: c.id,
    includeBigrams: c.includeBigrams,
    documents: c.docs,
    state,
    transforms,
  });

  // A loadState-from-persisted-JSON row: mirror the host read path
  // (JSON.parse(vocabulary), JSON.parse(idf)) then transform.
  const v2 = new TfIdfVectorizer(true);
  v2.loadState({
    vocabulary: JSON.parse(JSON.stringify(state.vocabulary)),
    idf: JSON.parse(JSON.stringify(state.idf)),
    avgDocLength: state.avgDocLength,
    vocabularySize: state.vocabularySize,
    includeBigrams: state.includeBigrams,
    fittedAt: state.fittedAt,
  });
  rows.push({
    kind: 'loadstate',
    id: c.id,
    vocabularyJson: JSON.stringify(state.vocabulary),
    idfJson: JSON.stringify(state.idf),
    avgDocLength: state.avgDocLength,
    vocabularySize: state.vocabularySize,
    includeBigrams: state.includeBigrams,
    fittedAt: state.fittedAt,
    transforms: c.queries.map((q) => ({ query: q, vector: v2.transform(q) })),
  });
}

// ---------------------------------------------------------------------------
// 5. Throw cases — the two exact messages.
// ---------------------------------------------------------------------------
function captureThrow(fn: () => void): string {
  try {
    fn();
    return '';
  } catch (e) {
    return e instanceof Error ? e.message : String(e);
  }
}
rows.push({
  kind: 'throw',
  id: 'empty-corpus',
  op: 'fit',
  message: captureThrow(() => new TfIdfVectorizer(true).fitCorpus([])),
});
rows.push({
  kind: 'throw',
  id: 'not-fitted',
  op: 'transform',
  message: captureThrow(() => new TfIdfVectorizer(true).transform('anything')),
});

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
