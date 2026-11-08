/**
 * The About page's acknowledgments (v4 `app/about/AboutView.tsx:371-380` at
 * the pin `624d0d25c`; `:373-383` at `236c04e35`) — pinned VERBATIM, label by
 * label and body by body. v4
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

describe('AboutPage — the acknowledgments (v4 AboutView.tsx:371-380 at 624d0d25c)', () => {
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

/**
 * P4.D388 (C17): the twelve About byte edits between `236c04e35` and the pin
 * `624d0d25c` — `70d2859d8` (the five blurbs, `TypeScript 6+`, the Database row,
 * the Core credit), `c34986421` (the `File Manager: SVAR` row and
 * `@svar-ui/react-filemanager`), `db4a76e39` (`chokidar`), `856ed4f2b`
 * (`Playwright`). Byte copies of `git diff 236c04e35 624d0d25c --
 * app/about/AboutView.tsx`'s `+` lines, rendered (`&ndash;` → `–`).
 */
describe('AboutPage — the 624d0d25c About pass (P4.D388)', () => {
  afterEach(() => TestBed.resetTestingModule());

  /** Each feature bullet as `[title, body]`. */
  function features(fixture: ComponentFixture<AboutPage>): Map<string, string> {
    const out = new Map<string, string>();
    for (const li of Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('li'))) {
      const strong = li.querySelector('strong');
      if (!strong) continue;
      const title = strong.textContent!.trim();
      const body = li.textContent!.replace(/\s+/g, ' ').trim();
      out.set(title, body.slice(body.indexOf(title) + title.length).replace(/^ – /, ''));
    }
    return out;
  }

  /** The tech-stack grid as `[label, value]`. */
  function stack(fixture: ComponentFixture<AboutPage>): [string, string][] {
    const heading = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('h2')).find(
      (h) => h.textContent?.trim() === 'The Machinery Behind the Curtain',
    )!;
    return Array.from(heading.parentElement!.querySelectorAll('div.grid > div')).map((d) => {
      const label = d.querySelector('.font-medium')!.textContent!.trim().replace(/:$/, '');
      return [label, d.textContent!.replace(d.querySelector('.font-medium')!.textContent!, '').trim()];
    });
  }

  it('carries the five rewritten blurbs whole', async () => {
    const f = features(await render());
    expect(f.get('The Salon – Chat')).toBe(
      'chat interface with tool palette, agent mode, server-side rendering, embedded tool messages, Inform notes from the wings, Deus Ex Machina established facts that every character knows, the Scenario Builder, and indexed message search that looks in the open conversation first',
    );
    expect(f.get('Pascal the Croupier – Gaming')).toBe(
      'persistent state across four registers, dice rolls, coin flips, the Inventory of objects characters carry, stats, plus user-authored custom tools with honest server-side rolls, outcome tables, side effects, and a visual workbench to build them',
    );
    expect(f.get('The Scriptorium – Document Stores')).toBe(
      'mountable external knowledge sources that characters can read, search, and (in Document Mode) write back to, browsed with one folder browser shared with the Wardrobe and the Inventory and read-only viewers for pictures, PDFs and Word files',
    );
    expect(f.get('Saquel Ytzama – Keeper of Secrets')).toBe(
      'ChaCha20-Poly1305-encrypted databases, the Pepper Vault for API keys, instance locking, the .dbkey covenant, and the auto-lock idle timer',
    );
    expect(f.get('Brahma – The Console')).toBe(
      'a character-less, memory-free generic-LLM surface for plain model chat and read-only SQL inspection, with lookup shortcuts for characters, memories and conversations, reachable from any Salon as the @Brahma answerer on an engine of its own choosing',
    );
  });

  it('the tech stack: TypeScript 6+, SQLite3 Multiple Ciphers, no File Manager row — eleven rows', async () => {
    const rows = stack(await render());
    expect(rows).toEqual([
      ['Runtime', 'Node.js 24+'],
      ['Framework', 'Next.js 16+'],
      ['UI', 'React 19'],
      ['Language', 'TypeScript 6+'],
      ['Database', 'SQLite + SQLite3 Multiple Ciphers'],
      ['Editor', 'Lexical'],
      ['Data Fetching', 'TanStack Query'],
      ['Desktop', 'Electron'],
      ['Styling', 'Tailwind CSS 4+'],
      ['Validation', 'Zod'],
      ['Containers', 'Docker'],
    ]);
  });

  it('carries the four moved acknowledgment bodies whole', async () => {
    const byLabel = new Map(lines(await render()));
    expect(byLabel.get('Core')).toBe(
      'React, Next.js, TypeScript, better-sqlite3-multiple-ciphers (SQLite3 Multiple Ciphers, ChaCha20-Poly1305), Zod, Ajv, @tanstack/react-query',
    );
    expect(byLabel.get('UI & Interaction')).toBe(
      'Tailwind CSS, dnd-kit, @tanstack/react-virtual, sharp, Lucide Icons',
    );
    expect(byLabel.get('Filesystem & Archives')).toBe('tar, yauzl, semver');
    expect(byLabel.get('Testing')).toBe('Jest, Storybook, Testing Library');
  });

  it('names none of the retired credits anywhere on the page', async () => {
    const text = ((await render()).nativeElement as HTMLElement).textContent ?? '';
    for (const gone of ['SQLCipher', 'SVAR', 'svar', 'chokidar', 'Playwright', 'File Manager']) {
      expect(text, gone).not.toContain(gone);
    }
  });
});
