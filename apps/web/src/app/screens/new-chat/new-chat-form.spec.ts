import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { RichEditor } from '../../editor/rich-editor';
import { NewChatForm } from './new-chat-form';
import { NewChatState } from './new-chat.state';
import type {
  NewChatSelectedCharacter,
  RoleplayTemplateOption,
  ScenarioOption,
} from './new-chat.types';

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
  it('lists only cast characters (v4 e2eb3d21)', () => {
    const state = makeState();
    const alice = char('a', 'Alice');
    const bob = char('b', 'Bob', { controlledBy: 'user' });
    const cast: NewChatSelectedCharacter = {
      character: alice,
      connectionProfileId: 'p1',
      controlledBy: 'llm',
    };
    state.selectedCharacters.set([cast]);
    // Bob is a default-user character but NOT in the cast, so he is absent from
    // the dropdown — he would be added via the picker on the left instead.
    state.userControlledCharacters.set([bob]);
    const fixture = render(state);
    const select = (fixture.nativeElement as HTMLElement).querySelector('#new-chat-partner');
    const options = Array.from(select?.querySelectorAll('option') ?? []).map((o) =>
      o.textContent?.trim(),
    );
    expect(options).toEqual(['Chat as yourself', 'Alice']);
  });
});

// --- Roleplay template picker (v4 `4bbeab47` NewChatForm.test.tsx) ------------

const TEMPLATES: RoleplayTemplateOption[] = [
  { id: 'tpl-classic', name: 'Classic Roleplay', description: null, isBuiltIn: true },
  { id: 'tpl-house', name: 'House Style', description: null, isBuiltIn: false },
];

function templateSelect(fixture: ComponentFixture<NewChatForm>): HTMLSelectElement | null {
  return (fixture.nativeElement as HTMLElement).querySelector('#new-chat-roleplay-template');
}

function templateOptions(fixture: ComponentFixture<NewChatForm>): string[] {
  return Array.from(templateSelect(fixture)?.querySelectorAll('option') ?? []).map(
    (o) => o.textContent?.replace(/\s+/g, ' ').trim() ?? '',
  );
}

describe('NewChatForm roleplay template picker', () => {
  it('is hidden when no templates are available', () => {
    const fixture = render(makeState());
    expect(templateSelect(fixture)).toBeNull();
  });

  it('defaults to the chat default and marks it in the list', async () => {
    const state = makeState();
    state.roleplayTemplates.set(TEMPLATES);
    state.defaultRoleplayTemplateId.set('tpl-house');
    state.patchForm({ roleplayTemplateId: 'tpl-house' });
    const fixture = render(state);
    // `[ngModel]` lands on the DOM asynchronously — settle before reading it,
    // or the assertion reads the pristine '' and passes for the wrong reason.
    await settle(fixture);

    expect(templateSelect(fixture)?.value).toBe('tpl-house');
    expect(templateOptions(fixture)).toEqual([
      'No Template',
      'Classic Roleplay (Built-in)',
      'House Style (default)',
    ]);
  });

  it('marks "No Template" as the default when no default template is set', async () => {
    const state = makeState();
    state.roleplayTemplates.set(TEMPLATES);
    state.defaultRoleplayTemplateId.set(null);
    const fixture = render(state);
    await settle(fixture);

    expect(templateSelect(fixture)?.value).toBe('');
    expect(templateOptions(fixture)[0]).toBe('No Template (default)');
  });

  it('records an explicit pick as touched so reference reloads cannot re-seed it', async () => {
    const state = makeState();
    state.roleplayTemplates.set(TEMPLATES);
    state.defaultRoleplayTemplateId.set('tpl-house');
    state.patchForm({ roleplayTemplateId: 'tpl-house' });
    const fixture = render(state);
    await settle(fixture);

    const select = templateSelect(fixture)!;
    select.value = 'tpl-classic';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);

    expect(state.form().roleplayTemplateId).toBe('tpl-classic');
    expect(state.form().roleplayTemplateTouched).toBe(true);
  });

  it('turns "No Template" into a null id, still marked as touched', async () => {
    const state = makeState();
    state.roleplayTemplates.set(TEMPLATES);
    state.defaultRoleplayTemplateId.set('tpl-house');
    state.patchForm({ roleplayTemplateId: 'tpl-house' });
    const fixture = render(state);
    await settle(fixture);

    const select = templateSelect(fixture)!;
    select.value = '';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);

    expect(state.form().roleplayTemplateId).toBeNull();
    expect(state.form().roleplayTemplateTouched).toBe(true);
  });
});

/**
 * P4.D121 — the archive behavior set in the New Chat form (v4 `d25dacc1`),
 * plus the group optgroup that commit finally connected.
 */
