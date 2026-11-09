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
  isSafeRelativePath,
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
  | { kind: 'status'; id: string; source: string; code: string; out: number }
  // === P4.D377 ===
  | {
      kind: 'normaliseOpts';
      id: string;
      input: string;
      allowRoot: boolean;
      out: { ok: string } | { err: { name: string; code: string; message: string } };
    }
  | { kind: 'safe'; id: string; rel: string; out: boolean };
  // === P4.D377 ===

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

// === P4.D377 ===
// `normaliseRelativePath(input, { allowRoot })` (v4 `600c2906f`): the options
// form the database store uses (`allowRoot: true` → the store root is `''`).
// Each input × allowRoot true / false, capturing the thrown FileOpError's
// name + code + message (the message carries the ORIGINAL input).
const p4d377NormCases: Array<[string, string]> = [
  ['empty', ''],
  ['dot', '.'],
  ['dot-a', './a'],
  ['double-slash-dot', 'a//b/./c'],
  ['a-up', 'a/..'],
  ['up-a', '../a'],
  ['backslash', 'a\\b'],
  ['slash-a-slash', '/a/'],
  ['a-dot-b', 'a/./b'],
  ['dotdot', '..'],
  ['a-up-up', 'a/../..'],
  ['dot-slash', './'],
  ['slashes', '///'],
  ['backslash-up', '..\\x'],
];
for (const [id, input] of p4d377NormCases) {
  for (const allowRoot of [true, false]) {
    let out: { ok: string } | { err: { name: string; code: string; message: string } };
    try {
      out = { ok: normaliseRelativePath(input, { allowRoot }) };
    } catch (e) {
      if (!(e instanceof FileOpError)) throw e;
      out = { err: { name: e.name, code: e.code, message: e.message } };
    }
    rows.push({ kind: 'normaliseOpts', id: `p4d377_${id}_${allowRoot ? 'root' : 'strict'}`, input, allowRoot, out });
  }
}

// `isSafeRelativePath` (moved out of the folders route by `600c2906f`). The
// 200 limit is JS `.length` — UTF-16 code units, so an astral char counts 2.
const astral = '\u{1F600}';
const p4d377SafeCases: Array<[string, string]> = [
  ['empty', ''],
  ['abs', '/abs'],
  ['plain', 'a/b'],
  ['empty-seg', 'a//b'],
  ['dot-seg', 'a/./b'],
  ['dotdot-seg', 'a/../b'],
  ['trailing-slash', 'a/'],
  ['ctrl-nul', 'a\u0000b'],
  ['ctrl-us', 'a/b\u001fc'],
  ['ctrl-tab', 'a\tb'],
  ['del-0x7f', 'a\u007fb'],
  ['lt', 'a<b'],
  ['gt', 'a>b'],
  ['colon', 'a:b'],
  ['dquote', 'a"b'],
  ['pipe', 'a|b'],
  ['qmark', 'a?b'],
  ['star', 'a*b'],
  ['backslash', 'a\\b'],
  ['seg-200', 'x'.repeat(200)],
  ['seg-201', 'x'.repeat(201)],
  ['nested-seg-201', 'ok/' + 'x'.repeat(201)],
  // 198 units + one astral (2 units) = 200 code units / 199 code points.
  ['astral-200', 'x'.repeat(198) + astral],
  // 199 units + one astral = 201 code units / 200 code points: refused by
  // `.length` though only 200 code points.
  ['astral-201', 'x'.repeat(199) + astral],
  ['unicode-ok', 'caf\u00e9/r\u00e9sum\u00e9'],
];
for (const [id, rel] of p4d377SafeCases) {
  rows.push({ kind: 'safe', id: `p4d377_${id}`, rel, out: isSafeRelativePath(rel) });
}
// === P4.D377 ===

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
