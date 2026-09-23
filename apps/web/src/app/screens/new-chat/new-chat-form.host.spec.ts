import { Component, input, output } from '@angular/core';
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { MarkdownField } from '../../editor/markdown-field';
import type { SavedScenarioTarget } from '../../scenario-builder/save-scenario-dialog';
import { ScenarioBuilderDialog } from '../../scenario-builder/scenario-builder-dialog';
import { NewChatForm } from './new-chat-form';
import { NewChatState, type RefetchedScenarioTiers } from './new-chat.state';
import type { GroupScenarioOption, NewChatFormState, ScenarioOption } from './new-chat.types';

/**
 * "Ask the Host to set the scene" on the New Chat form (v4 `NewChatForm.tsx`
 * at `d1c06cd9d`; its `NewChatForm.test.tsx` cases by name).
 *
 * v4's test mocks the lazily-loaded dialog with a stub whose buttons fire
 * `onUse` / `onSaved`; this does the same — the real dialog is swapped for a
 * stub with the same selector and outputs, and the spec emits through it. The
 * dialog itself is pinned by `scenario-builder-dialog.spec.ts`.
 */

@Component({
  selector: 'qt-scenario-builder-dialog',
  template: '<span data-testid="builder-stub"></span>',
})
class StubBuilderDialog {
  readonly cast = input<readonly { id: string; name: string }[]>([]);
  readonly projectId = input<string | null>(null);
  readonly projectName = input<string | null>(null);
  readonly chatId = input<string | null>(null);
  readonly closed = output<void>();
  readonly use = output<string>();
  readonly saved = output<SavedScenarioTarget>();
}

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

const stubCore = {
  dispatchData: async () => ({ profiles: [] }),
  dispatchExpect: async () => ({ type: 'chatSettings', data: {} }),
} as unknown as CoreClient;

const GENERAL_SCENARIO: ScenarioOption = {
  path: 'Scenarios/foggy-moor.md',
  filename: 'foggy-moor.md',
  name: 'Foggy Moor',
  isDefault: false,
  body: 'A foggy moor at dawn.',
};

const GROUP_OPTION: GroupScenarioOption = {
  path: 'Scenarios/aerodrome.md',
  filename: 'aerodrome.md',
  name: 'Aerodrome',
  isDefault: false,
  body: 'Dawn on the downs.',
  groupId: 'group-1',
  groupName: 'Aeronauts Club',
};

function makeState(
  profiles = [{ id: 'p1', name: 'Anthropic', modelName: 'claude' }],
): NewChatState {
  const state = new NewChatState(stubCore, {});
  state.loading.set(false);
  state.profiles.set(profiles);
  return state;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(state: NewChatState): Promise<ComponentFixture<NewChatForm>> {
  TestBed.configureTestingModule({
    imports: [NewChatForm],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: stubCore },
    ],
  });
  TestBed.overrideComponent(NewChatForm, {
    remove: { imports: [ScenarioBuilderDialog] },
    add: { imports: [StubBuilderDialog] },
  });
  await TestBed.compileComponents();
  const fixture = TestBed.createComponent(NewChatForm);
  fixture.componentRef.setInput('state', state);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function hostButton(fixture: ComponentFixture<NewChatForm>): HTMLButtonElement {
  const found = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => /Ask the Host to set the scene/i.test(b.textContent ?? ''),
  );
  if (!found) throw new Error('the Host button is not rendered');
  return found as HTMLButtonElement;
}

async function openBuilder(fixture: ComponentFixture<NewChatForm>): Promise<StubBuilderDialog> {
  hostButton(fixture).click();
  await settle(fixture);
  const stub = fixture.debugElement.query(By.directive(StubBuilderDialog));
  if (!stub) throw new Error('the builder dialog did not mount');
  return stub.componentInstance as StubBuilderDialog;
}

