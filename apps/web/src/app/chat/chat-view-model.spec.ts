import { describe, expect, it } from 'vitest';

import type { ChatDetail, MessageDto, ParticipantDetail } from '../core/core-contract';
import {
  buildRenderItems,
  resolveMessageAuthor,
  resolveToolRowAttributionMessage,
  splitSwipeGroups,
} from './chat-view-model';

function toolContent(extra: Record<string, unknown> = {}): string {
  return JSON.stringify({ toolName: 'rng', success: true, result: '17', ...extra });
}

function msg(over: Partial<MessageDto>): MessageDto {
  return {
    id: 'm',
    role: 'ASSISTANT',
    content: '',
    tokenCount: null,
    promptTokens: null,
    completionTokens: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    swipeGroupId: null,
    swipeIndex: null,
    participantId: null,
    attachments: [],
    provider: null,
    modelName: null,
    targetParticipantIds: null,
    isSilentMessage: null,
    systemSender: null,
    systemKind: null,
    hostEvent: null,
    customAnnouncer: null,
    carinaMeta: null,
    pendingExternalPrompt: null,
    pendingExternalPromptFull: null,
    pendingExternalAttachments: null,
    reasoningContent: null,
    reasoningSegments: null,
    ...over,
  };
}

describe('splitSwipeGroups', () => {
  it('drops SYSTEM-role messages', () => {
    const out = splitSwipeGroups([
      msg({ id: 'a', role: 'USER', createdAt: '2024-01-01T00:00:01.000Z' }),
      msg({ id: 's', role: 'SYSTEM', createdAt: '2024-01-01T00:00:02.000Z' }),
    ]);
    expect(out.messages.map((m) => m.id)).toEqual(['a']);
  });

  it('collapses a swipe group to the highest swipeIndex and records state', () => {
    const out = splitSwipeGroups([
      msg({ id: 'v0', swipeGroupId: 'g', swipeIndex: 0, createdAt: '2024-01-01T00:00:05.000Z' }),
      msg({ id: 'v2', swipeGroupId: 'g', swipeIndex: 2, createdAt: '2024-01-01T00:00:06.000Z' }),
      msg({ id: 'v1', swipeGroupId: 'g', swipeIndex: 1, createdAt: '2024-01-01T00:00:07.000Z' }),
    ]);
    expect(out.messages.map((m) => m.id)).toEqual(['v2']);
    expect(out.swipeStates['g']).toMatchObject({ current: 2, total: 3 });
    // Recorded variants are sorted ascending by swipeIndex.
    expect(out.swipeStates['g'].messages.map((m) => m.id)).toEqual(['v0', 'v1', 'v2']);
  });

  it('orders the collapsed flow by createdAt ascending', () => {
    const out = splitSwipeGroups([
      msg({ id: 'late', role: 'USER', createdAt: '2024-01-01T00:00:09.000Z' }),
      msg({ id: 'early', role: 'USER', createdAt: '2024-01-01T00:00:01.000Z' }),
    ]);
    expect(out.messages.map((m) => m.id)).toEqual(['early', 'late']);
  });
});

