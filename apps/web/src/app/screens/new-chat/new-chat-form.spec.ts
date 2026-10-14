import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CONCIERGE_STATE_PRESENTATION } from '../../chat/concierge-state-presentation';
import { CoreClient } from '../../core/core-client';
import type { CharacterListItem, ConciergeState } from '../../core/core-contract';
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

async function render(state: NewChatState): Promise<ComponentFixture<NewChatForm>> {
  TestBed.configureTestingModule({
    imports: [NewChatForm],
    providers: [
      provideTanStackQuery(new QueryClient()),
      // The off-duty hint links to Settings → The Concierge.
      provideRouter([]),
      { provide: CoreClient, useValue: stubCore },
    ],
  });
  // The form `@defer`s the Scenario Builder dialog (P4.D218), so the
  // component carries unresolved metadata until compiled.
  await TestBed.compileComponents();
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
  it('shows the "Starting scenario" editor when no preset is selected', async () => {
    const fixture = await render(makeState());
    expect(labelOf(fixture, 'Starting scenario')).not.toBeNull();
    expect(labelOf(fixture, 'Additional scenario notes')).toBeNull();
  });

  it('shows the preset preview, append hint, and relabeled editor when a preset is selected', async () => {
    const state = makeState();
    state.generalScenarios.set([GENERAL]);
    state.patchForm({ scenarioSelection: { kind: 'general', path: GENERAL.path } });
    const fixture = await render(state);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('A foggy moor at dawn.');
    expect(text).toMatch(/added beneath the scenario above/i);
    expect(labelOf(fixture, 'Additional scenario notes')).not.toBeNull();
    expect(labelOf(fixture, 'Starting scenario')).toBeNull();
  });

  it('carries the scenario notes markdown into the form state', async () => {
    const state = makeState();
    const fixture = await render(state);
    await settle(fixture);

    scenarioEditor(fixture).setMarkdown('They meet at *dusk*, by the folly.');
    await settle(fixture);

    expect(state.form().scenario).toBe('They meet at *dusk*, by the folly.');
  });

  it('seeds the field from the form scenario without dirtying it on load', async () => {
    const state = makeState();
    state.patchForm({ scenario: 'A __foggy__ moor.' });
    const fixture = await render(state);
    await settle(fixture);

    // Displayed canonically, but the load is not an edit: the stored form value
    // is untouched until the user types.
    expect(scenarioEditor(fixture).getMarkdown()).toBe('A **foggy** moor.');
    expect(state.form().scenario).toBe('A __foggy__ moor.');
  });
});

