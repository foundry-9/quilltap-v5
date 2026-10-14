import { Injector } from '@angular/core';
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { NewChatForm } from './new-chat-form';
import { NewChatState, type NewChatOptions } from './new-chat.state';
import type { NewChatFormState, NewChatSelectedCharacter } from './new-chat.types';

/**
 * New Chat holds ONE scenario selection (v4 `545e299bb`):
 * `components/new-chat/__tests__/useNewChat.request-body.test.tsx` at the pin
 * `83334b9a5`, "create request — scenario" (5 cases), transcribed case for
 * case against {@link NewChatState}, plus this lane's red-first ANCHOR — the
 * defect `545e299bb` fixed, which v5 reproduced: the `?characterId=` entrance
 * into a project with a default SHOWED the project's scene and SUBMITTED the
 * character's.
 *
 * The seed is an `effect` (P4.D302 L-I), so the state is built with an
 * injector and `TestBed.tick()` flushes it — v4's `waitFor` around the hook.
 * Form reads go through a structural view of the form state so the rows also
 * compile against the pre-port five-pointer shape (the red-first measurement).
 */

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
    defaultConnectionProfileId: 'p1',
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

const PROJECT_ID = 'proj-1';
const PROJECT_DEFAULT = {
  path: 'Scenarios/atrium.md',
  filename: 'atrium.md',
  name: 'The Atrium',
  isDefault: true,
  body: 'Glass overhead.',
};
const GENERAL_DEFAULT = {
  path: 'Scenarios/moor.md',
  filename: 'moor.md',
  name: 'The Moor',
  isDefault: true,
  body: 'Fog at dawn.',
};
const STUDY = { id: 'sc-study', title: 'Her Study', content: 'Books to the ceiling.' };
const ALICE = char('alice', 'Alice');
const ALICE_WITH_DEFAULT = char('alice', 'Alice', { scenarios: [STUDY], defaultScenarioId: STUDY.id });

function llm(c: CharacterListItem): NewChatSelectedCharacter {
  return { character: c, connectionProfileId: 'p1', selectedSystemPromptId: null, controlledBy: 'llm' };
}

interface Rig {
  state: NewChatState;
  created: () => Record<string, unknown>;
}

/** Like v4's `installScenarioStub`: each scenario tier answers with its own default. */
function rig(options: NewChatOptions = {}, characters: CharacterListItem[] = []): Rig {
  let createBody: Record<string, unknown> | null = null;
  const core = {
    dispatchData: async (req: Record<string, unknown>) => {
      switch (req['type']) {
        case 'characterList':
          return { characters };
        case 'characterGet':
          return { character: characters.find((c) => c.id === req['characterId']) ?? null };
        case 'characterDefaultPartner':
          return { partnerId: null };
        case 'projectScenarioList':
          return { scenarios: [PROJECT_DEFAULT] };
        case 'projectGet':
          return { project: { id: PROJECT_ID, name: 'Glasshouse' } };
        case 'scenarioList':
          return { scenarios: [GENERAL_DEFAULT] };
        case 'projectList':
          return { projects: [] };
        case 'groupScenariosUnion':
          return { groupScenarios: [] };
        case 'roleplayTemplateList':
          return [];
        default:
          return {};
      }
    },
    dispatchExpect: async (req: Record<string, unknown>) => {
      if (req['type'] === 'chatCreate') {
        createBody = req;
        return { type: 'chatCreate', data: { chat: { id: 'chat-new' } } };
      }
      return req['type'] === 'chatSettings'
        ? { type: 'chatSettings', data: {} }
        : { type: 'connectionProfiles', data: { profiles: [{ id: 'p1', name: 'Anthropic' }] } };
    },
  } as unknown as CoreClient;
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [NewChatForm],
    providers: [
      { provide: CoreClient, useValue: core },
      provideTanStackQuery(new QueryClient()),
      provideRouter([]),
    ],
  });
  // The injector argument is NEW in this lane; constructed structurally so the
  // rows also compile against the pre-port constructor.
  const Ctor = NewChatState as unknown as new (...args: unknown[]) => NewChatState;
  const state = new Ctor(core, options, null, null, TestBed.inject(Injector));
  return {
    state,
    created: () => {
      expect(createBody).not.toBeNull();
      const { type: _t, ...body } = createBody!;
      return body;
    },
  };
}

