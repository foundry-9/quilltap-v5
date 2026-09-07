import { describe, expect, it } from 'vitest';

import { renderGenerationPreviewMarkdown } from './generation-preview-markdown';

/**
 * `renderGenerationPreviewMarkdown` against v4 `components/characters/ai-wizard/
 * steps/GenerationStep.tsx:127-140` at `2f4254b42`: a bare `<ReactMarkdown>`
 * with **no `remarkPlugins`**, i.e. CommonMark only.
 *
 * The discriminating cases are the GFM ones. v5 has two other chat-independent
 * renderers and both add plugins this one must not have, so a spec that only
 * checked "renders bold" would pass against either of them and prove nothing.
 */
describe('renderGenerationPreviewMarkdown (v4’s bare ReactMarkdown)', () => {
  it('renders CommonMark', () => {
    const html = renderGenerationPreviewMarkdown('## Appearance\n\nShe stands **taller**.');
    expect(html).toContain('<h2>Appearance</h2>');
    expect(html).toContain('<strong>taller</strong>');
  });

  it('does NOT render GFM tables — v4 passes no remarkGfm here', () => {
    const html = renderGenerationPreviewMarkdown('| a | b |\n| - | - |\n| 1 | 2 |\n');
    expect(html).not.toContain('<table');
    // CommonMark reads the whole block as one paragraph, pipes and all.
    expect(html).toContain('| a | b |');
  });

  it('does NOT render GFM strikethrough', () => {
    const html = renderGenerationPreviewMarkdown('a ~~struck~~ phrase');
    expect(html).not.toContain('<del>');
    expect(html).toContain('~~struck~~');
  });

  it('does NOT autolink a bare URL — GFM literal autolinks are off too', () => {
    const html = renderGenerationPreviewMarkdown('see https://example.com now');
    expect(html).not.toContain('<a href="https://example.com"');
  });

  it('drops raw HTML (remark-rehype without allowDangerousHtml, ReactMarkdown’s own default)', () => {
    const html = renderGenerationPreviewMarkdown('a raw <b>bold</b> tag');
    expect(html).not.toContain('<b>');
    expect(html).toContain('a raw ');
    expect(html).toContain('bold');
    expect(renderGenerationPreviewMarkdown('before <script>alert(1)</script> after')).not.toContain(
      '<script>',
    );
  });

  it('keeps a `qtap://` href intact on a plain anchor (v5’s established handling)', () => {
    // RECORDED (P4.84 Tier 3): v4 routes this href through its `QtapLink`
    // component; v5 has no analog on ANY surface — the Salon, Almanack and help
    // renderers all leave a qtap:// href on a plain anchor, and the link-opening
    // path is a standing server-side deferral. The preview matches them.
    const html = renderGenerationPreviewMarkdown('read [the ledger](qtap://project/ledger.md)');
    expect(html).toContain('href="qtap://project/ledger.md"');
    expect(html).toContain('<a href=');
    expect(html).not.toContain('qt-qtap-doc');
  });

  it('renders empty input as empty output', () => {
    expect(renderGenerationPreviewMarkdown('')).toBe('');
  });
});
