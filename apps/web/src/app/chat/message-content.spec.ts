// The one surviving arm of v4 `__tests__/unit/components/chat/LazyMessageContent.diagrams.test.tsx`
// (236c04e35) — only the Salon passes `diagrams`; v5 has no pre-rendered fast
// path to leave — plus v5-authored cases for the slot hydration (prefixed `v5:`).
import { signal } from '@angular/core';
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ThemeService } from '../theme/theme.service';
import { DIAGRAM_RENDERERS } from './diagrams/diagram-block';
import { cacheClear } from './diagrams/diagram-render-cache';
import { MessageContent } from './message-content';
import * as renderCache from './render/render-cache';

const SVG = '<svg xmlns="http://www.w3.org/2000/svg" data-testid="drawn"><rect></rect></svg>';
const FENCED = 'Here it is:\n\n```mermaid\nflowchart TD\n  A --> B\n```';

const mermaidMock = vi.fn();

function render(content: string, diagrams: { style: 'handDrawn' | 'clean'; streaming?: boolean } | null) {
  TestBed.configureTestingModule({
    imports: [MessageContent],
    providers: [
      {
        provide: DIAGRAM_RENDERERS,
        useValue: { mermaid: mermaidMock, graphviz: mermaidMock, svg: mermaidMock },
      },
      {
        provide: ThemeService,
        useValue: { activeThemeId: signal(null), resolvedColorMode: signal('light') },
      },
    ],
  });
  const fixture = TestBed.createComponent(MessageContent);
  fixture.componentRef.setInput('content', content);
  fixture.componentRef.setInput('diagrams', diagrams);
  fixture.detectChanges();
  TestBed.tick();
  return fixture;
}

async function settle(fixture: ComponentFixture<MessageContent>): Promise<void> {
  for (let i = 0; i < 3; i++) {
    await new Promise((r) => setTimeout(r, 20));
    fixture.detectChanges();
    TestBed.tick();
  }
}

const el = (f: ComponentFixture<MessageContent>) => f.nativeElement as HTMLElement;

describe('MessageContent — diagrams', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    cacheClear();
    renderCache.clearRenderCache();
    mermaidMock.mockResolvedValue({ svg: SVG });
  });

  afterEach(() => vi.restoreAllMocks());

  it('keeps the pre-rendered fast path for a message without a diagram', () => {
    const f = render('Just prose.', { style: 'handDrawn' });
    expect(el(f).querySelector('qt-diagram-block')).toBeNull();
    expect(el(f).textContent).toContain('Just prose.');
  });

  it('keeps the fast path for an ordinary code fence', () => {
    const f = render('```ts\nconst x = 1\n```', { style: 'handDrawn' });
    expect(el(f).querySelector('qt-diagram-block')).toBeNull();
    expect(el(f).querySelector('pre > code.language-ts')).not.toBeNull();
  });

  it('takes the full render for a closed diagram fence, even with pre-rendered HTML', async () => {
    const f = render(FENCED, { style: 'handDrawn' });
    await settle(f);
    const block = el(f).querySelector('qt-diagram-block');
    expect(block).not.toBeNull();
    expect(block?.querySelector('.qt-diagram')?.getAttribute('data-diagram-style')).toBe('handDrawn');
    expect(el(f).querySelector('.qt-diagram-slot')).toBeNull();
    expect(mermaidMock).toHaveBeenCalledWith('flowchart TD\n  A --> B', expect.objectContaining({ style: 'handDrawn' }));
    expect(el(f).querySelector('[data-testid="drawn"]')).not.toBeNull();
    // The prose around it is untouched.
    expect(el(f).querySelector('p')?.textContent).toBe('Here it is:');
  });

  it('draws in the clean style when no author style is given', async () => {
    const f = render(FENCED, { style: 'clean' });
    await settle(f);
    expect(el(f).querySelector('.qt-diagram')?.getAttribute('data-diagram-style')).toBe('clean');
  });

  it('v5: without the diagrams input (every surface but the Salon) the fence stays a code block', async () => {
    const f = render(FENCED, null);
    await settle(f);
    expect(el(f).querySelector('qt-diagram-block')).toBeNull();
    expect(el(f).querySelector('pre > code.language-mermaid')?.textContent).toContain('flowchart TD');
    expect(mermaidMock).not.toHaveBeenCalled();
  });

  it('v5: while streaming the fence stays a code block and nothing is drawn', async () => {
    const f = render(FENCED, { style: 'handDrawn', streaming: true });
    await settle(f);
    expect(el(f).querySelector('qt-diagram-block')).toBeNull();
    expect(el(f).querySelector('pre > code.language-mermaid')).not.toBeNull();
    expect(mermaidMock).not.toHaveBeenCalled();
  });

  it('v5: a content change destroys the old block before mounting the new one', async () => {
    const f = render(FENCED, { style: 'handDrawn' });
    await settle(f);
    const first = el(f).querySelector('qt-diagram-block');
    expect(first).not.toBeNull();
    const destroyed: string[] = [];
    // Each block registers its own DestroyRef; the first must go when the content moves on.
    const blocks = (f.componentInstance as unknown as { mounted: { destroy(): void; hostView: { destroyed: boolean } }[] })
      .mounted;
    expect(blocks).toHaveLength(1);
    const firstRef = blocks[0];

    f.componentRef.setInput('content', 'Now:\n\n```dot\ndigraph { a -> b }\n```');
    f.detectChanges();
    TestBed.tick();
    await settle(f);
    if (firstRef.hostView.destroyed) destroyed.push('first');
    expect(destroyed).toEqual(['first']);
    const now = el(f).querySelectorAll('qt-diagram-block');
    expect(now).toHaveLength(1);
    expect(now[0]).not.toBe(first);
    expect(now[0].querySelector('.qt-diagram')?.getAttribute('data-diagram-engine')).toBe('graphviz');
  });

  it('v5: a message the pass leaves alone is the cached render byte for byte (render-cache.ts untouched)', () => {
    const f = render('```ts\nconst x = 1\n```\n\nand prose', { style: 'handDrawn' });
    const inner = el(f).querySelector('.qt-chat-message-content')!.innerHTML;
    expect(inner).toBe(
      renderCache.renderMarkdownCached('```ts\nconst x = 1\n```\n\nand prose', {
        blobMountPointId: null,
        renderingPatterns: undefined,
        dialogueDetection: undefined,
        displayQuotes: true,
        imagesHidden: false,
      }),
    );
  });

  it('v5: destroying the message destroys its mounted blocks', async () => {
    const f = render(FENCED, { style: 'handDrawn' });
    await settle(f);
    const ref = (f.componentInstance as unknown as { mounted: { hostView: { destroyed: boolean } }[] }).mounted[0];
    f.destroy();
    expect(ref.hostView.destroyed).toBe(true);
  });
});
