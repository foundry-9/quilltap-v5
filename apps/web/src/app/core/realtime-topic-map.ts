/**
 * Realtime topic → query-key mapping — v5's twin of v4 `lib/realtime/topic-map.ts`.
 *
 * The one table translating a server topic into the TanStack Query keys it
 * makes stale. v4 keeps it boring by naming topics after `lib/query/keys.ts`
 * namespaces (decision 8): adding an entity is one row here and one row in
 * `REALTIME_TOPICS`.
 *
 * **v5's twin of decision 8.** v5 has no central key module — keys are
 * per-feature consts — so the table imports each vertical's own const rather
 * than quoting a spelling. Every row below names the v5 key family it targets
 * and, where the shape differs from v4's, why.
 *
 * An unrecognised topic is ignored, deliberately. A tab left open across a
 * server upgrade will be handed topics its build has never heard of, and the
 * correct response is to shrug — never to throw inside a stream handler.
 *
 * @module core/realtime-topic-map
 */

import { chatKeys } from '../chat/chat-keys';
import { characterKeys } from '../screens/characters/characters.api';
import { projectKeys } from '../screens/prospero/projects.api';
import { storyBackgroundKeys } from '../screens/salon/story-background.api';
import { systemJobsKeys } from '../layout/system-jobs.api';
import { tasksQueueKeys } from '../screens/settings/system/tasks-queue.api';

/** A query key prefix to invalidate. */
export type QueryKeyPrefix = readonly unknown[];

/**
 * v4 `queryKeys.system.autonomousRooms`. v5's spelling is the raw
 * `['systemAutonomousRooms']` written at its two live sites
 * (`screens/salon/salon-list.ts`, `workspace/core/tab-refetch.ts`); it has no
 * const of its own, so the table carries it here rather than inventing a key
 * module the rest of the app would not import.
 */
export const AUTONOMOUS_ROOMS_KEY: QueryKeyPrefix = ['systemAutonomousRooms'];

/**
 * Every prefix this build knows how to invalidate, used for the reconnect
 * catch-up sweep — a client that was offline has no idea what it missed, so it
 * re-reads everything the stream could have told it about.
 *
 * v4's list has seven entries; v5's has six, because the `mountPoints` row has
 * nothing to target (below).
 */
export const ALL_REALTIME_PREFIXES: readonly QueryKeyPrefix[] = [
  systemJobsKeys.all,
  tasksQueueKeys.all,
  AUTONOMOUS_ROOMS_KEY,
  chatKeys.all,
  projectKeys.all,
  characterKeys.all,
];

/**
 * Resolve the query-key prefixes a topic invalidates.
 *
 * When `id` is present the change is row-scoped, and we narrow to that row's
 * keys rather than sweeping the whole namespace — an avatar landing in one chat
 * should not refetch every other open Salon tab.
 *
 * @returns The prefixes to invalidate; empty for an unknown topic.
 */
export function queryKeysForTopic(topic: string, id?: string): readonly QueryKeyPrefix[] {
  switch (topic) {
    case 'jobs':
      // The chips and the tasks queue read the same queue through different
      // endpoints, so one topic drives both (v4's own comment).
      return [systemJobsKeys.all, tasksQueueKeys.all];

    case 'autonomousRooms':
      return [AUTONOMOUS_ROOMS_KEY];

    case 'chats':
      // v4 narrows to detail/state/background. v5's per-chat keys are
      // `['chat', id]` SINGULAR with every sub-key beneath it
      // (`['chat', id, 'background' | 'outfit-summary' | 'cost']`), so ONE
      // prefix reaches the same three things v4 lists — and, because the
      // collection key is the plural word, still never touches the lists.
      return id ? [chatKeys.detail(id)] : [chatKeys.all];

    case 'projects':
      // v4: detail / state / background. v5 has no project `state` key (the
      // state editor fetches outside TanStack Query — recorded, not invented),
      // and its background key is spelled in the projects file's own idiom.
      return id ? [projectKeys.detail(id), projectKeys.background(id)] : [projectKeys.all];

    case 'characters':
      return id
        ? [characterKeys.detail(id), characterKeys.prompts(id), characterKeys.photos(id)]
        : [characterKeys.all];

    case 'mountPoints':
      // ⚠ NO v5 TARGET. v4 invalidates `queryKeys.mountPoints.all` — "no
      // per-store detail key exists yet; the namespace prefix is the narrowest
      // thing there is". v5 has no store key AT ALL: `ScriptoriumStore` and the
      // store-detail view hold plain signals and re-run their own loads via
      // `onTabActivated` (the same gap `workspace/core/tab-refetch.ts` records
      // for its `scriptorium` row). The row exists so the topic is recognised
      // rather than logged as unknown; it resolves to nothing until a document
      // store vertical grows a query key.
      return [];

    default:
      return [];
  }
}

/**
 * The story-background key is reached through the `chats` row's `['chat', id]`
 * prefix (it is `['chat', id, 'background']`). Exported only so the assertion
 * that this stays true has something to name.
 */
export const STORY_BACKGROUND_KEY_FOR = storyBackgroundKeys.background;
