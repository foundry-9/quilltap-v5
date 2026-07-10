/**
 * The Salon message renderer — a faithful TS port of v4's SERVER renderer
 * `lib/services/markdown-renderer.service.ts` (HEAD a7b1398d), which produces the
 * `renderedHtml` v4 injects. v5 drops the server `renderedHtml` fast path and
 * renders every message client-side through this service instead, so it must
 * match v4 byte-for-byte (proven by `markdown-renderer.spec.ts` against captured
 * v4 output).
 *
 * The one deliberate mechanical change: v4 uses `processor.process` (async); this
 * uses `processor.processSync` (all plugins are synchronous, so the output is
 * identical) so the rendering is a pure sync function a component can call.
 */

import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkGfm from 'remark-gfm';
import remarkBreaks from 'remark-breaks';
import remarkRehype from 'remark-rehype';
import rehypeStringify from 'rehype-stringify';
import rehypeHighlight from 'rehype-highlight';

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
}

// ---------------------------------------------------------------------------
// HTML post-processing helpers (verbatim ports of the v4 server helpers)
// ---------------------------------------------------------------------------

export function applyRoleplayPatterns(html: string, compiledRules: CompiledRule[]): string {
  const tagRegex = /(<[^>]*>)/g;
  const parts = html.split(tagRegex);

  let inCodeBlock = 0;

  const processedParts = parts.map((part, index) => {
    if (index % 2 === 1) {
      const lowerPart = part.toLowerCase();
      if (lowerPart.startsWith('<code') || lowerPart.startsWith('<pre')) {
        inCodeBlock++;
      } else if (lowerPart === '</code>' || lowerPart === '</pre>') {
        inCodeBlock = Math.max(0, inCodeBlock - 1);
      }
      return part;
    }

    if (inCodeBlock > 0) {
      return part;
    }

    return segmentsToHtml(tokenizeInline(part, compiledRules));
  });

  return processedParts.join('');
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

function createMarkdownProcessor() {
  return unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkBreaks)
    .use(remarkRehype)
    .use(rehypeHighlight, { ignoreMissing: true, detect: false })
    .use(rehypeStringify);
}

let cachedProcessor: ReturnType<typeof createMarkdownProcessor> | null = null;

function getProcessor() {
  if (!cachedProcessor) {
    cachedProcessor = createMarkdownProcessor();
  }
  return cachedProcessor;
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
  const { renderingPatterns = DEFAULT_RENDERING_PATTERNS, dialogueDetection = DEFAULT_DIALOGUE_DETECTION } =
    options;

  try {
    const patterns = renderingPatterns.length > 0 ? renderingPatterns : DEFAULT_RENDERING_PATTERNS;
    const compiledRules = compileRenderingPatterns(patterns);

    const trimmedContent = content.trim();
    const escapedContent = escapeMarkdownInBrackets(trimmedContent, patterns);
    const linkifiedContent = linkifyBareQtapUris(escapedContent);

    const processor = getProcessor();
    const file = processor.processSync(linkifiedContent);
    let html = String(file);

    html = applyWrapBlockClasses(html, compiledRules);
    html = applyRoleplayPatterns(html, compiledRules);
    html = applyLineScopedClasses(html, compiledRules);

    const dialogueConfig = dialogueDetection || DEFAULT_DIALOGUE_DETECTION;
    html = applyDialogueDetection(html, dialogueConfig);

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
