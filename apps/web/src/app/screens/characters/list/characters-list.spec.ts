import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CharacterListItem } from '../../../core/core-contract';
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
  it('renders the v4-verbatim heading and the Create Character action', async () => {
    const fixture = await render(stubClient([character({})]));
    const text = fixture.nativeElement.textContent as string;
    expect(fixture.nativeElement.querySelector('h1').textContent).toContain('Characters');
    expect(text).toContain('Create Character');
    expect(text).toContain('Import from SillyTavern');
  });

  it('disables the Summon From Lore and Reset Built-in Characters affordances', async () => {
    const fixture = await render(stubClient([character({})]));
    const buttons = Array.from(
      fixture.nativeElement.querySelectorAll('button'),
    ) as HTMLButtonElement[];
    const summon = buttons.find((b) => b.textContent?.includes('Summon From Lore'));
    const reset = buttons.find((b) => b.textContent?.includes('Reset Built-in Characters'));
    expect(summon?.disabled).toBe(true);
    expect(reset?.disabled).toBe(true);
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

  it('optimistically flips the favorite star and dispatches characterFavorite', async () => {
    const seen: string[] = [];
    const fixture = await render(
      stubClient([character({ id: 'a', name: 'Bertie', isFavorite: false })], (req) => {
        seen.push(req.type);
      }),
    );
    const starButton = Array.from(
      fixture.nativeElement.querySelectorAll('button'),
    ).find((b) => (b as HTMLButtonElement).textContent?.trim() === '☆') as HTMLButtonElement;
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
