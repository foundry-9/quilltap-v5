import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { RichEditor } from '../../editor/rich-editor';
import { NewChatForm } from './new-chat-form';
import { NewChatState } from './new-chat.state';
import type { NewChatSelectedCharacter, ScenarioOption } from './new-chat.types';

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

const GENERAL: ScenarioOption = {
  path: 'Scenarios/foggy-moor.md',
  filename: 'foggy-moor.md',
  name: 'Foggy Moor',
  isDefault: false,
  body: 'A foggy moor at dawn.',
};

// A stub CoreClient — the image-profile picker calls dispatchData; the autonomous
// settings-hint query calls dispatchExpect. Return empty shapes for both.
const stubCore = {
  dispatchData: async () => ({ profiles: [] }),
  dispatchExpect: async () => ({ type: 'chatSettings', data: {} }),
} as unknown as CoreClient;

function makeState(): NewChatState {
  const state = new NewChatState(stubCore, {});
  state.loading.set(false);
  state.profiles.set([{ id: 'p1', name: 'Anthropic', modelName: 'claude' }]);
  return state;
}

function render(state: NewChatState): ComponentFixture<NewChatForm> {
  TestBed.configureTestingModule({
    imports: [NewChatForm],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: stubCore },
    ],
  });
  const fixture = TestBed.createComponent(NewChatForm);
  fixture.componentRef.setInput('state', state);
  fixture.detectChanges();
  return fixture;
}

function labelOf(fixture: ComponentFixture<NewChatForm>, label: string): Element | null {
  return (fixture.nativeElement as HTMLElement).querySelector(`[aria-label="${label}"]`);
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** The starting-scenario editor (qt-markdown-field's RichEditor handle). */
function scenarioEditor(fixture: ComponentFixture<NewChatForm>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

describe('NewChatForm scenario layering', () => {
  it('shows the "Starting scenario" editor when no preset is selected', () => {
    const fixture = render(makeState());
    expect(labelOf(fixture, 'Starting scenario')).not.toBeNull();
    expect(labelOf(fixture, 'Additional scenario notes')).toBeNull();
  });

  it('shows the preset preview, append hint, and relabeled editor when a preset is selected', () => {
    const state = makeState();
    state.generalScenarios.set([GENERAL]);
    state.patchForm({ generalScenarioPath: GENERAL.path });
    const fixture = render(state);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('A foggy moor at dawn.');
    expect(text).toMatch(/added beneath the scenario above/i);
    expect(labelOf(fixture, 'Additional scenario notes')).not.toBeNull();
    expect(labelOf(fixture, 'Starting scenario')).toBeNull();
  });

  it('carries the scenario notes markdown into the form state', async () => {
    const state = makeState();
    const fixture = render(state);
    await settle(fixture);

    scenarioEditor(fixture).setMarkdown('They meet at *dusk*, by the folly.');
    await settle(fixture);

    expect(state.form().scenario).toBe('They meet at *dusk*, by the folly.');
  });

  it('seeds the field from the form scenario without dirtying it on load', async () => {
    const state = makeState();
    state.patchForm({ scenario: 'A __foggy__ moor.' });
    const fixture = render(state);
    await settle(fixture);

    // Displayed canonically, but the load is not an edit: the stored form value
    // is untouched until the user types.
    expect(scenarioEditor(fixture).getMarkdown()).toBe('A **foggy** moor.');
    expect(state.form().scenario).toBe('A __foggy__ moor.');
  });
});

describe('NewChatForm Play As dropdown', () => {
  it('lists cast characters and default-user characters', () => {
    const state = makeState();
    const alice = char('a', 'Alice');
    const bob = char('b', 'Bob', { controlledBy: 'user' });
    const cast: NewChatSelectedCharacter = {
      character: alice,
      connectionProfileId: 'p1',
      controlledBy: 'llm',
    };
    state.selectedCharacters.set([cast]);
    state.userControlledCharacters.set([bob]);
    const fixture = render(state);
    const select = (fixture.nativeElement as HTMLElement).querySelector('#new-chat-partner');
    const options = Array.from(select?.querySelectorAll('option') ?? []).map((o) =>
      o.textContent?.trim(),
    );
    expect(options).toEqual(['Chat as yourself', 'Alice', 'Bob']);
  });
});