describe('NewChatForm — archived scenarios and the group tier', () => {
  const DUEL = { id: 'cs-1', title: 'A Duel', content: 'Pistols at dawn.' };
  const WAKE = { id: 'cs-2', title: 'A Wake', content: 'Rain on the glass.', archived: true };

  function withCharacter(
    state: NewChatState,
    scenarios: { id: string; title: string; content: string; archived?: boolean }[],
  ): void {
    const c = char('c1', 'Aria', { scenarios });
    const cast: NewChatSelectedCharacter[] = [
      { character: c, connectionProfileId: 'p1', selectedSystemPromptId: null, controlledBy: 'llm' },
    ];
    state.selectedCharacters.set(cast);
  }

  function optionTexts(fixture: ComponentFixture<NewChatForm>): string[] {
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      '#new-chat-scenario-select',
    ) as HTMLSelectElement | null;
    return select ? [...select.querySelectorAll('option')].map((o) => o.textContent!.trim()) : [];
  }

  /**
   * The "Show archived" box specifically — the form has other checkboxes (the
   * autonomous toggle above it), so scope by the label's own text.
   */
  function archivedBox(fixture: ComponentFixture<NewChatForm>): HTMLInputElement {
    const label = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('label'),
    ).find((l) => (l.textContent ?? '').trim() === 'Show archived');
    if (!label) throw new Error('no "Show archived" label rendered');
    return label.querySelector('input[type="checkbox"]') as HTMLInputElement;
  }

  it('hides an archived character scenario until "Show archived" is ticked', () => {
    const state = makeState();
    withCharacter(state, [DUEL, WAKE]);
    const fixture = render(state);
    expect(optionTexts(fixture)).toEqual(['Custom...', 'A Duel']);

    state.showArchivedScenarios.set(true);
    fixture.detectChanges();
    expect(optionTexts(fixture)).toEqual(['Custom...', 'A Duel', 'A Wake (archived)']);
  });

  it('keeps the CURRENT selection visible even when archived and hidden', () => {
    const state = makeState();
    withCharacter(state, [DUEL, WAKE]);
    state.patchForm({ scenarioId: WAKE.id });
    const fixture = render(state);
    // The archived row stays — suffixed — because blanking the select out from
    // under a pick made a moment ago is worse than showing it (v4 `:160-170`).
    expect(optionTexts(fixture)).toEqual(['Custom...', 'A Duel', 'A Wake (archived)']);
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      '#new-chat-scenario-select',
    ) as HTMLSelectElement;
    expect(select.value).toBe(WAKE.id);
  });

  it('previews an archived selection against the UNFILTERED list', () => {
    const state = makeState();
    withCharacter(state, [DUEL, WAKE]);
    state.patchForm({ scenarioId: WAKE.id });
    const fixture = render(state);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Rain on the glass.');
  });

  it('renders the Group Scenarios optgroup now that the tier is wired (v4 d25dacc1)', () => {
    const state = makeState();
    withCharacter(state, [DUEL]);
    state.groupScenarios.set([
      {
        groupId: 'g1',
        groupName: 'The Irregulars',
        path: 'Scenarios/den.md',
        filename: 'den.md',
        name: 'The Den',
        isDefault: false,
        archived: false,
        body: 'A cellar.',
      },
    ]);
    const fixture = render(state);
    const groups = [
      ...(fixture.nativeElement as HTMLElement).querySelectorAll('#new-chat-scenario-select optgroup'),
    ].map((g) => g.getAttribute('label'));
    expect(groups).toContain('Group Scenarios: The Irregulars');
    expect(optionTexts(fixture)).toContain('The Den');
  });

  it('a picked group scenario previews its body and holds the select', () => {
    const state = makeState();
    withCharacter(state, [DUEL]);
    state.groupScenarios.set([
      {
        groupId: 'g1',
        groupName: 'The Irregulars',
        path: 'Scenarios/den.md',
        filename: 'den.md',
        name: 'The Den',
        isDefault: false,
        archived: false,
        body: 'A cellar.',
      },
    ]);
    state.patchForm({ groupScenarioPath: 'Scenarios/den.md', groupScenarioGroupId: 'g1' });
    const fixture = render(state);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('A cellar.');
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      '#new-chat-scenario-select',
    ) as HTMLSelectElement;
    expect(select.value).toBe('group:g1:Scenarios/den.md');
  });

  it('the "Show archived" checkbox drives the state setter, which refetches', async () => {
    const state = makeState();
    withCharacter(state, [DUEL]);
    const fixture = render(state);
    const box = archivedBox(fixture);
    expect(box.checked).toBe(false);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(state.showArchivedScenarios()).toBe(true);
  });
});
