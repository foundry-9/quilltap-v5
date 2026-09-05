/**
 * `help-doc-markdown.ts` — asserted against v4 `HelpTopicReader.tsx`'s
 * ReactMarkdown configuration, whose real output was RECORDED at the pin by
 * rendering v4's pipeline over a document carrying every link kind.
 *
 * The headline case is the "Open this page in Quilltap" callout at the top of
 * every help document. v4 has a `blockquote` override that would replace it
 * with a `qt-help-guide-nav-callout` — and that override is **dead code**:
 * ReactMarkdown renders the inner link through the `a` override first, so the
 * text the callout regex sees is `"\nOpen this page in Quilltap\n"` with no
 * `](url)` left in it. What v4 actually renders (measured) is a plain
 * `<blockquote>` around a `qt-help-guide-page-link` button. That is what this
 * pipeline produces, and this spec is what keeps a well-meaning "the callout
 * class is unused, let me wire it up" from inventing behaviour v4 lacks.
 */

import { describe, expect, it } from 'vitest';

import {
  processContent,
  removeLlmSections,
  renderHelpDocument,
  stripFrontmatter,
} from './help-doc-markdown';

const DOC = [
  '---',
  'title: Sample',
  'slug: sample',
  '---',
  '',
  '# Sample Topic',
  '',
  '> **[Open this page in Quilltap](/settings?tab=system&section=import-export)**',
  '',
  'Body with a [sibling doc](character-creation.md) and a [page link](/aurora)',
  'and an [external](https://example.com/x) and a [qtap link](qtap://chat/abc).',
  '',
  '## In-Chat Navigation',
  '',
  'Hidden from humans.',
  '',
  '## Visible Again',
  '',
  '```bash',
  'echo hi',
  '```',
  '',
  'Inline `code` here.',
  '',
].join('\n');

describe('help-doc-markdown — v4 processContent', () => {
  it('strips YAML frontmatter', () => {
    expect(stripFrontmatter('---\na: b\n---\nBody')).toBe('Body');
  });

  it('leaves content with no frontmatter alone', () => {
    expect(stripFrontmatter('# Title\n')).toBe('# Title\n');
  });

  it('removes an LLM-only section up to the next H2', () => {
    const out = removeLlmSections('## A\nkeep\n\n## In-Chat Navigation\ndrop\n\n## B\nkeep2\n');
    expect(out).not.toContain('drop');
    expect(out).toContain('keep');
    expect(out).toContain('keep2');
  });

  it('removes a trailing LLM-only section to the end, trimmed', () => {
    const out = removeLlmSections('## A\nkeep\n\n## In-Chat Settings Access\ndrop\n');
    expect(out).toBe('## A\nkeep');
  });

  it('removes only the FIRST occurrence of a header (v4 behaviour)', () => {
    // v4 does one `indexOf` per header, so a document repeating one keeps the
    // second. Pinned rather than repaired: it is v4's shape.
    const out = removeLlmSections(
      '## In-Chat Navigation\nfirst\n\n## B\nkeep\n\n## In-Chat Navigation\nsecond\n',
    );
    expect(out).not.toContain('first');
    expect(out).toContain('second');
  });

  it('processContent chains both and trims', () => {
    expect(processContent(DOC)).not.toContain('Hidden from humans');
    expect(processContent(DOC).startsWith('# Sample Topic')).toBe(true);
  });
});

describe('help-doc-markdown — the rendered document', () => {
  const html = renderHelpDocument(DOC);

  it('renders the callout as a blockquote around a PAGE-LINK button', () => {
    // What v4 actually produces (measured at the pin) — NOT the dead
    // `qt-help-guide-nav-callout` branch.
    expect(html).toContain('<blockquote>');
    expect(html).toContain('class="qt-help-guide-page-link"');
    expect(html).toContain('data-help-page="/settings?tab=system&#x26;section=import-export"');
    expect(html).toContain('Open this page in Quilltap');
  });

  it('never emits the dead nav-callout class', () => {
    expect(html).not.toContain('qt-help-guide-nav-callout');
  });

  it('turns a sibling .md link into a doc-link button carrying the slug', () => {
    expect(html).toContain('class="qt-help-guide-doc-link"');
    expect(html).toContain('data-help-doc="character-creation"');
  });

  it('turns an app route into a page-link button', () => {
    expect(html).toContain('data-help-page="/aurora"');
  });

  it('opens an external link in a new tab', () => {
    expect(html).toContain('href="https://example.com/x"');
    expect(html).toContain('target="_blank"');
    expect(html).toContain('rel="noopener noreferrer"');
  });

  it('leaves a qtap:// href intact', () => {
    // v5 has no `QtapLink` analog; leaving the href is the port's established
    // handling (`almanack/almanack-markdown.ts`).
    expect(html).toContain('href="qtap://chat/abc"');
  });

  it('wraps a fenced block in the code-block class, keeping language-*', () => {
    expect(html).toContain('<pre class="qt-help-guide-code-block">');
    expect(html).toContain('class="language-bash"');
  });

  it('styles only BARE inline code (v4 `className || inline`)', () => {
    expect(html).toContain('<code class="qt-help-guide-inline-code">code</code>');
    // The fenced block's inner <code> keeps its language class and does not
    // also get the inline one.
    expect(html).not.toContain('language-bash qt-help-guide-inline-code');
  });

  it('drops raw HTML, as ReactMarkdown does by default', () => {
    // This is the property the reader's `bypassSecurityTrustHtml` rests on.
    const out = renderHelpDocument('Hello <script>alert(1)</script> there');
    expect(out).not.toContain('<script>');
  });

  it('renders the LLM-only section away', () => {
    expect(html).not.toContain('Hidden from humans');
    expect(html).toContain('Visible Again');
  });
});