describe('NewChatForm Play As dropdown', () => {
  it('lists only cast characters (v4 e2eb3d21)', async () => {
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
    const fixture = await render(state);
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
  it('is hidden when no templates are available', async () => {
    const fixture = await render(makeState());
    expect(templateSelect(fixture)).toBeNull();
  });

  it('defaults to the chat default and marks it in the list', async () => {
    const state = makeState();
    state.roleplayTemplates.set(TEMPLATES);
    state.defaultRoleplayTemplateId.set('tpl-house');
    state.patchForm({ roleplayTemplateId: 'tpl-house' });
    const fixture = await render(state);
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
    const fixture = await render(state);
    await settle(fixture);

    expect(templateSelect(fixture)?.value).toBe('');
    expect(templateOptions(fixture)[0]).toBe('No Template (default)');
  });

  it('records an explicit pick as touched so reference reloads cannot re-seed it', async () => {
    const state = makeState();
    state.roleplayTemplates.set(TEMPLATES);
    state.defaultRoleplayTemplateId.set('tpl-house');
    state.patchForm({ roleplayTemplateId: 'tpl-house' });
    const fixture = await render(state);
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
    const fixture = await render(state);
    await settle(fixture);

    const select = templateSelect(fixture)!;
    select.value = '';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);

    expect(state.form().roleplayTemplateId).toBeNull();
    expect(state.form().roleplayTemplateTouched).toBe(true);
  });
});


// --- The Concierge picker (v4 NewChatForm.test.tsx at `ce2f1dabf`) ----------

/**
 * v4's `NewChatForm Concierge picker` block, transcribed — the four `it(`
 * names are v4's own so the mapping is greppable — plus the on-duty and
 * followed-default arms no v4 test pins (the order's red-first additions).
 *
 * v4's suite reaches the control with
 * `screen.getByRole('combobox', { name: /The Concierge/i })`; v5 reaches it by
 * its id, the idiom every other select in this spec already uses.
 */
function conciergeSelect(fixture: ComponentFixture<NewChatForm>): HTMLSelectElement {
  return (fixture.nativeElement as HTMLElement).querySelector(
    '#new-chat-concierge',
  ) as HTMLSelectElement;
}

function optionLabels(fixture: ComponentFixture<NewChatForm>): string[] {
  return Array.from(conciergeSelect(fixture).querySelectorAll('option')).map((o) =>
    o.textContent!.trim(),
  );
}

describe('NewChatForm Concierge picker', () => {
  it('offers the three states as a flat list, no optgroups', async () => {
    const fixture = await render(makeState());
    const select = conciergeSelect(fixture);

    expect(select.querySelectorAll('optgroup')).toHaveLength(0);
    const options = Array.from(select.querySelectorAll('option')).map((o) => o.value);
    expect(options).toEqual(['moderated', 'unmoderated', 'locked']);
  });

  it('starts on Moderated and marks it the default', async () => {
    const fixture = await render(makeState());
    await settle(fixture);
    expect(conciergeSelect(fixture).value).toBe('moderated');
    // Only Moderated carries the suffix.
    expect(optionLabels(fixture)).toEqual(['Moderated (default)', 'Unmoderated', 'Locked']);
  });

  it.each(['moderated', 'unmoderated', 'locked'] as const)(
    'shows the shared presentation helper sentence for %s',
    async (state) => {
      const s = makeState();
      s.patchForm({ conciergeState: state });
      const fixture = await render(s);
      const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
      expect(text).toContain(CONCIERGE_STATE_PRESENTATION[state].detail);
      // The `hint` is deliberately NOT shown here — the reader is looking at
      // the control that sets it (v4's own comment).
      expect(text).not.toContain(CONCIERGE_STATE_PRESENTATION[state].hint);
    },
  );

  it('records the chosen state on the form state', async () => {
    const state = makeState();
    const fixture = await render(state);
    await settle(fixture);

    const select = conciergeSelect(fixture);
    select.value = 'unmoderated';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);

    expect(state.form().conciergeState).toBe('unmoderated');
  });

  /**
   * Not in v4 (React re-applies a controlled `value` on every render for free).
   * v5 binds `[ngModel]`, so this pins that a state change made anywhere but
   * the control itself still reaches the DOM — the finding-#108 class.
   */
  it('re-renders the selected option after a programmatic state change', async () => {
    const state = makeState();
    const fixture = await render(state);
    await settle(fixture);
    expect(conciergeSelect(fixture).value).toBe('moderated');

    for (const next of ['unmoderated', 'locked', 'moderated'] as ConciergeState[]) {
      state.patchForm({ conciergeState: next });
      await settle(fixture);
      expect(conciergeSelect(fixture).value).toBe(next);
      expect((fixture.nativeElement as HTMLElement).textContent).toContain(
        CONCIERGE_STATE_PRESENTATION[next].detail,
      );
    }
  });

  // --- v4 `3b463d6b1` (#76): the followed default and the on-duty gate ------

  it('marks Unmoderated the default when new chats start Unmoderated', async () => {
    const state = makeState();
    state.conciergeServerDefault.set('unmoderated');
    const fixture = await render(state);
    await settle(fixture);
    expect(optionLabels(fixture)).toEqual(['Moderated', 'Unmoderated (default)', 'Locked']);
  });

  it('disables the select off duty and points at Settings → The Concierge', async () => {
    const state = makeState();
    state.conciergeOnDuty.set(false);
    const fixture = await render(state);
    await settle(fixture);

    expect(conciergeSelect(fixture).disabled).toBe(true);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain(
      'The Concierge is off duty — turn him on in Settings → The Concierge.',
    );
    // The hint REPLACES the helper sentence.
    expect(el.textContent).not.toContain(CONCIERGE_STATE_PRESENTATION.moderated.detail);
    const link = el.querySelector('qt-concierge-off-duty-hint a') as HTMLAnchorElement;
    expect(link.getAttribute('href')).toBe('/settings?tab=concierge&section=on-duty');
  });

  it('keeps the select enabled and the helper sentence while on duty', async () => {
    const fixture = await render(makeState());
    await settle(fixture);
    expect(conciergeSelect(fixture).disabled).toBe(false);
    expect((fixture.nativeElement as HTMLElement).querySelector('qt-concierge-off-duty-hint')).toBeNull();
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

  it('hides an archived character scenario until "Show archived" is ticked', async () => {
    const state = makeState();
    withCharacter(state, [DUEL, WAKE]);
    const fixture = await render(state);
    expect(optionTexts(fixture)).toEqual(['Custom...', 'Browse all scenarios…', 'A Duel']);

    state.showArchivedScenarios.set(true);
    fixture.detectChanges();
    expect(optionTexts(fixture)).toEqual(['Custom...', 'Browse all scenarios…', 'A Duel', 'A Wake (archived)']);
  });

  it('keeps the CURRENT selection visible even when archived and hidden', async () => {
    const state = makeState();
    withCharacter(state, [DUEL, WAKE]);
    state.patchForm({ scenarioSelection: { kind: 'character', scenarioId: WAKE.id } });
    const fixture = await render(state);
    // The archived row stays — suffixed — because blanking the select out from
    // under a pick made a moment ago is worse than showing it (v4 `:160-170`).
    expect(optionTexts(fixture)).toEqual(['Custom...', 'Browse all scenarios…', 'A Duel', 'A Wake (archived)']);
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      '#new-chat-scenario-select',
    ) as HTMLSelectElement;
    expect(select.value).toBe(WAKE.id);
  });

  it('previews an archived selection against the UNFILTERED list', async () => {
    const state = makeState();
    withCharacter(state, [DUEL, WAKE]);
    state.patchForm({ scenarioSelection: { kind: 'character', scenarioId: WAKE.id } });
    const fixture = await render(state);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Rain on the glass.');
  });

  it('renders the Group Scenarios optgroup now that the tier is wired (v4 d25dacc1)', async () => {
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
    const fixture = await render(state);
    const groups = [
      ...(fixture.nativeElement as HTMLElement).querySelectorAll('#new-chat-scenario-select optgroup'),
    ].map((g) => g.getAttribute('label'));
    expect(groups).toContain('Group Scenarios: The Irregulars');
    expect(optionTexts(fixture)).toContain('The Den');
  });

  it('a picked group scenario previews its body and holds the select', async () => {
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
    state.patchForm({ scenarioSelection: { kind: 'group', groupId: 'g1', path: 'Scenarios/den.md' } });
    const fixture = await render(state);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('A cellar.');
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      '#new-chat-scenario-select',
    ) as HTMLSelectElement;
    expect(select.value).toBe('group:g1:Scenarios/den.md');
  });

  it('the "Show archived" checkbox drives the state setter, which refetches', async () => {
    const state = makeState();
    withCharacter(state, [DUEL]);
    const fixture = await render(state);
    const box = archivedBox(fixture);
    expect(box.checked).toBe(false);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(state.showArchivedScenarios()).toBe(true);
  });
});

