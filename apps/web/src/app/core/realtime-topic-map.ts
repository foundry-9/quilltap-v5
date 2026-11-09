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
import { customToolsKeys } from '../chat/custom-tools.api';
import { mountPointKeys } from '../document-browser/document-store.keys'; // === P4.D387 ===
import { memoryKeys } from '../memory/memory.api';
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
 * v5's has ten: v4's rows in v4's order (P4.D177 §C.4 added `memories`, v4
 * `4a9be9878`, bug 128; P4.D387 the `mountPoints` row, `mountPointKeys.all`;
 * P4.D346 `customTools`), with `chatKeys.all` listed a second time for the
 * memories badge. v4's six `inventory` / `wardrobe` list prefixes have no row
 * here: v5's item stores are signal-held (P4.D330 R-2), not query keys.
 */
export const ALL_REALTIME_PREFIXES: readonly QueryKeyPrefix[] = [
  systemJobsKeys.all,
  tasksQueueKeys.all,
  AUTONOMOUS_ROOMS_KEY,
  chatKeys.all,
  projectKeys.all,
  characterKeys.all,
  // === P4.D387 (v4 `topic-map.ts:35`, after `characters.all`) ===
  mountPointKeys.all,
  // === end P4.D387 ===
  // `memories` (below): the Salon LIST's memory badge (the same `chatKeys.all`
  // prefix the `chats` row above already sweeps — listed again, matching v4's
  // topic-per-row shape) and EVERY memories key — v4's
  // `queryKeys.memories.all` (`topic-map.ts:35`; R-AB, P4.D300 L-6).
  chatKeys.all,
  memoryKeys.all,
  // === P4.D346 (v4 839575d89, `topic-map.ts:38-40`) ===
  // Held custom-tool runs (operator intervention). Also under the custom-tools
  // namespace the popup reads, which a reconnect may as well refresh too.
  customToolsKeys.all,
  // === end P4.D346 ===
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
      // === P4.D176: `chatKeys.gallery(id)` (`['chat', id, 'gallery']`) sits
      // under this same prefix — no separate row needed; the story-background
      // and avatar jobs already publish `{topic:'chats', id}`, which is what
      // refreshes a Lantern backdrop or an Aurora repaint into the gallery for
      // free. Pinned by `chat-keys.spec.ts`. === end P4.D176 ===
      // === P4.D206: so does `chatKeys.informs(id)` (`['chat', id, 'informs']`),
      // v4's own new row here (`e7d77bb60` adds `queryKeys.chats.informs(id)`
      // beside `gallery` in its row-scoped `chats` entry). v5 needs no entry for
      // the same reason it needed none for the gallery — the ONE `['chat', id]`
      // prefix is the parent of both — so what v4 spends a row on, v5 spends an
      // assertion on (`chat-keys.spec.ts`, `realtime-topic-map.spec.ts`). The
      // refresh story is the same either way: every event that changes a pending
      // inform already publishes `chats`, so the composer chips stay honest
      // without a poll of their own. === end P4.D206 ===
      return id ? [chatKeys.detail(id)] : [chatKeys.all];

    case 'projects':
      // v4: detail / state / background. v5 has no project `state` key (the
      // state editor fetches outside TanStack Query — recorded, not invented),
      // and its background key is spelled in the projects file's own idiom.
      return id ? [projectKeys.detail(id), projectKeys.background(id)] : [projectKeys.all];

    case 'characters':
      // The wardrobe dialog's item list is signal-held (no query key here can
      // reach it), so `WardrobeControlDialogInner` subscribes to this topic
      // DIRECTLY through `RealtimeService.onTopic` — v4's wardrobe views
      // refresh on a finished wardrobe-picture job this way (P4.D261, R-H).
      return id
        ? [
            characterKeys.detail(id),
            characterKeys.prompts(id),
            characterKeys.subprompts(id),
            characterKeys.photos(id),
          ]
        : [characterKeys.all];

    // === P4.D387 ===
    case 'mountPoints':
      // v4 `topic-map.ts:158-161` (`c34986421`): scoped by store id, that
      // store's detail, listing and link lookups (all under
      // `mountPointKeys.store(id)`), plus the store lists (their counts move).
      // Bare, the namespace. The keys are the document-store browser's
      // (`document-browser/document-store.keys.ts`, v4's spellings).
      return id ? [mountPointKeys.store(id), mountPointKeys.lists] : [mountPointKeys.all];
    // === end P4.D387 ===

    case 'memories':
      // v4 narrows to a per-chat memory count (`queryKeys.memories.chatCount
      // (id)`) when scoped, and takes `queryKeys.memories.all` collection-wide
      // (`topic-map.ts:102-106`). v5 holds the same reader (`memoryKeys.chatCount`,
      // the sidebar's Delete Memories (n)); the Salon LIST card's badge lives
      // under `chatKeys.all` whether the hint carried a chat id or not (P4.D177
      // §C.4, a measured mapping divergence from v4's — unchanged). The UNSCOPED
      // hint takes EVERY memories key, as v4's does (R-AB, P4.D300): the
      // maintenance ledger (`['memories', 'maintenance-status']`), the counts
      // and the settings documents refresh after a maintenance run commits.
      // The P4.D177 §C.4 / P4.D276 R-A narrowing to the per-chat count prefix
      // RETIRED here.
      return [chatKeys.all, id ? memoryKeys.chatCount(id) : memoryKeys.all];

    case 'inventory':
      // ⚠ NO v5 TARGET (P4.D330 R-2). v4 invalidates FOUR keys —
      // `inventory.lists`, `.folderLists`, `.archetypeLists` and
      // `.equippedLists` (`0276d10d9`, `topic-map.ts:134-146` at `236c04e35`).
      // v5's Inventory stores are signal-held (the wardrobe's idiom, R-1), so
      // the Inventory dialog subscribes to this topic DIRECTLY through
      // `RealtimeService.onTopic` and performs the same four re-reads — its
      // list, its folder tree, the open archetype picker, and the Carrying-now
      // pane's carried read (P4.D359) — the `characters` row's precedent.
      // Nothing to sweep on reconnect either, so `ALL_REALTIME_PREFIXES` is
      // unchanged (v4's reconnect sweep adds all four).
      return [];

    // === P4.D386 ===
    case 'wardrobe':
      // ⚠ NO v5 TARGET (the P4.D330 R-2 precedent). v4 `600c2906f`
      // (`topic-map.ts:150-156` at the pin): "A wardrobe file or folder was
      // written — through any wardrobe location (the operator's dialog, a
      // character's wardrobe_* tool, a Pascal effect), or by a job child's
      // buffered write; an open browser's lists and folder trees re-read",
      // returning `[queryKeys.wardrobe.lists, queryKeys.wardrobe.folderLists]`.
      // v5's wardrobe stores are signal-held, so the Wardrobe dialog and the
      // Prospero Wardrobe card subscribe through `RealtimeService.onTopic`
      // and re-read their list and folder tree. Nothing to sweep on
      // reconnect, so `ALL_REALTIME_PREFIXES` is unchanged.
      return [];
    // === end P4.D386 ===

    // === P4.D344: Storyteller mode ===
    case 'storytellerRounds':
      // v4 `topic-map.ts:101-103` byte for byte (`queryKeys.chats.all` ↔
      // `chatKeys.all`). The scoped key sits under `chatKeys.detail(id)`'s
      // prefix, so it is NOT a reconnect-sweep prefix of its own — exactly as
      // `chatKeys.informs` is not (P4.D344 item 3); the bare arm's
      // `chatKeys.all` is already swept, so `ALL_REALTIME_PREFIXES` is unchanged.
      return id ? [chatKeys.storytellerRound(id)] : [chatKeys.all];
    // === end P4.D344 ===

    // === P4.D346 (v4 839575d89, `topic-map.ts:100-102`) ===
    case 'customToolInterventions':
      // Scoped by chat id: the hold dialog for that chat re-reads its queue.
      return id ? [customToolsKeys.interventions(id)] : [customToolsKeys.all];
    // === end P4.D346 ===

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
