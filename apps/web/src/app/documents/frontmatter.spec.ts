import { describe, expect, it } from 'vitest';

import {
  countWords,
  extractFrontmatter,
  frontmatterRows,
  frontmatterValueToText,
  isMarkdownFile,
  toChipValues,
} from './frontmatter';

describe('isMarkdownFile (v4)', () => {
  it('recognizes .md / .markdown, case-insensitively', () => {
    expect(isMarkdownFile('a.md')).toBe(true);
    expect(isMarkdownFile('a.MARKDOWN')).toBe(true);
    expect(isMarkdownFile('notes/deep.Md')).toBe(true);
  });

  it('rejects everything else', () => {
    expect(isMarkdownFile('a.json')).toBe(false);
    expect(isMarkdownFile('a.yaml')).toBe(false);
    expect(isMarkdownFile('noext')).toBe(false);
    expect(isMarkdownFile('a.mdx')).toBe(false);
  });
});

describe('countWords (v4)', () => {
  it('counts whitespace-separated runs', () => {
    expect(countWords('one two three')).toBe(3);
    expect(countWords('  spaced   out \n words ')).toBe(3);
  });
  it('empty / whitespace-only → 0', () => {
    expect(countWords('')).toBe(0);
    expect(countWords('   \n\t ')).toBe(0);
  });
});

describe('extractFrontmatter (v4 loose-parser branch)', () => {
  it('splits leading --- frontmatter from the body', () => {
    const content = '---\ntitle: Notes\ntags: [a, b]\n---\n# Heading\nBody.';
    const fm = extractFrontmatter(content);
    expect(fm.rawBlock).toBe('---\ntitle: Notes\ntags: [a, b]\n---\n');
    expect(fm.body).toBe('# Heading\nBody.');
    expect(fm.data).toEqual({ title: 'Notes', tags: '[a, b]' });
  });

  it('no frontmatter → whole string is body, empty data', () => {
    const fm = extractFrontmatter('# Just a heading\nbody');
    expect(fm.rawBlock).toBeNull();
    expect(fm.body).toBe('# Just a heading\nbody');
    expect(fm.data).toEqual({});
  });

  it('a value-less key gets the em-dash placeholder', () => {
    const fm = extractFrontmatter('---\nempty:\n---\nbody');
    expect(fm.data).toEqual({ empty: '—' });
  });

  it('skips comment lines', () => {
    const fm = extractFrontmatter('---\n# a comment\nk: v\n---\nbody');
    expect(fm.data).toEqual({ k: 'v' });
  });
});

describe('toChipValues (v4)', () => {
  it('arrays become chip lists', () => {
    expect(toChipValues(['a', 'b', ''])).toEqual(['a', 'b']);
  });
  it('inline [a, b] strings become chip lists (quotes stripped)', () => {
    expect(toChipValues('["x", y]')).toEqual(['x', 'y']);
  });
  it('plain strings return null', () => {
    expect(toChipValues('hello')).toBeNull();
  });
});

describe('frontmatterValueToText + frontmatterRows', () => {
  it('coerces primitives and null', () => {
    expect(frontmatterValueToText(null)).toBe('—');
    expect(frontmatterValueToText(42)).toBe('42');
    expect(frontmatterValueToText(true)).toBe('true');
    expect(frontmatterValueToText('x')).toBe('x');
  });

  it('rows carry chips for array-like values and text otherwise', () => {
    const rows = frontmatterRows({ tags: '[a, b]', title: 'Notes' });
    expect(rows).toEqual([
      { key: 'tags', chips: ['a', 'b'], text: '[a, b]' },
      { key: 'title', chips: null, text: 'Notes' },
    ]);
  });
});
