import { describe, expect, it } from 'vitest';

import rawFixtures from './__fixtures__/markdown-fixtures.json';
import {
  applyBlobImageRewrite,
  renderMarkdownToHtml,
  type MarkdownRenderOptions,
} from './markdown-renderer';
import { linkifyBareQtapUris } from './qtap-linkify';
import { DEFAULT_RENDERING_PATTERNS } from './roleplay-rendering';

interface RenderFixture {
  name: string;
  input: string;
  html: string;
  /** Absent = v4's own `options = {}` default (the "no template" reset arm). */
  options?: MarkdownRenderOptions;
}

const fixtures = rawFixtures as RenderFixture[];

function fixtureByName(name: string): RenderFixture {
  const found = fixtures.find((f) => f.name === name);
  if (!found) throw new Error(`missing fixture "${name}" — regenerate the corpus`);
  return found;
}

/**
 * Byte-for-byte parity of the v5 markdown/roleplay/qtap pipeline against output
 * CAPTURED from v4's real `renderMarkdownToHtml` (see
 * `tooling/capture-markdown-fixtures.mts`). If v4's renderer changes, regenerate
 * the fixtures and this test enforces the port stays in lockstep.
 *
 * The `template-*` vectors (P4.30) pin the chat's roleplay template reaching the
 * render: each custom set is captured from v4 alongside a `-reset` twin over the
 * same input with no options, so the pair proves the values were honoured rather
 * than merely tolerated.
 */
describe('renderMarkdownToHtml — v4 parity fixtures', () => {
  for (const fixture of fixtures) {
    it(`matches v4 for "${fixture.name}"`, () => {
      expect(renderMarkdownToHtml(fixture.input, fixture.options ?? {})).toBe(fixture.html);
    });
  }

  it('covers both template arms (the corpus is not silently truncated)', () => {
    const withOptions = fixtures.filter((f) => f.options !== undefined);
    expect(withOptions.length).toBeGreaterThanOrEqual(11);
    expect(withOptions.some((f) => (f.options?.renderingPatterns?.length ?? 0) > 0)).toBe(true);
    expect(withOptions.some((f) => f.options?.renderingPatterns?.length === 0)).toBe(true);
    expect(withOptions.some((f) => f.options?.dialogueDetection != null)).toBe(true);
    expect(withOptions.some((f) => f.options?.dialogueDetection === null)).toBe(true);
  });
});

/**
 * v4 `MessageContent.tsx:333-338` — the two fallback arms, ported case-for-case.
 * They are deliberately ASYMMETRIC: patterns fall back on an EMPTY ARRAY as well
 * as on absence, dialogue detection falls back only on a nullish value. Each arm
 * is asserted against a v4-captured vector, so "falls back" means "byte-identical
 * to what v4 emitted with no template", not merely "renders something".
 */
describe('renderMarkdownToHtml — the template fallback arms (v4 MessageContent:333-338)', () => {
  it('an EMPTY renderingPatterns array falls back to the defaults', () => {
    const empty = fixtureByName('template-empty-patterns-falls-back');
    const defaults = fixtureByName('bracket-narration');
    expect(empty.input).toBe(defaults.input);
    // v4 itself captured these as the same bytes; the port must agree.
    expect(empty.html).toBe(defaults.html);
    expect(renderMarkdownToHtml(empty.input, { renderingPatterns: [] })).toBe(defaults.html);
  });

  it('an ABSENT renderingPatterns value falls back to the defaults', () => {
    const defaults = fixtureByName('bracket-narration');
    expect(renderMarkdownToHtml(defaults.input, {})).toBe(defaults.html);
    expect(renderMarkdownToHtml(defaults.input, { renderingPatterns: undefined })).toBe(
      defaults.html,
    );
  });

  it('an explicitly passed default set renders as the defaults do', () => {
    const defaults = fixtureByName('bracket-narration');
    expect(
      renderMarkdownToHtml(defaults.input, { renderingPatterns: DEFAULT_RENDERING_PATTERNS }),
    ).toBe(defaults.html);
  });

  it('a NON-empty renderingPatterns array supersedes the defaults', () => {
    const custom = fixtureByName('template-custom-patterns-supersede-defaults');
    const html = renderMarkdownToHtml(custom.input, custom.options ?? {});
    expect(html).toBe(custom.html);
    // The proof the defaults are gone, not merely joined: `*looks around*` is a
    // bare <em> with no qt-chat-narration span.
    expect(html).toContain('<em>looks around</em>');
    expect(html).not.toContain('qt-chat-narration');
  });

  it('a NULL dialogueDetection falls back to the default detection', () => {
    const nulled = fixtureByName('template-null-dialogue-detection-falls-back');
    const defaults = fixtureByName('dialogue-straight');
    expect(nulled.input).toBe(defaults.input);
    expect(nulled.html).toBe(defaults.html);
    expect(renderMarkdownToHtml(nulled.input, { dialogueDetection: null })).toBe(defaults.html);
  });

  it('an ABSENT dialogueDetection falls back to the default detection', () => {
    const defaults = fixtureByName('dialogue-straight');
    expect(renderMarkdownToHtml(defaults.input, { dialogueDetection: undefined })).toBe(
      defaults.html,
    );
  });

  it('a custom dialogueDetection supersedes the default detection', () => {
    const custom = fixtureByName('template-custom-dialogue-detection');
    const reset = fixtureByName('template-custom-dialogue-detection-reset');
    expect(custom.input).toBe(reset.input);
    expect(renderMarkdownToHtml(custom.input, custom.options ?? {})).toBe(custom.html);
    // The pair is the proof: with the template the paragraph wears the custom
    // class; without it, none at all.
    expect(custom.html).toContain('class="qt-chat-custom-dialogue"');
    expect(reset.html).not.toContain('qt-chat-custom-dialogue');
  });
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
