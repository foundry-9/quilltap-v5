/**
 * interpretWorkspaceLinkClick — the capture-phase link decision (v4
 * `WorkspaceLinkInterceptor.tsx`). Uses jsdom anchors.
 *
 * @vitest-environment jsdom
 */

import { Component, DebugElement, input } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CharacterListItem, EnrichedChatSummary } from '../../core/core-contract';
import { CharacterCard } from '../../screens/characters/list/character-card';
import { ChatCard } from '../../screens/salon/chat-card';
import { ToastService } from '../../ui/toast.service';
import { interpretWorkspaceLinkClick } from './link-interceptor';

function anchor(attrs: Record<string, string>): HTMLAnchorElement {
  const a = document.createElement('a');
  for (const [k, v] of Object.entries(attrs)) a.setAttribute(k, v);
  return a;
}

function click(target: HTMLElement | null, over: Partial<MouseEvent> = {}): MouseEvent {
  return {
    button: 0,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    defaultPrevented: false,
    target,
    ...over,
  } as unknown as MouseEvent;
}

describe('interpretWorkspaceLinkClick', () => {
  it('maps an in-app anchor href to a tab intent', () => {
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/characters' })))).toEqual({
      kind: 'aurora',
    });
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/salon/abc' })))).toEqual({
      kind: 'salon',
      payload: { chatId: 'abc' },
    });
  });

  it('resolves the nearest ancestor anchor from a nested target', () => {
    const a = anchor({ href: '/prospero' });
    const span = document.createElement('span');
    a.appendChild(span);
    expect(interpretWorkspaceLinkClick(click(span))).toEqual({ kind: 'prospero' });
  });

  it('passes through modifier / middle clicks', () => {
    const a = anchor({ href: '/characters' });
    expect(interpretWorkspaceLinkClick(click(a, { metaKey: true }))).toBeNull();
    expect(interpretWorkspaceLinkClick(click(a, { ctrlKey: true }))).toBeNull();
    expect(interpretWorkspaceLinkClick(click(a, { shiftKey: true }))).toBeNull();
    expect(interpretWorkspaceLinkClick(click(a, { button: 1 }))).toBeNull();
  });

  it('passes through download / target / external / already-prevented', () => {
    expect(
      interpretWorkspaceLinkClick(click(anchor({ href: '/characters', download: '' }))),
    ).toBeNull();
    expect(
      interpretWorkspaceLinkClick(click(anchor({ href: '/characters', target: '_blank' }))),
    ).toBeNull();
    expect(interpretWorkspaceLinkClick(click(anchor({ href: 'https://x.example' })))).toBeNull();
    expect(
      interpretWorkspaceLinkClick(
        click(anchor({ href: '/characters' }), { defaultPrevented: true }),
      ),
    ).toBeNull();
  });

  it('passes through when there is no anchor, and for hrefs with no tab equivalent', () => {
    expect(interpretWorkspaceLinkClick(click(document.createElement('div')))).toBeNull();
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/unlock' })))).toBeNull();
  });

  // P4.d16 tier 2: v4 intercepts /salon/new into its modal; v5 opens the tab
  // that hosts the New-Chat screen, seeds and all.
  it('opens the salon-new tab for /salon/new, carrying the modal seeds', () => {
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/salon/new' })))).toEqual({
      kind: 'salon-new',
      payload: undefined,
    });
    expect(
      interpretWorkspaceLinkClick(click(anchor({ href: '/salon/new?characterId=abc' }))),
    ).toEqual({
      kind: 'salon-new',
      payload: { characterId: 'abc', projectId: undefined, autonomous: false },
    });
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/salon/new?autonomous=1' })))).toEqual(
      {
        kind: 'salon-new',
        payload: { characterId: undefined, projectId: undefined, autonomous: true },
      },
    );
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/salon/new?projectId=p' })))).toEqual({
      kind: 'salon-new',
      payload: { characterId: undefined, projectId: 'p', autonomous: false },
    });
  });

  /**
   * P4.80 — the v5-only button guard. v5's chat card wraps its whole body in an
   * `<a routerLink>`, so its action buttons are anchor DESCENDANTS; v4's card is
   * a `<div>` whose own handler early-returns on `closest('button')`, so v4's
   * interceptor never sees such a click. Without the guard the capture handler
   * `stopImmediatePropagation`s and the button's handler never runs — the
   * delete / copy-link / Scriptorium / project-remove buttons all silently open
   * the chat instead. The nested-span case above is the counterexample that
   * keeps the guard honest: a plain element inside the anchor still navigates.
   */
  it('lets a click on a BUTTON inside an in-app anchor through (v5-only)', () => {
    const a = anchor({ href: '/salon/abc' });
    const button = document.createElement('button');
    const icon = document.createElement('span');
    button.appendChild(icon);
    a.appendChild(button);
    document.body.appendChild(a);
    // Both the button itself and anything nested inside it.
    expect(interpretWorkspaceLinkClick(click(button))).toBeNull();
    expect(interpretWorkspaceLinkClick(click(icon))).toBeNull();
    document.body.removeChild(a);
  });

  it('still intercepts a button that merely SITS BESIDE an in-app anchor', () => {
    // The guard is scoped to a button the anchor CONTAINS — a sibling button
    // whose own click happens to land on a link elsewhere is not this case, and
    // an anchor nested inside a button still opens its tab.
    const button = document.createElement('button');
    const a = anchor({ href: '/prospero' });
    button.appendChild(a);
    document.body.appendChild(button);
    expect(interpretWorkspaceLinkClick(click(a))).toEqual({ kind: 'prospero' });
    document.body.removeChild(button);
  });

  // v4 `8d86847a`: the salon list is a tab now — the rail's Chats item and the
  // home links stop unmounting the workspace.
  it('opens the salon-list tab for /salon (and legacy /chats)', () => {
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/salon' })))).toEqual({
      kind: 'salon-list',
    });
    expect(interpretWorkspaceLinkClick(click(anchor({ href: '/chats' })))).toEqual({
      kind: 'salon-list',
    });
  });
});


