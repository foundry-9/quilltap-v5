/**
 * The Guide reader's markdown pipeline — v4 `HelpTopicReader.tsx`'s
 * `ReactMarkdown` configuration, rendered to HTML instead of to components.
 *
 * v4 renders help documents through `ReactMarkdown` with GFM + math + KaTeX and
 * four component overrides. v5 has no component-per-node renderer, so the
 * overrides become a rehype pass and the two click targets become `data-help-*`
 * attributes the reader delegates on. The plugin ORDER is v4's, and so is the
 * safety property that follows from it: `remark-rehype` drops raw HTML unless
 * `allowDangerousHtml` is set, exactly as ReactMarkdown does by default, so the
 * output is the tag set this pipeline emits and nothing else — which is why the
 * reader trusts it verbatim (v5's established handling, see
 * `almanack/almanack-markdown.ts`).
 *
 * Deliberately NOT `chat/render/markdown-renderer.ts`: that is the SALON
 * renderer and always applies the roleplay patterns, which would wrap every
 * `[bracketed]` and `"quoted"` run in a help document in narration spans. A
 * manual is not a performance.
 *
 * ## The dead nav-callout branch (a v4 finding, MEASURED)
 *
 * v4's `blockquote` override matches `Open this page in Quilltap](url)` against
 * the blockquote's extracted TEXT and, on a hit, replaces it with a
 * `qt-help-guide-nav-callout`. That branch is **unreachable**: by the time the
 * override runs, ReactMarkdown has already rendered the inner link through the
 * `a` override, so `extractTextContent` sees `"\nOpen this page in Quilltap\n"`
 * — the `](url)` never survives into the text and the regex never matches.
 * Measured by rendering v4's real pipeline over a real callout at the pin: every
 * `> **[Open this page in Quilltap](/x)**` renders as a plain `<blockquote>`
 * around a `qt-help-guide-page-link` button, produced by the `a` branch.
 *
 * So this pipeline implements the REACHABLE shape and the callout branch is a
 * recorded NO-PORT: porting an unreachable branch into a different rendering
 * architecture would invent behaviour v4 does not have. `qt-help-guide-nav-callout`
 * therefore goes unused here, exactly as it does in v4.
 *
 * @module help/help-doc-markdown
 */

import rehypeKatex from 'rehype-katex';
import rehypeStringify from 'rehype-stringify';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import remarkParse from 'remark-parse';
import remarkRehype from 'remark-rehype';
import { unified } from 'unified';

import { REMARK_MATH_OPTIONS, normalizeMathDelimiters } from '../chat/render/math';

/** Strip YAML frontmatter from markdown content (v4 `stripFrontmatter`). */
export function stripFrontmatter(content: string): string {
  const match = content.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/);
  return match ? content.slice(match[0].length) : content;
}

/**
 * Remove LLM-only sections that should not be shown to human readers:
 * "## In-Chat Navigation" and "## In-Chat Settings Access".
 *
 * v4 `removeLlmSections`, verbatim: find the header, cut to the next H2 (or to
 * the end, `trimEnd`ed). Note it removes only the FIRST occurrence of each
 * header — a document repeating one keeps the second. That is v4's behaviour.
 */
export function removeLlmSections(content: string): string {
  const sectionsToRemove = ['## In-Chat Navigation', '## In-Chat Settings Access'];
  let result = content;
  for (const header of sectionsToRemove) {
    const headerIndex = result.indexOf(header);
    if (headerIndex === -1) continue;
    // Find the next H2 after this section, or end of content
    const afterHeader = result.indexOf('\n## ', headerIndex + header.length);
    if (afterHeader === -1) {
      // Remove from header to end
      result = result.slice(0, headerIndex).trimEnd();
    } else {
      // Remove from header to next H2
      result = result.slice(0, headerIndex) + result.slice(afterHeader);
    }
  }
  return result;
}

/** v4 `processContent` — frontmatter out, LLM sections out, trimmed. */
export function processContent(content: string): string {
  return removeLlmSections(stripFrontmatter(content)).trim();
}

// --- the hast walk (v4's component overrides) --------------------------------

interface HastNode {
  type: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: HastNode[];
}

function isQtapHref(href: string): boolean {
  return href.toLowerCase().startsWith('qtap://');
}

/**
 * v4's `a`, `pre` and `code` overrides as a hast transform.
 *
 * The two in-app link kinds become BUTTONS carrying their target in a data
 * attribute, because v5 renders to HTML rather than to components:
 * `data-help-doc` for a sibling `*.md` topic (which opens inside the Guide) and
 * `data-help-page` for an app route. `qtap://` hrefs are left untouched — v5 has
 * no `QtapLink` analog and leaving the href intact is the port's established
 * handling (`almanack-markdown.ts`).
 */
function applyHelpOverrides(): (tree: HastNode) => void {
  function addClass(node: HastNode, className: string): void {
    const existing = node.properties?.['className'];
    const classes = Array.isArray(existing) ? [...existing, className] : [className];
    node.properties = { ...node.properties, className: classes };
  }

  function rewriteAnchor(node: HastNode): void {
    const href = node.properties?.['href'];
    if (typeof href !== 'string') return;
    if (isQtapHref(href)) return;

    // Links to other help docs (e.g., "character-creation.md")
    if (href.endsWith('.md')) {
      node.tagName = 'button';
      node.properties = {
        type: 'button',
        className: ['qt-help-guide-doc-link'],
        'data-help-doc': href.replace(/\.md$/, ''),
      };
      return;
    }

    // Internal navigation links (start with /)
    if (href.startsWith('/')) {
      node.tagName = 'button';
      node.properties = {
        type: 'button',
        className: ['qt-help-guide-page-link'],
        'data-help-page': href,
      };
      return;
    }

    // External links open in new tab
    node.properties = { ...node.properties, target: '_blank', rel: ['noopener', 'noreferrer'] };
  }

  function walk(node: HastNode): void {
    if (node.tagName === 'a') {
      rewriteAnchor(node);
    } else if (node.tagName === 'pre') {
      addClass(node, 'qt-help-guide-code-block');
    } else if (node.tagName === 'code') {
      // v4: `className || 'qt-help-guide-inline-code'` — a fenced block's inner
      // <code> already carries `language-*`, so only bare inline code is styled.
      const existing = node.properties?.['className'];
      if (!existing || (Array.isArray(existing) && existing.length === 0)) {
        addClass(node, 'qt-help-guide-inline-code');
      }
    }
    for (const child of node.children ?? []) walk(child);
  }

  return (tree: HastNode) => walk(tree);
}

const processor = unified()
  .use(remarkParse)
  .use(remarkGfm)
  .use(remarkMath, REMARK_MATH_OPTIONS)
  .use(remarkRehype)
  .use(rehypeKatex)
  .use(applyHelpOverrides)
  .use(rehypeStringify);

/**
 * Render one help document's markdown to HTML, v4's processing included:
 * frontmatter and LLM sections stripped, then math delimiters normalised.
 */
export function renderHelpDocument(content: string): string {
  const processed = normalizeMathDelimiters(processContent(content));
  try {
    return String(processor.processSync(processed));
  } catch {
    return `<p>${escapeHtml(processed)}</p>`;
  }
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}
