import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CharacterListItem } from '../../../core/core-contract';
import { WORKSPACE_TAB_ID } from '../../../workspace/workspace-contract';
import { CharactersList, sortCharacters } from './characters-list';

function character(over: Partial<CharacterListItem>): CharacterListItem {
  return {
    id: 'c1',
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

/** A CoreClient stub whose `dispatchData` routes by request `type`. */
function stubClient(
  characters: CharacterListItem[],
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch?.(req);
      switch (req.type) {
        case 'characterList':
          return { characters };
        case 'connectionProfileList':
          return { profiles: [] };
        default:
          return {};
      }
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CharactersList>> {
  TestBed.configureTestingModule({
    imports: [CharactersList],
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(CharactersList);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('sortCharacters', () => {
  it('orders NPCs last, favorites first, then chat count desc, then name A–Z', () => {
    const list = [
      character({ id: 'npc', name: 'Aardvark', npc: true }),
      character({ id: 'fav', name: 'Zed', isFavorite: true, _count: { chats: 1 } }),
      character({ id: 'busy', name: 'Mabel', _count: { chats: 9 } }),
      character({ id: 'a', name: 'Alice', _count: { chats: 2 } }),
      character({ id: 'b', name: 'bob', _count: { chats: 2 } }),
    ];
    const ordered = sortCharacters(list).map((c) => c.id);
    // Favorite first, then non-favorites by chat count desc (busy=9), then the
    // chats=2 pair alphabetically case-insensitive (Alice < bob), NPC last.
    expect(ordered).toEqual(['fav', 'busy', 'a', 'b', 'npc']);
  });
});

describe('CharactersList', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders the v4-verbatim heading and the Create Character action', async () => {
    const fixture = await render(stubClient([character({})]));
    const text = fixture.nativeElement.textContent as string;
    expect(fixture.nativeElement.querySelector('h1').textContent).toContain('Characters');
    expect(text).toContain('Create Character');
    expect(text).toContain('Import from SillyTavern');
  });

  it('keeps Summon From Lore disabled but Reset Built-in Characters live (P4.6r)', async () => {
    const fixture = await render(stubClient([character({})]));
    const buttons = Array.from(
      fixture.nativeElement.querySelectorAll('button'),
    ) as HTMLButtonElement[];
    const summon = buttons.find((b) => b.textContent?.includes('Summon From Lore'));
    const reset = buttons.find((b) => b.textContent?.includes('Reset Built-in Characters'));
    expect(summon?.disabled).toBe(true);
    // Reset is now wired to the live WEB-EDGE ?action=reset-builtins route.
    expect(reset?.disabled).toBe(false);
  });

  it('shows the empty state when there are no characters', async () => {
    const fixture = await render(stubClient([]));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('No characters yet');
    expect(text).toContain('Create your first character');
  });

  it('renders a card per character with name, title, and chat count', async () => {
    const fixture = await render(
      stubClient([
        character({ id: 'a', name: 'Tea with Bertie', title: 'The Valet', _count: { chats: 5 } }),
      ]),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Tea with Bertie');
    expect(text).toContain('The Valet');
    expect(text).toContain('5 chats');
  });

  it('navigates to the detail view from a click anywhere on the card body (v4 handleCardClick)', async () => {
    const fixture = await render(
      stubClient([character({ id: 'a', name: 'Bertie', description: 'A gentleman of leisure.' })]),
    );
    const router = TestBed.inject(Router);
    const navigated: unknown[][] = [];
    router.navigate = ((commands: unknown[]) => {
      navigated.push(commands);
      return Promise.resolve(true);
    }) as Router['navigate'];

    // A click on the description paragraph — NOT the name link — navigates.
    const description = Array.from(fixture.nativeElement.querySelectorAll('p')).find((p) =>
      (p as HTMLParagraphElement).textContent?.includes('gentleman of leisure'),
    ) as HTMLParagraphElement;
    expect(description).toBeTruthy();
    description.click();
    expect(navigated).toEqual([['/characters', 'a']]);

    // A click on an inner interactive element (the favorite star) does NOT.
    const starButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === '☆',
    ) as HTMLButtonElement;
    starButton.click();
    expect(navigated.length).toBe(1);
  });

  it('exports a character as JSON: dispatches characterExport and downloads <name>.json', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const client: Partial<CoreClient> = {
      dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
        seen.push(req);
        switch (req.type) {
          case 'characterList':
            return { characters: [character({ id: 'x', name: 'Jeeves' })] };
          case 'connectionProfileList':
            return { profiles: [] };
          case 'characterExport':
            return { spec: 'chara_card_v2', spec_version: '2.0', data: { name: 'Jeeves' } };
          default:
            return {};
        }
      }) as CoreClient['dispatchData'],
    };
    const fixture = await render(client);

    const origCreate = URL.createObjectURL;
    const origRevoke = URL.revokeObjectURL;
    const createUrl = vi.fn(() => 'blob:mock');
    (URL as unknown as { createObjectURL: unknown }).createObjectURL = createUrl;
    (URL as unknown as { revokeObjectURL: unknown }).revokeObjectURL = vi.fn();
    let downloadName = '';
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      downloadName = this.download;
    });

    try {
      const exportBtn = fixture.nativeElement.querySelector(
        'button[title="Export as SillyTavern JSON"]',
      ) as HTMLButtonElement;
      expect(exportBtn).toBeTruthy();
      exportBtn.click();
      await new Promise((r) => setTimeout(r, 0));

      expect(seen.find((r) => r.type === 'characterExport')).toMatchObject({
        type: 'characterExport',
        characterId: 'x',
        format: 'json',
      });
      expect(createUrl).toHaveBeenCalled();
      expect(downloadName).toBe('Jeeves.json');
    } finally {
      (URL as unknown as { createObjectURL: unknown }).createObjectURL = origCreate;
      (URL as unknown as { revokeObjectURL: unknown }).revokeObjectURL = origRevoke;
    }
  });

  it('optimistically flips the favorite star and dispatches characterFavorite', async () => {
    const seen: string[] = [];
    const fixture = await render(
      stubClient([character({ id: 'a', name: 'Bertie', isFavorite: false })], (req) => {
        seen.push(req.type);
      }),
    );
    const starButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === '☆',
    ) as HTMLButtonElement;
    expect(starButton).toBeTruthy();
    starButton.click();
    // The optimistic `setQueryData` propagates to the query signal on the next
    // microtask; flush it before reading the DOM.
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('⭐');
    expect(seen).toContain('characterFavorite');
  });
});

