/**
 * The Salon message renderer — a faithful TS port of v4's SERVER renderer
 * `lib/services/markdown-renderer.service.ts` (HEAD b8b12695), which produces the
 * `renderedHtml` v4 injects. v5 drops the server `renderedHtml` fast path and
 * renders every message client-side through this service instead, so it must
 * match v4 byte-for-byte (proven by `markdown-renderer.spec.ts` against captured
 * v4 output).
 *
 * Two deliberate divergences from v4:
 * - v4 uses `processor.process` (async); this uses `processor.processSync` (all
 *   plugins are synchronous, so the output is identical) so the rendering is a
 *   pure sync function a component can call.
 * - Since b8b12695 v4 keeps the HTML post-processing helpers
 *   (`applyRoleplayPatterns` & friends) in `lib/services/markdown-postprocess.ts`,
 *   split out so Jest can import them past the service's ESM-only unified deps.
 *   v5 keeps them inline in this file — its spec runner is vitest, which loads
 *   ESM fine, so the split buys nothing here.
 */

import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkGfm from 'remark-gfm';
import remarkBreaks from 'remark-breaks';
import remarkMath from 'remark-math';
import remarkRehype from 'remark-rehype';
import rehypeStringify from 'rehype-stringify';
import rehypeHighlight from 'rehype-highlight';
import rehypeKatex from 'rehype-katex';
import remarkSmartypants from 'remark-smartypants';

import { REMARK_MATH_OPTIONS, normalizeMathDelimiters } from './math';
import { SMARTYPANTS_OPTIONS, shouldCurlQuotes } from './typography';
import {
  type CompiledRule,
  type DialogueDetection,
  type RenderingPattern,
  DEFAULT_RENDERING_PATTERNS,
  DEFAULT_DIALOGUE_DETECTION,
  compileRenderingPatterns,
  tokenizeInline,
  lineMatchFor,
  wrapBlockMatchFor,
  isDialogueParagraph,
  segmentsToHtml,
  escapeMarkdownInBrackets,
} from './roleplay-rendering';
import { linkifyBareQtapUris } from './qtap-linkify';

export interface MarkdownRenderOptions {
  renderingPatterns?: RenderingPattern[];
  dialogueDetection?: DialogueDetection | null;
  /**
   * When set, relative markdown image refs (`![alt](images/x.webp)`) are
   * rewritten to the chat's blob mount-point API (v4 `MessageContent` img
   * override). Absolute URLs, protocol-relative/`data:` URIs, and `/`-rooted
   * paths pass through untouched.
   */
  blobMountPointId?: string | null;
  /**
   * `smartTypographySettings.displayQuotes` — curl straight quotes on the way to
   * the screen (Layer 1.6, Part A). Presentational only: `content` is never
   * modified, so the model, the embeddings and every export still see what the
   * writer typed. A template that would be broken by curling overrides this —
   * see {@link shouldCurlQuotes}.
   */
  displayQuotes?: boolean;
}

// ---------------------------------------------------------------------------
// HTML post-processing helpers (verbatim ports of the v4 server helpers)
// ---------------------------------------------------------------------------

export function applyRoleplayPatterns(html: string, compiledRules: CompiledRule[]): string {
  const tagRegex = /(<[^>]*>)/g;
  const parts = html.split(tagRegex);

  let inCodeBlock = 0;

  // Track nesting depth while inside a KaTeX-rendered subtree. Roleplay
  // patterns must never rewrite math markup: KaTeX output is dense with text
  // runs (HTML glyph spans, MathML, the raw-LaTeX <annotation>) that patterns
  // like *emphasis* or "quotes" could match and corrupt. KaTeX markup contains
  // no void or self-closing tags (verified against katex 0.18 output), so a
  // generic open/close counter is sound here.
  let katexDepth = 0;

  const processedParts = parts.map((part, index) => {
    if (index % 2 === 1) {
      const lowerPart = part.toLowerCase();
      if (katexDepth > 0) {
        if (lowerPart.startsWith('</')) {
          katexDepth--;
        } else if (!lowerPart.endsWith('/>')) {
          katexDepth++;
        }
      } else if (lowerPart.startsWith('<span class="katex')) {
        // Root of a KaTeX subtree: .katex, .katex-display, or .katex-error.
        katexDepth = 1;
      } else if (lowerPart.startsWith('<code') || lowerPart.startsWith('<pre')) {
        inCodeBlock++;
      } else if (lowerPart === '</code>' || lowerPart === '</pre>') {
        inCodeBlock = Math.max(0, inCodeBlock - 1);
      }
      return part;
    }

    if (inCodeBlock > 0 || katexDepth > 0) {
      return part;
    }

    return segmentsToHtml(tokenizeInline(part, compiledRules));
  });

  return processedParts.join('');
}

