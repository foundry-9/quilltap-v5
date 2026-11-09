/**
 * Oracle case (P4.D380 item 2): v4 `4310d435f`'s resolved-file helpers —
 * `statResolvedFile` / `moveResolvedFile` / `deleteResolvedFile`
 * (`lib/doc-edit/path-resolver.ts:785-824` at `624d0d25c`), driven REAL over a
 * per-row temp tree (the `doc-edit-leaves.ts` shape).
 *
 * Each row declares its own tree (`setup`) and op, so the Rust side
 * (`crates/quilltap-harness/tests/doc_resolved_file_equivalence.rs`) rebuilds
 * the SAME tree and replays the SAME op. Emitted per row:
 *   { id, setup, op, result | errorCode, after, lines }
 * `result` is the stat as `{isFile, size, mtimeIsDate}` (the wall-clock mtime is
 * masked to its type), `null`, or `"ok"` for a move/delete; `errorCode` is the
 * rethrown error's Node `code`; `after` lists which of the row's probe paths
 * exist (lstat) once the op ran; `lines` are v4's logger lines (DEBUG included —
 * `LOG_LEVEL=debug` is set before the logger loads), captured by splicing the
 * singleton logger's transports.
 *
 * The EACCES rows chmod a directory to 000 — meaningful only for a NON-ROOT
 * user (root reads through mode bits); the case refuses to run as root.
 *
 * Run from the v4 pin worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   LOG_LEVEL=debug $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-resolved-file.ts \
 *     > /tmp/oracle-doc-resolved-file.ndjson
 */

