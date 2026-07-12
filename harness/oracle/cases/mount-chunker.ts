/**
 * P4.6v tier-1 pure-leaf ORACLE: the mount-index `lib/mount-index/` pure
 * functions — the chunker (`chunkDocument` / `estimateTokens`), the path
 * utilities (`normaliseRelativePath` / `detectNativeText` / `mimeForExtension`),
 * and the HTTP status mapper (`fileOpStatus`). Drives v4's REAL code so the Rust
 * ports (`services/mount_index/{chunker,path_utils,file_op_status}.rs`) diff at
 * exact string / integer equality.
 *
 * Run from inside the server checkout (Node 24 or tsx):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/mount-chunker.ts \
 *     > /tmp/oracle-mount-chunker.ndjson
 */

import { chunkDocument, estimateTokens } from '@/lib/mount-index/chunker';
import {
  normaliseRelativePath,
  detectNativeText,
  mimeForExtension,
} from '@/lib/mount-index/path-utils';
import { fileOpStatus } from '@/lib/mount-index/file-op-status';
import { FileOpError } from '@/lib/mount-index/file-op-error';
import { DatabaseStoreError } from '@/lib/mount-index/database-store';

type ChunkOpts = {
  targetMinTokens?: number;
  targetMaxTokens?: number;
  overlapTokens?: number;
};

type Row =
  | { kind: 'chunk'; id: string; text: string; options?: ChunkOpts; out: unknown }
  | { kind: 'estimate'; id: string; text: string; out: number }
  | { kind: 'normalise'; id: string; input: string; out: { ok: string } | { err: string } }
  | { kind: 'nativeText'; id: string; path: string; out: string | null }
  | { kind: 'mime'; id: string; path: string; out: string }
  | { kind: 'status'; id: string; source: string; code: string; out: number };

const rows: Row[] = [];

// ── chunkDocument ──────────────────────────────────────────────────────────
// Use small token targets so the multi-chunk / overlap / oversized-block
// branches are exercised on compact inputs. maxChars = targetMaxTokens * 4.
const SMALL: ChunkOpts = { targetMinTokens: 3, targetMaxTokens: 6, overlapTokens: 2 };
const chunkCases: Array<[string, string, ChunkOpts | undefined]> = [
  ['empty', '', undefined],
  ['whitespace', '   \n\n  \t ', undefined],
  ['single-small', 'Hello world.', undefined],
  ['single-with-heading', '# Chapter One\n\nThe body of the chapter.', undefined],
  ['multi-heading-tail', '## First\n\nAlpha.\n\n## Second\n\nBeta.', undefined],
  // maxChars = 24 → forces greedy flush + overlap between paragraphs.
  ['greedy-overlap', 'alpha beta gamma\n\ndelta epsilon zeta\n\neta theta iota', SMALL],
  // One oversized paragraph split at sentences.
  ['oversized-sentences', 'One two three four. Five six seven eight. Nine ten.', SMALL],
  // Oversized single sentence (no sentence break) → word split.
  ['oversized-words', 'alpha beta gamma delta epsilon zeta eta theta', SMALL],
  // Oversized single word (no whitespace) → hard char split at maxChars=24.
  ['oversized-hardsplit', 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOP', SMALL],
  // Heading inside a multi-line paragraph.
  ['heading-inline', '# Title\nbody line one\nbody line two\n\nmore body here', SMALL],
  // Unicode (café + emoji) to pin UTF-16 length handling.
  ['unicode', 'café world 😀 résumé test\n\nsecond café paragraph 😀 here', SMALL],
];
for (const [id, text, options] of chunkCases) {
  rows.push({ kind: 'chunk', id, text, options, out: chunkDocument(text, options) });
}

// ── estimateTokens ─────────────────────────────────────────────────────────
const estCases: Array<[string, string]> = [
  ['empty', ''],
  ['abcd', 'abcd'],
  ['abcde', 'abcde'],
  ['emoji', '😀😀'],
  ['accented', 'café résumé'],
];
for (const [id, text] of estCases) {
  rows.push({ kind: 'estimate', id, text, out: estimateTokens(text) });
}

// ── normaliseRelativePath ──────────────────────────────────────────────────
const normCases: Array<[string, string]> = [
  ['plain', 'notes/file.md'],
  ['leading-slash', '/notes/file.md'],
  ['trailing-slash', 'notes/file.md/'],
  ['dot-segments', './notes/./file.md'],
  ['double-slash', 'notes//file.md'],
  ['resolved-dotdot', 'notes/sub/../file.md'],
  ['backslash', 'notes\\sub\\file.md'],
  ['empty', ''],
  ['just-dot', '.'],
  ['leading-dotdot', '../secret'],
  ['bare-dotdot', '..'],
  ['nested-escape', 'a/../../etc/passwd'],
];
for (const [id, input] of normCases) {
  let out: { ok: string } | { err: string };
  try {
    out = { ok: normaliseRelativePath(input) };
  } catch (e) {
    out = { err: e instanceof FileOpError ? e.code : String(e) };
  }
  rows.push({ kind: 'normalise', id, input, out });
}

// ── detectNativeText / mimeForExtension ────────────────────────────────────
const pathCases: string[] = [
  'a.md',
  'a.MARKDOWN',
  'a.txt',
  'a.json',
  'a.jsonl',
  'a.ndjson',
  'a.png',
  'a.webp',
  'a.pdf',
  'a.docx',
  'noext',
  '.md',
  'dir/b.MD',
  'archive.tar.gz',
];
for (const p of pathCases) {
  rows.push({ kind: 'nativeText', id: p, path: p, out: detectNativeText(p) });
  rows.push({ kind: 'mime', id: p, path: p, out: mimeForExtension(p) });
}

// ── fileOpStatus ───────────────────────────────────────────────────────────
const fileOpCodes = [
  'SOURCE_NOT_FOUND',
  'DEST_EXISTS',
  'MOUNT_NOT_FOUND',
  'INVALID_PATH',
  'UNSUPPORTED',
  'VERIFY_FAILED',
  'CONFLICT',
] as const;
for (const code of fileOpCodes) {
  rows.push({
    kind: 'status',
    id: `fileop-${code}`,
    source: 'FileOpError',
    code,
    out: fileOpStatus(new FileOpError('x', code)),
  });
}
const dbCodes = ['NOT_FOUND', 'UNSUPPORTED', 'CONFLICT', 'INVALID', 'NOT_EMPTY'] as const;
for (const code of dbCodes) {
  rows.push({
    kind: 'status',
    id: `dbstore-${code}`,
    source: 'DatabaseStoreError',
    code,
    out: fileOpStatus(new DatabaseStoreError('x', code)),
  });
}
// Unknown error → 500 default arm.
rows.push({
  kind: 'status',
  id: 'unknown',
  source: 'Error',
  code: 'NONE',
  out: fileOpStatus(new Error('boom')),
});

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
