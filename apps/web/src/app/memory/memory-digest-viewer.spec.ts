import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { MemoryDto } from '../core/core-contract';
import { MemoryDigestViewer } from './memory-digest-viewer';

/**
 * P4.D276 — the digest viewer, transcribed from v4
 * `components/memory/memory-list.tsx:370-389` at `01a83539d` (R-H: v4's
 * hand markup, portalled; no backdrop close — v4 `dff76ebef` added the role and
 * Escape, P4.D358).
 */

function digest(over: Partial<MemoryDto> = {}): MemoryDto {
  return {
    id: 'dig-1',
    userId: 'u1',
    characterId: 'c1',
    content: 'Mnemo keeps the brass key in the left drawer.',
    summary: 'Where the key lives',
    keywords: [],
    tags: [],
    importance: 0.6,
    source: 'CONSOLIDATED',
    chatId: null,
    projectId: null,
    aboutCharacterId: null,
    sourceMessageId: null,
    witnessedContext: null,
    lastAccessedAt: null,
    reinforcementCount: 0,
    lastReinforcedAt: null,
    relatedMemoryIds: [],
    reinforcedImportance: 0.6,
    consolidatedFrom: ['m1'],
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  };
}

function mount(memory: MemoryDto) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [MemoryDigestViewer] });
  const fixture = TestBed.createComponent(MemoryDigestViewer);
  fixture.componentRef.setInput('memory', memory);
  fixture.detectChanges();
  TestBed.tick();
  return fixture;
}

describe('MemoryDigestViewer (P4.D276, v4 memory-list.tsx:370-389)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('reads v4\'s header strings, U+2019 included (v4 :375-376)', () => {
    const host = mount(digest()).nativeElement as HTMLElement;
    expect(host.querySelector('h2.qt-dialog-title')?.textContent).toBe('The Digest');
    expect(host.querySelector('p.qt-dialog-description')?.textContent).toBe(
      'The denser note that took this one’s place on the active shelf.',
    );
  });

  it('carries v4\'s overlay classes, with z-[100] on the overlay (v4 :371-372)', () => {
    const host = mount(digest()).nativeElement as HTMLElement;
    const overlay = host.querySelector('.qt-dialog-overlay') as HTMLElement;
    for (const cls of ['qt-dialog-overlay', 'p-4', 'z-[100]']) {
      expect(overlay.classList.contains(cls)).toBe(true);
    }
    const dialog = overlay.querySelector('.qt-dialog') as HTMLElement;
    for (const cls of ['max-w-xl', 'max-h-[90vh]', 'overflow-y-auto']) {
      expect(dialog.classList.contains(cls)).toBe(true);
    }
  });

  it('announces as a dialog named by its title (v4 dff76ebef retired R-H\'s "no role", P4.D358)', () => {
    const host = mount(digest()).nativeElement as HTMLElement;
    const dlg = host.querySelector('[role="dialog"]') as HTMLElement;
    expect(dlg.classList.contains('qt-dialog')).toBe(true);
    expect(dlg.getAttribute('aria-modal')).toBe('true');
    expect(dlg.getAttribute('tabindex')).toBe('-1');
    const id = dlg.getAttribute('aria-labelledby')!;
    expect(id).toMatch(/^qt-dialog-\d+$/);
    expect(document.getElementById(id)?.textContent?.trim()).toBe('The Digest');
  });

  it('a document Escape closes exactly once (v4 useEscapeKey, dff76ebef)', () => {
    const fixture = mount(digest());
    let closed = 0;
    fixture.componentInstance.close.subscribe(() => (closed += 1));
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
    );
    expect(closed).toBe(1);
  });

  it('reparents its host to document.body after the first render, and removes it on destroy', () => {
    const fixture = mount(digest());
    const host = fixture.nativeElement as HTMLElement;
    expect(host.parentElement).toBe(document.body);
    fixture.destroy();
    expect(document.body.contains(host)).toBe(false);
  });

  it('renders the digest on a handler-less card: Digest chip, provenance, no Edit/Delete', () => {
    const host = mount(digest()).nativeElement as HTMLElement;
    const card = host.querySelector('qt-memory-card') as HTMLElement;
    expect(card.textContent).toContain('Digest');
    expect(card.textContent).toContain('Digested from 1 note.');
    const texts = Array.from(card.querySelectorAll('button')).map((b) => b.textContent?.trim());
    expect(texts).not.toContain('Edit');
    expect(texts).not.toContain('Delete');
  });

  it('shows a superseded digest\'s TEXT, not a button (v4\'s handler-less card)', () => {
    const host = mount(digest({ tier: 'cold', supersededById: 'dig-2' })).nativeElement as HTMLElement;
    const card = host.querySelector('qt-memory-card') as HTMLElement;
    expect(card.textContent).toContain('Superseded by a digest.');
    expect(
      Array.from(card.querySelectorAll('button')).some((b) => b.textContent?.includes('Superseded')),
    ).toBe(false);
  });

  it('emits close from the footer Close button (v4 :382-384)', () => {
    const fixture = mount(digest());
    let closed = 0;
    fixture.componentInstance.close.subscribe(() => (closed += 1));
    const close = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Close',
    ) as HTMLButtonElement;
    expect(close.classList.contains('qt-button-secondary')).toBe(true);
    close.click();
    expect(closed).toBe(1);
  });
});