/** The scenario editor's re-key (v4's `data-remount-key`). */
function scenarioRecordKey(fixture: ComponentFixture<NewChatForm>): unknown {
  const field = fixture.debugElement
    .queryAll(By.directive(MarkdownField))
    .map((d) => d.componentInstance as MarkdownField)
    .find((f) => ['Starting scenario', 'Additional scenario notes'].includes(f.ariaLabel()));
  if (!field) throw new Error('the scenario editor is not rendered');
  return field.recordKey();
}

const FIVE_POINTERS = {
  scenario: 'old notes',
  scenarioId: 'preset-1',
  projectScenarioPath: 'project/x.md',
  generalScenarioPath: 'general/y.md',
  groupScenarioPath: 'group/z.md',
  groupScenarioGroupId: 'group-1',
} satisfies Partial<NewChatFormState>;

describe('NewChatForm — Ask the Host to set the scene (v4)', () => {
  it('is disabled while creating', async () => {
    const state = makeState();
    state.creating.set(true);
    const fixture = await render(state);
    expect(hostButton(fixture).disabled).toBe(true);
  });

  it('is disabled when there are no connection profiles', async () => {
    const fixture = await render(makeState([]));
    expect(hostButton(fixture).disabled).toBe(true);
  });

  it('is enabled with at least one profile and not creating', async () => {
    const fixture = await render(makeState());
    expect(hostButton(fixture).disabled).toBe(false);
  });

  it("fills state.scenario from the builder's scene and clears every preset pointer, and remounts the editor", async () => {
    const state = makeState();
    state.patchForm(FIVE_POINTERS);
    const fixture = await render(state);
    const keyBefore = scenarioRecordKey(fixture);
    const builder = await openBuilder(fixture);
    builder.use.emit('A scene from the Host.');
    await settle(fixture);
    const form = state.form();
    expect(form.scenario).toBe('A scene from the Host.');
    expect(form.scenarioId).toBeNull();
    expect(form.projectScenarioPath).toBeNull();
    expect(form.generalScenarioPath).toBeNull();
    expect(form.groupScenarioPath).toBeNull();
    expect(form.groupScenarioGroupId).toBeNull();
    expect(scenarioRecordKey(fixture)).not.toBe(keyBefore);
  });
});

