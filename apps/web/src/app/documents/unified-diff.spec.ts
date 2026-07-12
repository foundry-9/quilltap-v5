import { describe, expect, it } from 'vitest';

import { diffLines, formatAutosaveNotification, generateUnifiedDiff } from './unified-diff';

describe('diffLines (v4 Myers)', () => {
  it('all-equal → every op equal', () => {
    expect(diffLines(['a', 'b'], ['a', 'b'])).toEqual([
      { type: 'equal', line: 'a' },
      { type: 'equal', line: 'b' },
    ]);
  });

  it('empty sides', () => {
    expect(diffLines([], [])).toEqual([]);
    expect(diffLines([], ['x'])).toEqual([{ type: 'ins', line: 'x' }]);
    expect(diffLines(['x'], [])).toEqual([{ type: 'del', line: 'x' }]);
  });

  it('a single-line replacement is a del + ins around equal context', () => {
    expect(diffLines(['l1', 'l2', 'l3'], ['l1', 'CHANGED', 'l3'])).toEqual([
      { type: 'equal', line: 'l1' },
      { type: 'del', line: 'l2' },
      { type: 'ins', line: 'CHANGED' },
      { type: 'equal', line: 'l3' },
    ]);
  });
});

describe('generateUnifiedDiff (v4)', () => {
  it('identical → empty string', () => {
    expect(generateUnifiedDiff('a\nb', 'a\nb', 'f.md')).toBe('');
  });

  it('single-line change hunk', () => {
    expect(generateUnifiedDiff('l1\nl2\nl3', 'l1\nCHANGED\nl3', 'f.md')).toBe(
      '--- a/f.md\n+++ b/f.md\n@@ -1,3 +1,3 @@\n l1\n-l2\n+CHANGED\n l3',
    );
  });

  it('append to an empty file (git 0-byte semantics)', () => {
    expect(generateUnifiedDiff('', 'hello', 'f.md')).toBe(
      '--- a/f.md\n+++ b/f.md\n@@ -0,0 +1 @@\n+hello',
    );
  });
});

describe('formatAutosaveNotification (v4)', () => {
  it('wraps the diff in a fenced block', () => {
    expect(formatAutosaveNotification('a', 'b', 'Notes')).toBe(
      'I\'ve made changes to "Notes":\n\n```diff\n--- a/Notes\n+++ b/Notes\n@@ -1 +1 @@\n-a\n+b\n```',
    );
  });

  it('identical content → null (no announcement)', () => {
    expect(formatAutosaveNotification('same', 'same', 'Notes')).toBeNull();
  });
});
