import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type {
  CharacterChatSummary,
  CharacterListItem,
  EnrichedChatSummary,
  ProjectDetail,
  ProjectRosterCharacter,
} from '../core/core-contract';
import { CharactersList } from '../screens/characters/list/characters-list';
import { CharacterConversationsTab } from '../screens/characters/view/tabs/conversations-tab';
import { CharactersSection } from '../screens/home/characters-section';
import type { HomepageCharacter, RecentChat } from '../screens/home/home.api';
import { RecentChatsSection } from '../screens/home/recent-chats-section';
import { ProjectCharactersCard } from '../screens/prospero/cards/project-characters-card';
import { ProjectChatsSection } from '../screens/prospero/cards/project-chats-section';
import { SalonList } from '../screens/salon/salon-list';
import { QuickHideService } from './quick-hide.service';
import { ACTIVE_TAGS_KEY, HIDE_DANGEROUS_KEY } from './quick-hide.storage';

/**
 * The quick-hide CONSUMERS (work order P4.9d tier-1 item 2).
 *
 * Every v4 `useQuickHide` call site collects its tag ids differently, and the
 * differences are load-bearing — the Salon list unions chat tags with every
 * participant's, the homepage Recent Chats section consults participants only,
 * the character Conversations tab consults chat tags only. Each case below pins
 * one consumer against the v4 file:line it was transcribed from.
 *
 * Rather than stub the service, these render over the REAL `QuickHideService`
 * seeded through localStorage, so the storage format and the predicate are
 * exercised end-to-end the way a browser would.
 */

const HIDDEN = 't-hidden';

/** Seed localStorage so the eagerly-initialized service starts already filtering. */
function seedHidden(opts: { tags?: string[]; dangerous?: boolean } = {}): void {
  const store = new Map<string, string>();
  if (opts.tags) store.set(ACTIVE_TAGS_KEY, JSON.stringify(opts.tags));
  if (opts.dangerous) store.set(HIDE_DANGEROUS_KEY, 'true');
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  });
}

/** A CoreClient stub routing by request type; `tagList` keeps the tag valid. */
function stubClient(handler: (req: { type: string; [k: string]: unknown }) => unknown) {
  const dispatchData = (async (req: { type: string }) => {
    if (req.type === 'tagList') {
      return { tags: [{ id: HIDDEN, name: 'Hidden', quickHide: true }] };
    }
    const out = handler(req as { type: string });
    if (out instanceof Error) throw out;
    return out;
  }) as CoreClient['dispatchData'];
  return {
    dispatchData,
    dispatchExpect: (async (req: { type: string }, key: string) => ({
      type: key,
      data: handler(req as { type: string }),
    })) as CoreClient['dispatchExpect'],
    listAutonomousRooms: (async () => []) as CoreClient['listAutonomousRooms'],
  } as Partial<CoreClient>;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function configure(client: Partial<CoreClient>): void {
  TestBed.configureTestingModule({
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
    ],
  });
}

// --- payload builders -------------------------------------------------------

function enrichedChat(over: Partial<EnrichedChatSummary>): EnrichedChatSummary {
  return {
    id: 'c1',
    title: 'A Chat',
    contextSummary: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-02T00:00:00.000Z',
    lastMessageAt: null,
    participants: [],
    tags: [],
    project: null,
    storyBackground: null,
    conciergeState: 'monitored',
    dangerCategories: [],
    chatType: 'salon',
    scriptoriumStatus: 'none',
    _count: { messages: 3, memories: 0 },
    ...over,
  };
}

/** An `EnrichedParticipantSummary` carrying the given character tag ids. */
function participantWithTags(tags: string[], id = 'p1') {
  return {
    id,
    type: 'CHARACTER' as const,
    displayOrder: 0,
    isActive: true,
    status: 'ACTIVE',
    character: {
      id: `char-${id}`,
      name: 'Someone',
      title: null,
      avatarUrl: null,
      defaultImageId: null,
      defaultImage: null,
      talkativeness: 5,
      tags,
    },
  };
}

function recentChat(over: Partial<RecentChat>): RecentChat {
  return {
    id: 'rc1',
    title: 'A Chat',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-02T00:00:00.000Z',
    lastMessageAt: null,
    participants: [],
    _count: { messages: 2 },
    ...over,
  };
}

function homeCharacter(over: Partial<HomepageCharacter>): HomepageCharacter {
  return {
    id: 'hc1',
    name: 'Lorian',
    title: null,
    defaultImageId: null,
    defaultImage: null,
    tags: [],
    isFavorite: true,
    npc: false,
    chatCount: 1,
    ...over,
  };
}