/**
 * In-tab drill (p4.9j2, v4 `AuroraView` `selectedCharacterId` / `selectedGroupId`):
 * hosted as a workspace tab, a card drills IN PLACE (renders the detail /
 * group-editor embedded); the detail's back restores the list.
 */
describe('CharactersList (in-tab drill)', () => {
  function drillClient(): Partial<CoreClient> {
    return {
      dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
        switch (req.type) {
          case 'characterList':
            return { characters: [character({ id: 'c1', name: 'Bertie' })] };
          case 'connectionProfileList':
            return { profiles: [] };
          case 'groupList':
            return {
              groups: [
                {
                  id: 'g1',
                  name: 'The Drones',
                  description: null,
                  color: null,
                  icon: null,
                  createdAt: '2024-01-01T00:00:00.000Z',
                  updatedAt: '2024-01-01T00:00:00.000Z',
                  _count: { members: 0, documentStores: 0 },
                },
              ],
            };
          case 'characterGet':
            return {
              character: {
                ...character({ id: 'c1', name: 'Bertie', title: 'The Valet' }),
                identity: 'A gentleman.',
                manifesto: null,
                personality: null,
                firstMessage: null,
                exampleDialogues: null,
                physicalDescription: null,
                defaultConnectionProfileId: null,
                defaultAgentModeEnabled: null,
                defaultHelpToolsEnabled: null,
                canDressThemselves: null,
                canCreateOutfits: null,
                aliases: [],
                pronouns: null,
                characterDocumentMountPointId: null,
              },
            };
          case 'characterStats':
            return {
              stats: {
                memories: 0,
                conversations: 0,
                wardrobeItems: 0,
                photos: 0,
                scenarios: 0,
                knowledge: 0,
                core: 0,
                characterFiles: 0,
                characterFilesTotal: 0,
              },
              groups: [],
            };
          case 'characterDefaultPartner':
            return { partnerId: null };
          case 'groupGet':
            return {
              group: {
                id: 'g1',
                name: 'The Drones',
                description: null,
                color: null,
                icon: null,
                createdAt: '2024-01-01T00:00:00.000Z',
                updatedAt: '2024-01-01T00:00:00.000Z',
              },
            };
          default:
            return {};
        }
      }) as CoreClient['dispatchData'],
    };
  }

  async function renderTab(): Promise<ComponentFixture<CharactersList>> {
    TestBed.configureTestingModule({
      imports: [CharactersList],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: drillClient() },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-a' },
      ],
    });
    const fixture = TestBed.createComponent(CharactersList);
    fixture.detectChanges();
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return fixture;
  }

  it('a card click drills into the character detail in place; back restores the list', async () => {
    const fixture = await renderTab();
    const card = fixture.nativeElement.querySelector('.character-card') as HTMLElement;
    card.click();
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(fixture.nativeElement.querySelector('qt-character-detail')).toBeTruthy();

    const back = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.includes('Back to Characters'),
    ) as HTMLButtonElement;
    expect(back).toBeTruthy();
    back.click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('qt-character-detail')).toBeNull();
    expect(fixture.nativeElement.textContent).toContain('Create Character');
  });

  it('a group Edit drills into the group editor in place', async () => {
    const fixture = await renderTab();
    const edit = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Edit',
    ) as HTMLButtonElement;
    expect(edit).toBeTruthy();
    edit.click();
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(fixture.nativeElement.querySelector('qt-group-editor')).toBeTruthy();
  });
});
