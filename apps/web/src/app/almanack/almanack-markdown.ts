import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkGfm from 'remark-gfm';
import remarkRehype from 'remark-rehype';
import rehypeStringify from 'rehype-stringify';

/**
 * The Almanack report renderer — PLAIN markdown + GFM, deliberately not the
 * Salon pipeline.
 *
 * v4's dialog renders the report with bare `ReactMarkdown remarkPlugins={[
 * remarkGfm]}`: no roleplay processing whatever. v5's one ported renderer
 * (`chat/render/markdown-renderer.ts`) is the SALON renderer and always applies
 * `DEFAULT_RENDERING_PATTERNS`, which would wrap every `[bracketed]`,
 * `{braced}` and `"quoted"` run in a system report in narration / dialogue
 * spans — a report is not a performance. So this is v4's four-plugin pipeline
 * transcribed rather than a reuse: parse → GFM (the report's tables live here)
 * → rehype → stringify, in that order.
 *
 * Two safety properties are inherited from that plugin choice and are why the
 * output is trusted verbatim by the dialog (as v4 trusts ReactMarkdown's):
 *
 *  - `remark-rehype` DROPS raw HTML unless `allowDangerousHtml` is set, exactly
 *    as ReactMarkdown does by default, so the report's own markdown can only
 *    produce the safe element set this pipeline knows how to emit;
 *  - nothing here rewrites hrefs, so `qtap://` links survive to the DOM. That is
 *    v5's established handling for them (see `chat/message-content.ts`) — v4
 *    routes them through a `QtapLink` component, which v5 has no analog of
 *    because v5 renders to HTML rather than to components, and because the
 *    qtap:// link-OPENING path is a standing server-side deferral.
 */
const processor = unified()
  .use(remarkParse)
  .use(remarkGfm)
  .use(remarkRehype)
  .use(rehypeStringify);

/** Render one report's markdown to HTML. Falls back to escaped text. */
export function renderAlmanackMarkdown(content: string): string {
  try {
    return String(processor.processSync(content));
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
