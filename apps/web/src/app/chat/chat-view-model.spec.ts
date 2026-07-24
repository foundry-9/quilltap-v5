import { describe, expect, it } from 'vitest';

import type { MessageDto } from '../core/core-contract';
import { buildRenderItems, splitSwipeGroups } from './chat-view-model';

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

  it('borrows the preceding assistant participant for a standalone TOOL row with none (avatar walk)', () => {
    // A user-initiated tool WITHOUT a systemSender (no Prospero label) does not
    // fold and carries no participant — the walk borrows p1 from the assistant.
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
    expect(tool.type === 'tool' && tool.message.participantId).toBe('p1');
  });

  it('does not borrow across a USER boundary for the avatar walk', () => {
    const items = buildRenderItems([
      msg({ id: 'a', role: 'ASSISTANT', participantId: 'p1', createdAt: '2024-01-01T00:00:01.000Z' }),
      msg({ id: 'u', role: 'USER', createdAt: '2024-01-01T00:00:02.000Z' }),
      msg({
        id: 't',
        role: 'TOOL',
        participantId: null,
        content: TOOL({ initiatedBy: 'user' }),
        createdAt: '2024-01-01T00:00:03.000Z',
      }),
    ]);
    const tool = items[2];
    expect(tool.type).toBe('tool');
    // No borrow: the USER row broke the walk.
    expect(tool.type === 'tool' && tool.message.participantId).toBeNull();
  });
});
