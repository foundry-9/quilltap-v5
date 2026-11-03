/**
 * The About page's acknowledgments (v4 `app/about/AboutView.tsx:373-383` at
 * `236c04e35`) — pinned VERBATIM, label by label and body by body. v4
 * `a4945d8d9` (the 4.11.0 documentation pass) grew `Markdown & Documents` and
 * added `Diagrams` and `Terminals & Realtime`; v5's list is v4's verbatim (ruling
 * R-5), and this spec is the fence against the next silent vintage drift.
 */
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { AboutPage } from './about-page';

async function render(): Promise<ComponentFixture<AboutPage>> {
  TestBed.configureTestingModule({
    imports: [AboutPage],
    providers: [
      {
        provide: CoreClient,
        useValue: { fetchHealth: () => Promise.resolve({ kind: 'healthy', version: '0.0.1' }) },
      },
      provideRouter([]),
    ],
  });
  const fixture = TestBed.createComponent(AboutPage);
  fixture.detectChanges();
  await new Promise((r) => setTimeout(r, 0));
  fixture.detectChanges();
  return fixture;
}

/** Each acknowledgment line as `[label, body]`, read from the rendered page. */
function lines(fixture: ComponentFixture<AboutPage>): [string, string][] {
  const heading = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('h2')).find(
    (h) => h.textContent?.trim() === 'Acknowledgments',
  )!;
  const section = heading.parentElement!;
  return (Array.from(section.querySelectorAll('p')) as HTMLElement[])
    .filter((p) => p.querySelector('.font-medium'))
    .map((p) => {
      const label = p.querySelector('.font-medium')!.textContent!.trim().replace(/:$/, '');
      const body = p.textContent!.replace(p.querySelector('.font-medium')!.textContent!, '').trim();
      return [label, body];
    });
}

describe('AboutPage — the acknowledgments (v4 AboutView.tsx:373-383 at 236c04e35)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('lists v4’s ten groups in v4’s order', async () => {
    expect(lines(await render()).map(([label]) => label)).toEqual([
      'Core',
      'Editor',
      'AI & LLM',
      'Markdown & Documents',
      'Diagrams',
      'UI & Interaction',
      'Terminals & Realtime',
      'Filesystem & Archives',
      'Desktop & Infrastructure',
      'Testing',
    ]);
  });

  it('carries the three bodies v4 a4945d8d9 changed, byte for byte', async () => {
    const byLabel = new Map(lines(await render()));
    expect(byLabel.get('Markdown & Documents')).toBe(
      'unified, remark-parse, remark-gfm, remark-math, remark-rehype, rehype-stringify, rehype-highlight, rehype-katex, remark-breaks, remark-smartypants, KaTeX, react-markdown, react-syntax-highlighter, mammoth, PDF.js, yaml, jsonrepair, MessagePack',
    );
    expect(byLabel.get('Diagrams')).toBe(
      'Mermaid, Graphviz (@hpcc-js/wasm-graphviz), svg2roughjs, DOMPurify',
    );
    expect(byLabel.get('Terminals & Realtime')).toBe('xterm.js, node-pty, ws, croner');
  });
});