function listCharacter(over: Partial<CharacterListItem>): CharacterListItem {
  return {
    id: 'lc1',
    name: 'Bertie',
    title: null,
    description: '',
    defaultImageId: null,
    defaultImage: null,
    isFavorite: false,
    controlledBy: 'llm',
    canBeCarina: false,
    defaultConnectionProfileId: null,
    defaultPartnerId: null,
    defaultPartnerName: null,
    defaultTimestampConfig: null,
    defaultScenarioId: null,
    defaultSystemPromptId: null,
    defaultImageProfileId: null,
    npc: false,
    createdAt: '2024-01-01T00:00:00.000Z',
    tags: [],
    updatedAt: '2024-01-01T00:00:00.000Z',
    systemPrompts: [],
    scenarios: [],
    _count: { chats: 0 },
    ...over,
  };
}

function characterChat(over: Partial<CharacterChatSummary>): CharacterChatSummary {
  return {
    id: 'cc1',
    title: 'Tea at Midnight',
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-02T00:00:00.000Z',
    lastMessageAt: '2024-01-02T00:00:00.000Z',
    character: { id: 'c1', name: 'Jeeves' },
    project: null,
    storyBackground: null,
    messages: [],
    tags: [],
    conciergeState: 'monitored',
    dangerCategories: [],
    _count: { messages: 4, memories: 0 },
    scriptoriumStatus: 'none',
    ...over,
  };
}

function rosterCharacter(over: Partial<ProjectRosterCharacter>): ProjectRosterCharacter {
  return {
    id: 'rc1',
    name: 'Riya',
    defaultImageId: null,
    defaultImage: null,
    tags: [],
    chatCount: 0,
    ...over,
  };
}

function project(roster: ProjectRosterCharacter[]): ProjectDetail {
  return {
    id: 'proj-1',
    name: 'A Project',
    characterRoster: roster.map((r) => r.id),
    roster,
    allowAnyCharacter: false,
    defaultAgentModeEnabled: null,
    defaultAvatarGenerationEnabled: null,
    defaultImageProfileId: null,
    defaultRoleplayTemplateId: null,
    defaultAlertCharactersOfLanternImages: null,
  } as unknown as ProjectDetail;
}

// ---------------------------------------------------------------------------

