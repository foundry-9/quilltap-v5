/**
 * Tier-1 oracle case — the file text/MIME sniffing leaf (v4
 * `lib/files/text-detection.ts`).
 *
 * Drives v4's REAL `isTextContent` / `getMimeTypeFromExtension` /
 * `isTextMimeType` / `detectTextContent` / `getBestMimeType` over a fixed corpus
 * and emits one NDJSON row per case, each tagged with a `kind` discriminator the
 * Rust port (`quilltap_core::files::text_detection`) dispatches on. Every row is
 * a tier-1 exact-equality check.
 *
 * Buffers are carried as `bytes: number[]` and reconstructed via
 * `Buffer.from(bytes)`. The corpus covers: content sniffing (empty, plain ASCII,
 * 2 nulls ok, 3 nulls binary, exactly-10%-non-printable, high-byte UTF-8, a
 * >8192 buffer whose text head is sampled while its binary tail is ignored);
 * extension inference (compound `.d.ts`/`.min.js`/`.env.local`, uppercase `.PY`,
 * special files `Dockerfile`/`Makefile`/`README`, a path basename, a bare
 * `noext`, an unknown `.zzz`); MIME predicates; full detection (generic +
 * extension, non-generic, binary + generic, text + generic + no extension); and
 * best-MIME fallback (present / null / empty-string).
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/text-detection.ts \
 *     > /tmp/oracle-text-detection.ndjson
 */

import {
  detectTextContent,
  getBestMimeType,
  getMimeTypeFromExtension,
  isTextContent,
  isTextMimeType,
  type TextDetectionResult,
} from '@/lib/files/text-detection';

const emit = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

// --- isTextContent -----------------------------------------------------------
const asciiBytes = Array.from('hello world\n', (c) => c.charCodeAt(0));
// 2 nulls interleaved with text — the 3rd-null short-circuit is NOT hit; keep
// the buffer long enough that 2/len stays below 0.10 so it resolves as text.
const twoNulls = Array.from('a'.repeat(30), (c) => c.charCodeAt(0));
twoNulls[2] = 0;
twoNulls[17] = 0;
const threeNulls = [0, 0, 0, ...Array.from('a'.repeat(30), (c) => c.charCodeAt(0))];
// Exactly 10% non-printable: 1 control byte out of 10 → ratio 0.10, NOT < 0.10.
const tenPercent = [...Array.from('a'.repeat(9), (c) => c.charCodeAt(0)), 0x01];
const highBytes = [0xff, 0xfe, 0x80, 'a'.charCodeAt(0)];
// >8192: a text head (sampled) followed by a binary tail (ignored).
const bigTextHeadBinaryTail = [
  ...Array.from({ length: 8192 }, () => 'a'.charCodeAt(0)),
  0x00,
  0x00,
  0x00,
  0x00,
];

const contentCases: Array<{ id: string; bytes: number[] }> = [
  { id: 'empty', bytes: [] },
  { id: 'plain-ascii', bytes: asciiBytes },
  { id: 'two-nulls-ok', bytes: twoNulls },
  { id: 'three-nulls-binary', bytes: threeNulls },
  { id: 'exactly-ten-percent', bytes: tenPercent },
  { id: 'high-bytes-utf8', bytes: highBytes },
  { id: 'big-text-head-binary-tail', bytes: bigTextHeadBinaryTail },
];
for (const { id, bytes } of contentCases) {
  emit({ kind: 'isTextContent', id, bytes, out: isTextContent(Buffer.from(bytes)) });
}

// --- getMimeTypeFromExtension ------------------------------------------------
const mimeCases = [
  'foo.ts',
  'component.d.ts',
  'bundle.min.js',
  'App.PY',
  'Dockerfile',
  'Makefile',
  'README',
  'app.env.local',
  'noext',
  'weird.zzz',
  'dir/sub/file.py',
  'archive.tar.gz',
];
for (const filename of mimeCases) {
  emit({
    kind: 'getMime',
    id: `mime:${filename}`,
    filename,
    out: getMimeTypeFromExtension(filename),
  });
}

// --- isTextMimeType ----------------------------------------------------------
const mimeTypeCases = [
  'text/plain',
  'text/x-python',
  'application/json',
  'application/xml',
  'application/javascript',
  'application/typescript',
  'application/x-javascript',
  'application/x-typescript',
  'application/x-sh',
  'application/x-shellscript',
  'application/graphql',
  'application/octet-stream',
  'image/png',
];
for (const mime of mimeTypeCases) {
  emit({ kind: 'isTextMime', id: `textmime:${mime}`, mime, out: isTextMimeType(mime) });
}

// --- detectTextContent -------------------------------------------------------
const detectCases: Array<{
  id: string;
  bytes: number[];
  filename: string;
  provided: string;
}> = [
  // Generic octet-stream + .py extension → detect text/x-python.
  { id: 'generic-with-ext', bytes: asciiBytes, filename: 'script.py', provided: 'application/octet-stream' },
  // Non-generic provided type → detected stays null; still plain text.
  { id: 'nongeneric-provided', bytes: asciiBytes, filename: 'notes.txt', provided: 'text/markdown' },
  // Binary content + generic → detected null, not plain text.
  { id: 'binary-generic', bytes: threeNulls, filename: 'blob.bin', provided: 'application/octet-stream' },
  // Text content + generic + no known extension → text/plain.
  { id: 'text-generic-noext', bytes: asciiBytes, filename: 'mystery', provided: '' },
  // Text content + generic + unknown extension → text/plain (no extension mime).
  { id: 'text-generic-unknown-ext', bytes: asciiBytes, filename: 'data.zzz', provided: 'application/x-unknown' },
  // Non-generic image type over binary bytes → not plain text, detected null.
  { id: 'image-binary', bytes: threeNulls, filename: 'pic.png', provided: 'image/png' },
];
for (const { id, bytes, filename, provided } of detectCases) {
  const out = detectTextContent(Buffer.from(bytes), filename, provided);
  emit({ kind: 'detect', id, bytes, filename, provided, out });
}

// --- getBestMimeType ---------------------------------------------------------
const bestCases: Array<{ id: string; detectedMimeType: string | null; provided: string }> = [
  { id: 'detected-present', detectedMimeType: 'text/x-python', provided: 'application/octet-stream' },
  { id: 'detected-null', detectedMimeType: null, provided: 'application/octet-stream' },
  { id: 'detected-empty', detectedMimeType: '', provided: 'application/octet-stream' },
];
for (const { id, detectedMimeType, provided } of bestCases) {
  const result: TextDetectionResult = {
    isPlainText: false,
    detectedMimeType,
    encoding: 'utf-8',
  };
  emit({ kind: 'getBest', id, detectedMimeType, provided, out: getBestMimeType(result, provided) });
}
