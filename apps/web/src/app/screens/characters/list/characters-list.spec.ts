import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CharacterListItem } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import {
  WORKSPACE_HANDLE,
  WORKSPACE_TAB_ID,
  type WorkspaceHandle,
} from '../../../workspace/workspace-contract';
import { CharactersList, formatCharacterDeleteSuccess, sortCharacters } from './characters-list';

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

describe('formatCharacterDeleteSuccess (v4 AuroraView.tsx:188-199)', () => {
  it('names only the character when nothing cascaded', () => {
    expect(formatCharacterDeleteSuccess({})).toBe('Character deleted');
  });

  it('pluralizes each cascaded count independently', () => {
    expect(
      formatCharacterDeleteSuccess({ deletedChats: 1, deletedImages: 3, deletedMemories: 1 }),
    ).toBe('Character deleted. 1 chat deleted. 3 images deleted. 1 memory deleted');
    expect(
      formatCharacterDeleteSuccess({ deletedChats: 2, deletedImages: 0, deletedMemories: 5 }),
    ).toBe('Character deleted. 2 chats deleted. 5 memories deleted');
  });
});

/** Toast pins (P4.29): the census's remaining OPEN `AuroraView.tsx` arms. */
describe('CharactersList toasts (v4 AuroraView.tsx)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  function toasts(): { type: string; message: string }[] {
    return TestBed.inject(ToastService)
      .toasts()
      .map((t) => ({ type: t.type, message: t.message }));
  }

  function findButton(fixture: ComponentFixture<CharactersList>, title: string): HTMLButtonElement {
    return fixture.nativeElement.querySelector(`button[title="${title}"]`);
  }

  it('reverts and toasts the v4 sentence when the favorite toggle fails', async () => {
    const fixture = await render(
      stubClient([character({ id: 'a', name: 'Bertie', isFavorite: false })], (req) => {
        if (req.type === 'characterFavorite') {
          throw new Error('Failed to toggle favorite');
        }
      }),
    );
    findButton(fixture, 'Add to favorites').click();
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to toggle favorite' }]);
    expect(fixture.nativeElement.textContent).toContain('☆');
  });

  it('toasts the v4 sentence when the Carina toggle fails', async () => {
    const fixture = await render(
      stubClient([character({ id: 'a', name: 'Bertie', canBeCarina: false })], (req) => {
        if (req.type === 'characterToggleCarina') {
          throw new Error('Failed to toggle Carina eligibility');
        }
      }),
    );
    findButton(fixture, 'Enable Carina answers (@-queries)').click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to toggle Carina eligibility' }]);
  });

  it('toasts the v4 sentence when the controlled-by toggle fails', async () => {
    const fixture = await render(
      stubClient([character({ id: 'a', name: 'Bertie', controlledBy: 'llm' })], (req) => {
        if (req.type === 'characterToggleControlledBy') {
          throw new Error('Failed to toggle controlled-by');
        }
      }),
    );
    findButton(fixture, 'Switch to user control').click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to toggle controlled-by' }]);
  });

  function deleteClient(
    onDelete: (req: { type: string; [k: string]: unknown }) => Promise<unknown>,
  ): Partial<CoreClient> {
    return {
      dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
        switch (req.type) {
          case 'characterList':
            return { characters: [character({ id: 'a', name: 'Bertie' })] };
          case 'connectionProfileList':
            return { profiles: [] };
          case 'characterCascadePreview':
            return {
              characterId: 'a',
              characterName: 'Bertie',
              exclusiveChats: [],
              exclusiveCharacterImageCount: 0,
              exclusiveChatImageCount: 0,
              totalExclusiveImageCount: 0,
              memoryCount: 0,
            };
          case 'characterDelete':
            return onDelete(req);
          default:
            return {};
        }
      }) as CoreClient['dispatchData'],
    };
  }

  async function confirmDelete(
    client: Partial<CoreClient>,
  ): Promise<ComponentFixture<CharactersList>> {
    const fixture = await render(client);
    findButton(fixture, 'Delete this character').click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    const confirm = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Delete Character',
    ) as HTMLButtonElement;
    expect(confirm).toBeTruthy();
    confirm.click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return fixture;
  }

  it('deletes with a pluralized success toast and refetches (v4 handleDeleteConfirm)', async () => {
    const fixture = await confirmDelete(
      deleteClient(async () => ({
        success: true,
        deletedChats: 2,
        deletedImages: 0,
        deletedMemories: 1,
      })),
    );
    expect(toasts()).toEqual([
      { type: 'success', message: 'Character deleted. 2 chats deleted. 1 memory deleted' },
    ]);
    expect(fixture.nativeElement.querySelector('qt-character-delete-dialog')).toBeNull();
  });

  it('toasts a failed delete and keeps the dialog open (v4 :201-203)', async () => {
    const fixture = await confirmDelete(
      deleteClient(async () => {
        throw new Error('the registry has misplaced the file');
      }),
    );
    expect(toasts()).toEqual([{ type: 'error', message: 'the registry has misplaced the file' }]);
    expect(fixture.nativeElement.querySelector('qt-character-delete-dialog')).toBeTruthy();
  });

  it('closes the reset dialog and refetches on a successful reset (dialog toasts its own outcome)', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({ ok: true }), { status: 200 })),
    );
    const fixture = await render(stubClient([character({ id: 'a', name: 'Bertie' })]));

    findButton(fixture, 'Reset built-in characters to first-run defaults').click();
    fixture.detectChanges();
    const confirm = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Reset Built-ins',
    ) as HTMLButtonElement;
    expect(confirm).toBeTruthy();
    confirm.click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    // The dialog itself raises the "Built-in characters reset successfully."
    // toast (pinned in reset-builtins-dialog.spec.ts); here we only prove the
    // list-level wiring: the dialog closes and the roster refetches.
    expect(fixture.nativeElement.querySelector('qt-reset-builtins-dialog')).toBeNull();
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

