import { diagramEngineFor, type DiagramFence } from './diagram-fences';

/**
 * The Salon's diagram post-pass over the renderer's HTML (P4.D353 R-1).
 *
 * v4 draws a diagram from inside react-markdown's `pre` override
 * (`MessageContent.tsx:334-365`). v5's markdown pipeline returns a cached HTML
 * string that is bound with `[innerHTML]`, and the pipeline itself is not this
 * module's to change — so the diagram blocks are found AFTER the render: each
 * `<pre><code class="language-<lang>">` whose language is a diagram language
 * is swapped for an empty `<div class="qt-diagram-slot">` that
 * `message-content.ts` then mounts a {@link DiagramBlock} into.
 *
 * The fence walker ({@link listClosedDiagramFences}) decides how many to draw.
 * The renderer turns an UNCLOSED fence at the end of a message into a code
 * block too, but v4 never draws one; the walker's count excludes it, so the
 * Nth diagram block pairs with the Nth closed fence, and a surplus block (the
 * unclosed one) stays code. What is drawn — the engine, the language, the
 * source — is read from the block itself: its class is the fence's language
 * exactly as written and its text is exactly what remark handed v4's `pre`
 * override as `codeString` (blockquote markers and fence indentation already
 * stripped), less the one trailing newline v4 strips
 * (`codeString.replace(/\n$/, '')`).
 */

export interface DiagramSlot {
  index: number;
  fence: DiagramFence;
  /** The removed `<pre>` block, verbatim: the streaming / failure / source view. */
  fallbackHtml: string;
}

const LANGUAGE_CLASS = /^language-(.+)$/;

/** The diagram language a `<code>` element's class names, or null. */
function diagramLanguageOf(code: Element): string | null {
  for (const cls of Array.from(code.classList)) {
    const match = LANGUAGE_CLASS.exec(cls);
    if (match && diagramEngineFor(match[1]) !== null) return match[1];
  }
  return null;
}

const reportedMismatches = new Set<string>();

export function applyDiagramSlots(
  html: string,
  fences: readonly DiagramFence[],
): { html: string; slots: DiagramSlot[] } {
  const template = document.createElement('template');
  template.innerHTML = html;

  const blocks: { pre: Element; language: string; code: Element }[] = [];
  for (const code of Array.from(template.content.querySelectorAll('pre > code'))) {
    const language = diagramLanguageOf(code);
    const pre = code.parentElement;
    if (language !== null && pre) blocks.push({ pre, language, code });
  }

  if (blocks.length !== fences.length && !reportedMismatches.has(html)) {
    // Once per content string — bounded, so a long session cannot grow it.
    if (reportedMismatches.size >= 256) reportedMismatches.clear();
    reportedMismatches.add(html);
    // v5-authored: v4's `pre` override never pairs anything.
    console.debug(
      `[diagrams] Fence and code-block counts differ; drawing the first ${Math.min(blocks.length, fences.length)}`,
      { fences: fences.length, blocks: blocks.length },
    );
  }

  const slots: DiagramSlot[] = [];
  const count = Math.min(blocks.length, fences.length);
  for (let index = 0; index < count; index++) {
    const { pre, language, code } = blocks[index];
    const engine = diagramEngineFor(language);
    if (engine === null) continue;
    const fallbackHtml = pre.outerHTML;
    const slot = document.createElement('div');
    slot.className = 'qt-diagram-slot';
    slot.setAttribute('data-slot-index', String(index));
    pre.replaceWith(slot);
    slots.push({
      index,
      fence: { engine, language, source: (code.textContent ?? '').replace(/\n$/, '') },
      fallbackHtml,
    });
  }

  return { html: slots.length > 0 ? template.innerHTML : html, slots };
}
