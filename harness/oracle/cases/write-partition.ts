/**
 * Oracle case #5: write-batch partitioning + the folder-conflict id remap.
 *
 * Drives the REAL pure functions from the v4 server's
 * lib/background-jobs/host/write-partition.ts — the parent-side classification,
 * partitioning, main-primary policy, the concurrent-folder-create id remap
 * (`rewriteFolderRefs`), and the unique-constraint error sniff. These are the
 * named architectural invariants in CLAUDE.md (per-database partitioned apply,
 * main-primary vs idempotent ordering, the folder-conflict id remap).
 *
 * The module is pure (no I/O — its only import is `type ChildWritePayload`,
 * erased at runtime), so it is a clean tier-1 target. `rewriteFolderRefs` and
 * `isUniqueConstraintError` take arbitrary JSON shapes, so (as with
 * recall-history) those rows carry BOTH input and expected output and the Rust
 * port is fed the same bytes.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/write-partition.ts \
 *     > /tmp/oracle-write-partition.ndjson
 */

import {
  classifyWriteTarget,
  partitionWrites,
  isMainPrimaryJobType,
  rewriteFolderRefs,
  isUniqueConstraintError,
} from '@/lib/background-jobs/host/write-partition';
import type { ChildWritePayload } from '@/lib/background-jobs/ipc-types';

type Write = { method: string; args: unknown[] };

type Row =
  | { kind: 'classify'; id: string; method: string; out: string }
  | { kind: 'partition'; id: string; writes: Write[]; out: { main: Write[]; mountIndex: Write[]; llmLogs: Write[] } }
  | { kind: 'mainPrimary'; id: string; jobType: string | null; out: boolean }
  | { kind: 'rewrite'; id: string; write: Write; remap: Record<string, string>; out: Write }
  | { kind: 'uniqueErr'; id: string; err: unknown; out: boolean };

const rows: Row[] = [];

// ---------------------------------------------------------------------------
// classifyWriteTarget(method) — repoKey routing + the __finalizeFile special.
// ---------------------------------------------------------------------------
const classifyCases: Array<[string, string]> = [
  ['finalize-file', '__finalizeFile'],
  ['chats-update', 'chats.update'],
  ['memories-create', 'memories.create'],
  ['mount-folder-create', 'docMountFolders.create'],
  ['mount-file-create', 'docMountFiles.create'],
  ['project-doc-link', 'projectDocMountLinks.create'],
  ['llm-logs-dotted', 'llmLogs.create'],
  ['llm-logs-bare', 'llmLogs'],
  ['mount-folder-bare', 'docMountFolders'],
  ['unknown-repo', 'someRepo.update'],
  ['no-dot', 'foo'],
];
for (const [id, method] of classifyCases) {
  rows.push({ kind: 'classify', id, method, out: classifyWriteTarget(method) });
}

// ---------------------------------------------------------------------------
// partitionWrites(writes) — split by DB, preserve per-partition order.
// ---------------------------------------------------------------------------
const w = (method: string, args: unknown[] = []): ChildWritePayload => ({ method, args });
const partitionCases: Array<[string, Write[]]> = [
  ['empty', []],
  [
    'mixed-ordered',
    [
      w('chats.update', [{ id: 'c1' }]),
      w('docMountFolders.create', [{ id: 'f1' }]),
      w('llmLogs.create', [{ id: 'l1' }]),
      w('memories.create', [{ id: 'm1' }]),
      w('docMountFiles.create', [{ id: 'fi1' }]),
      w('__finalizeFile', ['/tmp/x']),
    ],
  ],
];
for (const [id, writes] of partitionCases) {
  rows.push({ kind: 'partition', id, writes, out: partitionWrites(writes as ChildWritePayload[]) });
}

// ---------------------------------------------------------------------------
// isMainPrimaryJobType(jobType).
// ---------------------------------------------------------------------------
const mainPrimaryCases: Array<[string, string | null]> = [
  ['autonomous', 'AUTONOMOUS_ROOM_TURN'],
  ['chat-response', 'CHAT_RESPONSE'],
  ['empty', ''],
  ['null', null],
];
for (const [id, jobType] of mainPrimaryCases) {
  // `null` exercises the `jobType !== undefined` guard's sibling nullish case.
  rows.push({ kind: 'mainPrimary', id, jobType, out: isMainPrimaryJobType(jobType ?? undefined) });
}

// ---------------------------------------------------------------------------
// rewriteFolderRefs(write, remap) — the folder-conflict id remap.
// ---------------------------------------------------------------------------
const rewriteCases: Array<[string, Write, Record<string, string>]> = [
  ['empty-remap', { method: 'docMountFolders.create', args: [{ parentId: 'A' }] }, {}],
  ['no-data-object', { method: 'x', args: ['stringarg'] }, { A: 'B' }],
  ['data-array', { method: 'x', args: [['a']] }, { A: 'B' }],
  ['null-data', { method: 'x', args: [null] }, { A: 'B' }],
  ['empty-args', { method: 'x', args: [] }, { A: 'B' }],
  ['no-matching-field', { method: 'x', args: [{ name: 'foo' }] }, { A: 'B' }],
  ['field-not-in-remap', { method: 'x', args: [{ parentId: 'Z' }] }, { A: 'B' }],
  ['rewrite-parent', { method: 'docMountFolders.create', args: [{ parentId: 'A', name: 'f' }] }, { A: 'B' }],
  ['rewrite-folderid', { method: 'docMountFileLinks.create', args: [{ folderId: 'A' }] }, { A: 'B' }],
  ['rewrite-both', { method: 'x', args: [{ parentId: 'A', folderId: 'C' }] }, { A: 'B', C: 'D' }],
  ['non-string-field', { method: 'x', args: [{ parentId: 123 }] }, { '123': 'B' }],
  ['preserve-other-args', { method: 'x', args: [{ parentId: 'A' }, 'second', 42] }, { A: 'B' }],
];
for (const [id, write, remap] of rewriteCases) {
  const out = rewriteFolderRefs(write as ChildWritePayload, new Map(Object.entries(remap)));
  rows.push({ kind: 'rewrite', id, write, remap, out });
}

// ---------------------------------------------------------------------------
// isUniqueConstraintError(err) — code-prefix then message-regex sniff.
// ---------------------------------------------------------------------------
const uniqueErrCases: Array<[string, unknown]> = [
  ['null', null],
  ['number', 5],
  ['string', 'boom'],
  ['bool', true],
  ['empty-object', {}],
  ['array', []],
  ['code-unique', { code: 'SQLITE_CONSTRAINT_UNIQUE' }],
  ['code-constraint-prefix', { code: 'SQLITE_CONSTRAINT' }],
  ['code-busy', { code: 'SQLITE_BUSY' }],
  ['code-nonstring', { code: 123 }],
  ['message-unique', { message: 'UNIQUE constraint failed: doc_mount_folders.name' }],
  ['message-case-insensitive', { message: 'oops: unique CONSTRAINT Failed here' }],
  ['message-other', { message: 'some other error' }],
  ['code-busy-but-message-unique', { code: 'SQLITE_BUSY', message: 'UNIQUE constraint failed: x' }],
];
for (const [id, err] of uniqueErrCases) {
  rows.push({ kind: 'uniqueErr', id, err, out: isUniqueConstraintError(err) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
