import { describe, expect, it } from 'vitest';

import type { CharacterListItem } from '../../core/core-contract';
import {
  applyPlayAs,
  applyProfileChange,
  buildCreateRequest,
  clearCharacterScenarioSelection,
  generateTitle,
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

/**
 * v4 `545e299bb` DELETED the five-pointer patches (`scenarioSelectPatch` /
 * `scenarioSelectionPatch`, P4.D302): the form holds ONE `scenarioSelection`,
 * so a pick is a plain `{ scenarioSelection, scenarioTouched: true }` write
 * (pinned in the form specs) and the token parsing is `scenario.types`'s own
 * (pinned there and in the recorded oracle). What remains here is the cast
 * change's rule (v4 `NewChatModal.tsx:236-242`).
 */
describe('clearCharacterScenarioSelection (v4 NewChatModal onCharactersChanged, 545e299bb)', () => {
  it('a character selection leaves with the cast that offered it — to Custom, touched as it was', () => {
    for (const touched of [false, true]) {
      const prev = form({
        scenarioSelection: { kind: 'character', scenarioId: 'sc-1' },
        scenarioTouched: touched,
        scenario: 'notes',
      });
      expect(clearCharacterScenarioSelection(prev)).toEqual({
        ...prev,
        scenarioSelection: { kind: 'custom' },
      });
    }
  });

  it('every other selection stands, and the state object is returned untouched', () => {
    for (const selection of [
      { kind: 'custom' },
      { kind: 'project', path: 'p.md' },
      { kind: 'general', path: 'g.md' },
      { kind: 'group', groupId: 'g1', path: 'c.md' },
      { kind: 'store', mountPointId: 'mp', path: 'Scenarios/x.md', label: 'X › x' },
    ] as const) {
      const prev = form({ scenarioSelection: selection, scenarioTouched: true });
      expect(clearCharacterScenarioSelection(prev)).toBe(prev);
    }
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

  /**
   * v4 `components/new-chat/__tests__/useNewChat.request-body.test.tsx:127-171`
   * ("useNewChat create request — subprompts"), transcribed 1:1 against
   * `buildCreateRequest` — v5's home for the same rule (v4
   * `useNewChat.ts:751-768`).
   */
  describe('subprompts', () => {
    it('omits selectedSubpromptIds when none are ticked, so a plain create is unchanged (v4 :146-150)', () => {
      const body = buildCreateRequest([llm(char('a', 'Alice'))], form(), null, undefined);
      const [participant] = body.participants as unknown as Array<Record<string, unknown>>;
      expect(participant).not.toHaveProperty('selectedSubpromptIds');
    });

    it('…and an EMPTY array is an omission too, not an empty key', () => {
      const body = buildCreateRequest(
        [{ ...llm(char('a', 'Alice')), selectedSubpromptIds: [] }],
        form(),
        null,
        undefined,
      );
      const [participant] = body.participants as unknown as Array<Record<string, unknown>>;
      expect(participant).not.toHaveProperty('selectedSubpromptIds');
    });

    it('sends the ticked subprompt ids verbatim for an LLM seat (v4 :152-156)', () => {
      const body = buildCreateRequest(
        [{ ...llm(char('a', 'Alice')), selectedSubpromptIds: ['terse', 'verse'] }],
        form(),
        null,
        undefined,
      );
      const [participant] = body.participants as unknown as Array<Record<string, unknown>>;
      expect(participant['selectedSubpromptIds']).toEqual(['terse', 'verse']);
    });

    it('flipping a seat to user-controlled KEEPS the ids in state — only the body omits them', () => {
      // v4's user-flip handler does not clear `selectedSubpromptIds` either
      // (measured at `2f4254b42`: no seed or flip site was touched), so
      // flipping back restores the selection rather than silently losing it.
      const flipped = applyProfileChange(
        [{ ...llm(char('a', 'Alice')), selectedSubpromptIds: ['terse'] }],
        'a',
        USER_CONTROLLED_PROFILE,
      );
      expect(flipped[0].selectedSubpromptIds).toEqual(['terse']);
      const body = buildCreateRequest(flipped, form(), null, undefined);
      const [participant] = body.participants as unknown as Array<Record<string, unknown>>;
      expect(participant).not.toHaveProperty('selectedSubpromptIds');
    });

    it('never sends subprompts for a user-controlled seat (v4 :158-170)', () => {
      const body = buildCreateRequest(
        [
          { ...llm(char('a', 'Alice')), selectedSubpromptIds: ['terse'] },
          { ...user(char('b', 'Bob')), selectedSubpromptIds: ['stray'] },
        ],
        form(),
        null,
        undefined,
      );
      const participants = body.participants as unknown as Array<Record<string, unknown>>;
      expect(participants[0]['selectedSubpromptIds']).toEqual(['terse']);
      expect(participants[1]).not.toHaveProperty('selectedSubpromptIds');
    });
  });

  // P4.D302 (v4 `545e299bb` `useNewChat.ts:846`): the precedence chain over
  // five fields is gone — the ONE selection's payload is sent, exactly one
  // pointer or none. (This row pinned `scenarioId > project > group > general`
  // over the five fields before.)
  it('sends exactly the selection’s one pointer — or none for Custom', () => {
    const cast = [llm(char('a', 'Alice'))];
    const pointers = ['scenarioRef', 'scenarioId', 'projectScenarioPath', 'generalScenarioPath', 'groupScenarioPath', 'groupScenarioGroupId'];
    const sent = (selection: NewChatFormState['scenarioSelection']) => {
      const body = buildCreateRequest(cast, form({ scenarioSelection: selection }), null, undefined) as unknown as Record<string, unknown>;
      return Object.fromEntries(pointers.filter((k) => k in body).map((k) => [k, body[k]]));
    };
    expect(sent({ kind: 'custom' })).toEqual({});
    expect(sent({ kind: 'character', scenarioId: 'sc-1' })).toEqual({ scenarioId: 'sc-1' });
    expect(sent({ kind: 'project', path: 'p.md' })).toEqual({ projectScenarioPath: 'p.md' });
    expect(sent({ kind: 'general', path: 'g.md' })).toEqual({ generalScenarioPath: 'g.md' });
    expect(sent({ kind: 'group', groupId: 'grp', path: 'b.md' })).toEqual({
      groupScenarioPath: 'b.md',
      groupScenarioGroupId: 'grp',
    });
    expect(sent({ kind: 'store', mountPointId: 'mp-9', path: 'Scenarios/x.md', label: 'Bob › X' })).toEqual({
      scenarioRef: { mountPointId: 'mp-9', path: 'Scenarios/x.md' },
    });
  });

  it('places the pointer after the notes, where v4’s `Object.assign` puts it', () => {
    const body = buildCreateRequest(
      [llm(char('a', 'Alice'))],
      form({ scenario: 'notes', scenarioSelection: { kind: 'general', path: 'g.md' } }),
      null,
      undefined,
    );
    const keys = Object.keys(body);
    expect(keys.indexOf('generalScenarioPath')).toBe(keys.indexOf('scenario') + 1);
  });

  it('sends free-text scenario independently of a preset', () => {
    const body = buildCreateRequest(
      [llm(char('a', 'Alice'))],
      form({ scenario: 'notes', scenarioSelection: { kind: 'general', path: 'g.md' } }),
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

/**
 * The New Chat form's Concierge pick, on the create request (v4
 * `useNewChat.request-body.test.tsx` at `ce2f1dabf`, three `it(` names 1:1,
 * plus the #76 arms no v4 test pins).
 *
 * v4 `useNewChat.ts:820-829` omits the key ONLY when the pick is Moderated AND
 * the server's default (`newChatsStartAs` on duty) is Moderated — so a plain
 * create stays byte-identical, while under an Unmoderated default an explicit
 * `'moderated'` IS sent (absence would mean Unmoderated to the server).
 *
 * v4's suite drives the whole `useNewChat` hook against a stubbed `fetch` and
 * reads the POST body back; v5's body is built by the pure
 * {@link buildCreateRequest}, so the names are transcribed onto it.
 */
describe('useNewChat create request — Concierge state', () => {
  const cast = [llm(char('char-alice', 'Alice'))];

  it('defaults to Moderated on the form', () => {
    expect(INITIAL_FORM_STATE.conciergeState).toBe('moderated');
  });

  it('omits conciergeState entirely when Moderated', () => {
    const body = buildCreateRequest(cast, form({ conciergeState: 'moderated' }), null, undefined);
    expect(body).not.toHaveProperty('conciergeState');
    // The rest of the request is unchanged — the participant still rides along.
    expect(body.participants).toHaveLength(1);
  });

  it.each(['unmoderated', 'locked'] as const)('sends conciergeState verbatim when %s', (state) => {
    const body = buildCreateRequest(cast, form({ conciergeState: state }), null, undefined);
    expect(body.conciergeState).toBe(state);
  });

  it('SENDS an explicit Moderated when new chats start Unmoderated (v4 :827)', () => {
    const body = buildCreateRequest(cast, form({ conciergeState: 'moderated' }), null, undefined, {
      templateDefaultsLoaded: false,
      conciergeServerDefault: 'unmoderated',
    });
    expect(body.conciergeState).toBe('moderated');
  });

  it('still omits a Moderated pick under a Moderated server default', () => {
    const body = buildCreateRequest(cast, form({ conciergeState: 'moderated' }), null, undefined, {
      templateDefaultsLoaded: false,
      conciergeServerDefault: 'moderated',
    });
    expect(body).not.toHaveProperty('conciergeState');
  });

  // Never `null`: the key is either absent or one of the three values, because
  // v4's `createChatSchema` is `.optional()` and NOT nullable.
  it('never sends null', () => {
    for (const state of ['moderated', 'unmoderated', 'locked'] as const) {
      const body = buildCreateRequest(cast, form({ conciergeState: state }), null, undefined);
      expect(body.conciergeState).not.toBeNull();
    }
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

  /**
   * v4 `baa85e19b` (bug 154): the broken ternary read the column and, when it
   * named a prompt the character no longer had, seeded NOTHING — the chat
   * opened with no system prompt at all. Neither existing seed spec above can
   * see it: one passes a VALID column, the other passes none.
   */
  it('keeps the flagged prompt when the column names a prompt the character no longer has', () => {
    const c = char('a', 'Alice', {
      defaultSystemPromptId: 'gone',
      systemPrompts: [
        { id: 'sp-1', name: 'One', isDefault: false },
        { id: 'sp-2', name: 'Two', isDefault: true },
      ],
    });
    const seeded = seedSelectedCharacter(c, [{ id: 'p-first' }]);
    expect(seeded.selectedSystemPromptId).toBe('sp-2');
  });

  it('falls all the way to the first prompt when the column is stale and nothing is flagged', () => {
    const c = char('a', 'Alice', {
      defaultSystemPromptId: 'gone',
      systemPrompts: [
        { id: 'sp-1', name: 'One', isDefault: false },
        { id: 'sp-2', name: 'Two', isDefault: false },
      ],
    });
    expect(seedSelectedCharacter(c, [{ id: 'p-first' }]).selectedSystemPromptId).toBe('sp-1');
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