import { mkdtempSync, mkdirSync, writeFileSync, symlinkSync, chmodSync, rmSync, lstatSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

type Step =
  | { file: string; content: string }
  | { dir: string }
  | { symlink: string; target: string }
  | { chmod: string; mode: number };

type Op =
  | { kind: 'stat'; path: string; mountType?: string; emptyAbsolute?: boolean }
  | { kind: 'move'; from: string; to: string }
  | { kind: 'delete'; path: string };

interface Row {
  id: string;
  setup: Step[];
  op: Op;
  probes: string[];
}

const ROWS: Row[] = [
  { id: 'stat-file', setup: [{ file: 'a.md', content: 'hello' }], op: { kind: 'stat', path: 'a.md' }, probes: [] },
  { id: 'stat-folder', setup: [{ dir: 'd' }, { file: 'd/x.md', content: '1' }], op: { kind: 'stat', path: 'd' }, probes: [] },
  { id: 'stat-missing', setup: [], op: { kind: 'stat', path: 'missing.md' }, probes: [] },
  { id: 'stat-enotdir', setup: [{ file: 'a.md', content: 'x' }], op: { kind: 'stat', path: 'a.md/child.md' }, probes: [] },
  { id: 'stat-dangling-link', setup: [{ symlink: 'l.md', target: 'nowhere.md' }], op: { kind: 'stat', path: 'l.md' }, probes: [] },
  {
    id: 'stat-link-to-file',
    setup: [{ file: 'a.md', content: 'abc' }, { symlink: 'l.md', target: 'a.md' }],
    op: { kind: 'stat', path: 'l.md' },
    probes: [],
  },
  {
    id: 'stat-link-to-dir',
    setup: [{ dir: 'd' }, { symlink: 'l', target: 'd' }],
    op: { kind: 'stat', path: 'l' },
    probes: [],
  },
  {
    id: 'stat-eacces',
    setup: [{ dir: 'locked' }, { file: 'locked/a.md', content: 'x' }, { chmod: 'locked', mode: 0 }],
    op: { kind: 'stat', path: 'locked/a.md' },
    probes: [],
  },
  {
    id: 'stat-eacces-missing-child',
    setup: [{ dir: 'locked' }, { chmod: 'locked', mode: 0 }],
    op: { kind: 'stat', path: 'locked/none.md' },
    probes: [],
  },
  { id: 'stat-database', setup: [{ file: 'a.md', content: 'x' }], op: { kind: 'stat', path: 'a.md', mountType: 'database' }, probes: [] },
  { id: 'stat-empty-absolute', setup: [{ file: 'a.md', content: 'x' }], op: { kind: 'stat', path: 'a.md', emptyAbsolute: true }, probes: [] },
  {
    id: 'move-into-new-folders',
    setup: [{ file: 'a.md', content: 'x' }],
    op: { kind: 'move', from: 'a.md', to: 'deep/er/b.md' },
    probes: ['a.md', 'deep/er/b.md'],
  },
  {
    id: 'move-over-existing-file',
    setup: [{ file: 'a.md', content: 'x' }, { file: 'b.md', content: 'y' }],
    op: { kind: 'move', from: 'a.md', to: 'b.md' },
    probes: ['a.md', 'b.md'],
  },
  { id: 'move-missing-source', setup: [], op: { kind: 'move', from: 'a.md', to: 'b.md' }, probes: ['b.md'] },
  {
    id: 'move-onto-nonempty-dir',
    setup: [{ file: 'a.md', content: 'x' }, { dir: 'd' }, { file: 'd/x.md', content: '1' }],
    op: { kind: 'move', from: 'a.md', to: 'd' },
    probes: ['a.md', 'd/x.md'],
  },
  {
    id: 'move-out-of-locked',
    setup: [{ dir: 'locked' }, { file: 'locked/a.md', content: 'x' }, { chmod: 'locked', mode: 0 }],
    op: { kind: 'move', from: 'locked/a.md', to: 'b.md' },
    probes: ['b.md'],
  },
  { id: 'delete-file', setup: [{ file: 'a.md', content: 'x' }], op: { kind: 'delete', path: 'a.md' }, probes: ['a.md'] },
  { id: 'delete-missing', setup: [], op: { kind: 'delete', path: 'a.md' }, probes: [] },
  { id: 'delete-a-folder', setup: [{ dir: 'd' }], op: { kind: 'delete', path: 'd' }, probes: ['d'] },
  {
    id: 'delete-in-locked',
    setup: [{ dir: 'locked' }, { file: 'locked/a.md', content: 'x' }, { chmod: 'locked', mode: 0 }],
    op: { kind: 'delete', path: 'locked/a.md' },
    probes: [],
  },
  {
    id: 'delete-a-link-leaves-its-target',
    setup: [{ file: 'a.md', content: 'x' }, { symlink: 'l.md', target: 'a.md' }],
    op: { kind: 'delete', path: 'l.md' },
    probes: ['a.md', 'l.md'],
  },
];

function exists(p: string): boolean {
  try {
    lstatSync(p);
    return true;
  } catch {
    return false;
  }
}

async function main(): Promise<void> {
  if (typeof process.getuid === 'function' && process.getuid() === 0) {
    throw new Error('doc-resolved-file: the EACCES rows are meaningless as root');
  }
  const { logger } = await import('@/lib/logger');
  const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      captured.push({ level: data.level, message: data.message, fields });
    },
  });
  const { statResolvedFile, moveResolvedFile, deleteResolvedFile } = await import('@/lib/doc-edit');

  for (const row of ROWS) {
    const root = mkdtempSync(join(tmpdir(), 'qt-resolved-file-'));
    const locked: string[] = [];
    try {
      for (const step of row.setup) {
        if ('file' in step) writeFileSync(join(root, step.file), step.content);
        else if ('dir' in step) mkdirSync(join(root, step.dir), { recursive: true });
        else if ('symlink' in step) symlinkSync(step.target, join(root, step.symlink));
        else {
          chmodSync(join(root, step.chmod), step.mode);
          locked.push(join(root, step.chmod));
        }
      }
      const resolvedAt = (rel: string, mountType?: string, emptyAbsolute?: boolean) => ({
        absolutePath: emptyAbsolute ? '' : join(root, rel),
        scope: 'general' as const,
        ...(mountType ? { mountType, mountPointId: 'mp-1' } : {}),
        basePath: root,
        relativePath: rel,
      });
      captured.length = 0;
      const out: Record<string, unknown> = { id: row.id, setup: row.setup, op: row.op };
      try {
        if (row.op.kind === 'stat') {
          const s = await statResolvedFile(resolvedAt(row.op.path, row.op.mountType, row.op.emptyAbsolute) as never);
          out.result = s === null ? null : { isFile: s.isFile, size: s.size, mtimeIsDate: s.mtime instanceof Date };
        } else if (row.op.kind === 'move') {
          await moveResolvedFile(resolvedAt(row.op.from) as never, resolvedAt(row.op.to) as never);
          out.result = 'ok';
        } else {
          await deleteResolvedFile(resolvedAt(row.op.path) as never);
          out.result = 'ok';
        }
      } catch (error) {
        out.errorCode = (error as NodeJS.ErrnoException).code ?? null;
      }
      for (const p of locked) chmodSync(p, 0o755);
      out.after = row.probes.map((p) => ({ path: p, exists: exists(join(root, p)) }));
      out.lines = captured.map((l) => ({ level: l.level, message: l.message, fields: l.fields }));
      process.stdout.write(JSON.stringify(out) + '\n');
    } finally {
      for (const p of locked) {
        try {
          chmodSync(p, 0o755);
        } catch {
          /* already restored */
        }
      }
      rmSync(root, { recursive: true, force: true });
    }
  }
}

main().catch((e) => {
  process.stderr.write(String(e?.stack ?? e) + '\n');
  process.exit(1);
});