describe('buildRenderItems', () => {
  it('packs consecutive Staff announcements into one chip group and keeps Carina full', () => {
    const items = buildRenderItems([
      msg({ id: 'u', role: 'USER' }),
      msg({ id: 'h1', systemSender: 'host', systemKind: 'turn-pass' }),
      msg({ id: 'l1', systemSender: 'librarian', systemKind: 'saved' }),
      msg({ id: 'c', systemSender: 'carina', systemKind: 'carina-response' }),
      msg({ id: 'a', role: 'ASSISTANT' }),
    ]);
    expect(items.map((i) => i.type)).toEqual([
      'message', // user
      'announcement-group', // host + librarian
      'message', // carina (full row)
      'message', // assistant
    ]);
    const group = items[1];
    expect(group.type).toBe('announcement-group');
    if (group.type === 'announcement-group') {
      expect(group.chips.map((c) => c.sender)).toEqual(['The Host', 'The Librarian']);
      expect(group.chips[0].kind).toBe('nothing to add');
      expect(group.chips[0].importance).toBe('low');
    }
  });

  it('keeps a Pascal roll outcome as its own full row, not a chip (P4.6ba)', () => {
    const items = buildRenderItems([
      msg({ id: 'h1', systemSender: 'host', systemKind: 'turn-pass' }),
      msg({ id: 'p', systemSender: 'pascal', systemKind: 'custom-tool-result' }),
      msg({ id: 'h2', systemSender: 'host', systemKind: 'turn-pass' }),
    ]);
    // The Pascal row breaks the announcement run — it is a full message row.
    expect(items.map((i) => i.type)).toEqual([
      'announcement-group', // host
      'message', // pascal (full row)
      'announcement-group', // host
    ]);
    const pascal = items[1];
    expect(pascal.type === 'message' && pascal.message.id).toBe('p');
  });

  it('labels a nudge announcement "invited to speak" at medium importance (v4 6a8a77aa)', () => {
    const items = buildRenderItems([
      msg({ id: 'n', systemSender: 'host', systemKind: 'nudge' }),
    ]);
    const group = items[0];
    expect(group.type).toBe('announcement-group');
    if (group.type === 'announcement-group') {
      expect(group.chips[0].sender).toBe('The Host');
      expect(group.chips[0].kind).toBe('invited to speak');
      expect(group.chips[0].importance).toBe('medium');
    }
  });
});

const TOOL = (extra: Record<string, unknown> = {}) =>
  JSON.stringify({ toolName: 'rng', success: true, result: '17', ...extra });

describe('buildRenderItems — TOOL rows (P4.17)', () => {
  it('folds a character-initiated TOOL row into its host assistant (embedded, no tool item)', () => {
    const items = buildRenderItems([
      msg({ id: 'u', role: 'USER', createdAt: '2024-01-01T00:00:01.000Z' }),
      msg({ id: 'a', role: 'ASSISTANT', participantId: 'p1', createdAt: '2024-01-01T00:00:02.000Z' }),
      msg({ id: 't', role: 'TOOL', participantId: 'p1', content: TOOL(), createdAt: '2024-01-01T00:00:03.000Z' }),
    ]);
    expect(items.map((i) => i.type)).toEqual(['message', 'message']);
    const host = items[1];
    expect(host.type === 'message' && host.message.attachedToolMessages?.map((m) => m.id)).toEqual([
      't',
    ]);
  });

  it('renders an orphan character TOOL row (no host in its turn) as a standalone tool item', () => {
    const items = buildRenderItems([
      msg({ id: 'u', role: 'USER', createdAt: '2024-01-01T00:00:01.000Z' }),
      msg({ id: 't', role: 'TOOL', participantId: 'p1', content: TOOL(), createdAt: '2024-01-01T00:00:02.000Z' }),
    ]);
    expect(items.map((i) => i.type)).toEqual(['message', 'tool']);
  });

  it('renders a user-initiated Prospero TOOL row as a collapsed announcement chip', () => {
    const items = buildRenderItems([
      msg({ id: 'a', role: 'ASSISTANT', participantId: 'p1', createdAt: '2024-01-01T00:00:01.000Z' }),
      msg({
        id: 't',
        role: 'TOOL',
        systemSender: 'prospero',
        systemKind: 'tool-run',
        content: TOOL({ initiatedBy: 'user', operatorName: 'Charles' }),
        createdAt: '2024-01-01T00:00:02.000Z',
      }),
    ]);
    expect(items.map((i) => i.type)).toEqual(['message', 'announcement-group']);
    const group = items[1];
    expect(group.type === 'announcement-group' && group.chips[0].sender).toBe('Prospero');
    expect(group.type === 'announcement-group' && group.chips[0].message.id).toBe('t');
  });

  it('heads a user-initiated standalone TOOL row as the operator, not the last speaker (Bug 29)', () => {
    // A composer/Run-Tool result (initiatedBy: 'user', no systemSender) does not
    // fold and carries no participant. Before Bug 29 the avatar walk borrowed p1
    // from the assistant that spoke last; now the row heads as a USER row (the
    // operator's face) instead. The direct helper cases live in
    // resolveToolRowAttributionMessage's own describe.
    const items = buildRenderItems([
      msg({ id: 'a', role: 'ASSISTANT', participantId: 'p1', createdAt: '2024-01-01T00:00:01.000Z' }),
      msg({
        id: 't',
        role: 'TOOL',
        participantId: null,
        content: TOOL({ initiatedBy: 'user' }),
        createdAt: '2024-01-01T00:00:02.000Z',
      }),
    ]);
    const tool = items[1];
    expect(tool.type).toBe('tool');
    expect(tool.type === 'tool' && tool.message.role).toBe('USER');
    expect(tool.type === 'tool' && tool.message.participantId).toBeNull();
  });

  it('does not borrow across a USER boundary for the avatar walk', () => {
    const items = buildRenderItems([
      msg({ id: 'a', role: 'ASSISTANT', participantId: 'p1', createdAt: '2024-01-01T00:00:01.000Z' }),
      msg({ id: 'u', role: 'USER', createdAt: '2024-01-01T00:00:02.000Z' }),
      msg({
        id: 't',
        role: 'TOOL',
        participantId: null,
        content: TOOL(),
        createdAt: '2024-01-01T00:00:03.000Z',
      }),
    ]);
    const tool = items[2];
    expect(tool.type).toBe('tool');
    // No borrow: the USER row broke the walk.
    expect(tool.type === 'tool' && tool.message.participantId).toBeNull();
  });
});