describe('NewChatForm — selecting a scene the Host just filed (v4)', () => {
  async function saveWith(
    target: SavedScenarioTarget,
    fresh: RefetchedScenarioTiers,
    prepare?: (state: NewChatState) => void,
  ) {
    const state = makeState();
    state.patchForm({ scenario: 'old notes' });
    prepare?.(state);
    const refetch = vi.spyOn(state, 'refetchScenarioTiers').mockImplementation(async () => {
      if (fresh.general) state.generalScenarios.set(fresh.general);
      if (fresh.project) state.projectScenarios.set(fresh.project);
      if (fresh.group) state.groupScenarios.set(fresh.group);
      return fresh;
    });
    const fixture = await render(state);
    const keyBefore = scenarioRecordKey(fixture);
    const builder = await openBuilder(fixture);
    builder.saved.emit(target);
    await settle(fixture);
    return { state, fixture, refetch, keyBefore };
  }

  it('selects a group preset the re-read tiers now offer, and clears the custom text', async () => {
    const { state, refetch, fixture, keyBefore } = await saveWith(
      { kind: 'group', groupId: 'group-1', path: GROUP_OPTION.path },
      { general: [], project: null, group: [GROUP_OPTION] },
    );
    expect(refetch).toHaveBeenCalledTimes(1);
    expect(state.form().groupScenarioPath).toBe(GROUP_OPTION.path);
    expect(state.form().groupScenarioGroupId).toBe('group-1');
    expect(state.form().scenario).toBe('');
    expect(scenarioRecordKey(fixture)).not.toBe(keyBefore);
  });

  it('leaves the form alone when the saved tier is not offered here', async () => {
    const { state, fixture, keyBefore } = await saveWith(
      { kind: 'group', groupId: 'group-1', path: GROUP_OPTION.path },
      { general: [], project: null, group: null },
    );
    expect(state.form().scenario).toBe('old notes');
    expect(state.form().groupScenarioPath).toBeNull();
    expect(scenarioRecordKey(fixture)).toBe(keyBefore);
  });

  it('selects a general preset when the re-read general tier carries it', async () => {
    const { state } = await saveWith(
      { kind: 'general', path: GENERAL_SCENARIO.path },
      { general: [GENERAL_SCENARIO], project: null, group: null },
    );
    expect(state.form().generalScenarioPath).toBe(GENERAL_SCENARIO.path);
    expect(state.form().scenario).toBe('');
  });

  // --- beyond v4's cases ----------------------------------------------------

  it('a group preset for a DIFFERENT group at the same path is not offered', async () => {
    const { state } = await saveWith(
      { kind: 'group', groupId: 'group-2', path: GROUP_OPTION.path },
      { general: [], project: null, group: [GROUP_OPTION] },
    );
    expect(state.form().groupScenarioPath).toBeNull();
    expect(state.form().scenario).toBe('old notes');
  });

  it('selects a project preset only for the form’s own project', async () => {
    const project: ScenarioOption = { ...GENERAL_SCENARIO, path: 'Scenarios/siege.md' };
    const mine = await saveWith(
      { kind: 'project', projectId: 'proj-1', path: project.path },
      { general: [], project: [project], group: null },
      (state) => state.selectedProjectId.set('proj-1'),
    );
    expect(mine.state.form().projectScenarioPath).toBe(project.path);
    TestBed.resetTestingModule();
    const other = await saveWith(
      { kind: 'project', projectId: 'proj-2', path: project.path },
      { general: [], project: [project], group: null },
      (state) => state.selectedProjectId.set('proj-1'),
    );
    expect(other.state.form().projectScenarioPath).toBeNull();
    // v4 awaits the re-read BEFORE any arm is checked, so it runs regardless.
    expect(other.refetch).toHaveBeenCalledTimes(1);
  });

  it('a character save with ONE LLM character appends the scenario locally and selects it', async () => {
    const alice = char('char-1', 'Alice', {
      scenarios: [{ id: 'old', title: 'Old', content: 'Old scene.' }],
    });
    const { state } = await saveWith(
      {
        kind: 'character',
        characterId: 'char-1',
        scenarioId: 'scn-new',
        title: 'Rain',
        content: 'Rain on the cobbles.',
      },
      { general: [], project: null, group: null },
      (s) =>
        s.selectedCharacters.set([
          { character: alice, connectionProfileId: 'p1', controlledBy: 'llm' },
        ]),
    );
    expect(state.selectedCharacters()[0].character.scenarios).toEqual([
      { id: 'old', title: 'Old', content: 'Old scene.' },
      { id: 'scn-new', title: 'Rain', content: 'Rain on the cobbles.' },
    ]);
    expect(state.form().scenarioId).toBe('scn-new');
    expect(state.form().scenario).toBe('');
  });

  it('a character save with two LLM characters leaves the form alone', async () => {
    const { state } = await saveWith(
      {
        kind: 'character',
        characterId: 'char-1',
        scenarioId: 'scn-new',
        title: 'Rain',
        content: 'Rain.',
      },
      { general: [], project: null, group: null },
      (s) =>
        s.selectedCharacters.set([
          { character: char('char-1', 'Alice'), connectionProfileId: 'p1', controlledBy: 'llm' },
          { character: char('char-2', 'Bob'), connectionProfileId: 'p1', controlledBy: 'llm' },
        ]),
    );
    expect(state.form().scenarioId).toBeNull();
    expect(state.selectedCharacters()[0].character.scenarios).toEqual([]);
    expect(state.form().scenario).toBe('old notes');
  });
});

