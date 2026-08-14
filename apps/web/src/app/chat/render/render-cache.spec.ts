import { describe, expect, it, beforeEach } from 'vitest';

import { renderMarkdownToHtml } from './markdown-renderer';
import { clearRenderCache, renderMarkdownCached } from './render-cache';
import type { DialogueDetection, RenderingPattern } from './roleplay-rendering';

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

/**
 * P4.30 — template sensitivity. The chat's roleplay template arrives AFTER the
 * first paint: the conversation screen renders rows with the defaults, the
 * template GET resolves, and every already-rendered row must re-render with the
 * template's patterns. A cache keyed only on content would keep serving the
 * default-rendered HTML forever, which is exactly the failure this guards.
 *
 * v4 gets this for free — `LazyMessageContent` memoizes on the two option
 * objects BY REFERENCE (`LazyMessageContent.tsx:72-81`), so a newly-fetched
 * template (a new array) always busts it. The key here mirrors that: reference
 * ids, not deep equality.
 */
describe('renderMarkdownCached — the roleplay template busts the memo', () => {
  beforeEach(() => clearRenderCache());

  const CUSTOM: RenderingPattern[] = [{ pattern: '@@[^@]+@@', className: 'qt-chat-emote' }];
  const CUSTOM_DIALOGUE: DialogueDetection = {
    openingChars: ['«'],
    closingChars: ['»'],
    className: 'qt-chat-custom-dialogue',
  };
  const CONTENT = 'She paused. @@leans in@@ and whispered.';

  it('re-renders an already-cached row when a template arrives', () => {
    // First paint: no template yet — the defaults leave `@@…@@` alone.
    const beforeTemplate = renderMarkdownCached(CONTENT);
    expect(beforeTemplate).not.toContain('qt-chat-emote');

    // The template GET resolves; the same content re-renders through the memo.
    const afterTemplate = renderMarkdownCached(CONTENT, { renderingPatterns: CUSTOM });
    expect(afterTemplate).toContain('qt-chat-emote');
    expect(afterTemplate).toBe(renderMarkdownToHtml(CONTENT, { renderingPatterns: CUSTOM }));
  });

  it('re-renders again when the template is REMOVED (back to the defaults)', () => {
    renderMarkdownCached(CONTENT, { renderingPatterns: CUSTOM });
    const reset = renderMarkdownCached(CONTENT);
    expect(reset).not.toContain('qt-chat-emote');
    expect(reset).toBe(renderMarkdownToHtml(CONTENT));
  });

  it('keys two DIFFERENT template pattern sets apart', () => {
    const other: RenderingPattern[] = [{ pattern: '@@[^@]+@@', className: 'qt-chat-narration' }];
    const a = renderMarkdownCached(CONTENT, { renderingPatterns: CUSTOM });
    const b = renderMarkdownCached(CONTENT, { renderingPatterns: other });
    expect(a).toContain('qt-chat-emote');
    expect(b).toContain('qt-chat-narration');
  });

  it('keys the dialogue detection too', () => {
    const line = '«Bonjour, mon ami»';
    expect(renderMarkdownCached(line)).not.toContain('qt-chat-custom-dialogue');
    expect(renderMarkdownCached(line, { dialogueDetection: CUSTOM_DIALOGUE })).toContain(
      'qt-chat-custom-dialogue',
    );
  });

  it('still hits the memo when the SAME template reference comes back', () => {
    const first = renderMarkdownCached(CONTENT, { renderingPatterns: CUSTOM });
    const second = renderMarkdownCached(CONTENT, { renderingPatterns: CUSTOM });
    // Same reference proves the re-mounted row did not re-run the pipeline —
    // the point of the cache, kept intact by the template threading.
    expect(second).toBe(first);
  });
});

/**
 * P4.D74 — the typographer state belongs in the key. Flipping the Smart
 * Typography toggle must repaint every message already in the memo; a key
 * without `displayQuotes` would go on serving straight quotes until the content
 * itself changed, which is the same class of bug the template threading above
 * guards against. (v4 has the analogous problem one layer up — its persisted
 * messages carry server-rendered HTML — and solves it by invalidating the whole
 * chats query when the setting is saved.)
 */
describe('renderMarkdownCached — the displayQuotes key (P4.D74)', () => {
  beforeEach(() => clearRenderCache());

  const LINE = '"Hello there," she said warmly.';

  it('keys the two typographer states apart', () => {
    const plain = renderMarkdownCached(LINE);
    const curled = renderMarkdownCached(LINE, { displayQuotes: true });
    expect(plain).toContain('"Hello there,"');
    expect(curled).toContain('“Hello there,”');
  });

  it('renders the same bytes as the uncached call in both states', () => {
    expect(renderMarkdownCached(LINE, { displayQuotes: true })).toBe(
      renderMarkdownToHtml(LINE, { displayQuotes: true }),
    );
    expect(renderMarkdownCached(LINE, { displayQuotes: false })).toBe(
      renderMarkdownToHtml(LINE, { displayQuotes: false }),
    );
  });

  it('still hits the memo when the state has not moved', () => {
    const first = renderMarkdownCached(LINE, { displayQuotes: true });
    const second = renderMarkdownCached(LINE, { displayQuotes: true });
    expect(second).toBe(first);
  });

  it('treats absent and false as the same key (v4 default is false)', () => {
    const absent = renderMarkdownCached(LINE);
    const explicit = renderMarkdownCached(LINE, { displayQuotes: false });
    expect(explicit).toBe(absent);
  });
});