describe('resolveMessageAuthor — the customAnnouncer character arm', () => {
  function participant(over: Partial<ParticipantDetail> = {}): ParticipantDetail {
    return {
      id: 'p-cast',
      type: 'CHARACTER',
      displayOrder: 0,
      isActive: true,
      controlledBy: 'llm',
      status: 'active',
      character: {
        id: 'char-cast',
        name: 'Friday',
        title: 'Companion Heart',
        avatarUrl: null,
        defaultImageId: null,
        defaultImage: null,
      },
      connectionProfile: null,
      imageProfile: null,
      createdAt: '2026-01-01T00:00:00.000Z',
      updatedAt: '2026-01-01T00:00:00.000Z',
      ...over,
    };
  }

  function chatDetail(over: Partial<ChatDetail> = {}): ChatDetail {
    return {
      id: 'chat-1',
      title: 'Tea',
      contextSummary: null,
      roleplayTemplateId: null,
      chatType: 'salon',
      createdAt: '2026-01-01T00:00:00.000Z',
      updatedAt: '2026-01-01T00:00:00.000Z',
      isPaused: false,
      isManuallyRenamed: false,
      participants: [participant()],
      user: { id: 'user1', name: 'Bertie', image: null },
      messages: [],
      projectId: null,
      projectName: null,
      turnSkippingEnabled: null,
      agentModeEnabled: false,
      resolvedAgentModeEnabled: false,
      agentModeSource: 'global',
      isDangerousChat: false,
      dangerCategories: [],
      conciergeOverride: null,
      offSceneCharacters: [],
      lastTurnParticipantId: null,
      ...over,
    };
  }

  /** The bubble as the writer stores it: no participantId, no systemSender. */
  const announcement = (characterId: string) =>
    msg({
      id: 'ann',
      role: 'ASSISTANT',
      participantId: null,
      systemSender: null,
      systemKind: 'announcement',
      customAnnouncer: { kind: 'character', characterId },
    });

  it('names an OFF-SCENE announcing character from offSceneCharacters', () => {
    const author = resolveMessageAuthor(
      announcement('char-revenant'),
      chatDetail({
        offSceneCharacters: [
          { id: 'char-revenant', name: 'Revenant', title: 'Security Officer', avatarUrl: null },
        ],
      }),
    );
    expect(author.name).toBe('Revenant');
    expect(author.title).toBe('Security Officer');
    expect(author.isUser).toBe(false);
  });

  it('THE GUARD: does not fall through to the first cast member', () => {
    // The shipped bug: with no character arm the row reached the role fallback
    // and wore the name of whichever character sorted first — here, Friday.
    const author = resolveMessageAuthor(
      announcement('char-revenant'),
      chatDetail({
        offSceneCharacters: [
          { id: 'char-revenant', name: 'Revenant', title: null, avatarUrl: null },
        ],
      }),
    );
    expect(author.name).not.toBe('Friday');
  });

  it('prefers a participant over the off-scene card when the announcer is in the cast', () => {
    const author = resolveMessageAuthor(
      announcement('char-cast'),
      chatDetail({
        offSceneCharacters: [
          { id: 'char-cast', name: 'Stale Copy', title: null, avatarUrl: null },
        ],
      }),
    );
    expect(author.name).toBe('Friday');
    expect(author.title).toBe('Companion Heart');
  });

  it('falls back to a legible placeholder when the character is gone', () => {
    const author = resolveMessageAuthor(announcement('char-deleted'), chatDetail());
    expect(author.name).toBe('Off-scene character');
    expect(author.avatarUrl).toBeNull();
  });

  it('still labels a custom-kind announcer by its display name', () => {
    const author = resolveMessageAuthor(
      msg({
        id: 'ann2',
        customAnnouncer: { kind: 'custom', displayName: 'The Management' },
      }),
      chatDetail(),
    );
    expect(author.name).toBe('The Management');
  });
});

