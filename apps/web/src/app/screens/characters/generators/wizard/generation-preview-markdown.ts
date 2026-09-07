import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkRehype from 'remark-rehype';
import rehypeStringify from 'rehype-stringify';

/**
 * The AI Wizard review pane's Markdown renderer — **bare CommonMark, no GFM**.
 *
 * v4's `GenerationStep.tsx:127-140` renders the generated physical
 * description's `fullDescription` through `<ReactMarkdown>` with **no
 * `remarkPlugins` at all**. That is a narrower pipeline than either of v5's two
 * existing chat-independent renderers, so neither can be reused:
 *
 *  - `almanack/almanack-markdown.ts` adds `remark-gfm` (v4's Almanack dialog
 *    passes `remarkPlugins={[remarkGfm]}`);
 *  - `help/help-doc-markdown.ts` adds GFM **and** math (plus the help overrides).
 *
 * A GFM-bearing pipeline would render a generated `| a | b |` line as a table
 * and `~~x~~` as a strikethrough where v4 shows the literal text, so this is
 * v4's three-plugin chain transcribed rather than a reuse: parse → rehype →
 * stringify. (This refutes the note `generation-step.ts` carried until now,
 * which said v5 "has no chat-independent Markdown renderer to reuse here" — it
 * has two, and the reason to add a third is the plugin list, not their absence.)
 *
 * Safety, inherited from that plugin choice and why the output is trusted
 * verbatim by the pane exactly as the other two are: `remark-rehype` DROPS raw
 * HTML unless `allowDangerousHtml` is set, which is also ReactMarkdown's own
 * default — so a generated `<b>` or `<script>` reaches the DOM as nothing at
 * all, on both sides.
 *
 * **`qtap://` hrefs (a recorded deferral, Tier 3 of P4.84).** v4 overrides the
 * `a` renderer to send `isQtapUri(href)` links through its `QtapLink`
 * component, which resolves the URI against the document store and renders
 * either an inert `span.qt-qtap-doc--inert` or an opening affordance. **v5 has
 * no `QtapLink` analog anywhere** — the Salon renderer, the Almanack renderer
 * and the help renderer all leave `qtap://` hrefs on a plain anchor, and the
 * qtap link-OPENING path is a standing server-side deferral (see
 * `almanack-markdown.ts`'s note and `chat/message-content.ts`). So this
 * pipeline leaves them alone too: the preview renders the SAME anchor shape
 * every other v5 surface renders, and inventing a `qt-qtap-doc` shape here —
 * which nothing else in v5 emits — would be a divergence from v5's own
 * established handling on top of the one from v4. Pinned by
 * `generation-preview-markdown.spec.ts`; when a `QtapLink` analog lands, this
 * module is one of its call sites.
 */
const processor = unified().use(remarkParse).use(remarkRehype).use(rehypeStringify);

/** Render one generated `fullDescription` to HTML. Falls back to escaped text. */
export function renderGenerationPreviewMarkdown(content: string): string {
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
