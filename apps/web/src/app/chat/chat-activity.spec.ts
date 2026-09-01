import { describe, expect, it } from 'vitest';

import { chatActivityAt } from './chat-activity';

/**
 * Parity with v4's `lib/chat/__tests__/chat-activity.test.ts` for the one export
 * the client uses, plus the operator question that transcription can silently
 * get wrong (`??` vs `||`). The server's own differential
 * (`chat_activity_equivalence`) measures the same rows against v4's real module.
 */
describe('chatActivityAt (the client twin)', () => {
  it('reports when a character last posted', () => {
    expect(
      chatActivityAt({
        lastMessageAt: '2026-05-01T00:00:00.000Z',
        createdAt: '2026-01-01T00:00:00.000Z',
      }),
    ).toBe('2026-05-01T00:00:00.000Z');
  });

  it('falls back to createdAt when nobody has posted', () => {
    expect(chatActivityAt({ lastMessageAt: null, createdAt: '2026-01-01T00:00:00.000Z' })).toBe(
      '2026-01-01T00:00:00.000Z',
    );
  });

  it('falls back to createdAt when lastMessageAt is absent entirely', () => {
    expect(chatActivityAt({ createdAt: '2026-01-01T00:00:00.000Z' })).toBe(
      '2026-01-01T00:00:00.000Z',
    );
  });

  it('never reaches for updatedAt — the drift the chokepoint exists to stop', () => {
    const chat = {
      lastMessageAt: null,
      createdAt: '2024-01-01T00:00:00.000Z',
      updatedAt: '2026-08-30T00:00:00.000Z',
    };
    expect(chatActivityAt(chat)).toBe('2024-01-01T00:00:00.000Z');
  });

  it('is NULLISH, not falsy — an empty stamp wins over createdAt, as v4s ?? does', () => {
    expect(chatActivityAt({ lastMessageAt: '', createdAt: '2026-01-01T00:00:00.000Z' })).toBe('');
  });
});
