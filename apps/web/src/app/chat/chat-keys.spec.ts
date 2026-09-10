import { describe, expect, it } from 'vitest';

import { chatKeys } from './chat-keys';

/**
 * The sweep's guard (P4.D125). Its whole value is that the spellings didn't
 * move: `['chats']` for the collection, `['chat', id]` SINGULAR for the row —
 * so every pre-existing cache entry, and the prefix rule the workspace's
 * tab-refetch map leans on, still line up.
 */
describe('chatKeys (the swept chat query-key spellings)', () => {
  it('keeps the collection key as the bare plural prefix', () => {
    expect(chatKeys.all).toEqual(['chats']);
  });

  it('keeps the detail key SINGULAR, so the collection prefix cannot reach it', () => {
    expect(chatKeys.detail('chat-1')).toEqual(['chat', 'chat-1']);
    expect(chatKeys.detail('chat-1')[0]).not.toBe(chatKeys.all[0]);
  });

  it('is the prefix of every per-chat sub-key the app already writes', () => {
    const detail = chatKeys.detail('chat-1');
    for (const sub of [
      ['chat', 'chat-1', 'background'],
      ['chat', 'chat-1', 'outfit-summary'],
      ['chat', 'chat-1', 'cost', 3],
      chatKeys.gallery('chat-1'),
    ]) {
      expect(sub.slice(0, detail.length)).toEqual([...detail]);
    }
  });

  it('accepts a nullable id, preserving the key those call sites already built', () => {
    expect(chatKeys.detail(null)).toEqual(['chat', null]);
  });

  describe('gallery (P4.D176 — v4 `queryKeys.chats.gallery`, bug 129)', () => {
    it('is the raw shape the gallery query and the Organize count both read', () => {
      expect(chatKeys.gallery('chat-1')).toEqual(['chat', 'chat-1', 'gallery']);
    });

    it("sits under the SAME prefix the 'chats' realtime topic invalidates", () => {
      // realtime-topic-map.ts's `chats` row returns `chatKeys.detail(id)` as
      // ONE prefix — this is the invariant that comment leans on: a Lantern
      // backdrop or an Aurora repaint's `{topic:'chats', id}` publish must
      // reach the gallery query with no dedicated row of its own.
      const prefix = chatKeys.detail('chat-1');
      const gallery = chatKeys.gallery('chat-1');
      expect(gallery.slice(0, prefix.length)).toEqual([...prefix]);
    });

    it('is a per-chat key, not reachable from the bare collection prefix', () => {
      expect(chatKeys.gallery('chat-1')[0]).not.toBe(chatKeys.all[0]);
    });
  });
});