/**
 * Rewrite relative markdown image srcs to the chat's blob mount-point API — a
 * port of v4's client-side `MessageContent` img override (`MessageContent.tsx:
 * 468-480`). v4 operates on the react-markdown img node; v5 renders to an HTML
 * string, so this post-processes the emitted `<img>` tags. Only bare relative
 * refs are rewritten: absolute URLs (`^([a-z]+:)?//`), `data:` URIs, and
 * `/`-rooted paths pass through, exactly as v4's predicate.
 */
export function applyBlobImageRewrite(html: string, blobMountPointId: string): string {
  return html.replace(/<img\b[^>]*>/gi, (tag) =>
    tag.replace(/(\ssrc=")([^"]*)(")/i, (_m, pre, src, post) => `${pre}${rewriteBlobSrc(src, blobMountPointId)}${post}`),
  );
}

function rewriteBlobSrc(src: string, blobMountPointId: string): string {
  if (src && !/^([a-z]+:)?\/\//i.test(src) && !src.startsWith('data:') && !src.startsWith('/')) {
    const encoded = src.split('/').map(encodeURIComponent).join('/');
    return `/api/v1/mount-points/${blobMountPointId}/blobs/${encoded}`;
  }
  return src;
}

export function applyDialogueDetection(html: string, detection: DialogueDetection): string {
  return html.replace(/<p([^>]*)>([\s\S]*?)<\/p>/g, (match, attrs, content) => {
    const plainText = content.replace(/<[^>]+>/g, '');

    if (isDialogueParagraph(plainText, detection)) {
      if (attrs.includes('class="')) {
        const newAttrs = attrs.replace(/class="([^"]*)"/, `class="$1 ${detection.className}"`);
        return `<p${newAttrs}>${content}</p>`;
      } else {
        return `<p${attrs} class="${detection.className}">${content}</p>`;
      }
    }

    return match;
  });
}

export function applyLineScopedClasses(html: string, compiledRules: CompiledRule[]): string {
  const lineRules = compiledRules.filter((r) => r.scope === 'line');
  if (lineRules.length === 0) return html;

  return html.replace(
    /<(p|li|blockquote|h[1-6])([^>]*)>([\s\S]*?)<\/\1>/g,
    (match, tag, attrs, content) => {
      const plainText = content.replace(/<[^>]+>/g, '');
      const lm = lineMatchFor(plainText, lineRules);
      if (!lm) return match;

      let body = content as string;
      if (lm.hideDelimiters && lm.prefix && body.startsWith(lm.prefix)) {
        body = body.slice(lm.prefix.length);
      }

      if (attrs.includes('class="')) {
        const newAttrs = attrs.replace(/class="([^"]*)"/, `class="$1 ${lm.className}"`);
        return `<${tag}${newAttrs}>${body}</${tag}>`;
      }
      return `<${tag}${attrs} class="${lm.className}">${body}</${tag}>`;
    },
  );
}

export function applyWrapBlockClasses(html: string, compiledRules: CompiledRule[]): string {
  const wrapRules = compiledRules.filter((r) => r.scope === 'inline' && r.hideDelimiters);
  if (wrapRules.length === 0) return html;

  return html.replace(
    /<(p|li|blockquote|h[1-6])([^>]*)>([\s\S]*?)<\/\1>/g,
    (match, tag, attrs, content) => {
      const plainText = (content as string).replace(/<[^>]+>/g, '');
      const wrap = wrapBlockMatchFor(plainText, wrapRules);
      if (!wrap) return match;

      let inner = content as string;
      if (wrap.prefix && inner.startsWith(wrap.prefix)) {
        inner = inner.slice(wrap.prefix.length);
      }
      if (wrap.suffix && inner.endsWith(wrap.suffix)) {
        inner = inner.slice(0, inner.length - wrap.suffix.length);
      }

      return `<${tag}${attrs}><span class="${wrap.className}">${inner}</span></${tag}>`;
    },
  );
}

// ---------------------------------------------------------------------------
// The processor (built once, matching v4's cached processor)
// ---------------------------------------------------------------------------

