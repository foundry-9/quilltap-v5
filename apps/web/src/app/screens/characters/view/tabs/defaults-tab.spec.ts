import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterDetail, CharacterListItem } from '../../../../core/core-contract';
import { CharacterDefaultsTab } from './defaults-tab';

function character(over: Partial<CharacterDetail> = {}): CharacterDetail {
  return {
    id: 'c1',
    name: 'Bertie',
    title: null,
    identity: null,
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

function partner(id: string, name: string): CharacterListItem {
  return {
    id,
    name,
    title: null,
    description: '',
    defaultImageId: null,
    defaultImage: null,
    isFavorite: false,
    controlledBy: 'user',
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
  };
}

function stubClient(onDispatch: (req: { type: string; [k: string]: unknown }) => void): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      onDispatch(req);
      return { character: {} };
    }) as CoreClient['dispatchData'],
  };
}

function selectByFirstOptionText(fixture: ComponentFixture<CharacterDefaultsTab>, text: string): HTMLSelectElement {
  const selects = Array.from(fixture.nativeElement.querySelectorAll('select')) as HTMLSelectElement[];
  const found = selects.find((s) => s.querySelector('option')?.textContent?.trim() === text);
  if (!found) throw new Error(`No <select> found with first option "${text}"`);
  return found;
}

function change(select: HTMLSelectElement, value: string): void {
  select.value = value;
  select.dispatchEvent(new Event('change'));
}

async function render(
  client: Partial<CoreClient>,
  inputs: {
    character: CharacterDetail;
    connectionProfiles?: Array<{ id: string; name: string }>;
    userControlledCharacters?: CharacterListItem[];
    defaultPartnerId?: string | null;
  },
): Promise<ComponentFixture<CharacterDefaultsTab>> {
  TestBed.configureTestingModule({
    imports: [CharacterDefaultsTab],
    providers: [provideRouter([]), provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CharacterDefaultsTab);
  fixture.componentRef.setInput('characterId', inputs.character.id);
  fixture.componentRef.setInput('character', inputs.character);
  fixture.componentRef.setInput('connectionProfiles', inputs.connectionProfiles ?? []);
  fixture.componentRef.setInput('userControlledCharacters', inputs.userControlledCharacters ?? []);
  fixture.componentRef.setInput('defaultPartnerId', inputs.defaultPartnerId ?? null);
  fixture.detectChanges();
  await new Promise((r) => setTimeout(r, 0));
  fixture.detectChanges();
  return fixture;
}

describe('CharacterDefaultsTab (autosave contract)', () => {
  it('dispatches characterUpdate {controlledBy:"llm", defaultConnectionProfileId} when a profile is picked', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)), {
      character: character({}),
      connectionProfiles: [{ id: 'profile-1', name: 'My GPT' }],
    });
    change(selectByFirstOptionText(fixture, 'No default profile'), 'profile-1');
    await new Promise((r) => setTimeout(r, 0));
    const req = seen.find((r) => r.type === 'characterUpdate');
    expect(req).toBeTruthy();
    expect(req!['character']).toEqual({ controlledBy: 'llm', defaultConnectionProfileId: 'profile-1' });
  });

  it('dispatches characterUpdate {controlledBy:"user"} when "User Acts As Character" is picked', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)), { character: character({}) });
    change(selectByFirstOptionText(fixture, 'No default profile'), '__user_controlled__');
    await new Promise((r) => setTimeout(r, 0));
    const req = seen.find((r) => r.type === 'characterUpdate');
    expect(req).toBeTruthy();
    expect(req!['character']).toEqual({ controlledBy: 'user' });
  });

  it('dispatches characterSetDefaultPartner {characterId, partnerId} when the partner select changes', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)), {
      character: character({}),
      userControlledCharacters: [partner('p1', 'Jeeves')],
    });
    change(selectByFirstOptionText(fixture, 'No default partner'), 'p1');
    await new Promise((r) => setTimeout(r, 0));
    const req = seen.find((r) => r.type === 'characterSetDefaultPartner');
    expect(req).toEqual({ type: 'characterSetDefaultPartner', characterId: 'c1', partnerId: 'p1' });
  });

  it('dispatches characterUpdate {defaultSystemPromptId} from the default-prompt select (only shown when >1 prompt)', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)), {
      character: character({
        systemPrompts: [
          { id: 'sp1', name: 'Formal', content: '', isDefault: true, createdAt: '', updatedAt: '' },
          { id: 'sp2', name: 'Casual', content: '', isDefault: false, createdAt: '', updatedAt: '' },
        ],
      }),
    });
    change(selectByFirstOptionText(fixture, 'Use first prompt marked as default'), 'sp2');
    await new Promise((r) => setTimeout(r, 0));
    const req = seen.find((r) => r.type === 'characterUpdate');
    expect(req!['character']).toEqual({ defaultSystemPromptId: 'sp2' });
  });

  it('dispatches characterUpdate {defaultScenarioId} from the default-scenario select (only shown when >1 scenario)', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)), {
      character: character({
        scenarios: [
          { id: 'sc1', title: 'Tea Time', content: '', createdAt: '', updatedAt: '' },
          { id: 'sc2', title: 'Garden Party', content: '', createdAt: '', updatedAt: '' },
        ],
      }),
    });
    change(selectByFirstOptionText(fixture, 'No default scenario'), 'sc2');
    await new Promise((r) => setTimeout(r, 0));
    const req = seen.find((r) => r.type === 'characterUpdate');
    expect(req!['character']).toEqual({ defaultScenarioId: 'sc2' });
  });

  it('dispatches characterUpdate {defaultAgentModeEnabled} for each tri-state option', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)), { character: character({}) });
    const select = selectByFirstOptionText(fixture, 'Inherit from global settings');

    change(select, 'enabled');
    await new Promise((r) => setTimeout(r, 0));
    change(select, 'disabled');
    await new Promise((r) => setTimeout(r, 0));
    change(select, 'inherit');
    await new Promise((r) => setTimeout(r, 0));

    const bodies = seen.filter((r) => r.type === 'characterUpdate').map((r) => r['character']);
    expect(bodies).toEqual([
      { defaultAgentModeEnabled: true },
      { defaultAgentModeEnabled: false },
      { defaultAgentModeEnabled: null },
    ]);
  });

  it('dispatches characterUpdate {canDressThemselves} from the self-dressing select', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)), { character: character({}) });
    change(selectByFirstOptionText(fixture, 'Inherit from global settings (enabled)'), 'disabled');
    await new Promise((r) => setTimeout(r, 0));
    const req = seen.find((r) => r.type === 'characterUpdate');
    expect(req!['character']).toEqual({ canDressThemselves: false });
  });

  it('dispatches characterUpdate {defaultTimestampConfig: null} when the timestamp mode is Disabled', async () => {
    const seen: Array<{ type: string; [k: string]: unknown }> = [];
    const fixture = await render(stubClient((r) => seen.push(r)), {
      character: character({ defaultTimestampConfig: { mode: 'START_ONLY', format: 'FRIENDLY' } }),
    });
    const radios = Array.from(fixture.nativeElement.querySelectorAll('input[type="radio"]')) as HTMLInputElement[];
    const disabledRadio = radios[0]; // "Disabled" is first in TIMESTAMP_MODES
    disabledRadio.click();
    await new Promise((r) => setTimeout(r, 0));
    const req = seen.find((r) => r.type === 'characterUpdate');
    expect(req!['character']).toEqual({ defaultTimestampConfig: null });
  });
});