/**
 * P4.26 — the chip/full-row membership rule, adjudicated against v4
 * `announcement-render-items.ts`'s `isCollapsedAnnouncement` (`ff12f491`). Two of
 * v4's three exemptions are KIND-scoped and v5 had them sender-scoped, so every
 * Suparṇā letter collapsed into a chip and every Pascal announcement escaped one.
 */
describe('buildRenderItems — v4’s exemptions are kind-scoped (P4.26)', () => {
  const types = (messages: MessageDto[]) => buildRenderItems(messages).map((i) => i.type);

  it('keeps a Suparṇā mail-delivery out of the chip group', () => {
    // "significant enough to read in full rather than pack into a chip" (v4).
    expect(types([msg({ id: 's', systemSender: 'suparna', systemKind: 'mail-delivery' })])).toEqual([
      'message',
    ]);
  });

  it('still chips a Suparṇā row of any OTHER kind', () => {
    expect(types([msg({ id: 's', systemSender: 'suparna', systemKind: 'announcement' })])).toEqual([
      'announcement-group',
    ]);
  });

  it('keeps a Pascal roll outcome out of the chip group', () => {
    expect(
      types([msg({ id: 'p', systemSender: 'pascal', systemKind: 'custom-tool-result' })]),
    ).toEqual(['message']);
  });

  it('chips a Pascal row of any other kind — the error chip is v4’s', () => {
    expect(
      types([msg({ id: 'p', systemSender: 'pascal', systemKind: 'custom-tool-error' })]),
    ).toEqual(['announcement-group']);
  });

  it('keeps every Carina answer full, whatever its kind', () => {
    expect(types([msg({ id: 'c', systemSender: 'carina', systemKind: null })])).toEqual(['message']);
  });

  it('breaks a chip run around the exempt rows rather than swallowing them', () => {
    expect(
      types([
        msg({ id: 'h', systemSender: 'host', systemKind: 'add' }),
        msg({ id: 's', systemSender: 'suparna', systemKind: 'mail-delivery' }),
        msg({ id: 'l', systemSender: 'librarian', systemKind: 'saved' }),
      ]),
    ).toEqual(['announcement-group', 'message', 'announcement-group']);
  });
});

