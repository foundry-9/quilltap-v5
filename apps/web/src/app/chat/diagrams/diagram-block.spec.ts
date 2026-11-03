// Transcribed case for case from v4 `__tests__/unit/components/chat/diagrams/DiagramBlock.test.tsx`
// (236c04e35); v5-authored cases are prefixed `v5:`.
/**
 * DiagramBlock — routing to the right engine, the streaming hold, the
 * could-not-draw fallback, the toolbar, and quick-hide's indifference.
 *
 * Mermaid and Graphviz cannot lay out under jsdom (no getBBox, no WASM), so
 * the three renderers are mocked — through the `DIAGRAM_RENDERERS` token, as
 * v4 mocks the three modules; each real renderer is exercised in the browser
 * (`e2e/diagrams-flow.spec.ts`). v4's `CodeBlockWithCopy` stand-in is the
 * renderer's own `<pre><code>` here (`fallbackHtml`, R-9).
 */

import { signal } from '@angular/core';
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ThemeService } from '../../theme/theme.service';
import { IMAGES_HIDDEN } from '../hidden-image/images-hidden';
import { DiagramBlock, DIAGRAM_RENDERERS } from './diagram-block';
import type { DiagramEngine, DiagramStyle } from './diagram-fences';
import { DiagramLightboxService } from './diagram-lightbox.service';
import { cacheClear } from './diagram-render-cache';

const SVG = '<svg xmlns="http://www.w3.org/2000/svg" data-testid="drawn"><rect width="1" height="1"></rect></svg>';

const mermaidMock = vi.fn();
const graphvizMock = vi.fn();
const svgMock = vi.fn();

const themeId = signal<string | null>(null);
const colorMode = signal<'light' | 'dark'>('light');

function code(source: string, language: string): string {
  return `<pre data-testid="code-block"><code class="hljs language-${language}">${source}\n</code></pre>`;
}

interface Props {
  engine: DiagramEngine;
  language: string;
  source: string;
  style: DiagramStyle;
  streaming?: boolean;
}

function render(props: Props, imagesHidden = false): ComponentFixture<DiagramBlock> {
  TestBed.configureTestingModule({
    imports: [DiagramBlock],
    providers: [
      {
        provide: DIAGRAM_RENDERERS,
        useValue: {
          mermaid: (s: string, o: unknown) => mermaidMock(s, o),
          graphviz: (s: string, o: unknown) => graphvizMock(s, o),
          svg: (s: string) => svgMock(s),
        },
      },
      { provide: ThemeService, useValue: { activeThemeId: themeId, resolvedColorMode: colorMode } },
      { provide: IMAGES_HIDDEN, useValue: signal(imagesHidden).asReadonly() },
    ],
  });
  const fixture = TestBed.createComponent(DiagramBlock);
  setProps(fixture, props);
  return fixture;
}

function setProps(fixture: ComponentFixture<DiagramBlock>, props: Props): void {
  fixture.componentRef.setInput('engine', props.engine);
  fixture.componentRef.setInput('language', props.language);
  fixture.componentRef.setInput('source', props.source);
  fixture.componentRef.setInput('style', props.style);
  fixture.componentRef.setInput('streaming', props.streaming ?? false);
  fixture.componentRef.setInput('fallbackHtml', code(props.source, props.language));
  fixture.detectChanges();
}

/** Let the render frame fire and its promise land, then re-render. */
async function settle(fixture: ComponentFixture<DiagramBlock>): Promise<void> {
  for (let i = 0; i < 3; i++) {
    await new Promise((r) => setTimeout(r, 20));
    fixture.detectChanges();
  }
}

const el = (fixture: ComponentFixture<DiagramBlock>) => fixture.nativeElement as HTMLElement;
const drawn = (fixture: ComponentFixture<DiagramBlock>) => el(fixture).querySelector('[data-testid="drawn"]');
const codeBlock = (fixture: ComponentFixture<DiagramBlock>) =>
  el(fixture).querySelector('[data-testid="code-block"]');