/**
 * In-tab Create-Character + Chat arms (p4.9j3 items 2 & 3): hosted as a
 * workspace tab, Create-Character opens a `character-new` tab (no route) and the
 * card's Chat action drills into the detail with its start-chat auto-open
 * flagged (which lands on `/salon/new?characterId=`).
 */
describe('CharactersList (in-tab Create + Chat arms)', () => {
  function fullClient(): Partial<CoreClient> {
    return {
      dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
        switch (req.type) {
          case 'characterList':
            return { characters: [character({ id: 'c1', name: 'Bertie' })] };
          case 'connectionProfileList':
            return { profiles: [] };
          case 'groupList':
            return { groups: [] };
          case 'characterGet':
            return {
              character: {
                ...character({ id: 'c1', name: 'Bertie' }),
                identity: 'A gentleman.',
                manifesto: null,
                personality: null,
                firstMessage: null,
                exampleDialogues: null,
                physicalDescription: null,
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
          default:
            return {};
        }
      }) as CoreClient['dispatchData'],
    };
  }

  function makeHandle(): { handle: WorkspaceHandle; openTab: ReturnType<typeof vi.fn> } {
    const openTab = vi.fn(() => 'new-tab');
    return { openTab, handle: { openTab, closeTab: vi.fn(), refreshTab: vi.fn() } };
  }

  async function renderTab(h: WorkspaceHandle): Promise<ComponentFixture<CharactersList>> {
    TestBed.configureTestingModule({
      imports: [CharactersList],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: fullClient() },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-a' },
        { provide: WORKSPACE_HANDLE, useValue: h },
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

  it('Create Character opens a character-new tab and does NOT route', async () => {
    const { handle: h, openTab } = makeHandle();
    const fixture = await renderTab(h);
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate');
    const create = [...fixture.nativeElement.querySelectorAll('a')].find(
      (a: HTMLAnchorElement) => a.textContent?.trim() === 'Create Character',
    ) as HTMLAnchorElement;
    expect(create).toBeTruthy();
    // The routerLink is nulled in tab mode.
    expect(create.getAttribute('href')).toBeNull();
    create.click();
    expect(openTab).toHaveBeenCalledWith('character-new');
    expect(navigate).not.toHaveBeenCalled();
  });

  it('the card Chat action drills and auto-opens the chat (/salon/new?characterId=)', async () => {
    const { handle: h } = makeHandle();
    const fixture = await renderTab(h);
    const navigate = vi.fn(async () => true);
    vi.spyOn(TestBed.inject(Router), 'navigate').mockImplementation(navigate as never);

    const chat = fixture.nativeElement.querySelector(
      '.character-card__action--chat',
    ) as HTMLAnchorElement;
    expect(chat).toBeTruthy();
    chat.click();
    for (let i = 0; i < 10; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    // Drilled in place…
    expect(fixture.nativeElement.querySelector('qt-character-detail')).toBeTruthy();
    // …and the start-chat auto-open fired to the v5 divergence entry.
    expect(navigate).toHaveBeenCalledWith(['/salon/new'], { queryParams: { characterId: 'c1' } });
  });
});

// ===========================================================================
// P4.D64 — the archive tombstone on the roster (v4 `AuroraView.tsx`, `d553f72a`)
// ===========================================================================

describe('sortCharacters — archived last (P4.D64 rule 0)', () => {
  it('sinks archived characters below everything else, outranking every other key', () => {
    // Rule 0 sits AHEAD of NPC / favorite / chat-count / name, so the most
    // favoured, busiest character still lands last once archived.
    const list = [
      character({ id: 'arch-fav', name: 'Aaron', isFavorite: true, _count: { chats: 99 } }),
      character({ id: 'npc', name: 'Zeno', npc: true }),
      character({ id: 'plain', name: 'Mabel' }),
    ];
    list[0].archivedAt = '2026-08-01T00:00:00.000Z';
    expect(sortCharacters(list).map((c) => c.id)).toEqual(['plain', 'npc', 'arch-fav']);
  });

  it('orders two archived characters by the remaining keys', () => {
    const a = character({ id: 'a', name: 'Zeb', isFavorite: true });
    const b = character({ id: 'b', name: 'Abe' });
    a.archivedAt = '2026-08-01T00:00:00.000Z';
    b.archivedAt = '2026-08-02T00:00:00.000Z';
    // Both archived ⇒ rule 0 is a tie and the favorite wins (rule 2).
    expect(sortCharacters([b, a]).map((c) => c.id)).toEqual(['a', 'b']);
  });
});

describe('CharactersList — the Show Archived toggle (P4.D64)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  /** The toolbar toggle, located by its v4 label in either state. */
  function toggle(fixture: ComponentFixture<CharactersList>): HTMLButtonElement {
    const buttons = Array.from(
      fixture.nativeElement.querySelectorAll('button'),
    ) as HTMLButtonElement[];
    const found = buttons.find(
      (b) => b.textContent?.includes('Show Archived') || b.textContent?.includes('Hide Archived'),
    );
    expect(found).toBeTruthy();
    return found!;
  }

  it('is FIRST in the toolbar row, labelled Show Archived, with v4 title + inactive styling', async () => {
    const fixture = await render(stubClient([character({})]));
    const row = fixture.nativeElement.querySelector('.flex.flex-wrap.gap-3') as HTMLElement;
    const first = row.querySelector('button') as HTMLButtonElement;
    expect(first.textContent).toContain('Show Archived');
    expect(first.getAttribute('title')).toBe('Show characters resting in the archive');
    expect(first.className).toContain('qt-bg-muted/70');
    expect(first.className).not.toContain('qt-bg-primary/20');
  });

  it('flips its label, title and styling when switched on', async () => {
    const fixture = await render(stubClient([character({})]));
    toggle(fixture).click();
    fixture.detectChanges();
    const on = toggle(fixture);
    expect(on.textContent).toContain('Hide Archived');
    expect(on.getAttribute('title')).toBe('Tuck the archived characters back out of sight');
    expect(on.className).toContain('qt-bg-primary/20');
    expect(on.className).toContain('qt-text-primary');
  });

  it('refetches with `archived: include` under a DISTINCT cache key, and reverts', async () => {
    // The mutation-killer: a shared cache key would serve the first fetch's rows
    // for both states, so the second dispatch would never happen and the
    // archived row would never appear. Assert BOTH the wire and the rows.
    const seen: Array<Record<string, unknown>> = [];
    const live = character({ id: 'live', name: 'Bertie' });
    const tomb = character({ id: 'tomb', name: 'Marchpane' });
    tomb.archivedAt = '2026-08-01T00:00:00.000Z';
    const client: Partial<CoreClient> = {
      dispatchData: (async (req: Record<string, unknown>) => {
        seen.push(req);
        if (req['type'] === 'characterList') {
          return { characters: req['archived'] === 'include' ? [live, tomb] : [live] };
        }
        if (req['type'] === 'connectionProfileList') return { profiles: [] };
        return {};
      }) as CoreClient['dispatchData'],
    };
    const fixture = await render(client);
    expect(fixture.nativeElement.textContent).not.toContain('Marchpane');

    toggle(fixture).click();
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(seen.filter((r) => r['type'] === 'characterList' && r['archived'] === 'include')).toHaveLength(1);
    expect(fixture.nativeElement.textContent).toContain('Marchpane');

    toggle(fixture).click();
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(fixture.nativeElement.textContent).not.toContain('Marchpane');
    // The OFF fetch never carries the param — absent IS the exclude default.
    expect(seen.filter((r) => r['type'] === 'characterList' && !('archived' in r)).length).toBeGreaterThan(0);
  });

  it('widens delete invalidation to the characters PREFIX so the other variant refreshes', async () => {
    const client: Partial<CoreClient> = {
      dispatchData: (async (req: Record<string, unknown>) => {
        switch (req['type']) {
          case 'characterList':
            return { characters: [character({ id: 'c1', name: 'Bertie' })] };
          case 'connectionProfileList':
            return { profiles: [] };
          case 'characterCascadePreview':
            return {
              characterId: 'c1',
              characterName: 'Bertie',
              exclusiveChats: [],
              exclusiveCharacterImageCount: 0,
              exclusiveChatImageCount: 0,
              totalExclusiveImageCount: 0,
              memoryCount: 0,
            };
          default:
            return { success: true };
        }
      }) as CoreClient['dispatchData'],
    };
    const fixture = await render(client);
    const qc = TestBed.inject(QueryClient);
    const invalidate = vi.spyOn(qc, 'invalidateQueries');
    const del = fixture.nativeElement.querySelector('.qt-button-destructive') as HTMLButtonElement;
    del.click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    const confirm = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Delete Character',
    ) as HTMLButtonElement;
    expect(confirm).toBeTruthy();
    confirm.click();
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    // v4 `:72-73` — `characters.all`, not `characters.list`.
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['characters'] });
  });
});

