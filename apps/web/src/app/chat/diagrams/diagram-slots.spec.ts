// v5-authored: v4 draws from react-markdown's `pre` override and has no
// post-pass to pair (P4.D353 R-1). Every case runs over the renderer's REAL
// output.
import { afterEach, describe, expect, it, vi } from 'vitest';

import { renderMarkdownToHtml } from '../render/markdown-renderer';
import { listClosedDiagramFences } from './diagram-fences';
import { applyDiagramSlots } from './diagram-slots';

function pass(markdown: string) {
  const html = renderMarkdownToHtml(markdown);
  return applyDiagramSlots(html, listClosedDiagramFences(markdown));
}

function dom(html: string): HTMLElement {
  const div = document.createElement('div');
  div.innerHTML = html;
  return div;
}

describe('applyDiagramSlots', () => {
  afterEach(() => vi.restoreAllMocks());

  it('v5: the renderer emits a bare <pre><code class="hljs language-…"> for each diagram language (measured)', () => {
    expect(renderMarkdownToHtml('```mermaid\ngraph TD; A-->B\n```')).toBe(
      '<pre><code class="hljs language-mermaid">graph TD; A-->B\n</code></pre>',
    );
    expect(renderMarkdownToHtml('```dot\ndigraph { a -> b }\n```')).toContain(
      '<pre><code class="hljs language-dot">',
    );
    expect(renderMarkdownToHtml('```graphviz\ngraph { a -- b }\n```')).toContain(
      '<pre><code class="hljs language-graphviz">',
    );
    // highlight.js knows `svg` as an XML alias: the block is highlighted, its text intact.
    const svg = dom(renderMarkdownToHtml('```svg\n<svg viewBox="0 0 1 1"></svg>\n```'));
    expect(svg.querySelector('pre > code')?.className).toBe('hljs language-svg');
    expect(svg.querySelector('pre > code')?.textContent).toBe('<svg viewBox="0 0 1 1"></svg>\n');
  });

  it('v5: one mermaid fence becomes one slot, and fallbackHtml is the original block', () => {
    const md = 'Here:\n\n```mermaid\ngraph TD; A-->B\n```';
    const rendered = renderMarkdownToHtml(md);
    const { html, slots } = applyDiagramSlots(rendered, listClosedDiagramFences(md));
    expect(slots).toHaveLength(1);
    expect(slots[0]).toEqual({
      index: 0,
      fence: { engine: 'mermaid', language: 'mermaid', source: 'graph TD; A-->B' },
      // The same element, re-serialized (`>` in text comes back as `&gt;`).
      fallbackHtml: dom(rendered).querySelector('pre')!.outerHTML,
    });
    expect(dom(slots[0].fallbackHtml).querySelector('pre > code')?.textContent).toBe('graph TD; A-->B\n');
    const out = dom(html);
    expect(out.querySelector('pre')).toBeNull();
    expect(out.querySelector('.qt-diagram-slot')?.getAttribute('data-slot-index')).toBe('0');
    expect(out.querySelector('p')?.textContent).toBe('Here:');
  });

  it('v5: mermaid + ordinary ts + dot → two slots at 0 and 1, the ts block untouched', () => {
    const { html, slots } = pass('```mermaid\ngraph TD; A-->B\n```\n\n```ts\nconst x = 1\n```\n\n```dot\ndigraph { a -> b }\n```');
    expect(slots.map((s) => [s.index, s.fence.engine, s.fence.language])).toEqual([
      [0, 'mermaid', 'mermaid'],
      [1, 'graphviz', 'dot'],
    ]);
    const out = dom(html);
    expect(out.querySelectorAll('.qt-diagram-slot')).toHaveLength(2);
    expect(out.querySelectorAll('pre')).toHaveLength(1);
    expect(out.querySelector('pre > code')?.className).toBe('hljs language-ts');
  });

  it('v5: a capitalised Mermaid fence is paired, its language kept as written', () => {
    const { slots } = pass('```Mermaid\nflowchart TD\n```');
    expect(slots[0].fence).toEqual({ engine: 'mermaid', language: 'Mermaid', source: 'flowchart TD' });
  });

  it('v5: a diagram fence inside a blockquote is paired, its source as the walker reads it', () => {
    const md = '> ```mermaid\n> flowchart TD\n>   A --> B\n> ```';
    const { slots } = pass(md);
    expect(slots).toHaveLength(1);
    expect(slots[0].fence.source).toBe('flowchart TD\n  A --> B');
    expect(slots[0].fence.source).toBe(listClosedDiagramFences(md)[0].source);
  });

  it('v5: a diagram fence nested inside another fence is text, not a slot', () => {
    const { slots, html } = pass('````markdown\n```mermaid\nflowchart TD\n```\n````');
    expect(slots).toEqual([]);
    expect(dom(html).querySelector('pre > code')?.className).toBe('hljs language-markdown');
  });

  it('v5: the svg source is the block text, not its highlighting markup', () => {
    const { slots } = pass('```svg\n<svg viewBox="0 0 1 1"><rect/></svg>\n```');
    expect(slots[0].fence.source).toBe('<svg viewBox="0 0 1 1"><rect/></svg>');
  });

  it('v5: on a count mismatch the surplus block stays code, with one debug line per content', () => {
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => undefined);
    // An unterminated fence at the end: the renderer still emits a code block for it.
    const md = '```mermaid\nflowchart TD\n```\n\n```mermaid\nflowchart LR';
    const html = renderMarkdownToHtml(md);
    const fences = listClosedDiagramFences(md);
    expect(fences).toHaveLength(1);
    const first = applyDiagramSlots(html, fences);
    expect(first.slots).toHaveLength(1);
    const out = dom(first.html);
    expect(out.querySelectorAll('.qt-diagram-slot')).toHaveLength(1);
    expect(out.querySelector('pre > code')?.textContent).toBe('flowchart LR\n');
    expect(debug).toHaveBeenCalledWith(
      '[diagrams] Fence and code-block counts differ; drawing the first 1',
      { fences: 1, blocks: 2 },
    );
    applyDiagramSlots(html, fences);
    expect(debug).toHaveBeenCalledTimes(1);
  });

  it('v5: no fences → the HTML passes through byte for byte', () => {
    const html = renderMarkdownToHtml('```mermaid\nflowchart TD');
    expect(applyDiagramSlots(html, [])).toEqual({ html, slots: [] });
  });
});