function createMarkdownProcessor(curlQuotes: boolean) {
  const processor = unified()
    .use(remarkParse)
    .use(remarkGfm)
    // remark-math parses $$…$$ math (single-dollar math is off — see math.ts);
    // rehype-katex below renders it. Sits between gfm and breaks, matching v4.
    .use(remarkMath, REMARK_MATH_OPTIONS)
    .use(remarkBreaks);

  // Smart typography (Layer 1.6, Part A): quotes only, never dashes. Sits
  // between remarkBreaks and remarkRehype so it operates on mdast TEXT nodes —
  // which is what structurally excludes code, math and link targets, each of
  // which is a distinct node type or a property rather than text. v4's exact
  // insertion point; the shared options live in `typography.ts`.
  if (curlQuotes) {
    processor.use(remarkSmartypants, SMARTYPANTS_OPTIONS);
  }

  return (
    processor
      .use(remarkRehype)
      // Render math to KaTeX markup BEFORE highlight, so highlight never touches
      // the math subtrees (matches v4's plugin order).
      .use(rehypeKatex)
      .use(rehypeHighlight, { ignoreMissing: true, detect: false })
      .use(rehypeStringify)
  );
}

// Cached processor instances — one per typographer state, because the plugin
// list is fixed at construction time. Two booleans, two processors (v4's own
// `cachedProcessors` pair).
const cachedProcessors: {
  plain: ReturnType<typeof createMarkdownProcessor> | null;
  curled: ReturnType<typeof createMarkdownProcessor> | null;
} = { plain: null, curled: null };

function getProcessor(curlQuotes: boolean) {
  const slot = curlQuotes ? 'curled' : 'plain';
  if (!cachedProcessors[slot]) {
    cachedProcessors[slot] = createMarkdownProcessor(curlQuotes);
  }
  return cachedProcessors[slot];
}

// ---------------------------------------------------------------------------
// Main render function
// ---------------------------------------------------------------------------

/**
 * Render markdown content to HTML with roleplay pattern support. Byte-for-byte
 * port of v4 `renderMarkdownToHtml`, using `processSync` (see the module note).
 * The returned HTML has NO outer container — the caller adds the container class.
 */
export function renderMarkdownToHtml(content: string, options: MarkdownRenderOptions = {}): string {
  const {
    renderingPatterns = DEFAULT_RENDERING_PATTERNS,
    dialogueDetection = DEFAULT_DIALOGUE_DETECTION,
    blobMountPointId = null,
    displayQuotes = false,
  } = options;

  try {
    const patterns = renderingPatterns.length > 0 ? renderingPatterns : DEFAULT_RENDERING_PATTERNS;
    const compiledRules = compileRenderingPatterns(patterns);
    // `dialogueConfig` is resolved HERE rather than at the dialogue pass below,
    // because the typographer's suppression check needs the SAME config that
    // pass will eventually use — an explicit `null` means the fallback, not "no
    // dialogue detection" (v4's own note at this line).
    const dialogueConfig = dialogueDetection || DEFAULT_DIALOGUE_DETECTION;

    const trimmedContent = content.trim();

    // Step 2.5: Normalize `\(...\)`/`\[...\]` math delimiters to `$$` form.
    // Must run before bracket escaping (which would otherwise claim `\[...\]`
    // as a roleplay bracket span). v4 also logs a debug line here when the
    // normalized content contains `$$`; v5's renderer has no logger, and that
    // line is output-neutral, so it is omitted.
    const mathNormalizedContent = normalizeMathDelimiters(trimmedContent);

    const escapedContent = escapeMarkdownInBrackets(mathNormalizedContent, patterns);
    const linkifiedContent = linkifyBareQtapUris(escapedContent);

    // The typographer, when on, runs inside this step and therefore BEFORE the
    // roleplay layer below — which is why a template whose patterns match a
    // literal straight quote must suppress it.
    const curlQuotes = shouldCurlQuotes({
      displayQuotes,
      renderingPatterns: patterns,
      dialogueDetection: dialogueConfig,
    });
    const processor = getProcessor(curlQuotes);
    const file = processor.processSync(linkifiedContent);
    let html = String(file);

    html = applyWrapBlockClasses(html, compiledRules);
    html = applyRoleplayPatterns(html, compiledRules);
    html = applyLineScopedClasses(html, compiledRules);

    html = applyDialogueDetection(html, dialogueConfig);

    // The blob-image rewrite runs last, on the finished HTML — the roleplay
    // post-processors leave `<img>` tags (odd split-parts) untouched, so the src
    // is still the raw relative ref at this point (v4 applies it in the img node).
    if (blobMountPointId) {
      html = applyBlobImageRewrite(html, blobMountPointId);
    }

    return html;
  } catch {
    return `<p>${escapeHtml(content)}</p>`;
  }
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