describe('CharacterCard — the archived tombstone card (P4.D64)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  async function archivedCard(): Promise<ComponentFixture<CharactersList>> {
    const tomb = character({ id: 'tomb', name: 'Marchpane', isFavorite: true });
    tomb.archivedAt = '2026-08-01T00:00:00.000Z';
    return render(stubClient([tomb]));
  }

  it('badges the name with Archived and dates the tooltip', async () => {
    const fixture = await archivedCard();
    const badge = fixture.nativeElement.querySelector('.qt-badge') as HTMLElement;
    expect(badge.textContent?.trim()).toBe('Archived');
    expect(badge.getAttribute('title')).toBe(
      `Resting in the archive since ${new Date('2026-08-01T00:00:00.000Z').toLocaleDateString()}`,
    );
  });

  it('hides the favorite / Carina / controlled-by cluster', async () => {
    const fixture = await archivedCard();
    const text = fixture.nativeElement.textContent as string;
    // The favorite affordance is the only ⭐/☆ on the card.
    expect(text).not.toContain('☆');
    expect(text).not.toContain('⭐');
    expect(fixture.nativeElement.querySelector('qt-icon[name="monitor"]')).toBeNull();
  });

  it('replaces Chat AND both exports with one inert span, keeping Delete', async () => {
    const fixture = await archivedCard();
    const actions = fixture.nativeElement.querySelector('.character-card-actions') as HTMLElement;
    expect(actions.querySelector('.character-card__action--chat')).toBeNull();
    expect(actions.querySelector('qt-icon[name="download"]')).toBeNull();
    expect(actions.querySelector('qt-icon[name="image"]')).toBeNull();
    const span = actions.querySelector('span') as HTMLElement;
    expect(span.textContent?.trim()).toBe('Resting in the archive');
    expect(span.getAttribute('title')).toBe(
      'An archived character neither chats nor exports; open their page to rehydrate them.',
    );
    // Delete survives — an archived character can still be thrown away.
    expect(actions.querySelector('.qt-button-destructive')).toBeTruthy();
  });

  it('leaves a live card\u2019s actions and toggles untouched', async () => {
    const fixture = await render(stubClient([character({ id: 'live', name: 'Bertie' })]));
    const actions = fixture.nativeElement.querySelector('.character-card-actions') as HTMLElement;
    expect(actions.querySelector('.character-card__action--chat')).toBeTruthy();
    expect(actions.querySelector('qt-icon[name="download"]')).toBeTruthy();
    expect(actions.querySelector('qt-icon[name="image"]')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('.qt-badge')).toBeNull();
    expect(fixture.nativeElement.textContent).toContain('☆');
  });
});
