import { describe, expect, it } from 'vitest';

import fixtures from './__fixtures__/markdown-fixtures.json';
import { applyBlobImageRewrite, renderMarkdownToHtml } from './markdown-renderer';
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

/**
 * The store-image rewrite (v4 `MessageContent` img override). Rendering with a
 * `blobMountPointId` turns relative image refs into mount-point blob URLs;
 * absolute/protocol-relative/`data:`/`/`-rooted srcs pass through — the byte-
 * visible contract the SPA relies on (asserted directly since v4 never populated
 * the field to capture a fixture).
 */
describe('renderMarkdownToHtml — blob image rewrite', () => {
  it('rewrites a relative store image to the mount-point blob route', () => {
    const html = renderMarkdownToHtml('![a](images/x.webp)', { blobMountPointId: 'mp-9' });
    expect(html).toContain('src="/api/v1/mount-points/mp-9/blobs/images/x.webp"');
  });

  it('encodeURIComponent-encodes each path segment (preserving slashes)', () => {
    expect(applyBlobImageRewrite('<img src="sub dir/my pic.webp" alt="a">', 'mp-9')).toBe(
      '<img src="/api/v1/mount-points/mp-9/blobs/sub%20dir/my%20pic.webp" alt="a">',
    );
  });

  it('leaves the src untouched when no blobMountPointId is set', () => {
    const html = renderMarkdownToHtml('![a](images/x.webp)');
    expect(html).toContain('src="images/x.webp"');
  });

  it.each([
    ['http://host/x.webp', 'http://host/x.webp'],
    ['https://host/x.webp', 'https://host/x.webp'],
    ['//host/x.webp', '//host/x.webp'],
    ['/api/v1/files/abc', '/api/v1/files/abc'],
    ['data:image/png;base64,AAAA', 'data:image/png;base64,AAAA'],
  ])('passes %s through untouched', (src, expected) => {
    expect(applyBlobImageRewrite(`<img src="${src}" alt="a">`, 'mp-9')).toBe(
      `<img src="${expected}" alt="a">`,
    );
  });
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
