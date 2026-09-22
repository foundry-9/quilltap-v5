/**
 * Oracle case: the compressed-text codec (P4.D203, tier 1).
 *
 * Drives the REAL functions from the v4 server's
 * `lib/database/text-compression.ts` — `textToBlob`, `blobToText` and
 * `isCompressedTextBlob` — over the COMMITTED corpus at
 * `harness/oracle/fixtures/text-compression.json`, and prints the results as
 * newline-delimited JSON on stdout. The Rust differential
 * (`text_compression_equivalence`) feeds the same corpus through
 * `quilltap_core::db::text_compression` and asserts byte-equal results.
 *
 * IMPORTANT — this imports the actual app code; it does not reimplement it.
 * Run it from inside the server checkout (or a pinned worktree) so `@/`
 * resolves:
 *
 *   cd /tmp/qt-v4-pin-p4d203-f45a517a9
 *   QT_ORACLE=~/source/quilltap-v5/harness/oracle \
 *     npx tsx "$QT_ORACLE/cases/text-compression.ts" \
 *       "$QT_ORACLE/fixtures/text-compression.json" \
 *       > /tmp/p4.d203/oracle-text-compression.ndjson
 *
 * Three row kinds, in this order:
 *
 *  - `encode`     — v4's `textToBlob` decision (`string` | `blob`), the stored
 *                   byte length, and the hex of the stored bytes. This is the
 *                   byte pin: v5 is a PEER WRITER of the same synced database,
 *                   so its compressed bytes must be v4's compressed bytes.
 *  - `crossDecode`— v4's REAL `blobToText` run over blobs **v5 produced**
 *                   (`v5Blobs` in the fixture, written by the Rust regenerator
 *                   before this case runs). This is the direction that matters
 *                   if byte parity should ever lapse: whatever v5 writes, v4
 *                   must be able to read.
 *  - `decode`     — v4's `blobToText` over every legacy/corrupt shape, plus
 *                   `isCompressedTextBlob`'s verdict on the same value.
 *
 * The corpus is fixed in a committed file (no randomness, no clock), so the
 * oracle is reproducible and the Rust side reads the identical bytes.
 */

import { readFileSync } from 'fs';

import {
  textToBlob,
  blobToText,
  isCompressedTextBlob,
} from '@/lib/database/text-compression';

type TextRow = { label: string; text: string };
type ShapeRow = {
  label: string;
  kind: 'null' | 'undefined' | 'string' | 'blob' | 'number';
  text?: string;
  hex?: string;
  value?: number;
};
type V5BlobRow = { label: string; hex: string };

const fixturePath = process.argv[2];
if (!fixturePath) {
  throw new Error('usage: text-compression.ts <fixtures/text-compression.json>');
}
const corpus = JSON.parse(readFileSync(fixturePath, 'utf-8')) as {
  texts: TextRow[];
  decodeShapes: ShapeRow[];
  v5Blobs: V5BlobRow[];
};

const emit = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

// --- encode: the byte pin -------------------------------------------------
for (const { label, text } of corpus.texts) {
  const stored = textToBlob(text);
  const isBuffer = Buffer.isBuffer(stored);
  emit({
    kind: 'encode',
    label,
    rawBytes: Buffer.byteLength(text, 'utf-8'),
    decision: isBuffer ? 'blob' : 'string',
    storedBytes: isBuffer ? stored.length : Buffer.byteLength(stored as string, 'utf-8'),
    // Hex either way, so the comparand is one shape: a `string` decision
    // stores the original UTF-8 bytes verbatim.
    storedHex: isBuffer
      ? stored.toString('hex')
      : Buffer.from(stored as string, 'utf-8').toString('hex'),
    // v4's own round-trip assertion, recorded rather than asserted here.
    roundTrip: blobToText(stored),
  });
}

// --- crossDecode: v4's real decoder over v5's bytes -----------------------
for (const { label, hex } of corpus.v5Blobs) {
  const buf = Buffer.from(hex, 'hex');
  emit({
    kind: 'crossDecode',
    label,
    isCompressed: isCompressedTextBlob(buf),
    text: blobToText(buf),
  });
}

// --- decode: every legacy and corrupt shape -------------------------------
for (const shape of corpus.decodeShapes) {
  let value: unknown;
  switch (shape.kind) {
    case 'null':
      value = null;
      break;
    case 'undefined':
      value = undefined;
      break;
    case 'string':
      value = shape.text;
      break;
    case 'blob':
      value = Buffer.from(shape.hex ?? '', 'hex');
      break;
    case 'number':
      value = shape.value;
      break;
  }
  emit({
    kind: 'decode',
    label: shape.label,
    isCompressed: isCompressedTextBlob(value),
    text: blobToText(value),
  });
}
