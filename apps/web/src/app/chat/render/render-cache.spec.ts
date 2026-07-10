import { describe, expect, it, beforeEach } from 'vitest';

import { renderMarkdownToHtml } from './markdown-renderer';
import { clearRenderCache, renderMarkdownCached } from './render-cache';

describe('renderMarkdownCached', () => {
  beforeEach(() => clearRenderCache());

  it('matches renderMarkdownToHtml byte-for-byte', () => {
    const content = 'Hello **world** — *she said*, then walked away.';
    expect(renderMarkdownCached(content)).toBe(renderMarkdownToHtml(content));
  });

  it('returns the identical string reference on a second call (memo hit)', () => {
    const content = 'A paragraph with `code` and a [link](qttap://x).';
    const first = renderMarkdownCached(content);
    const second = renderMarkdownCached(content);
    // Same reference proves the second call did not re-render.
    expect(second).toBe(first);
  });

  it('keys distinct content separately', () => {
    const a = renderMarkdownCached('First message.');
    const b = renderMarkdownCached('Second message.');
    expect(a).not.toBe(b);
    expect(a).toContain('First message.');
    expect(b).toContain('Second message.');
  });
});