/** A structural view of the form, so the rows compile on either shape. */
const formOf = (state: NewChatState) => state.form() as unknown as Record<string, unknown>;

async function flush(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    TestBed.tick();
  }
}

async function renderLoaded(options: NewChatOptions = {}, characters: CharacterListItem[] = []): Promise<Rig> {
  const r = rig(options, characters);
  await r.state.load();
  await flush();
  return r;
}

async function submit(r: Rig): Promise<Record<string, unknown>> {
  await r.state.handleCreate();
  return r.created();
}

describe('useNewChat create request — scenario (v4 useNewChat.request-body.test.tsx @ 83334b9a5)', () => {
  it("shows and sends the character's default over the project's (regression: display and submit agreed to differ)", async () => {
    const r = await renderLoaded({ projectId: PROJECT_ID });
    r.state.setSelectedCharacters(() => [llm(ALICE_WITH_DEFAULT)]);
    await flush();

    expect(formOf(r.state)['scenarioSelection']).toEqual({ kind: 'character', scenarioId: STUDY.id });

    const body = await submit(r);
    expect(body['scenarioId']).toBe(STUDY.id);
    expect(body).not.toHaveProperty('projectScenarioPath');
    expect(body).not.toHaveProperty('generalScenarioPath');
    expect(body).not.toHaveProperty('scenarioRef');
  });

  it('falls to the project default when the character has none', async () => {
    const r = await renderLoaded({ projectId: PROJECT_ID });
    r.state.setSelectedCharacters(() => [llm(ALICE)]);
    await flush();
    expect(formOf(r.state)['scenarioSelection']).toEqual({ kind: 'project', path: PROJECT_DEFAULT.path });

    const body = await submit(r);
    expect(body['projectScenarioPath']).toBe(PROJECT_DEFAULT.path);
    expect(body).not.toHaveProperty('scenarioId');
    expect(body).not.toHaveProperty('generalScenarioPath');
  });

  it('falls to the general default with no project', async () => {
    const r = await renderLoaded();
    r.state.setSelectedCharacters(() => [llm(ALICE)]);
    await flush();
    expect(formOf(r.state)['scenarioSelection']).toEqual({ kind: 'general', path: GENERAL_DEFAULT.path });

    const body = await submit(r);
    expect(body['generalScenarioPath']).toBe(GENERAL_DEFAULT.path);
  });

  it('never re-seeds a selection the user picked by hand', async () => {
    const r = await renderLoaded({ projectId: PROJECT_ID });
    r.state.form.update(
      (prev) =>
        ({
          ...prev,
          scenarioSelection: { kind: 'custom' },
          scenarioTouched: true,
          scenario: 'My own scene.',
          // The pre-port shape's pointers, nulled, so the row means the same thing there.
          projectScenarioPath: null,
          generalScenarioPath: null,
        }) as unknown as NewChatFormState,
    );
    // A cast change brings a character default along; the hand pick stands.
    r.state.setSelectedCharacters(() => [llm(ALICE_WITH_DEFAULT)]);
    await flush();
    expect(r.state.selectedCharacters()).toHaveLength(1);
    expect(formOf(r.state)['scenarioSelection']).toEqual({ kind: 'custom' });

    const body = await submit(r);
    expect(body['scenario']).toBe('My own scene.');
    for (const key of ['scenarioId', 'projectScenarioPath', 'generalScenarioPath', 'groupScenarioPath', 'scenarioRef']) {
      expect(body).not.toHaveProperty(key);
    }
  });

  it('sends a store pick as a scenarioRef, with the notes beside it', async () => {
    const r = await renderLoaded();
    r.state.setSelectedCharacters(() => [llm(ALICE)]);
    r.state.form.update(
      (prev) =>
        ({
          ...prev,
          scenarioSelection: {
            kind: 'store',
            mountPointId: 'mp-9',
            path: 'Scenarios/borrowed.md',
            label: 'Bob › Borrowed',
          },
          scenarioTouched: true,
          scenario: 'And it is raining.',
        }) as unknown as NewChatFormState,
    );
    await flush();

    const body = await submit(r);
    expect(body['scenarioRef']).toEqual({ mountPointId: 'mp-9', path: 'Scenarios/borrowed.md' });
    expect(body['scenario']).toBe('And it is raining.');
    expect(body).not.toHaveProperty('generalScenarioPath');
  });
});

