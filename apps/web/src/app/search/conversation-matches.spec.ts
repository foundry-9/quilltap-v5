import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { ConversationMatch } from '../chat/conversation-search';
import { ConversationMatches, formatWhen } from './conversation-matches';

/** v4 `components/search/conversation-matches.tsx` (`4f5047241`). */

function match(partial: Partial<ConversationMatch> = {}): ConversationMatch {
  return {
    messageId: 'm1',
    speaker: 'Abigail',
    isAnnouncement: false,
    snippet: 'a fine morning at the club',
    occurrences: 1,
    createdAt: '2026-10-29T14:05:00.000Z',
    ...partial,
  };
}

function render(matches: ConversationMatch[], total: number, query: string) {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ConversationMatches] });
  const fixture = TestBed.createComponent(ConversationMatches);
  fixture.componentRef.setInput('matches', matches);
  fixture.componentRef.setInput('total', total);
  fixture.componentRef.setInput('query', query);
  const picked: string[] = [];
  fixture.componentInstance.select.subscribe((id) => picked.push(id));
  fixture.detectChanges();
  return { fixture, host: fixture.nativeElement as HTMLElement, picked };
}

const squash = (s: string | null | undefined) => (s ?? '').replace(/\s+/g, ' ').trim();

describe('formatWhen', () => {
  it("is exactly Node's own toLocaleString(undefined, …) for the ambient locale", () => {
    // Pinned against the runtime itself, so a CI locale change cannot pass a
    // wrong option set.
    for (const iso of ['2026-10-29T14:05:00.000Z', '2026-01-01T00:00:00Z', '1999-12-31T23:59:59Z']) {
      expect(formatWhen(iso)).toBe(
        new Date(iso).toLocaleString(undefined, {
          month: 'short',
          day: 'numeric',
          hour: 'numeric',
          minute: '2-digit',
        }),
      );
    }
  });

  it("is '' for an invalid or empty date", () => {
    expect(formatWhen('not a date')).toBe('');
    expect(formatWhen('')).toBe('');
  });
});

describe('ConversationMatches', () => {
  it('is a labelled group with the bookmark header and the bare total', () => {
    const { host } = render([match(), match({ messageId: 'm2' })], 2, 'fine');
    const group = host.querySelector('[role="group"]')!;
    expect(group.getAttribute('aria-label')).toBe('Matches in this conversation');
    const header = group.firstElementChild as HTMLElement;
    expect(header.className).toBe('px-3 py-1 qt-text-xs font-semibold uppercase tracking-wider');
    const icon = header.querySelector('span')!;
    expect(icon.getAttribute('aria-hidden')).toBe('true');
    expect(icon.textContent).toBe('🔖');
    expect(squash(header.textContent)).toBe('🔖 This conversation (2)');
  });

  it('shows shown/total when the list is capped', () => {
    const { host } = render([match()], 7, 'fine');
    expect(squash(host.querySelector('[role="group"] > div')!.textContent)).toBe(
      '🔖 This conversation (1/7)',
    );
  });

  it('the empty state names the query verbatim', () => {
    const { host } = render([], 0, ' gin ');
    const p = host.querySelector('p.px-3.py-2.qt-text-small')!;
    expect(p.textContent).toBe('Not a whisper of " gin " in this conversation');
    expect(squash(host.querySelector('[role="group"] > div')!.textContent)).toBe(
      '🔖 This conversation (0)',
    );
    expect(host.querySelector('[role="button"]')).toBeNull();
  });

  it('rows are div role=button (NOT <button>), focusable, with v4 classes', () => {
    const { host } = render([match()], 1, 'fine');
    const row = host.querySelector('[role="button"]') as HTMLElement;
    // Two bundled themes set every <button> in tracked capitals; the row is prose.
    expect(row.tagName).toBe('DIV');
    expect(host.querySelector('button')).toBeNull();
    expect(row.getAttribute('tabindex')).toBe('0');
    expect(row.className).toBe(
      'block w-full text-left p-3 qt-hover-accent rounded-lg transition-colors cursor-pointer',
    );
  });

  it('a click and Enter / Space emit the message id', () => {
    const { host, picked } = render([match({ messageId: 'abc' })], 1, 'fine');
    const row = host.querySelector('[role="button"]') as HTMLElement;
    row.click();
    row.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    row.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true }));
    row.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', bubbles: true }));
    expect(picked).toEqual(['abc', 'abc', 'abc']);
  });

  it('names an unknown speaker, badges an announcement, and counts repeats', () => {
    const { host } = render(
      [match({ speaker: null, isAnnouncement: true, occurrences: 3 })],
      1,
      'fine',
    );
    const row = host.querySelector('[role="button"]') as HTMLElement;
    expect(row.querySelector('.qt-text-primary.truncate')!.textContent).toBe('Unknown speaker');
    const badge = row.querySelector('.qt-badge-info') as HTMLElement;
    expect(badge.className).toBe('text-xs px-1.5 py-0.5 rounded qt-badge-info');
    expect(badge.textContent).toBe('Announcement');
    expect(Array.from(row.querySelectorAll('.qt-text-xs')).map((e) => e.textContent)).toEqual([
      '×3',
      formatWhen('2026-10-29T14:05:00.000Z'),
    ]);
  });

  it('a single occurrence shows no ×n and no badge for a plain line', () => {
    const { host } = render([match()], 1, 'fine');
    const row = host.querySelector('[role="button"]') as HTMLElement;
    expect(row.querySelector('.qt-badge-info')).toBeNull();
    expect(row.textContent).not.toContain('×');
    expect(row.querySelector('.qt-text-primary')!.textContent).toBe('Abigail');
    expect(row.querySelector('.ml-auto')!.textContent).toBe(formatWhen('2026-10-29T14:05:00.000Z'));
  });

  it('highlights the trimmed query inside the snippet', () => {
    const { host } = render([match({ snippet: 'A FINE day, a fine day' })], 1, '  fine ');
    const p = host.querySelector('p.qt-text-small.mt-1.line-clamp-2')!;
    expect(Array.from(p.querySelectorAll('mark.qt-highlight')).map((m) => m.textContent)).toEqual([
      'FINE',
      'fine',
    ]);
    expect(squash(p.textContent)).toBe('A FINE day, a fine day');
  });
});
