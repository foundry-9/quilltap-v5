import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { CharacterPickerPanel } from './character-picker-panel';
import { NewChatState } from './new-chat.state';
import type { NewChatSelectedCharacter } from './new-chat.types';

function char(id: string, name: string, over: Partial<CharacterListItem> = {}): CharacterListItem {
  return {
    id,
    name,
    title: null,
    description: null,
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
    createdAt: '',
    tags: [],
    updatedAt: '',
    systemPrompts: [],
    scenarios: [],
    _count: { chats: 0 },
    ...over,
  };
}

const stubCore = {} as CoreClient;

function makeState(): NewChatState {
  const state = new NewChatState(stubCore, {});
  state.loading.set(false);
  state.profiles.set([{ id: 'p1', name: 'Anthropic', modelName: 'claude' }]);
  state.characters.set([char('a', 'Alice'), char('b', 'Bob')]);
  return state;
}

function render(state: NewChatState): ComponentFixture<CharacterPickerPanel> {
  TestBed.configureTestingModule({ imports: [CharacterPickerPanel] });
  const fixture = TestBed.createComponent(CharacterPickerPanel);
  fixture.componentRef.setInput('state', state);
  fixture.detectChanges();
  return fixture;
}

describe('CharacterPickerPanel', () => {
  it('lists the roster and the empty selected-cast message', () => {
    const fixture = render(makeState());
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Alice');
    expect(text).toContain('Bob');
    expect(text).toContain('Click on characters to add them to the chat');
  });

  it('selecting a roster character seeds it as an LLM entry with the first profile', () => {
    const state = makeState();
    const fixture = render(state);
    const rosterButtons = (fixture.nativeElement as HTMLElement).querySelectorAll(
      '.new-chat-character-picker button',
    );
    // The first roster row is the first sorted character.
    (rosterButtons[0] as HTMLButtonElement).click();
    fixture.detectChanges();

    const cast = state.selectedCharacters();
    expect(cast).toHaveLength(1);
    expect(cast[0].controlledBy).toBe('llm');
    expect(cast[0].connectionProfileId).toBe('p1');
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Speaks First');
  });

  it('renders the Play As (User) option in the per-character profile select', () => {
    const state = makeState();
    const selected: NewChatSelectedCharacter = {
      character: char('a', 'Alice'),
      connectionProfileId: 'p1',
      controlledBy: 'llm',
    };
    state.selectedCharacters.set([selected]);
    const fixture = render(state);
    const options = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('option'),
    ).map((o) => o.textContent?.trim());
    expect(options).toContain('Play As (User)');
  });
});
