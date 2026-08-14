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
 * Auto-Scroll sit between Composition Mode and Text Replacement, Custom Tools
 * sits between Automation and Agent Mode, and the engine-facing cards sit
 * between Text Replacement and Data Retention. It is easy to "tidy" by accident,
 * so it is pinned here against `ChatTabContent.tsx` L70-206.
 */

/** v4 `ChatTabContent.tsx` L70-210, top to bottom (Taboo added at `7df7de8e`,
 *  Brahma Console at `6452e2c3` — between Data Retention and Autonomous Rooms;
 *  Smart Typography at `2d31810f` — between Text Replacement and Token
 *  Display, where the feature reads as the Text Replacement card's sibling). */
const V4_CARD_ORDER = [
  ['Composition Mode', 'composition-mode'],
  ['Composer', 'composer-spellcheck'],
  ['Auto-Scroll', 'auto-scroll'],
  ['Text Replacement', 'text-replacements'],
  ['Smart Typography', 'smart-typography'],
  ['Token Display', 'token-display'],
  ['Context Compression', 'context-compression'],
  ['Memory Cascade', 'memory-cascade'],
  ['Image Description', 'image-description'],
  ['Automation', 'automation'],
  ['Custom Tools', 'custom-tools'],
  ['General State', 'general-state'],
  ['Agent Mode', 'agent-mode'],
  ['Thinking / Reasoning', 'thinking-display'],
  ['Answer Confirmation', 'answer-confirmation'],
  ['Dangerous Content', 'dangerous-content'],
  ['Taboo', 'taboo'],
  ['Data Retention', 'data-retention'],
  ['Brahma Console', 'brahma-console'],
  ['Autonomous Rooms', 'autonomous-rooms'],
  ['Scheduled Autonomous Rooms', 'autonomous-room-schedules'],
];

/**
 * jsdom has no `scrollIntoView`, and a deep-linked (`?section=`) card calls it
 * from a rAF once it force-opens — the same shim the Brahma console dialog spec
 * installs, for the same reason.
 */
function shimScrollIntoView(): void {
  const proto = globalThis.HTMLElement?.prototype as unknown as { scrollIntoView?: () => void };
  if (proto && !proto.scrollIntoView) proto.scrollIntoView = () => undefined;
}

function mount(section: string | null = null) {
  shimScrollIntoView();
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
          getTabooSettings: async () => ({ phrases: [] }),
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
  it('mounts all twenty-one v4 cards in v4\'s exact order', () => {
    const fixture = mount();
    const titles = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('qt-collapsible-card'),
    ).map((el) => el.getAttribute('title'));
    expect(titles).toEqual(V4_CARD_ORDER.map(([title]) => title));
    // The count itself is a pin: a card silently dropped from the template
    // would otherwise only shift the array and read as an order change.
    expect(titles.length).toBe(21);
  });

  it('carries v4\'s sectionId on every card (the ?section= deep link)', () => {
    const fixture = mount();
    const ids = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('qt-collapsible-card'),
    ).map((el) => el.getAttribute('sectionId'));
    expect(ids).toEqual(V4_CARD_ORDER.map(([, id]) => id));
  });

  it('hosts the Smart Typography card body, not just a titled shell', () => {
    // The two assertions above read only the wrapper's attributes, so a card
    // whose CONTENT went missing would still pass them — which is exactly what
    // a mutation check found. A collapsible renders its body only while open,
    // so this deep-links the section (v4's `?section=` arm) and then looks for
    // the component itself.
    const fixture = mount('smart-typography');
    const card = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('qt-collapsible-card'),
    ).find((c) => c.getAttribute('sectionId') === 'smart-typography');
    expect(card?.querySelector('qt-smart-typography-settings')).toBeTruthy();
  });

  it('the Composer card hosts spellcheck, then emoji, then unicode (v4 ChatTabContent order; the P4.D75 at-unify mounts)', () => {
    const fixture = mount('composer-spellcheck');
    const card = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('qt-collapsible-card'),
    ).find((c) => c.getAttribute('sectionId') === 'composer-spellcheck');
    const rows = Array.from(
      card?.querySelectorAll(
        'qt-composer-spellcheck-settings, qt-composer-emoji-settings, qt-composer-unicode-settings',
      ) ?? [],
    ).map((el) => el.tagName.toLowerCase());
    expect(rows).toEqual([
      'qt-composer-spellcheck-settings',
      'qt-composer-emoji-settings',
      'qt-composer-unicode-settings',
    ]);
  });

  it('no longer advertises the retired "not yet fitted out" placeholder', () => {
    const fixture = mount();
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).not.toContain('not yet fitted out');
    expect(text).not.toContain('remain in the workshop');
  });
});