/**
 * P4.26 — the Staff identity arm. v5 had none, so the three Staff rows that
 * render in FULL (Carina / Suparṇā / Pascal) fell through to the role fallback
 * and wore whichever cast character sorted first — dogfood finding #31's shape,
 * on a different set of rows.
 */
describe('resolveMessageAuthor — Staff senders (P4.26)', () => {
  const cast = (id: string, name: string): ParticipantDetail => ({
    id: `p-${id}`,
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy: 'llm',
    status: 'active',
    character: { id, name, title: null, avatarUrl: null, defaultImageId: null, defaultImage: null },
    connectionProfile: null,
    imageProfile: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
  });

  const chat = (over: Partial<ChatDetail> = {}): ChatDetail =>
    ({
      id: 'chat-1',
      participants: [cast('char-first', 'Aria')],
      user: { id: 'u', name: 'Bertie', image: null },
      offSceneCharacters: [],
      ...over,
    }) as ChatDetail;

  it('gives each Staff sender v4’s own name and portrait', () => {
    const table: [NonNullable<MessageDto['systemSender']>, string, string][] = [
      ['lantern', 'The Lantern', '/images/avatars/lantern-avatar.webp'],
      ['aurora', 'Aurora', '/images/avatars/aurora-avatar.webp'],
      ['librarian', 'The Librarian', '/images/avatars/librarian-avatar.webp'],
      ['concierge', 'The Concierge', '/images/avatars/concierge-avatar.webp'],
      ['prospero', 'Prospero', '/images/avatars/prospero-avatar.webp'],
      ['host', 'The Host', '/images/avatars/host-avatar.webp'],
      ['commonplaceBook', 'The Commonplace Book', '/images/avatars/commonplace-book-avatar.webp'],
      ['ariel', 'Ariel', '/images/avatars/ariel-avatar.webp'],
      ['suparna', 'Suparṇā', '/images/avatars/suparna-avatar.webp'],
      ['pascal', 'Pascal', '/images/avatars/pascal-avatar.webp'],
    ];
    for (const [sender, name, avatarUrl] of table) {
      const author = resolveMessageAuthor(msg({ systemSender: sender }), chat());
      expect(author).toMatchObject({ name, avatarUrl, isUser: false });
      // THE GUARD: never borrowed from the cast (the pre-P4.26 behavior).
      expect(author.name).not.toBe('Aria');
    }
  });

  it('is the only Staff member v4 gives a title', () => {
    expect(resolveMessageAuthor(msg({ systemSender: 'pascal' }), chat()).title).toBe(
      'the Croupier',
    );
    expect(resolveMessageAuthor(msg({ systemSender: 'host' }), chat()).title).toBeNull();
  });

  it('renders a Carina answer under the ANSWERER character, not Carina', () => {
    const author = resolveMessageAuthor(
      msg({
        systemSender: 'carina',
        carinaMeta: { answererId: 'char-answerer', question: 'What year is it?' },
      }),
      chat({ participants: [cast('char-first', 'Aria'), cast('char-answerer', 'Bram')] }),
    );
    expect(author.name).toBe('Bram');
  });

  it('reaches off-scene for an answerer who has left the room', () => {
    const author = resolveMessageAuthor(
      msg({
        systemSender: 'carina',
        carinaMeta: { answererId: 'char-gone', question: 'q' },
      }),
      chat({
        offSceneCharacters: [
          { id: 'char-gone', name: 'Cleo', title: 'the Absent', avatarUrl: 'x.webp' },
        ] as ChatDetail['offSceneCharacters'],
      }),
    );
    expect(author).toMatchObject({ name: 'Cleo', title: 'the Absent', avatarUrl: '/x.webp' });
  });

  it('names the Brahma Console’s pseudo-answerer, which has no character record', () => {
    const author = resolveMessageAuthor(
      msg({
        systemSender: 'carina',
        carinaMeta: { answererId: 'b4a4c0de-0000-4000-8000-000000000001', question: 'q' },
      }),
      chat(),
    );
    expect(author).toMatchObject({
      name: 'Brahma',
      avatarUrl: '/images/avatars/brahma-avatar.webp',
    });
  });

  it('falls back to "Carina" when the answerer cannot be resolved at all', () => {
    expect(resolveMessageAuthor(msg({ systemSender: 'carina' }), chat())).toMatchObject({
      name: 'Carina',
      avatarUrl: null,
    });
  });

  it('leaves an ordinary participant row untouched', () => {
    const author = resolveMessageAuthor(
      msg({ participantId: 'p-char-first' }),
      chat({ participants: [cast('char-first', 'Aria')] }),
    );
    expect(author.name).toBe('Aria');
  });
});

