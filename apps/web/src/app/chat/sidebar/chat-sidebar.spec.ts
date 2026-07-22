import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';

import type { ParticipantDetail } from '../../core/core-contract';
import { createInitialTurnState, type TurnSelectionResult, type TurnState } from '../turn-order';
import { ChatSidebar } from './chat-sidebar';
import type { ChatSectionState } from './chat-section';
import type { VisibilityState } from './visibility-section';

/**
 * The sidebar scaffold (v4 `ChatSidebar.tsx`): the persisted collapse
 * preference and its localStorage key, the collapsed mini-strip, the expand /
 * collapse round-trip, the single-open accordion, and the participants section
 * in predicted turn order.
 */

function participant(
  id: string,
  name: string,
  overrides: Partial<ParticipantDetail> = {},
): ParticipantDetail {
  return {
    id,
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy: 'llm',
    status: 'active',
    character: {
      id: `char-${id}`,
      name,
      title: null,
      avatarUrl: null,
      defaultImageId: null,
      defaultImage: null,
      talkativeness: 0.5,
    },
    connectionProfile: null,
    imageProfile: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...overrides,
  };
}

@Component({
  imports: [ChatSidebar],
  template: `
    <div class="qt-chat-layout">
      <qt-chat-sidebar
        [chatSectionState]="chatSectionState"
        [visibilityState]="visibilityState"
        [participants]="participants()"
        [turnState]="turnState()"
        [turnSelectionResult]="turnSelectionResult()"
        [isPaused]="isPaused()"
        [userParticipantId]="'user'"
        (togglePause)="paused.push(true)"
        (nudge)="nudged.push($event)"
      />
    </div>
  `,
})
class Host {
  readonly participants = signal<ParticipantDetail[]>([
    participant('user', 'You', { controlledBy: 'user' }),
    participant('alice', 'Alice'),
    participant('bob', 'Bob'),
  ]);
  readonly turnState = signal<TurnState>(createInitialTurnState());
  readonly turnSelectionResult = signal<TurnSelectionResult | null>({
    nextSpeakerId: 'bob',
    reason: 'weighted_selection',
    cycleComplete: false,
  });
  readonly isPaused = signal(false);
  readonly chatSectionState: ChatSectionState = {
    roleplayTemplateId: null,
    timelineMode: null,
    imageProfileId: null,
    alertCharactersOfLanternImages: null,
    projectId: null,
    projectName: null,
  };
  readonly visibilityState: VisibilityState = {
    allowCrossCharacterVaultReads: false,
    coreWhisperEnabled: null,
    coreWhisperInterval: null,
    turnSkippingEnabled: null,
  };
  readonly paused: boolean[] = [];
  readonly nudged: string[] = [];
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.configureTestingModule({
    imports: [Host],
    // The Chat section is a projected child, so Angular instantiates it even
    // while its card is closed — it needs its injectables present. (Its own
    // `hasEverOpened` latch still keeps the reference fetches from firing.)
    providers: [
      { provide: CoreClient, useValue: { dispatch: async () => ({ type: 'chat', data: {} }) } },
      provideTanStackQuery(new QueryClient()),
      provideRouter([]),
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function sidebarEl(fixture: ComponentFixture<Host>): HTMLElement {
  return fixture.nativeElement.querySelector('qt-chat-sidebar') as HTMLElement;
}

function button(fixture: ComponentFixture<Host>, label: string): HTMLButtonElement {
  const found = Array.from(
    fixture.nativeElement.querySelectorAll('button'),
  ).find((b) => (b as HTMLButtonElement).getAttribute('aria-label') === label);
  return found as HTMLButtonElement;
}

describe('ChatSidebar', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('starts collapsed with no stored preference and shows the mini strip', async () => {
    const fixture = await render();
    const el = sidebarEl(fixture);
    expect(el.classList.contains('qt-chat-sidebar-collapsed')).toBe(true);
    expect(el.classList.contains('qt-chat-sidebar')).toBe(false);
    // One strip avatar per participant.
    expect(el.querySelectorAll('.qt-chat-sidebar-collapsed-avatar').length).toBe(3);
    // The expanded panel's accordion is absent while collapsed.
    expect(el.querySelector('.qt-chat-sidebar-list')).toBeNull();
  });

  it('expands on the strip toggle and persists the preference under v4’s key', async () => {
    const fixture = await render();
    button(fixture, 'Expand chat sidebar').click();
    fixture.detectChanges();

    const el = sidebarEl(fixture);
    expect(el.classList.contains('qt-chat-sidebar')).toBe(true);
    expect(localStorage.getItem('quilltap.chat-sidebar.collapsed')).toBe('false');
    // The default width applies to the host element (v4 puts it on the panel).
    expect(el.style.width).toBe('288px');

    button(fixture, 'Collapse chat sidebar').click();
    fixture.detectChanges();
    expect(sidebarEl(fixture).classList.contains('qt-chat-sidebar-collapsed')).toBe(true);
    expect(localStorage.getItem('quilltap.chat-sidebar.collapsed')).toBe('true');
  });

  it('honours a stored expanded preference and a stored width', async () => {
    localStorage.setItem('quilltap.chat-sidebar.collapsed', 'false');
    localStorage.setItem('quilltap.chat-sidebar.width', '400');
    const fixture = await render();
    const el = sidebarEl(fixture);
    expect(el.classList.contains('qt-chat-sidebar')).toBe(true);
    expect(el.style.width).toBe('400px');
  });

  it('opens Participants by default and closes it on a header click (single-open accordion)', async () => {
    localStorage.setItem('quilltap.chat-sidebar.collapsed', 'false');
    const fixture = await render();
    const el = sidebarEl(fixture);

    const header = el.querySelector('.qt-collapsible-card-header') as HTMLButtonElement;
    expect(header.getAttribute('aria-expanded')).toBe('true');
    expect(el.querySelector('.qt-chat-sidebar-section-participants')).not.toBeNull();
    // v4's card description counts LLM characters only (the user seat is excluded).
    expect(header.textContent).toContain('2 characters');

    header.click();
    fixture.detectChanges();
    expect(header.getAttribute('aria-expanded')).toBe('false');
    expect(el.querySelector('.qt-chat-sidebar-section-participants')).toBeNull();
  });

  it('lists the cast in predicted turn order and nudges through the card action', async () => {
    localStorage.setItem('quilltap.chat-sidebar.collapsed', 'false');
    const fixture = await render();
    const names = Array.from(
      sidebarEl(fixture).querySelectorAll('.qt-participant-card-name'),
    ).map((n) => n.textContent?.trim());
    // Bob is the selected next speaker, so he leads; the user seat trails the
    // eligible character (v4's ordering: next → eligible → user-turn).
    expect(names).toEqual(['Bob', 'Alice', 'You']);

    const badges = Array.from(
      sidebarEl(fixture).querySelectorAll('[data-testid="position-badge"]'),
    ).map((b) => b.textContent?.trim());
    expect(badges).toEqual(['1', '2', '3']);

    const nudge = Array.from(sidebarEl(fixture).querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Nudge',
    ) as HTMLButtonElement;
    nudge.click();
    expect(fixture.componentInstance.nudged).toEqual(['bob']);
  });

  it('reports the pause toggle from the collapsed strip', async () => {
    const fixture = await render();
    button(fixture, 'Pause auto-responses').click();
    expect(fixture.componentInstance.paused).toEqual([true]);
  });
});
