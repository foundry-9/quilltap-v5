import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { coreStreamStub } from '../../core/core-client.testing';
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

/**
 * The stream surface + a `characterSubpromptList` answer are required since
 * P4.D165 put a `qt-subprompt-picker` on every LLM seat's card: the picker's
 * shared query injects `RealtimeService`, whose constructor subscribes to
 * `events$` (`core-client.testing.ts`'s header).
 */
const stubCore = {
  ...coreStreamStub(),
  dispatchData: async () => ({ subprompts: [] }),
} as unknown as CoreClient;

function makeState(): NewChatState {
  const state = new NewChatState(stubCore, {});
  state.loading.set(false);
  state.profiles.set([{ id: 'p1', name: 'Anthropic', modelName: 'claude' }]);
  state.characters.set([char('a', 'Alice'), char('b', 'Bob')]);
  return state;
}

function render(state: NewChatState): ComponentFixture<CharacterPickerPanel> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CharacterPickerPanel],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: stubCore },
    ],
  });
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

  /** v4 `CharacterPickerPanel.tsx:307-317` at `2f4254b42`. */
  describe('the subprompt picker on each seat', () => {
    function withCast(controlledBy: 'llm' | 'user'): ComponentFixture<CharacterPickerPanel> {
      const state = makeState();
      state.selectedCharacters.set([
        { character: char('a', 'Alice'), connectionProfileId: 'p1', controlledBy },
      ]);
      return render(state);
    }

    it('renders under an LLM seat', () => {
      const fixture = withCast('llm');
      expect(
        (fixture.nativeElement as HTMLElement).querySelector('qt-subprompt-picker'),
      ).toBeTruthy();
      expect((fixture.nativeElement as HTMLElement).textContent).toContain('Subprompts');
    });

    it('is withheld from a USER-controlled seat — it has no identity stack to carry them', () => {
      const fixture = withCast('user');
      expect(
        (fixture.nativeElement as HTMLElement).querySelector('qt-subprompt-picker'),
      ).toBeNull();
    });

    it('a selection change writes `selectedSubpromptIds` onto that cast entry alone', () => {
      const state = makeState();
      state.selectedCharacters.set([
        { character: char('a', 'Alice'), connectionProfileId: 'p1', controlledBy: 'llm' },
        { character: char('b', 'Bob'), connectionProfileId: 'p1', controlledBy: 'llm' },
      ]);
      const fixture = render(state);
      (
        fixture.componentInstance as unknown as {
          onSubpromptsChange(id: string, ids: string[]): void;
        }
      ).onSubpromptsChange('a', ['terse', 'verse']);
      fixture.detectChanges();
      expect(state.selectedCharacters()[0].selectedSubpromptIds).toEqual(['terse', 'verse']);
      expect(state.selectedCharacters()[1].selectedSubpromptIds).toBeUndefined();
    });
  });
});
