import { describe, expect, it } from 'vitest';

import fixtures from './__fixtures__/markdown-fixtures.json';
import { renderMarkdownToHtml } from './markdown-renderer';
import { linkifyBareQtapUris } from './qtap-linkify';

/**
 * Byte-for-byte parity of the v5 markdown/roleplay/qtap pipeline against output
 * CAPTURED from v4's real `renderMarkdownToHtml` (see
 * `tooling/capture-markdown-fixtures.mts`). If v4's renderer changes, regenerate
 * the fixtures and this test enforces the port stays in lockstep.
 */
describe('renderMarkdownToHtml — v4 parity fixtures', () => {
  for (const fixture of fixtures) {
    it(`matches v4 for "${fixture.name}"`, () => {
      expect(renderMarkdownToHtml(fixture.input)).toBe(fixture.html);
    });
  }
});

describe('linkifyBareQtapUris', () => {
  it('rewrites a bare qtap uri to markdown-link form', () => {
    expect(linkifyBareQtapUris('go to qtap://a/b now')).toBe('go to [qtap://a/b](qtap://a/b) now');
  });

  it('leaves an inline-code qtap uri untouched', () => {
    expect(linkifyBareQtapUris('use `qtap://a/b`')).toBe('use `qtap://a/b`');
  });

  it('does not double-link a uri already in markdown link target', () => {
    expect(linkifyBareQtapUris('[x](qtap://a/b)')).toBe('[x](qtap://a/b)');
  });
});