/**
 * v4 `components/new-chat/__tests__/NewChatForm.test.tsx` at the pin
 * `83334b9a5` — "NewChatForm — the scenario selection drives the display" (the
 * 5 NEW cases of `545e299bb`), transcribed case for case, plus the P4.D302
 * rows the order names: every hand pick sets `scenarioTouched` (L-J), the
 * notes hint + editor label key on the SELECTION (a store body still loading),
 * and the browser's hierarchy.
 */
describe('NewChatForm — the scenario selection drives the display (v4 545e299bb)', () => {
  const PROJECT_DEFAULT: ScenarioOption = {
    path: 'Scenarios/atrium.md',
    filename: 'atrium.md',
    name: 'The Atrium',
    isDefault: true,
    body: 'Glass overhead.',
  };
  const STUDY = { id: 'sc-study', title: 'Her Study', content: 'Books to the ceiling.' };
  const alice = char('a', 'Alice', { scenarios: [STUDY], defaultScenarioId: STUDY.id });
  const llmSeat = (c: CharacterListItem): NewChatSelectedCharacter => ({
    character: c,
    connectionProfileId: 'p1',
    selectedSystemPromptId: null,
    controlledBy: 'llm',
  });

  /** A core that answers the browser's catalogue and a ref read. */
  function scenarioCore(sent: Record<string, unknown>[]): CoreClient {
    return {
      dispatchData: async (req: Record<string, unknown>) => {
        sent.push(req);
        if (req['type'] === 'scenarioEverywhere') return { sources: [] };
        if (req['type'] === 'scenarioReadByRef') {
          return { scenario: { body: 'A borrowed scene.' }, owner: {} };
        }
        return { profiles: [] };
      },
      dispatchExpect: async () => ({ type: 'chatSettings', data: {} }),
    } as unknown as CoreClient;
  }

  async function renderWith(state: NewChatState, core: CoreClient): Promise<ComponentFixture<NewChatForm>> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [NewChatForm],
      providers: [
        provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
        provideRouter([]),
        { provide: CoreClient, useValue: core },
      ],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(NewChatForm);
    fixture.componentRef.setInput('state', state);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  const scenarioSelect = (f: ComponentFixture<NewChatForm>) =>
    (f.nativeElement as HTMLElement).querySelector('#new-chat-scenario-select') as HTMLSelectElement;
  const text = (f: ComponentFixture<NewChatForm>) => (f.nativeElement as HTMLElement).textContent ?? '';

  it("shows the character's default, not the project's, when the selection names the character (regression)", async () => {
    // The old form displayed the project default while submitting the
    // character's. With one selection the two cannot disagree: the select,
    // the preview and the override note all read `scenarioSelection`.
    const state = makeState();
    state.selectedCharacters.set([llmSeat(alice)]);
    state.projectScenarios.set([PROJECT_DEFAULT]);
    state.patchForm({ scenarioSelection: { kind: 'character', scenarioId: STUDY.id } });
    const f = await renderWith(state, scenarioCore([]));

    expect(scenarioSelect(f).value).toBe(STUDY.id);
    expect(text(f)).toContain('Books to the ceiling.');
    expect(text(f)).not.toContain('Glass overhead.');
    expect(text(f)).not.toMatch(/Using the project default/);
  });

  it('offers the character default as a switch when the project default is held', async () => {
    const state = makeState();
    state.selectedCharacters.set([llmSeat(alice)]);
    state.projectScenarios.set([PROJECT_DEFAULT]);
    state.patchForm({ scenarioSelection: { kind: 'project', path: PROJECT_DEFAULT.path }, scenario: 'notes' });
    const f = await renderWith(state, scenarioCore([]));

    expect(scenarioSelect(f).value).toBe(`project:${PROJECT_DEFAULT.path}`);
    expect(text(f)).toContain('Glass overhead.');
    const switchButton = [...(f.nativeElement as HTMLElement).querySelectorAll('button')].find(
      (b) => b.textContent!.trim() === STUDY.title,
    )!;
    switchButton.click();
    await settle(f);

    expect(state.form().scenarioSelection).toEqual({ kind: 'character', scenarioId: STUDY.id });
    expect(state.form().scenarioTouched).toBe(true);
    expect(state.form().scenario).toBe('');
  });

  it('always renders the dropdown, with Browse, even when no tier has anything to offer', async () => {
    const f = await renderWith(makeState(), scenarioCore([]));
    const values = [...scenarioSelect(f).querySelectorAll('option')].map((o) => o.value);
    expect(values).toEqual(['__custom__', '__browse__']);
  });

  it('treats "Browse all scenarios…" as an action, never a selection', async () => {
    const state = makeState();
    state.generalScenarios.set([GENERAL]);
    const sent: Record<string, unknown>[] = [];
    const f = await renderWith(state, scenarioCore(sent));
    const before = state.form();

    const select = scenarioSelect(f);
    select.value = '__browse__';
    select.dispatchEvent(new Event('change'));
    await settle(f);

    expect(state.form()).toBe(before);
    expect(scenarioSelect(f).value).toBe('__custom__');
    // The browser opened and asked for the catalogue.
    expect(document.body.querySelector('qt-scenario-browser-dialog')).not.toBeNull();
    expect(sent.some((r) => r['type'] === 'scenarioEverywhere')).toBe(true);
    document.body.querySelectorAll('qt-scenario-browser-dialog').forEach((n) => n.remove());
  });

  it('shows a store pick under "Elsewhere" with its body read by ref, and the notes hint', async () => {
    const state = makeState();
    state.patchForm({
      scenarioSelection: {
        kind: 'store',
        mountPointId: 'mp-9',
        path: 'Scenarios/borrowed.md',
        label: 'Bob › Borrowed',
      },
    });
    const sent: Record<string, unknown>[] = [];
    const f = await renderWith(state, scenarioCore(sent));

    const elsewhere = scenarioSelect(f).querySelector('optgroup[label="Elsewhere"]');
    expect(elsewhere).not.toBeNull();
    expect(elsewhere!.textContent!.trim()).toBe('Bob › Borrowed');
    expect(scenarioSelect(f).value).toBe('store:mp-9:Scenarios/borrowed.md');

    expect(text(f)).toContain('A borrowed scene.');
    expect(text(f)).toMatch(/added beneath the scenario above/i);
    expect(labelOf(f, 'Additional scenario notes')).not.toBeNull();

    // v4 asserts the GET URL; v5's equivalent is the dispatch body — the path
    // exactly as the selection holds it, never encoded (R-Z).
    expect(sent.filter((r) => r['type'] === 'scenarioReadByRef')).toEqual([
      { type: 'scenarioReadByRef', mountPointId: 'mp-9', path: 'Scenarios/borrowed.md' },
    ]);
  });

  // --- P4.D302 rows beyond v4's file ------------------------------------------

  it('keys the notes hint and the editor label on the SELECTION, even while a store body loads', async () => {
    const state = makeState();
    state.patchForm({
      scenarioSelection: { kind: 'store', mountPointId: 'mp-9', path: 'Scenarios/slow.md', label: 'Bob › Slow' },
    });
    const core = {
      dispatchData: (req: Record<string, unknown>) =>
        req['type'] === 'scenarioReadByRef' ? new Promise(() => undefined) : Promise.resolve({ profiles: [] }),
      dispatchExpect: async () => ({ type: 'chatSettings', data: {} }),
    } as unknown as CoreClient;
    const f = await renderWith(state, core);
    // No body yet, so no preview block…
    expect((f.nativeElement as HTMLElement).querySelector('.whitespace-pre-wrap')).toBeNull();
    // …but it IS a preset already.
    expect(text(f)).toMatch(/added beneath the scenario above/i);
    expect(labelOf(f, 'Additional scenario notes')).not.toBeNull();
  });

  it('sets scenarioTouched on a dropdown pick (L-J)', async () => {
    const state = makeState();
    state.generalScenarios.set([GENERAL]);
    const f = await renderWith(state, scenarioCore([]));
    const select = scenarioSelect(f);
    select.value = `general:${GENERAL.path}`;
    select.dispatchEvent(new Event('change'));
    await settle(f);
    expect(state.form().scenarioSelection).toEqual({ kind: 'general', path: GENERAL.path });
    expect(state.form().scenarioTouched).toBe(true);
  });

  it('labels the dropdown `new-chat-scenario-select` from "Starting Scenario (Optional)"', async () => {
    const f = await renderWith(makeState(), scenarioCore([]));
    const label = [...(f.nativeElement as HTMLElement).querySelectorAll('label')].find((l) =>
      l.textContent!.includes('Starting Scenario (Optional)'),
    )!;
    expect(label.getAttribute('for')).toBe('new-chat-scenario-select');
    expect(scenarioSelect(f)).not.toBeNull();
  });

  it('hands the browser the hierarchy: the project, the cast’s groups, the lone LLM character', async () => {
    const state = makeState();
    state.selectedProjectId.set('proj-1');
    state.selectedCharacters.set([llmSeat(alice)]);
    state.groupScenarios.set([
      { ...GENERAL, groupId: 'g-1', groupName: 'One' },
      { ...GENERAL, path: 'Scenarios/b.md', groupId: 'g-1', groupName: 'One' },
      { ...GENERAL, path: 'Scenarios/c.md', groupId: 'g-2', groupName: 'Two' },
    ]);
    const f = await renderWith(state, scenarioCore([]));
    const hierarchy = (f.componentInstance as unknown as { scenarioHierarchy: () => unknown }).scenarioHierarchy();
    expect(hierarchy).toEqual({ projectId: 'proj-1', groupIds: ['g-1', 'g-2'], characterId: 'a' });
  });
});