function button(fixture: ComponentFixture<DiagramBlock>, name: RegExp): HTMLElement {
  const found = Array.from(el(fixture).querySelectorAll<HTMLElement>('button, [role="button"]')).find(
    (b) => name.test(b.getAttribute('aria-label') ?? b.textContent ?? ''),
  );
  if (!found) throw new Error(`no button ${name}`);
  return found;
}
const dialog = () => document.querySelector<HTMLElement>('[role="dialog"]');

describe('DiagramBlock', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    cacheClear();
    themeId.set(null);
    colorMode.set('light');
    mermaidMock.mockResolvedValue({ svg: SVG });
    graphvizMock.mockResolvedValue({ svg: SVG });
    svgMock.mockResolvedValue({ svg: SVG });
  });

  afterEach(() => {
    TestBed.inject(DiagramLightboxService).close();
    vi.restoreAllMocks();
  });

  it("routes mermaid to the Mermaid renderer with the author's style", async () => {
    const fixture = render({ engine: 'mermaid', language: 'mermaid', source: 'flowchart TD; A-->B', style: 'handDrawn' });
    await settle(fixture);
    expect(drawn(fixture)).not.toBeNull();
    expect(mermaidMock).toHaveBeenCalledWith('flowchart TD; A-->B', expect.objectContaining({ style: 'handDrawn' }));
    expect(graphvizMock).not.toHaveBeenCalled();
    expect(svgMock).not.toHaveBeenCalled();
  });

  it('routes graphviz to the Graphviz renderer', async () => {
    const fixture = render({ engine: 'graphviz', language: 'dot', source: 'digraph { a -> b }', style: 'clean' });
    await settle(fixture);
    expect(drawn(fixture)).not.toBeNull();
    expect(graphvizMock).toHaveBeenCalledWith('digraph { a -> b }', expect.objectContaining({ style: 'clean' }));
  });

  it('routes svg to the raw-SVG renderer and marks no style', async () => {
    const fixture = render({ engine: 'svg', language: 'svg', source: '<svg/>', style: 'handDrawn' });
    await settle(fixture);
    expect(drawn(fixture)).not.toBeNull();
    expect(svgMock).toHaveBeenCalledWith('<svg/>');
    expect(el(fixture).querySelector('.qt-diagram')?.hasAttribute('data-diagram-style')).toBe(false);
  });

  it('shows a fixed placeholder while the drawing is under way', async () => {
    mermaidMock.mockReturnValue(new Promise(() => {}));
    const fixture = render({ engine: 'mermaid', language: 'mermaid', source: 'flowchart TD', style: 'clean' });
    await settle(fixture);
    expect(el(fixture).querySelector('[role="status"]')?.textContent).toContain('Sketching the diagram');
  });

  it('shows the source as code while streaming, and never tries to draw', async () => {
    const fixture = render({
      engine: 'mermaid',
      language: 'mermaid',
      source: 'flowchart TD\n  A --',
      style: 'clean',
      streaming: true,
    });
    expect(codeBlock(fixture)?.textContent).toContain('flowchart TD');
    // Give any stray frame a chance to fire.
    await settle(fixture);
    expect(mermaidMock).not.toHaveBeenCalled();
    expect(el(fixture).textContent).not.toMatch(/pen has blotted/);

    setProps(fixture, { engine: 'mermaid', language: 'mermaid', source: 'flowchart TD\n  A --> B', style: 'clean' });
    await settle(fixture);
    expect(drawn(fixture)).not.toBeNull();
    expect(mermaidMock).toHaveBeenCalledTimes(1);
  });

  it("falls back to the code block, a note and the engine's message on failure", async () => {
    vi.spyOn(console, 'debug').mockImplementation(() => undefined);
    mermaidMock.mockRejectedValue(new Error('Parse error on line 2'));
    const fixture = render({ engine: 'mermaid', language: 'mermaid', source: 'flowchart TD\n  A[[[', style: 'clean' });
    await settle(fixture);
    expect(el(fixture).textContent).toMatch(/pen has blotted/);
    expect(codeBlock(fixture)?.textContent).toContain('A[[[');
    expect(el(fixture).querySelector('.qt-diagram-error-details pre')?.textContent).toBe('Parse error on line 2');
    expect(drawn(fixture)).toBeNull();
  });

  it('toggles the source under the drawing', async () => {
    const fixture = render({ engine: 'mermaid', language: 'mermaid', source: 'flowchart TD', style: 'clean' });
    await settle(fixture);
    expect(drawn(fixture)).not.toBeNull();
    expect(codeBlock(fixture)).toBeNull();
    button(fixture, /view source/i).click();
    fixture.detectChanges();
    expect(codeBlock(fixture)?.textContent).toContain('flowchart TD');
    button(fixture, /hide source/i).click();
    fixture.detectChanges();
    expect(codeBlock(fixture)).toBeNull();
  });

  it('copies the source and the SVG', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, writable: true, value: { writeText } });
    const fixture = render({ engine: 'mermaid', language: 'mermaid', source: 'flowchart TD', style: 'clean' });
    await settle(fixture);
    expect(drawn(fixture)).not.toBeNull();

    button(fixture, /copy source/i).click();
    await settle(fixture);
    expect(writeText).toHaveBeenLastCalledWith('flowchart TD');
    button(fixture, /copy svg/i).click();
    await settle(fixture);
    expect(writeText).toHaveBeenLastCalledWith(SVG);
  });

  it('downloads the SVG as <engine>-diagram.svg', async () => {
    const createObjectURL = vi.fn(() => 'blob:diagram');
    const revokeObjectURL = vi.fn();
    Object.assign(URL, { createObjectURL, revokeObjectURL });
    const clicked: string[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
      clicked.push(this.download);
    });
    const fixture = render({ engine: 'graphviz', language: 'dot', source: 'digraph {}', style: 'clean' });
    await settle(fixture);
    expect(drawn(fixture)).not.toBeNull();
    button(fixture, /download svg/i).click();
    expect(createObjectURL).toHaveBeenCalled();
    expect(clicked).toEqual(['graphviz-diagram.svg']);
  });

  it('re-uses a finished drawing when the same diagram mounts again', async () => {
    const first = render({ engine: 'mermaid', language: 'mermaid', source: 'flowchart LR', style: 'clean' });
    await settle(first);
    expect(drawn(first)).not.toBeNull();
    first.destroy();

    const second = TestBed.createComponent(DiagramBlock);
    setProps(second, { engine: 'mermaid', language: 'mermaid', source: 'flowchart LR', style: 'clean' });
    // Drawn on the first paint — no placeholder, no second render.
    expect(drawn(second)).not.toBeNull();
    await settle(second);
    expect(mermaidMock).toHaveBeenCalledTimes(1);
  });

  it('draws again when the style changes', async () => {
    const fixture = render({ engine: 'mermaid', language: 'mermaid', source: 'flowchart LR', style: 'clean' });
    await settle(fixture);
    expect(mermaidMock).toHaveBeenCalledTimes(1);
    setProps(fixture, { engine: 'mermaid', language: 'mermaid', source: 'flowchart LR', style: 'handDrawn' });
    await settle(fixture);
    expect(mermaidMock).toHaveBeenCalledTimes(2);
    expect(mermaidMock).toHaveBeenLastCalledWith('flowchart LR', expect.objectContaining({ style: 'handDrawn' }));
  });

  it('stays visible with Salon Images quick-hide on', async () => {
    const fixture = render({ engine: 'svg', language: 'svg', source: '<svg/>', style: 'clean' }, true);
    await settle(fixture);
    expect(drawn(fixture)).not.toBeNull();
    expect(el(fixture).querySelector('.qt-diagram img')).toBeNull();
  });

  it('opens the drawing in the lightbox on click and returns focus to it on Escape', async () => {
    const fixture = render({ engine: 'mermaid', language: 'mermaid', source: 'flowchart LR; A-->B', style: 'clean' });
    document.body.appendChild(el(fixture));
    await settle(fixture);
    expect(drawn(fixture)).not.toBeNull();
    const canvas = button(fixture, /Diagram — open to zoom and copy/);

    canvas.click();
    TestBed.tick(); // the lightbox binds its keys after its first render
    const box = dialog();
    expect(box?.getAttribute('aria-label')).toBe('Diagram, enlarged');
    // A copy of the drawing, in a root of its own outside the message.
    expect(box?.querySelector('[data-testid="drawn"]')).not.toBeNull();
    expect(canvas.contains(box)).toBe(false);

    box!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle(fixture);
    expect(dialog()).toBeNull();
    expect(document.activeElement).toBe(canvas);
  });

  it('opens the lightbox from the Enlarge button and from the keyboard', async () => {
    const fixture = render({ engine: 'svg', language: 'svg', source: '<svg/>', style: 'clean' });
    await settle(fixture);
    expect(drawn(fixture)).not.toBeNull();

    button(fixture, /Enlarge/).click();
    TestBed.tick();
    expect(dialog()?.getAttribute('aria-label')).toBe('Drawing, enlarged');
    dialog()!.querySelector<HTMLElement>('[aria-label="Close"]')!.click();
    await settle(fixture);
    expect(dialog()).toBeNull();

    button(fixture, /Drawing — open to zoom and copy/).dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
    );
    TestBed.tick();
    expect(dialog()).not.toBeNull();
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle(fixture);
    expect(dialog()).toBeNull();
  });

  it('v5: a click with a label selected does not open the lightbox', async () => {
    const fixture = render({ engine: 'svg', language: 'svg', source: '<svg/>', style: 'clean' });
    await settle(fixture);
    vi.spyOn(window, 'getSelection').mockReturnValue({
      isCollapsed: false,
      toString: () => 'a label',
    } as unknown as Selection);
    button(fixture, /Drawing — open to zoom and copy/).click();
    expect(dialog()).toBeNull();
  });

  it('v5: the render frame is cancelled on destroy (no render after destroy)', async () => {
    const fixture = render({ engine: 'mermaid', language: 'mermaid', source: 'flowchart TB', style: 'clean' });
    fixture.destroy();
    await new Promise((r) => setTimeout(r, 40));
    expect(mermaidMock).not.toHaveBeenCalled();
  });

  it('v5: a theme switch re-draws in the new palette (the theme is part of the cache key)', async () => {
    const fixture = render({ engine: 'mermaid', language: 'mermaid', source: 'flowchart RL', style: 'clean' });
    await settle(fixture);
    expect(mermaidMock).toHaveBeenCalledTimes(1);
    colorMode.set('dark');
    fixture.detectChanges();
    // The previous drawing stays up while the re-render is under way.
    expect(drawn(fixture)).not.toBeNull();
    await settle(fixture);
    expect(mermaidMock).toHaveBeenCalledTimes(2);
    themeId.set('art-deco');
    fixture.detectChanges();
    await settle(fixture);
    expect(mermaidMock).toHaveBeenCalledTimes(3);
  });

  it('v5: the error state names the engine and logs the debug line', async () => {
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => undefined);
    graphvizMock.mockRejectedValue(new Error('syntax error in line 1'));
    const fixture = render({ engine: 'graphviz', language: 'dot', source: 'digraph {', style: 'clean' });
    await settle(fixture);
    expect(el(fixture).querySelector('summary')?.textContent).toBe('What the Graphviz engine said');
    expect(el(fixture).querySelector('.qt-diagram-error-note')?.textContent).toBe(
      'The draughtsman’s pen has blotted — the diagram below could not be drawn.',
    );
    expect(el(fixture).querySelector('.qt-diagram-error-note')?.getAttribute('title')).toBe('syntax error in line 1');
    expect(debug).toHaveBeenCalledWith('[diagrams] Diagram could not be drawn', {
      engine: 'graphviz',
      error: 'syntax error in line 1',
    });
  });
});
