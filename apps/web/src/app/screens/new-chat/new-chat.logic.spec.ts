import { describe, expect, it } from 'vitest';

import type { CharacterListItem } from '../../core/core-contract';
import {
  applyPlayAs,
  applyProfileChange,
  buildCreateRequest,
  generateTitle,
  scenarioSelectionPatch,
  scenarioSelectPatch,
  seedSelectedCharacter,
  sortRoster,
} from './new-chat.logic';
import {
  INITIAL_FORM_STATE,
  USER_CONTROLLED_PROFILE,
  type NewChatFormState,
  type NewChatSelectedCharacter,
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

function llm(c: CharacterListItem, connectionProfileId = 'profile-1'): NewChatSelectedCharacter {
  return { character: c, connectionProfileId, controlledBy: 'llm' };
}
function user(c: CharacterListItem): NewChatSelectedCharacter {
  return { character: c, connectionProfileId: '', controlledBy: 'user' };
}
function form(over: Partial<NewChatFormState> = {}): NewChatFormState {
  return { ...INITIAL_FORM_STATE, ...over };
}

describe('generateTitle', () => {
  it('is "New Chat" with no LLM characters', () => {
    expect(generateTitle([])).toBe('New Chat');
    expect(generateTitle([user(char('u', 'You'))])).toBe('New Chat');
  });
  it('names one, two, and three LLM characters verbatim', () => {
    const a = char('a', 'Alice');
    const b = char('b', 'Bob');
    const c = char('c', 'Carol');
    expect(generateTitle([llm(a)])).toBe('Chat with Alice');
    expect(generateTitle([llm(a), llm(b)])).toBe('Chat with Alice and Bob');
    expect(generateTitle([llm(a), llm(b), llm(c)])).toBe('Chat with Alice, Bob, and Carol');
  });
  it('collapses to a group title at four or more', () => {
    const cast = ['a', 'b', 'c', 'd'].map((id) => llm(char(id, id.toUpperCase())));
    expect(generateTitle(cast)).toBe('Group Chat (4 characters)');
  });
});

describe('applyPlayAs (in-place, v4 e2eb3d21 NewChatForm.test.tsx)', () => {
  it('flips exactly the chosen cast character to user, leaving the rest LLM', () => {
    const cast = [llm(char('a', 'Alice')), llm(char('c', 'Carol'))];
    const next = applyPlayAs(cast, 'a');
    const a = next.find((sc) => sc.character.id === 'a')!;
    const c = next.find((sc) => sc.character.id === 'c')!;
    expect(a.controlledBy).toBe('user');
    expect(a.connectionProfileId).toBe('');
    expect(c.controlledBy).toBe('llm');
    expect(c.connectionProfileId).toBe('profile-1');
  });

  it('"Chat as yourself" reverts a flipped default-LLM character to llm with no profile', () => {
    const cast = [user(char('a', 'Alice'))]; // default-LLM, currently the user persona
    const next = applyPlayAs(cast, '');
    expect(next).toHaveLength(1);
    expect(next[0].controlledBy).toBe('llm');
    expect(next[0].connectionProfileId).toBe('');
  });

  it('"Chat as yourself" reverts a default-user cast member to llm, keeping it in the cast', () => {
    // Bob is a default-user character added to the cast and currently the persona.
    // Reverting hands him back to the LLM IN PLACE (he stays in the cast) rather
    // than being removed (v4 e2eb3d21 changed this behavior).
    const alice = char('a', 'Alice');
    const bob = char('b', 'Bob', { controlledBy: 'user' });
    const next = applyPlayAs([llm(alice), user(bob)], '');
    expect(next).toHaveLength(2);
    const b = next.find((sc) => sc.character.id === 'b')!;
    expect(b.controlledBy).toBe('llm');
    expect(b.connectionProfileId).toBe('');
  });

  it('swapping the persona to another cast member removes no one (blanket revert)', () => {
    const alice = char('a', 'Alice');
    const bob = char('b', 'Bob', { controlledBy: 'user' });
    // Bob is the current user; picking Alice reverts Bob to llm (kept) and flips Alice to user.
    const next = applyPlayAs([llm(alice), user(bob)], 'a');
    expect(next).toHaveLength(2);
    const a = next.find((sc) => sc.character.id === 'a')!;
    const b = next.find((sc) => sc.character.id === 'b')!;
    expect(a.controlledBy).toBe('user');
    expect(a.connectionProfileId).toBe('');
    expect(b.controlledBy).toBe('llm');
    expect(b.connectionProfileId).toBe('');
  });
});

describe('applyProfileChange (the picker per-character select)', () => {
  it('the USER_CONTROLLED option flips to user + clears the profile', () => {
    const cast = [llm(char('a', 'Alice'))];
    const next = applyProfileChange(cast, 'a', USER_CONTROLLED_PROFILE);
    expect(next[0].controlledBy).toBe('user');
    expect(next[0].connectionProfileId).toBe('');
  });
  it('a real profile hands the entry back to the LLM', () => {
    const cast = [user(char('a', 'Alice'))];
    const next = applyProfileChange(cast, 'a', 'profile-9');
    expect(next[0].controlledBy).toBe('llm');
    expect(next[0].connectionProfileId).toBe('profile-9');
  });
});

describe('scenarioSelectPatch (v4 handleScenarioSelectChange)', () => {
  it('a project token sets projectScenarioPath and nulls the rest', () => {
    expect(scenarioSelectPatch('project:Scenarios/a.md')).toEqual({
      scenarioId: null,
      projectScenarioPath: 'Scenarios/a.md',
      generalScenarioPath: null,
      groupScenarioPath: null,
      groupScenarioGroupId: null,
    });
  });
  it('a group token splits groupId:path', () => {
    expect(scenarioSelectPatch('group:grp-1:Scenarios/b.md')).toEqual({
      scenarioId: null,
      projectScenarioPath: null,
      generalScenarioPath: null,
      groupScenarioPath: 'Scenarios/b.md',
      groupScenarioGroupId: 'grp-1',
    });
  });
  it('a general token sets generalScenarioPath', () => {
    expect(scenarioSelectPatch('general:Scenarios/c.md').generalScenarioPath).toBe(
      'Scenarios/c.md',
    );
  });
  it('a bare UUID is a character scenario', () => {
    expect(scenarioSelectPatch('uuid-123').scenarioId).toBe('uuid-123');
  });
  it('custom / empty clears every source', () => {
    expect(scenarioSelectPatch('__custom__')).toEqual({
      scenarioId: null,
      projectScenarioPath: null,
      generalScenarioPath: null,
      groupScenarioPath: null,
      groupScenarioGroupId: null,
    });
  });
  /**
   * The ONE arm v4's `44a8137e` refactor genuinely moved. Before the
   * extraction, `handleScenarioSelectChange` let a malformed group token fall
   * out of the group branch into the character arm and stored the whole token
   * as a `scenarioId`; the shared `scenarioValueToSelection` returns
   * `{ kind: 'custom' }` from inside that branch, so it now clears every
   * source. Unreachable from the rendered dropdown either way — pinned because
   * it moved.
   */
  it('a MALFORMED group token now clears every source (v4 44a8137e)', () => {
    expect(scenarioSelectPatch('group:no-colon-after-this')).toEqual({
      scenarioId: null,
      projectScenarioPath: null,
      generalScenarioPath: null,
      groupScenarioPath: null,
      groupScenarioGroupId: null,
    });
  });
  it('does NOT touch free-text scenario notes (the layering invariant)', () => {
    // The patch owns only the source fields; applied over typed notes, they survive.
    const next = {
      ...form({ scenario: 'typed notes' }),
      ...scenarioSelectPatch('general:Scenarios/c.md'),
    };
    expect(next.generalScenarioPath).toBe('Scenarios/c.md');
    expect(next.scenario).toBe('typed notes');
  });

});

describe('scenarioSelectionPatch (v4 44a8137e handleScenarioSelectionChange)', () => {
  it('maps each tier to its own source field, nulling the rest', () => {
    expect(scenarioSelectionPatch({ kind: 'character', scenarioId: 'uuid-1' })).toEqual({
      scenarioId: 'uuid-1',
      projectScenarioPath: null,
      generalScenarioPath: null,
      groupScenarioPath: null,
      groupScenarioGroupId: null,
    });
    expect(scenarioSelectionPatch({ kind: 'group', groupId: 'g1', path: 'c.md' })).toEqual({
      scenarioId: null,
      projectScenarioPath: null,
      generalScenarioPath: null,
      groupScenarioPath: 'c.md',
      groupScenarioGroupId: 'g1',
    });
    expect(scenarioSelectionPatch({ kind: 'custom' })).toEqual({
      scenarioId: null,
      projectScenarioPath: null,
      generalScenarioPath: null,
      groupScenarioPath: null,
      groupScenarioGroupId: null,
    });
  });
});

describe('buildCreateRequest (v4 handleCreateChat payload)', () => {
  it('builds participants with LLM profiles, omits profile for the user persona', () => {
    const alice = char('a', 'Alice');
    const bob = char('b', 'Bob', { controlledBy: 'user' });
    const body = buildCreateRequest(
      [llm(alice, 'p1'), { ...user(bob), selectedSystemPromptId: null }],
      form(),
      null,
      undefined,
    );
    expect(body.title).toBe('Chat with Alice');
    expect(body.participants).toEqual([
      { type: 'CHARACTER', characterId: 'a', connectionProfileId: 'p1', controlledBy: 'llm' },
      { type: 'CHARACTER', characterId: 'b', controlledBy: 'user' },
    ]);
  });

  it('honors the scenario precedence chain scenarioId > project > group > general', () => {
    const cast = [llm(char('a', 'Alice'))];
    const both = buildCreateRequest(
      cast,
      form({ scenarioId: 'sc-1', projectScenarioPath: 'p.md', generalScenarioPath: 'g.md' }),
      null,
      undefined,
    );
    expect(both.scenarioId).toBe('sc-1');
    expect(both.projectScenarioPath).toBeUndefined();
    const grp = buildCreateRequest(
      cast,
      form({ groupScenarioPath: 'b.md', groupScenarioGroupId: 'grp', generalScenarioPath: 'g.md' }),
      null,
      undefined,
    );
    expect(grp.groupScenarioPath).toBe('b.md');
    expect(grp.groupScenarioGroupId).toBe('grp');
    expect(grp.generalScenarioPath).toBeUndefined();
  });

  it('sends free-text scenario independently of a preset', () => {
    const body = buildCreateRequest(
      [llm(char('a', 'Alice'))],
      form({ scenario: 'notes', generalScenarioPath: 'g.md' }),
      null,
      undefined,
    );
    expect(body.scenario).toBe('notes');
    expect(body.generalScenarioPath).toBe('g.md');
  });

  it('drops timestampConfig when its mode is NONE, keeps it otherwise', () => {
    const cast = [llm(char('a', 'Alice'))];
    expect(
      buildCreateRequest(cast, form({ timestampConfig: { mode: 'NONE' } }), null, undefined)
        .timestampConfig,
    ).toBeUndefined();
    const kept = buildCreateRequest(
      cast,
      form({ timestampConfig: { mode: 'START_ONLY' } }),
      null,
      undefined,
    );
    expect(kept.timestampConfig).toEqual({ mode: 'START_ONLY' });
  });

  it('sends booleans only-when-true and optionals only when present', () => {
    const cast = [llm(char('a', 'Alice'))];
    const off = buildCreateRequest(cast, form(), null, undefined);
    expect(off.avatarGenerationEnabled).toBeUndefined();
    expect(off.imageProfileId).toBeUndefined();
    expect(off.projectId).toBeUndefined();
    expect(off.outfitSelections).toBeUndefined();
    expect(off.progressId).toBeUndefined();

    const on = buildCreateRequest(
      cast,
      form({
        avatarGenerationEnabled: true,
        imageProfileId: 'img-1',
        outfitSelections: [{ characterId: 'a', mode: 'default' }],
      }),
      'proj-1',
      'progress-1',
    );
    expect(on.avatarGenerationEnabled).toBe(true);
    expect(on.imageProfileId).toBe('img-1');
    expect(on.projectId).toBe('proj-1');
    expect(on.outfitSelections).toEqual([{ characterId: 'a', mode: 'default' }]);
    expect(on.progressId).toBe('progress-1');
  });
});

describe('sortRoster + seedSelectedCharacter', () => {
  it('sorts favorites > user-controlled > chat count > name', () => {
    const fav = char('f', 'Zed', { isFavorite: true });
    const usr = char('u', 'Mona', { controlledBy: 'user' });
    const busy = char('b', 'Nora', { _count: { chats: 9 } });
    const plain = char('p', 'Abe');
    const sorted = sortRoster([plain, busy, usr, fav]).map((c) => c.id);
    expect(sorted).toEqual(['f', 'u', 'b', 'p']);
  });

  it('seeds the default profile + default prompt', () => {
    const c = char('a', 'Alice', {
      defaultConnectionProfileId: 'p-def',
      defaultSystemPromptId: 'sp-2',
      systemPrompts: [
        { id: 'sp-1', name: 'One', isDefault: true },
        { id: 'sp-2', name: 'Two', isDefault: false },
      ],
    });
    const seeded = seedSelectedCharacter(c, [{ id: 'p-first' }]);
    expect(seeded.connectionProfileId).toBe('p-def');
    expect(seeded.selectedSystemPromptId).toBe('sp-2');
    expect(seeded.controlledBy).toBe('llm');
  });

  it('falls back to the first profile + the isDefault prompt', () => {
    const c = char('a', 'Alice', {
      systemPrompts: [
        { id: 'sp-1', name: 'One', isDefault: false },
        { id: 'sp-2', name: 'Two', isDefault: true },
      ],
    });
    const seeded = seedSelectedCharacter(c, [{ id: 'p-first' }]);
    expect(seeded.connectionProfileId).toBe('p-first');
    expect(seeded.selectedSystemPromptId).toBe('sp-2');
  });
});
