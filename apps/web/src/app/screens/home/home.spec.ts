import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { formatMessageTime } from './format-time';
import { HomeCharacterCard } from './home-character-card';
import { HomePage } from './home-page';
import {
  profileDisplayName,
  type HomeData,
  type HomepageCharacter,
  type HomepageProject,
  type RecentChat,
} from './home.api';

interface DispatchReq {
  type: string;
  [k: string]: unknown;
}

function stubClient(handler: (req: DispatchReq) => unknown): Partial<CoreClient> {
  return {
    dispatchData: (async (req: DispatchReq) => {
      const out = handler(req);
      if (out instanceof Error) {
        throw out;
      }
      return (out ?? {}) as Record<string, unknown>;
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function chat(over: Partial<RecentChat> = {}): RecentChat {
  return {
    id: 'c1',
    title: 'A Chat',
    updatedAt: '2026-01-01T00:00:00Z',
    lastMessageAt: '2026-01-02T00:00:00Z',
    participants: [],
    _count: { messages: 3 },
    ...over,
  };
}

function participant(id: string, name: string, displayOrder: number): RecentChat['participants'][0] {
  return {
    id: `p-${id}`,
    type: 'CHARACTER',
    isActive: true,
    displayOrder,
    character: { id, name },
  };
}

function project(over: Partial<HomepageProject> = {}): HomepageProject {
  return {
    id: 'pr1',
    name: 'Airship Saga',
    chatCount: 1,
    lastActivity: '2026-01-03T00:00:00Z',
    ...over,
  };
}

function character(over: Partial<HomepageCharacter> = {}): HomepageCharacter {
  return {
    id: 'ch1',
    name: 'Lorian',
    title: 'Wayfarer',
    defaultImageId: null,
    defaultImage: null,
    isFavorite: true,
    npc: false,
    chatCount: 2,
    ...over,
  };
}

function homeData(over: Partial<HomeData> = {}): HomeData {
  return {
    displayName: 'Friday',
    lastChatId: 'c1',
    recentChats: [chat()],
    projects: [project()],
    characters: [character()],
    ...over,
  };
}

/** Route the two home queries; unmatched types get an empty bag. */
function homeClient(data: HomeData, profiles: unknown[] = []): Partial<CoreClient> {
  return stubClient((req) => {
    if (req.type === 'systemHome') {
      return data as unknown as Record<string, unknown>;
    }
    if (req.type === 'connectionProfileList') {
      return { profiles };
    }
    return {};
  });
}

describe('HomePage', () => {
  afterEach(() => {
    vi.useRealTimers();
    TestBed.resetTestingModule();
  });

  async function render(client: Partial<CoreClient>): Promise<ComponentFixture<HomePage>> {
    TestBed.configureTestingModule({
      imports: [HomePage],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(HomePage);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('shows the loading state while the payload is pending', async () => {
    const client: Partial<CoreClient> = {
      dispatchData: (() => new Promise(() => undefined)) as CoreClient['dispatchData'],
    };
    const fixture = await render(client);
    expect(fixture.nativeElement.textContent).toContain('Setting the table…');
  });

  it('shows the v4 error copy when the fetch fails', async () => {
    const fixture = await render(stubClient(() => new Error('boom')));
    expect(fixture.nativeElement.textContent).toContain(
      'The home parlour could not be readied just now.',
    );
  });

  it('renders the greeting, quick actions, and all three sections from the §1 payload', async () => {
    const fixture = await render(homeClient(homeData()));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Welcome back,');
    expect(text).toContain('Friday');
    expect(text).toContain('What would you like to do today?');
    expect(text).toContain('Recent Chats');
    expect(text).toContain('Active Projects');
    expect(text).toContain('Characters');
    // The section rows.
    expect(text).toContain('A Chat');
    expect(text).toContain('Airship Saga');
    expect(text).toContain('Lorian');
  });

  it('renders rows sort/slice-blind — server order, no client re-sort', async () => {
    // Payload deliberately NOT name-sorted; the screen must render as sent.
    const fixture = await render(
      homeClient(
        homeData({
          recentChats: [chat({ id: 'c2', title: 'Zeppelin' }), chat({ id: 'c3', title: 'Abacus' })],
          projects: [
            project({ id: 'p2', name: 'Zamboni' }),
            project({ id: 'p3', name: 'Aqueduct' }),
          ],
          characters: [
            character({ id: 'ch2', name: 'Zed' }),
            character({ id: 'ch3', name: 'Abe' }),
          ],
        }),
      ),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text.indexOf('Zeppelin')).toBeGreaterThan(-1);
    expect(text.indexOf('Zeppelin')).toBeLessThan(text.indexOf('Abacus'));
    expect(text.indexOf('Zamboni')).toBeLessThan(text.indexOf('Aqueduct'));
    expect(text.indexOf('Zed')).toBeLessThan(text.indexOf('Abe'));
  });

  it('renders every empty-section arm (v4 copy)', async () => {
    const fixture = await render(
      homeClient(homeData({ recentChats: [], projects: [], characters: [], lastChatId: null })),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('No chats yet');
    expect(text).toContain('Start your first chat');
    expect(text).toContain('No projects yet');
    expect(text).toContain('Create a project to organize your chats');
    expect(text).toContain('No favorite characters');
    expect(text).toContain('Mark some as favorites');
  });

  it('disables Continue Last when lastChatId is null (v4 QuickActionsRow)', async () => {
    const fixture = await render(homeClient(homeData({ lastChatId: null })));
    const disabled = fixture.nativeElement.querySelector(
      'button[title="No recent chats"]',
    ) as HTMLButtonElement;
    expect(disabled).not.toBeNull();
    expect(disabled.disabled).toBe(true);
  });

  it('links Continue Last to /salon/{lastChatId} when present', async () => {
    const fixture = await render(homeClient(homeData({ lastChatId: 'abc-123' })));
    const links = Array.from(
      fixture.nativeElement.querySelectorAll('.qt-quick-actions a'),
    ) as HTMLAnchorElement[];
    const hrefs = links.map((a) => a.getAttribute('href'));
    expect(hrefs).toContain('/salon/abc-123');
    // The other two navigation actions.
    expect(hrefs).toContain('/salon/new');
    expect(hrefs).toContain('/salon/new?autonomous=1');
  });

  it('links the Generate Image action to /generate-image (v4 QuickActionsRow :90-98)', async () => {
    const fixture = await render(homeClient(homeData()));
    const links = Array.from(
      fixture.nativeElement.querySelectorAll('.qt-quick-actions a'),
    ) as HTMLAnchorElement[];
    const generate = links.find((a) => a.textContent!.includes('Generate Image'));
    expect(generate).toBeDefined();
    expect(generate!.getAttribute('href')).toBe('/generate-image');
  });

  it('marks dangerous chats with the * marker (v4 RecentChatItem)', async () => {
    const fixture = await render(
      homeClient(homeData({ recentChats: [chat({ isDangerousChat: true })] })),
    );
    const marker = fixture.nativeElement.querySelector('span[title="Flagged as dangerous"]');
    expect(marker).not.toBeNull();
    expect(marker!.textContent).toBe('*');
  });

  it('prefers the story background strip over the avatar stack (v4 RecentChatItem)', async () => {
    const withBg = await render(
      homeClient(
        homeData({
          recentChats: [chat({ storyBackgroundUrl: 'store-images/bg.png' })],
        }),
      ),
    );
    const img = withBg.nativeElement.querySelector(
      'qt-recent-chat-item img',
    ) as HTMLImageElement | null;
    expect(img).not.toBeNull();
    // §2: resolved through normalizeAvatarSrc (leading slash), not inline-built.
    expect(img!.getAttribute('src')).toBe('/store-images/bg.png');
    expect(withBg.nativeElement.querySelector('qt-recent-chat-item qt-avatar-stack')).toBeNull();

    TestBed.resetTestingModule();
    const withoutBg = await render(homeClient(homeData({ recentChats: [chat()] })));
    expect(
      withoutBg.nativeElement.querySelector('qt-recent-chat-item qt-avatar-stack'),
    ).not.toBeNull();
  });

  it('joins participant names in display order (v4 characterNames)', async () => {
    const fixture = await render(
      homeClient(
        homeData({
          recentChats: [
            chat({
              participants: [
                participant('b', 'Beta', 2),
                participant('a', 'Alpha', 1),
                participant('c', 'Gamma', 3),
              ],
            }),
          ],
        }),
      ),
    );
    expect(fixture.nativeElement.textContent).toContain('Alpha + Beta + Gamma');
  });

  it("falls back to 'Unknown' when a chat has no active character participants", async () => {
    const fixture = await render(homeClient(homeData({ recentChats: [chat()] })));
    expect(fixture.nativeElement.textContent).toContain('Unknown');
  });

  it('pluralizes the project chat count (v4 ProjectItem)', async () => {
    const fixture = await render(
      homeClient(
        homeData({
          projects: [
            project({ id: 'p1', name: 'One', chatCount: 1 }),
            project({ id: 'p2', name: 'Many', chatCount: 4 }),
          ],
        }),
      ),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('1 chat');
    expect(text).toContain('4 chats');
  });

  it('shows the provider badge from the resolved connection profile (v4 CharacterCard)', async () => {
    const fixture = await render(
      homeClient(
        homeData({
          characters: [character({ defaultConnectionProfileId: 'cp1' })],
        }),
        [{ id: 'cp1', name: 'GROK/grok-4', provider: 'GROK', modelName: 'grok-4' }],
      ),
    );
    const badge = fixture.nativeElement.querySelector('span[title="GROK/grok-4: grok-4"]');
    expect(badge).not.toBeNull();
    // The compact display strips the provider prefix.
    expect(badge!.textContent).toContain('grok-4');
    expect(badge!.textContent).not.toContain('GROK/');
  });

  it('renders no badge when the character has no default profile', async () => {
    const fixture = await render(
      homeClient(homeData(), [
        { id: 'cp1', name: 'GROK/grok-4', provider: 'GROK', modelName: 'grok-4' },
      ]),
    );
    expect(fixture.nativeElement.querySelector('qt-home-character-card .qt-text-xs')).toBeNull();
  });
});

describe('HomeCharacterCard click semantics (dogfood finding #4 / P4.6f pattern)', () => {
  afterEach(() => TestBed.resetTestingModule());

  async function renderCard(): Promise<{
    fixture: ComponentFixture<HomeCharacterCard>;
    navigate: ReturnType<typeof vi.fn>;
  }> {
    TestBed.configureTestingModule({
      imports: [HomeCharacterCard],
      providers: [provideRouter([])],
    });
    const fixture = TestBed.createComponent(HomeCharacterCard);
    fixture.componentRef.setInput('character', character());
    const router = TestBed.inject(Router);
    const navigate = vi.fn().mockResolvedValue(true);
    router.navigate = navigate as unknown as Router['navigate'];
    // RouterLink's own navigation must be inert too — a real navigateByUrl
    // settles after TestBed teardown (NG0205 unhandled rejection).
    router.navigateByUrl = vi.fn().mockResolvedValue(true) as unknown as Router['navigateByUrl'];
    fixture.detectChanges();
    await settle(fixture);
    return { fixture, navigate };
  }

  it('navigates to /characters/:id from a card-body click', async () => {
    const { fixture, navigate } = await renderCard();
    (fixture.nativeElement.querySelector('.qt-character-card') as HTMLElement).click();
    expect(navigate).toHaveBeenCalledWith(['/characters', 'ch1']);
  });

  it('does NOT card-navigate from a click landing on an inner link', async () => {
    const { fixture, navigate } = await renderCard();
    // The name <p> sits inside the avatar+name <a>; closest('a') must swallow it.
    (fixture.nativeElement.querySelector('a .qt-card-title') as HTMLElement).click();
    expect(navigate).not.toHaveBeenCalledWith(['/characters', 'ch1']);
  });

  it('routes the Chat button to /salon/new?characterId= (the P4.6q precedent, not v4 NewChatModal)', async () => {
    const { fixture, navigate } = await renderCard();
    (
      fixture.nativeElement.querySelector('button[title="Start a chat with Lorian"]') as HTMLElement
    ).click();
    expect(navigate).toHaveBeenCalledWith(['/salon/new'], {
      queryParams: { characterId: 'ch1' },
    });
    // The guard swallowed the card navigation — only the Chat navigation fired.
    expect(navigate).toHaveBeenCalledTimes(1);
  });
});

describe('formatMessageTime (transcribed from v4 lib/format-time.ts:6-37)', () => {
  afterEach(() => vi.useRealTimers());

  it('pins the v4 branches: just now / Xm ago / Xh ago / dated', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-16T15:00:00'));
    expect(formatMessageTime('2026-07-16T14:59:40')).toBe('just now');
    expect(formatMessageTime('2026-07-16T14:15:00')).toBe('45m ago');
    expect(formatMessageTime('2026-07-16T09:00:00')).toBe('6h ago');
    // Older than today: dated, year only when it differs (en-US).
    expect(formatMessageTime('2026-07-01T09:00:00')).toBe('Jul 1');
    expect(formatMessageTime('2025-12-31T09:00:00')).toBe('Dec 31, 2025');
  });
});

describe('profileDisplayName (transcribed from v4 components/homepage/CharacterCard.tsx)', () => {
  it('strips the provider prefix in both cases and falls back name → modelName → empty', () => {
    expect(
      profileDisplayName({ name: 'GROK/grok-4.20', provider: 'GROK', modelName: 'grok-4.20' }),
    ).toBe('grok-4.20');
    expect(
      profileDisplayName({ name: 'grok/grok-4.20', provider: 'GROK', modelName: 'grok-4.20' }),
    ).toBe('grok-4.20');
    expect(profileDisplayName({ name: 'My Profile', provider: 'GROK', modelName: 'grok-4' })).toBe(
      'My Profile',
    );
    expect(profileDisplayName({ provider: 'GROK', modelName: 'grok-4' })).toBe('grok-4');
    expect(profileDisplayName({ provider: 'GROK' })).toBe('');
    expect(profileDisplayName(null)).toBe('');
  });
});