describe('quick-hide consumers', () => {
  beforeEach(() => TestBed.resetTestingModule());
  afterEach(() => vi.unstubAllGlobals());

  describe('Salon list (v4 SalonListView.tsx:89-112)', () => {
    async function render(chats: EnrichedChatSummary[]): Promise<ComponentFixture<SalonList>> {
      configure(stubClient((r) => (r.type === 'listChats' ? chats : {})));
      const fixture = TestBed.createComponent(SalonList);
      fixture.detectChanges();
      await settle(fixture);
      return fixture;
    }

    it('hides a chat by its CHAT-level tag (v4 :91)', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([
        enrichedChat({ id: 'keep', title: 'Kept' }),
        enrichedChat({ id: 'drop', title: 'Dropped', tags: [{ tag: { id: HIDDEN, name: 'H' } }] }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Kept');
      expect(text).not.toContain('Dropped');
    });

    it('hides a chat by a PARTICIPANT character tag (v4 :93-97)', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([
        enrichedChat({ id: 'keep', title: 'Kept' }),
        enrichedChat({
          id: 'drop',
          title: 'Dropped',
          participants: [participantWithTags([HIDDEN])],
        }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Kept');
      expect(text).not.toContain('Dropped');
    });

    it('hides a chat on the uncensored row only when the filter is on (v4 :99-103)', async () => {
      seedHidden({ dangerous: true });
      const fixture = await render([
        enrichedChat({ id: 'keep', title: 'Kept' }),
        enrichedChat({ id: 'drop', title: 'Dropped', conciergeState: 'flagged' }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Kept');
      expect(text).not.toContain('Dropped');
    });

    it('follows the uncensored ROW, not the raw label (v4 c43d3b1b4)', async () => {
      // The behaviour delta: a Vouched Safe chat keeps its dangerous label
      // underneath by design and used to vanish; an Uncensored chat takes
      // every spicy route and never did.
      seedHidden({ dangerous: true });
      const fixture = await render([
        enrichedChat({ id: 'v', title: 'Vouched Kept', conciergeState: 'vouched' }),
        enrichedChat({ id: 'u', title: 'Uncensored Dropped', conciergeState: 'uncensored' }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Vouched Kept');
      expect(text).not.toContain('Uncensored Dropped');
    });

    it('hides nothing by default', async () => {
      seedHidden();
      const fixture = await render([
        enrichedChat({ id: 'a', title: 'Kept', conciergeState: 'flagged' }),
        enrichedChat({ id: 'b', title: 'Also', tags: [{ tag: { id: HIDDEN, name: 'H' } }] }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Kept');
      expect(text).toContain('Also');
    });

    it('the header autonomous toggle drives the shared service signal', async () => {
      seedHidden();
      const fixture = await render([]);
      const service = TestBed.inject(QuickHideService);
      expect(service.includeAutonomousRooms()).toBe(false);

      const button = (fixture.nativeElement as HTMLElement).querySelector(
        'button[title="Show autonomous character-to-character rooms in the Salon chat list"]',
      ) as HTMLButtonElement;
      button.click();
      fixture.detectChanges();

      expect(service.includeAutonomousRooms()).toBe(true);
    });
  });

  describe('Home Recent Chats (v4 RecentChatsSection.tsx:20-37)', () => {
    async function render(chats: RecentChat[]): Promise<ComponentFixture<RecentChatsSection>> {
      configure(stubClient(() => ({})));
      const fixture = TestBed.createComponent(RecentChatsSection);
      fixture.componentRef.setInput('chats', chats);
      fixture.detectChanges();
      await settle(fixture);
      return fixture;
    }

    it('hides by PARTICIPANT tags (v4 :30-36)', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([
        recentChat({ id: 'keep', title: 'Kept' }),
        recentChat({
          id: 'drop',
          title: 'Dropped',
          participants: [
            {
              id: 'p1',
              type: 'CHARACTER',
              isActive: true,
              displayOrder: 0,
              character: { id: 'ch1', name: 'Someone', tags: [HIDDEN] },
            },
          ],
        }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Kept');
      expect(text).not.toContain('Dropped');
    });

    it('does NOT consult chat-level tags — the projection carries none (v4 :28-36)', async () => {
      seedHidden({ tags: [HIDDEN] });
      // A chat-level tag bag is simply not part of `RecentChat`; the row stays.
      const fixture = await render([recentChat({ id: 'keep', title: 'Kept' })]);
      expect(fixture.nativeElement.textContent).toContain('Kept');
    });

    it('hides the uncensored row when the filter is on (v4 :20-33)', async () => {
      seedHidden({ dangerous: true });
      const fixture = await render([
        recentChat({ id: 'keep', title: 'Kept' }),
        recentChat({ id: 'drop', title: 'Dropped', conciergeState: 'flagged' }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Kept');
      expect(text).not.toContain('Dropped');
    });

    it('keeps a Vouched Safe chat and drops an Uncensored one (v4 c43d3b1b4)', async () => {
      seedHidden({ dangerous: true });
      const fixture = await render([
        recentChat({ id: 'v', title: 'Vouched Kept', conciergeState: 'vouched' }),
        recentChat({ id: 'u', title: 'Uncensored Dropped', conciergeState: 'uncensored' }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Vouched Kept');
      expect(text).not.toContain('Uncensored Dropped');
    });

    it('falls to the empty arm when everything is hidden', async () => {
      seedHidden({ dangerous: true });
      const fixture = await render([
        recentChat({ id: 'drop', title: 'Dropped', conciergeState: 'uncensored' }),
      ]);
      expect(fixture.nativeElement.textContent).toContain('No chats yet');
    });
  });

  describe('Home Characters section (v4 CharactersSection.tsx:29-32)', () => {
    async function render(
      characters: HomepageCharacter[],
    ): Promise<ComponentFixture<CharactersSection>> {
      configure(stubClient(() => ({})));
      const fixture = TestBed.createComponent(CharactersSection);
      fixture.componentRef.setInput('characters', characters);
      fixture.detectChanges();
      await settle(fixture);
      return fixture;
    }

    it('hides a character carrying a hidden tag', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([
        homeCharacter({ id: 'keep', name: 'Lorian' }),
        homeCharacter({ id: 'drop', name: 'Sekhmet', tags: [HIDDEN] }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Lorian');
      expect(text).not.toContain('Sekhmet');
    });

    it('gates the empty arm on the FILTERED list (v4 :86)', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([homeCharacter({ id: 'drop', name: 'Sekhmet', tags: [HIDDEN] })]);
      expect(fixture.nativeElement.textContent).toContain('No favorite characters');
    });
  });

  describe('Characters roster (v4 AuroraView.tsx:124-140)', () => {
    async function render(list: CharacterListItem[]): Promise<ComponentFixture<CharactersList>> {
      configure(
        stubClient((r) => {
          if (r.type === 'characterList') return { characters: list };
          if (r.type === 'connectionProfileList') return { profiles: [] };
          return {};
        }),
      );
      const fixture = TestBed.createComponent(CharactersList);
      fixture.detectChanges();
      await settle(fixture);
      return fixture;
    }

    it('filters before sorting, so hidden characters cannot influence order', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([
        listCharacter({ id: 'keep', name: 'Bertie' }),
        listCharacter({ id: 'drop', name: 'Aunt Agatha', isFavorite: true, tags: [HIDDEN] }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Bertie');
      expect(text).not.toContain('Aunt Agatha');
    });
  });

  describe('Character Conversations tab (v4 character-conversations-tab.tsx:38-44)', () => {
    async function render(
      chats: CharacterChatSummary[],
    ): Promise<ComponentFixture<CharacterConversationsTab>> {
      configure(
        stubClient((r) =>
          r.type === 'characterChats' ? { chats, total: chats.length } : {},
        ),
      );
      const fixture = TestBed.createComponent(CharacterConversationsTab);
      fixture.componentRef.setInput('characterId', 'c1');
      fixture.detectChanges();
      await settle(fixture);
      return fixture;
    }

    it('hides by CHAT tags only (v4 :41)', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([
        characterChat({ id: 'keep', title: 'Kept' }),
        characterChat({ id: 'drop', title: 'Dropped', tags: [{ tag: { id: HIDDEN, name: 'H' } }] }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Kept');
      expect(text).not.toContain('Dropped');
    });

    it('hides the uncensored row when the filter is on (v4 :49-55)', async () => {
      seedHidden({ dangerous: true });
      const fixture = await render([
        characterChat({ id: 'keep', title: 'Kept' }),
        characterChat({ id: 'drop', title: 'Dropped', conciergeState: 'uncensored' }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Kept');
      expect(text).not.toContain('Dropped');
    });

    it('keeps a Vouched Safe chat the raw label would have hidden (v4 c43d3b1b4)', async () => {
      seedHidden({ dangerous: true });
      const fixture = await render([
        characterChat({ id: 'v', title: 'Vouched Kept', conciergeState: 'vouched' }),
      ]);
      expect(fixture.nativeElement.textContent).toContain('Vouched Kept');
    });
  });

  describe('Prospero chats section (v4 ChatsSection.tsx:65-83)', () => {
    async function render(
      chats: EnrichedChatSummary[],
    ): Promise<ComponentFixture<ProjectChatsSection>> {
      configure(
        stubClient((r) =>
          r.type === 'projectChatList' ? { chats, total: chats.length } : {},
        ),
      );
      const fixture = TestBed.createComponent(ProjectChatsSection);
      fixture.componentRef.setInput('projectId', 'proj-1');
      fixture.detectChanges();
      await settle(fixture);
      return fixture;
    }

    it('hides by chat tags and by participant tags', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([
        enrichedChat({ id: 'keep', title: 'Kept' }),
        enrichedChat({ id: 'd1', title: 'DroppedByChat', tags: [{ tag: { id: HIDDEN, name: 'H' } }] }),
        enrichedChat({
          id: 'd2',
          title: 'DroppedByParticipant',
          participants: [participantWithTags([HIDDEN])],
        }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Kept');
      expect(text).not.toContain('DroppedByChat');
      expect(text).not.toContain('DroppedByParticipant');
    });

    it('shows v4’s distinct "some may be hidden" empty state (v4 :146)', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([
        enrichedChat({ id: 'drop', title: 'Dropped', tags: [{ tag: { id: HIDDEN, name: 'H' } }] }),
      ]);
      expect(fixture.nativeElement.textContent).toContain('No visible chats (some may be hidden).');
    });

    it('keeps the "no chats at all" empty state when nothing was fetched (v4 :146)', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([]);
      expect(fixture.nativeElement.textContent).toContain('No chats in this project yet.');
    });
  });

  describe('Prospero characters card (v4 CharactersCard.tsx:31-38)', () => {
    async function render(
      roster: ProjectRosterCharacter[],
    ): Promise<ComponentFixture<ProjectCharactersCard>> {
      configure(stubClient(() => ({})));
      const fixture = TestBed.createComponent(ProjectCharactersCard);
      fixture.componentRef.setInput('project', project(roster));
      fixture.componentRef.setInput('defaultOpen', true);
      fixture.detectChanges();
      await settle(fixture);
      return fixture;
    }

    it('dedupes by id and drops hidden characters', async () => {
      seedHidden({ tags: [HIDDEN] });
      const fixture = await render([
        rosterCharacter({ id: 'keep', name: 'Riya' }),
        rosterCharacter({ id: 'keep', name: 'Riya' }),
        rosterCharacter({ id: 'drop', name: 'Sekhmet', tags: [HIDDEN] }),
      ]);
      const text = fixture.nativeElement.textContent as string;
      expect(text).not.toContain('Sekhmet');
      // Deduped AND filtered: the subtitle counts one survivor (v4 :53).
      expect(text).toContain('1 character in roster');
    });
  });
});
