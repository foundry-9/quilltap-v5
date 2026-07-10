import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { of } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CharacterDetail as CharacterDetailDto } from '../../../core/core-contract';
import { CharacterDetail } from './character-detail';

function character(over: Partial<CharacterDetailDto> = {}): CharacterDetailDto {
  return {
    id: 'c1',
    name: 'Bertie',
    title: 'The Valet',
    identity: 'A gentleman of leisure.',
    description: null,
    manifesto: null,
    personality: null,
    scenarios: [],
    firstMessage: null,
    exampleDialogues: null,
    systemPrompts: [],
    physicalDescription: null,
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
  c: CharacterDetailDto,
  onDispatch?: (req: { type: string; [k: string]: unknown }) => void,
): Partial<CoreClient> {
  let current = c;
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch?.(req);
      switch (req.type) {
        case 'characterGet':
          return { character: current };
        case 'connectionProfileList':
          return { profiles: [] };
        case 'characterList':
          return { characters: [] };
        case 'characterDefaultPartner':
          return { partnerId: null };
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
        case 'characterFavorite':
          current = { ...current, isFavorite: !current.isFavorite };
          return { character: current };
        case 'characterToggleCarina':
          current = { ...current, canBeCarina: !current.canBeCarina };
          return { character: current };
        case 'characterToggleControlledBy':
          current = { ...current, controlledBy: current.controlledBy === 'user' ? 'llm' : 'user' };
          return { character: current };
        case 'characterUpdate':
          return { character: current };
        default:
          return {};
      }
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CharacterDetail>> {
  TestBed.configureTestingModule({
    imports: [CharacterDetail],
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
      {
        provide: ActivatedRoute,
        useValue: {
          paramMap: of(convertToParamMap({ id: 'c1' })),
          queryParamMap: of(convertToParamMap({})),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(CharacterDetail);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('CharacterDetail', () => {
  it('loads the character and renders the header + the nine-tab set', async () => {
    const fixture = await render(stubClient(character({})));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Bertie');
    expect(text).toContain('The Valet');
    expect(text).toContain('← Back to Characters');
    for (const label of [
      'Details',
      'System Prompts',
      'Conversations',
      'Memories',
      'Tags',
      'Wardrobe',
      'Default Settings',
      'Photo Gallery',
      'Appearance',
    ]) {
      expect(text).toContain(label);
    }
  });

  it('dispatches characterFavorite when the favorite star is toggled', async () => {
    const seen: string[] = [];
    const fixture = await render(stubClient(character({ isFavorite: false }), (req) => seen.push(req.type)));
    const buttons = Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[];
    const favorite = buttons.find((b) => b.title === 'Add to favorites');
    expect(favorite).toBeTruthy();
    favorite!.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(seen).toContain('characterFavorite');
  });

  it('dispatches characterToggleCarina when the Carina toggle is clicked', async () => {
    const seen: string[] = [];
    const fixture = await render(stubClient(character({ canBeCarina: false }), (req) => seen.push(req.type)));
    const buttons = Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[];
    const carina = buttons.find((b) => b.title === 'Enable Carina answers (@-queries)');
    expect(carina).toBeTruthy();
    carina!.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(seen).toContain('characterToggleCarina');
  });

  it('dispatches characterToggleControlledBy when the controlled-by toggle is clicked', async () => {
    const seen: string[] = [];
    const fixture = await render(
      stubClient(character({ controlledBy: 'llm' }), (req) => seen.push(req.type)),
    );
    const buttons = Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[];
    const controlledBy = buttons.find((b) => b.title === 'Switch to user control');
    expect(controlledBy).toBeTruthy();
    controlledBy!.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(seen).toContain('characterToggleControlledBy');
  });

  it('dispatches characterUpdate with npc:true when Convert to NPC is clicked', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient(character({ npc: false }), (req) => seen.push(req)));
    const buttons = Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[];
    const convert = buttons.find((b) => b.textContent?.trim() === 'Convert to NPC');
    expect(convert).toBeTruthy();
    convert!.click();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    const update = seen.find((r) => r.type === 'characterUpdate');
    expect(update).toBeTruthy();
    expect(update!['character']).toEqual({ npc: true });
  });
});
