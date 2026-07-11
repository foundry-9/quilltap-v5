import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, Router, convertToParamMap, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { of } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CharacterDetail, CoreRequest } from '../../../core/core-contract';
import { CharacterEdit } from './character-edit';

function character(over: Partial<CharacterDetail> = {}): CharacterDetail {
  return {
    id: 'char-1',
    name: 'Bertie Wooster',
    title: 'The Valet',
    identity: 'A gentleman about town',
    description: 'Affable and a little dim',
    manifesto: 'Never let a pal down',
    personality: 'Cheerful, easily flustered',
    scenarios: [],
    firstMessage: 'I say, old bean!',
    exampleDialogues: '',
    systemPrompt: '',
    systemPrompts: [],
    physicalDescription: null,
    avatarUrl: null,
    defaultImageId: null,
    defaultImage: null,
    defaultConnectionProfileId: null,
    controlledBy: 'llm',
    isFavorite: false,
    canBeCarina: false,
    npc: false,
    defaultAgentModeEnabled: null,
    defaultHelpToolsEnabled: null,
    canDressThemselves: null,
    canCreateOutfits: null,
    defaultTimestampConfig: null,
    defaultScenarioId: null,
    defaultSystemPromptId: null,
    defaultPartnerId: null,
    defaultImageProfileId: null,
    aliases: [],
    pronouns: null,
    characterDocumentMountPointId: null,
    tags: [],
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    _count: { chats: 0 },
    ...over,
  };
}

function stubClient(
  char: CharacterDetail,
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch?.(req);
      switch (req.type) {
        case 'characterGet':
          return { character: char };
        case 'tagList':
          return { tags: [] };
        case 'characterUpdate':
          return { character: char };
        case 'characterCascadePreview':
          return {
            characterId: char.id,
            characterName: char.name,
            exclusiveChats: [],
            exclusiveCharacterImageCount: 0,
            exclusiveChatImageCount: 0,
            totalExclusiveImageCount: 0,
            memoryCount: 0,
          };
        case 'characterDelete':
          return { success: true, deletedChats: 0, deletedImages: 0, deletedMemories: 0 };
        default:
          return {};
      }
    }) as CoreClient['dispatchData'],
  } as Partial<CoreClient>;
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CharacterEdit>> {
  TestBed.configureTestingModule({
    imports: [CharacterEdit],
    providers: [
      // A real (if trivial) route so `router.navigate` resolves instead of
      // rejecting with NG04002 (no match) — CharacterEdit itself is a fine
      // stand-in target, its content is irrelevant to these assertions.
      provideRouter([{ path: '**', component: CharacterEdit }]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
      {
        provide: ActivatedRoute,
        useValue: {
          paramMap: of(convertToParamMap({ id: 'char-1' })),
          queryParamMap: of(convertToParamMap({})),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(CharacterEdit);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function findByText(
  fixture: ComponentFixture<CharacterEdit>,
  tag: string,
  text: string,
): HTMLElement | undefined {
  return Array.from(fixture.nativeElement.querySelectorAll(tag)).find(
    (el) => (el as HTMLElement).textContent?.trim() === text,
  ) as HTMLElement | undefined;
}

describe('CharacterEdit', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders the header with the character name and the Details tab by default', async () => {
    const fixture = await render(stubClient(character()));
    expect(fixture.nativeElement.textContent).toContain('Edit: Bertie Wooster');
    expect(fixture.nativeElement.querySelector('#name')).toBeTruthy();
  });

  it('the dirty guard confirms before navigating away on Cancel', async () => {
    const fixture = await render(stubClient(character()));
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    const nameInput = fixture.nativeElement.querySelector('#name') as HTMLInputElement;
    nameInput.value = 'Reginald Jeeves';
    nameInput.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const cancelButton = findByText(fixture, 'button', 'Cancel') as HTMLButtonElement;
    cancelButton.click();
    fixture.detectChanges();

    expect(confirmSpy).toHaveBeenCalled();
    expect(navigateSpy).not.toHaveBeenCalled();
  });

  it('Cancel navigates away without confirming when the form is unchanged', async () => {
    const fixture = await render(stubClient(character()));
    const confirmSpy = vi.spyOn(window, 'confirm');
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    const cancelButton = findByText(fixture, 'button', 'Cancel') as HTMLButtonElement;
    cancelButton.click();
    fixture.detectChanges();

    expect(confirmSpy).not.toHaveBeenCalled();
    expect(navigateSpy).toHaveBeenCalledWith(['/characters', 'char-1']);
  });

  it('Delete Character opens the cascade dialog, dispatches characterDelete, and navigates to the roster', async () => {
    const seen: CoreRequest[] = [];
    const fixture = await render(
      stubClient(character(), (req) => seen.push(req as unknown as CoreRequest)),
    );
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    // Open the delete dialog from the edit-view danger zone.
    const trigger = fixture.nativeElement.querySelector(
      '.mt-6 button.qt-button-destructive',
    ) as HTMLButtonElement;
    expect(trigger.textContent?.trim()).toBe('Delete Character');
    trigger.click();
    fixture.detectChanges();
    // Let the cascade-preview query settle so the confirm button enables.
    for (let i = 0; i < 5; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    const confirm = fixture.nativeElement.querySelector(
      'qt-character-delete-dialog button.qt-button-destructive',
    ) as HTMLButtonElement;
    expect(confirm).toBeTruthy();
    confirm.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const deleteCall = seen.find(
      (r) => (r as unknown as { type: string }).type === 'characterDelete',
    ) as unknown as { characterId: string; cascadeChats: boolean; cascadeImages: boolean };
    expect(deleteCall).toMatchObject({
      characterId: 'char-1',
      cascadeChats: true,
      cascadeImages: true,
    });
    expect(navigateSpy).toHaveBeenCalledWith(['/characters']);
  });

  it('"Save Character" dispatches ONE characterUpdate with the exact PUT bag', async () => {
    const seen: CoreRequest[] = [];
    const fixture = await render(
      stubClient(character(), (req) => seen.push(req as unknown as CoreRequest)),
    );

    const nameInput = fixture.nativeElement.querySelector('#name') as HTMLInputElement;
    nameInput.value = 'Reginald Jeeves';
    nameInput.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit', { cancelable: true }));
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const updateCalls = seen.filter(
      (r) => (r as unknown as { type: string }).type === 'characterUpdate',
    );
    expect(updateCalls.length).toBe(1);

    const call = updateCalls[0] as unknown as {
      type: string;
      characterId: string;
      character: Record<string, unknown>;
    };
    expect(call.characterId).toBe('char-1');
    const bag = call.character;
    expect(bag).toEqual({
      name: 'Reginald Jeeves',
      aliases: [],
      pronouns: null,
      title: 'The Valet',
      identity: 'A gentleman about town',
      description: 'Affable and a little dim',
      manifesto: 'Never let a pal down',
      personality: 'Cheerful, easily flustered',
      scenarios: [],
      firstMessage: 'I say, old bean!',
      exampleDialogues: '',
      systemPrompt: '',
      tags: [],
      systemTransparency: false,
      coreWhisperEnabled: null,
      canBeCarina: false,
    });
  });
});