/**
 * Ported case-for-case from v4 `app/salon/[id]/group-tool-messages.test.ts`
 * (`resolveToolRowAttributionMessage` describe). v5 keeps the helper in
 * `chat-view-model.ts` — `buildRenderItems` is its only caller — so its cases
 * live here rather than in `group-tool-messages.spec.ts` (Bug 29).
 */
describe('resolveToolRowAttributionMessage (v4 group-tool-messages.ts)', () => {
  it('heads a user-initiated tool card with the operator, not the last speaker', () => {
    // The pending TOOL row is persisted before the user's own message, so the
    // nearest preceding assistant is an unrelated character (Bug 29).
    const other = msg({ id: 'other', role: 'ASSISTANT', content: 'char B just spoke', participantId: 'pB' });
    const userTool = msg({ id: 'ut', role: 'TOOL', content: toolContent({ initiatedBy: 'user' }) });
    const messages = [other, userTool];

    const resolved = resolveToolRowAttributionMessage(userTool, 1, messages);

    // Resolved as a USER row (operator's face) — it does NOT borrow pB.
    expect(resolved.role).toBe('USER');
    expect(resolved.participantId).toBeNull();
  });

  it('still borrows the calling character for a character-initiated tool row', () => {
    const caller = msg({ id: 'caller', role: 'ASSISTANT', content: 'I roll', participantId: 'pA' });
    const charTool = msg({ id: 'ct', role: 'TOOL', content: toolContent() });
    const messages = [caller, charTool];

    const resolved = resolveToolRowAttributionMessage(charTool, 1, messages);

    expect(resolved.role).toBe('TOOL');
    expect(resolved.participantId).toBe('pA');
  });

  it('does not borrow across a USER boundary for a character-initiated row', () => {
    const caller = msg({ id: 'caller', role: 'ASSISTANT', content: 'turn 1', participantId: 'pA' });
    const user = msg({ id: 'u', role: 'USER', content: 'next' });
    const charTool = msg({ id: 'ct', role: 'TOOL', content: toolContent() });
    const messages = [caller, user, charTool];

    const resolved = resolveToolRowAttributionMessage(charTool, 2, messages);

    // Walk stops at the USER boundary before reaching pA — row heads itself.
    // v4 asserts `undefined` (its Message.participantId is optional); v5's
    // MessageDto uses `null`, so the untouched row keeps its own `null`.
    expect(resolved.participantId).toBeNull();
  });

  it('heads a TOOL row that already knows its author with itself', () => {
    const sysTool = msg({ id: 'st', role: 'TOOL', content: toolContent(), systemSender: 'prospero' });
    expect(resolveToolRowAttributionMessage(sysTool, 0, [sysTool])).toBe(sysTool);

    const ownedTool = msg({ id: 'ot', role: 'TOOL', content: toolContent(), participantId: 'pC' });
    expect(resolveToolRowAttributionMessage(ownedTool, 0, [ownedTool])).toBe(ownedTool);
  });
});
