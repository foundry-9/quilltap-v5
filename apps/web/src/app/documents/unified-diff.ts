/**
 * Unified-diff helpers for the Document Pane autosave notification.
 *
 * A byte-for-byte port of v4 `lib/doc-edit/line-diff.ts` (`diffLines`, a Myers
 * O(ND) shortest-edit-script) + `lib/doc-edit/unified-diff.ts`
 * (`generateUnifiedDiff` / `formatAutosaveNotification`). The client generates
 * this diff so a save can hand the server the human-readable change summary in
 * the same round-trip — the server posts it as a single Librarian save
 * announcement (v4 `handleWriteDocument`: `librarianMessage` is posted only when
 * `diffContent` is present). The in-editor change gutter's `changedBlockIndices`
 * is a tier-3 deferral and is not ported.
 *
 * @module documents/unified-diff
 */

export type LineOp = { type: 'equal' | 'del' | 'ins'; line: string };

/** Myers O(ND) shortest-edit-script diff over two arrays of lines (v4 `diffLines`). */
export function diffLines(oldLines: string[], newLines: string[]): LineOp[] {
  const n = oldLines.length;
  const m = newLines.length;

  if (n === 0 && m === 0) return [];
  if (n === 0) return newLines.map((line) => ({ type: 'ins', line }));
  if (m === 0) return oldLines.map((line) => ({ type: 'del', line }));

  const max = n + m;
  const offset = max;
  const v = new Array<number>(2 * max + 1).fill(0);
  const trace: number[][] = [];

  let found = false;
  for (let d = 0; d <= max; d++) {
    trace.push(v.slice());
    for (let k = -d; k <= d; k += 2) {
      let x: number;
      if (k === -d || (k !== d && v[k - 1 + offset] < v[k + 1 + offset])) {
        x = v[k + 1 + offset];
      } else {
        x = v[k - 1 + offset] + 1;
      }
      let y = x - k;
      while (x < n && y < m && oldLines[x] === newLines[y]) {
        x++;
        y++;
      }
      v[k + offset] = x;
      if (x >= n && y >= m) {
        found = true;
        break;
      }
    }
    if (found) break;
  }

  const ops: LineOp[] = [];
  let x = n;
  let y = m;
  for (let d = trace.length - 1; d >= 0; d--) {
    const vPrev = trace[d];
    const k = x - y;
    let prevK: number;
    if (k === -d || (k !== d && vPrev[k - 1 + offset] < vPrev[k + 1 + offset])) {
      prevK = k + 1;
    } else {
      prevK = k - 1;
    }
    const prevX = vPrev[prevK + offset];
    const prevY = prevX - prevK;

    while (x > prevX && y > prevY) {
      ops.push({ type: 'equal', line: oldLines[x - 1] });
      x--;
      y--;
    }
    if (d > 0) {
      if (x === prevX) {
        ops.push({ type: 'ins', line: newLines[y - 1] });
      } else {
        ops.push({ type: 'del', line: oldLines[x - 1] });
      }
    }
    x = prevX;
    y = prevY;
  }

  ops.reverse();
  return ops;
}

/** Lines of unchanged context kept on each side of a change (git's default). */
const CONTEXT_LINES = 3;

/** Guard against pathological inputs (Myers memory is O(D·(N+M))). */
const MAX_DIFFABLE_LINES = 10000;

/** Format one side of a hunk header (`start,count`), matching git's conventions. */
function formatRange(start: number, count: number): string {
  if (count === 0) return `${start - 1},0`;
  if (count === 1) return `${start}`;
  return `${start},${count}`;
}

type EnrichedOp = LineOp & { oldNo: number; newNo: number };

/** Group an edit script into unified-diff hunks with context (v4 `buildHunks`). */
function buildHunks(ops: LineOp[]): string[] {
  if (!ops.some((op) => op.type !== 'equal')) {
    return [];
  }

  const enriched: EnrichedOp[] = [];
  let oldNo = 1;
  let newNo = 1;
  for (const op of ops) {
    enriched.push({ ...op, oldNo, newNo });
    if (op.type === 'equal') {
      oldNo++;
      newNo++;
    } else if (op.type === 'del') {
      oldNo++;
    } else {
      newNo++;
    }
  }

  const merged: Array<{ start: number; end: number }> = [];
  let i = 0;
  while (i < enriched.length) {
    if (enriched[i].type === 'equal') {
      i++;
      continue;
    }
    const changeStart = i;
    while (i < enriched.length && enriched[i].type !== 'equal') {
      i++;
    }
    const changeEnd = i - 1;
    const start = Math.max(0, changeStart - CONTEXT_LINES);
    const end = Math.min(enriched.length - 1, changeEnd + CONTEXT_LINES);

    const last = merged[merged.length - 1];
    if (last && start <= last.end + 1) {
      last.end = Math.max(last.end, end);
    } else {
      merged.push({ start, end });
    }
  }

  const hunks: string[] = [];
  for (const group of merged) {
    const slice = enriched.slice(group.start, group.end + 1);
    const oldStart = slice[0].oldNo;
    const newStart = slice[0].newNo;
    let oldCount = 0;
    let newCount = 0;
    const body: string[] = [];
    for (const op of slice) {
      if (op.type === 'equal') {
        oldCount++;
        newCount++;
        body.push(` ${op.line}`);
      } else if (op.type === 'del') {
        oldCount++;
        body.push(`-${op.line}`);
      } else {
        newCount++;
        body.push(`+${op.line}`);
      }
    }
    hunks.push(`@@ -${formatRange(oldStart, oldCount)} +${formatRange(newStart, newCount)} @@`);
    hunks.push(...body);
  }

  return hunks;
}

/** Coarse fallback for oversized inputs: replace the whole file in one hunk. */
function wholeFileHunk(oldLines: string[], newLines: string[]): string[] {
  if (oldLines.length === 0 && newLines.length === 0) return [];
  const hunks: string[] = [
    `@@ -${formatRange(1, oldLines.length)} +${formatRange(1, newLines.length)} @@`,
  ];
  for (const line of oldLines) hunks.push(`-${line}`);
  for (const line of newLines) hunks.push(`+${line}`);
  return hunks;
}

/** Generate a git-style unified diff between two strings (v4 `generateUnifiedDiff`). */
export function generateUnifiedDiff(oldText: string, newText: string, filename: string): string {
  if (oldText === newText) {
    return '';
  }

  const oldLines = oldText === '' ? [] : oldText.split('\n');
  const newLines = newText === '' ? [] : newText.split('\n');

  const hunks =
    oldLines.length + newLines.length > MAX_DIFFABLE_LINES
      ? wholeFileHunk(oldLines, newLines)
      : buildHunks(diffLines(oldLines, newLines));

  if (hunks.length === 0) {
    return '';
  }

  return `--- a/${filename}\n+++ b/${filename}\n${hunks.join('\n')}`;
}

/**
 * Build the autosave save-announcement body (v4 `formatAutosaveNotification`):
 * a fenced diff block, or null when the two inputs are identical.
 */
export function formatAutosaveNotification(
  oldText: string,
  newText: string,
  filename: string,
): string | null {
  const diff = generateUnifiedDiff(oldText, newText, filename);
  if (!diff) {
    return null;
  }
  return `I've made changes to "${filename}":\n\n\`\`\`diff\n${diff}\n\`\`\``;
}