describe('the red-first anchor — the ?characterId= entrance into a project with a default', () => {
  async function renderForm(state: NewChatState): Promise<ComponentFixture<NewChatForm>> {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(NewChatForm);
    fixture.componentRef.setInput('state', state);
    fixture.detectChanges();
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return fixture;
  }

  it("SHOWS the character's default scene and CREATES with it — the two agree", async () => {
    const r = await renderLoaded({ initialCharacterId: 'alice', projectId: PROJECT_ID }, [ALICE_WITH_DEFAULT]);
    const fixture = await renderForm(r.state);
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      '#new-chat-scenario-select',
    ) as HTMLSelectElement;
    // What the user SEES…
    expect(select.value).toBe(STUDY.id);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Books to the ceiling.');
    expect((fixture.nativeElement as HTMLElement).textContent).not.toContain('Glass overhead.');
    // …is what the chat is created with.
    const body = await submit(r);
    expect(body['scenarioId']).toBe(STUDY.id);
    expect(body).not.toHaveProperty('projectScenarioPath');
  });
});

describe('the seed effect (P4.D302 L-I)', () => {
  it("keeps the SAME selection object when a re-seed lands on the same token (v4's guard)", async () => {
    const r = await renderLoaded({ projectId: PROJECT_ID });
    r.state.setSelectedCharacters(() => [llm(ALICE)]);
    await flush();
    const held = formOf(r.state)['scenarioSelection'];
    const formBefore = r.state.form();
    expect(held).toEqual({ kind: 'project', path: PROJECT_DEFAULT.path });
    // The GENERAL default moves, so the effect re-runs — but the project
    // default still wins, the same `<option>` token. (A refetch that leaves
    // every default where it was never re-runs the effect at all: the three
    // inputs are `computed` VALUES, v4's dependency array.)
    r.state.generalScenarios.set([{ ...GENERAL_DEFAULT, path: 'Scenarios/heath.md' }]);
    await flush();
    expect(formOf(r.state)['scenarioSelection']).toBe(held);
    expect(r.state.form()).toBe(formBefore);
  });

  it('re-seeds when a default moves, while untouched', async () => {
    const r = await renderLoaded({ projectId: PROJECT_ID });
    r.state.setSelectedCharacters(() => [llm(ALICE)]);
    await flush();
    r.state.projectScenarios.set([{ ...PROJECT_DEFAULT, isDefault: false }]);
    await flush();
    // No project default any more → the general default.
    expect(formOf(r.state)['scenarioSelection']).toEqual({ kind: 'general', path: GENERAL_DEFAULT.path });
  });

  it("skips the character's default for a continuation (v4's arm; unreachable in v5 production — Tier 3 item 14)", async () => {
    const r = await renderLoaded({ projectId: PROJECT_ID, continuationFromChatId: 'chat-old' });
    r.state.setSelectedCharacters(() => [llm(ALICE_WITH_DEFAULT)]);
    await flush();
    expect(formOf(r.state)['scenarioSelection']).toEqual({ kind: 'project', path: PROJECT_DEFAULT.path });
  });

  it('offers no character default with two LLM characters in the cast', async () => {
    const r = await renderLoaded({ projectId: PROJECT_ID });
    r.state.setSelectedCharacters(() => [llm(ALICE_WITH_DEFAULT), llm(char('bob', 'Bob', { defaultScenarioId: 'sc-bob' }))]);
    await flush();
    expect(formOf(r.state)['scenarioSelection']).toEqual({ kind: 'project', path: PROJECT_DEFAULT.path });
  });
});