// ===========================================================================
// The nested-button census (P4.84).
//
// `link-interceptor.ts:59-60` passes a click THROUGH when `target.closest(
// 'button')` is inside the anchor. That rule is v5-only — v4's chat card is a
// `<div>` whose own handler early-returns for `closest('button')`, so its
// action controls never reach v4's interceptor as link clicks at all. v5 wraps
// the whole card body in an `<a routerLink>`, which makes every action control
// an anchor DESCENDANT, and the capture handler `stopImmediatePropagation()`s
// before their own handlers can run.
//
// So the rule's safety rests on an unstated invariant: every clickable control
// inside a card's anchor is a `<button>`. Nothing enforced it. This census
// does: it renders each card and walks Angular's own `DebugElement.listeners`
// — which report template event bindings, including those inside CHILD
// components rendered in the anchor, where a source scan of the card's template
// would see nothing — and refuses any `click`-bound element inside an `<a>`
// that is not a button.
//
// Plant `<span (click)="…">` inside a card's anchor and this reddens.
//
// Measured card set (2026-09-07): the two cards whose anchor has clickable
// descendants at all. `prospero/project-card.ts`, `home/recent-chat-item.ts`,
// `home/home-character-card.ts` and `home/project-item.ts` were checked and
// have NONE — their `<a>` elements carry no click-bound descendants — so they
// are outside this census rather than silently omitted from it.
// ===========================================================================

/** Every element in the fixture that carries a template `(click)` binding. */
function clickBound(fixture: ComponentFixture<unknown>): DebugElement[] {
  const out: DebugElement[] = [];
  const walk = (de: DebugElement): void => {
    if (de.listeners.some((l) => l.name === 'click')) out.push(de);
    for (const child of de.children) walk(child);
  };
  walk(fixture.debugElement);
  return out;
}