describe('NewChatForm — the builder’s inputs', () => {
  it('casts every selected character, and names the project from the list', async () => {
    const state = makeState();
    state.selectedCharacters.set([
      { character: char('char-1', 'Alice'), connectionProfileId: 'p1', controlledBy: 'llm' },
      {
        character: char('char-9', 'Me', { controlledBy: 'user' }),
        connectionProfileId: '',
        controlledBy: 'user',
      },
    ]);
    state.availableProjects.set([{ id: 'proj-1', name: 'The Siege', color: null }]);
    state.selectedProjectId.set('proj-1');
    const fixture = await render(state);
    const builder = await openBuilder(fixture);
    expect(builder.cast()).toEqual([
      { id: 'char-1', name: 'Alice' },
      { id: 'char-9', name: 'Me' },
    ]);
    expect(builder.projectId()).toBe('proj-1');
    expect(builder.projectName()).toBe('The Siege');
    expect(builder.chatId()).toBeNull();
  });

  it('closing the builder unmounts it', async () => {
    const fixture = await render(makeState());
    const builder = await openBuilder(fixture);
    builder.closed.emit();
    await settle(fixture);
    expect(fixture.debugElement.query(By.directive(StubBuilderDialog))).toBeNull();
  });
});

describe('NewChatState.refetchScenarioTiers (v4 useNewChat)', () => {
  const DTO = {
    path: 'Scenarios/x.md',
    filename: 'x.md',
    name: 'X',
    isDefault: false,
    body: 'X.',
  };

  function stateWith(dispatchData: (req: Record<string, unknown>) => Promise<unknown>) {
    const core = { dispatchData, dispatchExpect: stubCore.dispatchExpect } as unknown as CoreClient;
    const state = new NewChatState(core, {});
    state.loading.set(false);
    return state;
  }

  it('re-reads general only, with no project and no LLM cast', async () => {
    const seen: Record<string, unknown>[] = [];
    const state = stateWith(async (req) => {
      seen.push(req);
      return { scenarios: [DTO] };
    });
    const fresh = await state.refetchScenarioTiers();
    expect(seen).toEqual([{ type: 'scenarioList', includeArchived: false }]);
    expect(fresh.general).toHaveLength(1);
    expect(fresh.project).toBeNull();
    expect(fresh.group).toBeNull();
    expect(state.generalScenarios()).toEqual(fresh.general);
  });

  it('re-reads the project and the LLM cast’s groups, honouring Show archived', async () => {
    const seen: Record<string, unknown>[] = [];
    const state = stateWith(async (req) => {
      seen.push(req);
      if (req['type'] === 'groupScenariosUnion') {
        return { groupScenarios: [{ groupId: 'g-1', groupName: 'G', scenarios: [DTO] }] };
      }
      return { scenarios: [DTO] };
    });
    state.selectedProjectId.set('proj-1');
    state.showArchivedScenarios.set(true);
    state.selectedCharacters.set([
      { character: char('char-1', 'Alice'), connectionProfileId: 'p1', controlledBy: 'llm' },
      {
        character: char('char-9', 'Me', { controlledBy: 'user' }),
        connectionProfileId: '',
        controlledBy: 'user',
      },
    ]);
    const fresh = await state.refetchScenarioTiers();
    expect(seen).toEqual([
      { type: 'scenarioList', includeArchived: true },
      { type: 'projectScenarioList', projectId: 'proj-1', includeArchived: true },
      // The LLM cast only — the user's persona has no say in the group tier here.
      { type: 'groupScenariosUnion', characterIds: ['char-1'], includeArchived: true },
    ]);
    expect(fresh.group).toEqual([{ ...DTO, archived: false, groupId: 'g-1', groupName: 'G' }]);
    expect(state.projectScenarios()).toEqual(fresh.project);
    expect(state.groupScenarios()).toEqual(fresh.group);
  });

  it('a failed tier answers null and leaves that tier’s list alone', async () => {
    const state = stateWith(async (req) => {
      if (req['type'] === 'scenarioList') throw new Error('nope');
      return { scenarios: [DTO] };
    });
    state.generalScenarios.set([GENERAL_SCENARIO]);
    state.selectedProjectId.set('proj-1');
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const fresh = await state.refetchScenarioTiers();
    warn.mockRestore();
    expect(fresh.general).toBeNull();
    expect(fresh.project).toHaveLength(1);
    expect(state.generalScenarios()).toEqual([GENERAL_SCENARIO]);
  });
});
