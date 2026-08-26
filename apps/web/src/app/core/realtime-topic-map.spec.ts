import { describe, expect, it } from 'vitest';

import { chatKeys } from '../chat/chat-keys';
import { characterKeys } from '../screens/characters/characters.api';
import { projectKeys } from '../screens/prospero/projects.api';
import { storyBackgroundKeys } from '../screens/salon/story-background.api';
import { systemJobsKeys } from '../layout/system-jobs.api';
import { tasksQueueKeys } from '../screens/settings/system/tasks-queue.api';
import { REALTIME_TOPICS, realtimeHintFromFrame } from './realtime.types';
import {
  ALL_REALTIME_PREFIXES,
  AUTONOMOUS_ROOMS_KEY,
  queryKeysForTopic,
} from './realtime-topic-map';

/**
 * Parity specs against v4 `lib/realtime/topic-map.ts` and its own
 * `__tests__/unit/realtime/topic-map.test.ts` at `f3892158d`, adapted to v5's
 * per-feature key consts (v4's decision-8 twin). Structure is v4's; targets are
 * v5's, and every row that differs says why in the map's own comments.
 */

describe('queryKeysForTopic (v4 lib/realtime/topic-map.ts)', () => {
  it('jobs drives BOTH the chips and the tasks queue', () => {
    expect(queryKeysForTopic('jobs')).toEqual([systemJobsKeys.all, tasksQueueKeys.all]);
    // v4 ignores an id on this topic; so does v5.
    expect(queryKeysForTopic('jobs', 'anything')).toEqual([
      systemJobsKeys.all,
      tasksQueueKeys.all,
    ]);
  });

  it('autonomousRooms drives the one room key', () => {
    expect(queryKeysForTopic('autonomousRooms')).toEqual([AUTONOMOUS_ROOMS_KEY]);
  });

  it('chats: collection-wide without an id, row-scoped with one', () => {
    expect(queryKeysForTopic('chats')).toEqual([chatKeys.all]);
    expect(queryKeysForTopic('chats', 'chat-7')).toEqual([chatKeys.detail('chat-7')]);
  });

  it('the row-scoped chats prefix reaches the per-chat background key', () => {
    // v4 lists detail/state/background separately; v5's single `['chat', id]`
    // prefix is the parent of all three spellings, which is why one entry does.
    const [prefix] = queryKeysForTopic('chats', 'chat-7') as readonly (readonly unknown[])[];
    const background = storyBackgroundKeys.background('chat-7');
    expect(background.slice(0, prefix.length)).toEqual([...prefix]);
  });

  it('a row-scoped chats hint must NOT reach the collection reads', () => {
    const [prefix] = queryKeysForTopic('chats', 'chat-7') as readonly (readonly unknown[])[];
    expect(prefix[0]).not.toBe(chatKeys.all[0]);
  });

  it('projects: detail + background when scoped, the namespace otherwise', () => {
    expect(queryKeysForTopic('projects')).toEqual([projectKeys.all]);
    expect(queryKeysForTopic('projects', 'p-1')).toEqual([
      projectKeys.detail('p-1'),
      projectKeys.background('p-1'),
    ]);
  });

  it('characters: v4\'s exact detail/prompts/photos trio when scoped', () => {
    expect(queryKeysForTopic('characters')).toEqual([characterKeys.all]);
    expect(queryKeysForTopic('characters', 'c-1')).toEqual([
      characterKeys.detail('c-1'),
      characterKeys.prompts('c-1'),
      characterKeys.photos('c-1'),
    ]);
  });

  it('mountPoints is RECOGNISED but has no v5 target (recorded gap, not a bug)', () => {
    // v4 invalidates `queryKeys.mountPoints.all`; v5 has no document-store query
    // key at all (the same gap `workspace/core/tab-refetch.ts` records). The row
    // exists so the topic is known rather than unknown.
    expect(REALTIME_TOPICS).toContain('mountPoints');
    expect(queryKeysForTopic('mountPoints')).toEqual([]);
  });

  it('an unknown topic is ignored, never thrown on', () => {
    expect(queryKeysForTopic('memories')).toEqual([]);
    expect(queryKeysForTopic('')).toEqual([]);
    expect(() => queryKeysForTopic('a-topic-from-a-newer-server', 'x')).not.toThrow();
  });

  it('every topic with a target contributes it to the reconnect sweep', () => {
    for (const topic of REALTIME_TOPICS) {
      for (const prefix of queryKeysForTopic(topic)) {
        expect(ALL_REALTIME_PREFIXES).toContainEqual(prefix);
      }
    }
  });
});

describe('realtimeHintFromFrame (§B.5 discrimination + v4\'s safeParse)', () => {
  it('accepts the wire shape §B.2 pins, with and without a scope id', () => {
    expect(realtimeHintFromFrame({ v: 1, topic: 'jobs', at: 17 })).toEqual({
      v: 1,
      topic: 'jobs',
      at: 17,
    });
    expect(realtimeHintFromFrame({ v: 1, topic: 'chats', id: 'c-1', at: 17 })).toEqual({
      v: 1,
      topic: 'chats',
      id: 'c-1',
      at: 17,
    });
  });

  it('an unknown topic still PARSES — the map, not the parser, does the ignoring', () => {
    expect(realtimeHintFromFrame({ v: 1, topic: 'somethingNew', at: 1 })?.topic).toBe(
      'somethingNew',
    );
  });

  it('rejects the OTHER frames sharing this stream (the discrimination rule)', () => {
    // A chat-stream frame.
    expect(realtimeHintFromFrame({ chatId: 'c-1', type: 'token', content: 'hi' })).toBeNull();
    // A creation-progress frame.
    expect(realtimeHintFromFrame({ progressId: 'p-1', kind: 'status' })).toBeNull();
    // A frame that carries a `topic` but no `v` is not ours either.
    expect(realtimeHintFromFrame({ topic: 'jobs' })).toBeNull();
    // …nor a `v` without a `topic`.
    expect(realtimeHintFromFrame({ v: 1, at: 1 })).toBeNull();
  });

  it('drops a malformed hint rather than throwing (v4 `if (!parsed.success) return`)', () => {
    expect(realtimeHintFromFrame({ v: 2, topic: 'jobs', at: 1 })).toBeNull();
    expect(realtimeHintFromFrame({ v: 1, topic: 7, at: 1 })).toBeNull();
    expect(realtimeHintFromFrame({ v: 1, topic: 'jobs' })).toBeNull();
    expect(realtimeHintFromFrame({ v: 1, topic: 'jobs', at: 'soon' })).toBeNull();
    expect(realtimeHintFromFrame({ v: 1, topic: 'chats', id: 9, at: 1 })).toBeNull();
    expect(realtimeHintFromFrame(null)).toBeNull();
    expect(realtimeHintFromFrame('a string')).toBeNull();
  });

  it('carries NONE of the scope keys the other frames use (§B.2)', () => {
    const hint = realtimeHintFromFrame({
      v: 1,
      topic: 'chats',
      id: 'c-1',
      at: 5,
      chatId: 'nope',
    });
    expect(hint).not.toBeNull();
    expect(Object.keys(hint!).sort()).toEqual(['at', 'id', 'topic', 'v']);
  });
});