/** Those of them that sit INSIDE an anchor — the interceptor's blast radius. */
function insideAnchor(des: DebugElement[]): DebugElement[] {
  return des.filter((de) => {
    const el = de.nativeElement as HTMLElement | null;
    return !!el?.parentElement?.closest?.('a');
  });
}

function chatSummary(): EnrichedChatSummary {
  return {
    id: 'c1',
    title: 'A conversation',
    contextSummary: null,
    createdAt: '2024-03-04T00:00:00.000Z',
    updatedAt: '2026-08-30T00:00:00.000Z',
    lastMessageAt: null,
    participants: [],
    tags: [],
    project: { id: 'proj1', name: 'The Expedition' },
    storyBackground: null,
    conciergeState: 'monitored',
    dangerCategories: [],
    chatType: 'salon',
    scriptoriumStatus: 'none',
  } as unknown as EnrichedChatSummary;
}

function characterItem(): CharacterListItem {
  return {
    id: 'ch1',
    name: 'Perpetua',
    title: null,
    description: null,
    avatarUrl: null,
    defaultImageId: null,
    defaultImage: null,
    isFavorite: false,
    controlledBy: 'llm',
    carinaEnabled: false,
    archivedAt: null,
    tags: [],
    _count: { chats: 0 },
  } as unknown as CharacterListItem;
}

describe('the interceptor’s nested-button rule — the card census', () => {
  it('every click-bound element inside the chat card’s anchor is a <button>', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [ChatCard],
      providers: [
        provideRouter([]),
        { provide: CoreClient, useValue: { renderConversation: vi.fn() } },
        { provide: ToastService, useValue: { showSuccess: vi.fn(), showError: vi.fn() } },
      ],
    });
    const fixture = TestBed.createComponent(ChatCard);
    fixture.componentRef.setInput('chat', chatSummary());
    fixture.componentRef.setInput('deletable', true);
    fixture.componentRef.setInput('removable', true);
    fixture.detectChanges();

    // The premise the rule rests on: the card body really IS one big anchor.
    const anchor = (fixture.nativeElement as HTMLElement).querySelector('a');
    expect(anchor).toBeTruthy();

    const inside = insideAnchor(clickBound(fixture));
    // Not a vacuous pass: with both optional actions on, there are controls in
    // there to judge (remove, copy-link, delete).
    expect(inside.length).toBeGreaterThanOrEqual(3);
    expect(
      inside
        .map((de) => (de.nativeElement as HTMLElement).tagName)
        .filter((tag) => tag !== 'BUTTON'),
    ).toEqual([]);
  });

  it('every click-bound element inside the character card’s anchors is a <button>', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [CharacterCard],
      providers: [provideRouter([])],
    });
    const fixture = TestBed.createComponent(CharacterCard);
    fixture.componentRef.setInput('character', characterItem());
    fixture.detectChanges();

    expect((fixture.nativeElement as HTMLElement).querySelector('a')).toBeTruthy();
    expect(
      insideAnchor(clickBound(fixture))
        .map((de) => (de.nativeElement as HTMLElement).tagName)
        .filter((tag) => tag !== 'BUTTON'),
    ).toEqual([]);
  });

  it('the census SEES a planted non-button control (the guard’s own guard)', () => {
    // Without this, a census that silently found nothing would look identical
    // to a census that works.
    @Component({
      selector: 'qt-planted-card',
      template: `
        <a href="/salon/c1">
          <span (click)="hit = hit + 1">a span nobody made a button</span>
          <button type="button" (click)="hit = hit + 1">a proper one</button>
        </a>
      `,
    })
    class PlantedCard {
      hit = 0;
      readonly unusedInput = input<string>('');
    }

    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [PlantedCard] });
    const fixture = TestBed.createComponent(PlantedCard);
    fixture.detectChanges();

    const offenders = insideAnchor(clickBound(fixture))
      .map((de) => (de.nativeElement as HTMLElement).tagName)
      .filter((tag) => tag !== 'BUTTON');
    expect(offenders).toEqual(['SPAN']);
  });
});
