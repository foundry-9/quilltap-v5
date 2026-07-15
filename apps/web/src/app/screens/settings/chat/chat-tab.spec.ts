import { TestBed } from '@angular/core/testing';
import { ActivatedRoute } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ChatTab } from './chat-tab';

/**
 * The Chat tab's card ORDER and deep-link ids (P4.6an unit 7).
 *
 * v4's order is not alphabetical and not grouped by theme — Composer and
 * Auto-Scroll sit between Composition Mode and Text Replacement, and the nine
 * engine-facing cards sit between Text Replacement and Data Retention. It is
 * easy to "tidy" by accident, so it is pinned here against
 * `ChatTabContent.tsx` L70-197.
 */

/** v4 `ChatTabContent.tsx` L70-197, top to bottom. */
const V4_CARD_ORDER = [
  ['Composition Mode', 'composition-mode'],
  ['Composer', 'composer-spellcheck'],
  ['Auto-Scroll', 'auto-scroll'],
  ['Text Replacement', 'text-replacements'],
  ['Token Display', 'token-display'],
  ['Context Compression', 'context-compression'],
  ['Memory Cascade', 'memory-cascade'],
  ['Image Description', 'image-description'],
  ['Automation', 'automation'],
  ['Agent Mode', 'agent-mode'],
  ['Thinking / Reasoning', 'thinking-display'],
  ['Answer Confirmation', 'answer-confirmation'],
  ['Dangerous Content', 'dangerous-content'],
  ['Data Retention', 'data-retention'],
  ['Autonomous Rooms', 'autonomous-rooms'],
  ['Scheduled Autonomous Rooms', 'autonomous-room-schedules'],
];

function mount(section: string | null = null) {
  const dispatchExpect = vi.fn(async () => ({ type: 'chatSettings', data: {} }));
  const dispatchData = vi.fn(async () => ({}));
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ChatTab],
    providers: [
      provideTanStackQuery(new QueryClient()),
      {
        provide: CoreClient,
        useValue: {
          dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'],
          dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
          getDataRetentionSettings: async () => ({ staleChatDays: 30 }),
          getAutonomousRooms: async () => [],
        },
      },
      {
        provide: ActivatedRoute,
        useValue: {
          queryParamMap: of({ get: (k: string) => (k === 'section' ? section : null) }),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(ChatTab);
  fixture.detectChanges();
  return fixture;
}

describe('ChatTab', () => {
  it('mounts all sixteen v4 cards in v4\'s exact order', () => {
    const fixture = mount();
    const titles = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('qt-collapsible-card'),
    ).map((el) => el.getAttribute('title'));
    expect(titles).toEqual(V4_CARD_ORDER.map(([title]) => title));
  });

  it('carries v4\'s sectionId on every card (the ?section= deep link)', () => {
    const fixture = mount();
    const ids = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('qt-collapsible-card'),
    ).map((el) => el.getAttribute('sectionId'));
    expect(ids).toEqual(V4_CARD_ORDER.map(([, id]) => id));
  });

  it('no longer advertises the retired "not yet fitted out" placeholder', () => {
    const fixture = mount();
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).not.toContain('not yet fitted out');
    expect(text).not.toContain('remain in the workshop');
  });
});
