/**
 * Tab re-activation → TanStack Query invalidation (port of v4
 * `lib/workspace/tab-refetch.ts`, baseline `979652a9`).
 *
 * The workspace keeps every open tab's view mounted (hidden via CSS, never
 * unmounted), so a view's queries never remount — and without help, whatever a
 * tab showed when you left is what it still shows when you come back. This map
 * is the single source of truth for which query-key prefixes go stale when a
 * tab is re-activated (navigated back to). {@link TabView} invalidates them on
 * every hidden→visible transition; TanStack Query then refetches every mounted
 * observer, so sibling tabs sharing a prefix come back fresh too.
 *
 * **Deliberately left empty** (v4's roster and reasoning, quoted):
 *  - `salon` / `terminal` / `document` — "live surfaces fed by SSE/PTY; a
 *    blanket invalidation risks disturbing an in-flight stream".
 *  - `character-edit`, `character-new`, `document-standalone`,
 *    `settings-wizard` — "editors holding unsaved user state; refreshing under
 *    the user's feet could clobber work in progress".
 *  - `brahma` — "a console chat, live by nature".
 *  - `about` — "static".
 *  - `scenarios` / `wardrobe` — "their views fetch outside TanStack Query and
 *    re-run their own loads via `useOnTabActivated`" (v5: `onTabActivated`).
 *  - `salon-new` — **v5-only** (v4 opens New Chat as a modal). It is a form
 *    holding unsaved user input, so it belongs in the editor bucket by v4's own
 *    reasoning.
 *
 * **v5 mechanism divergences from v4's map** — v5 has no central key module
 * (keys live per vertical), and several v5 surfaces are hand-rolled where v4's
 * were TanStack-backed:
 *  - `scriptorium` — v4 invalidates `mountPoints.all`; v5 has NO TanStack key
 *    for document stores (`ScriptoriumStore` + `store-detail` hold signals), so
 *    the entry is empty and both views re-run their own loads via
 *    `onTabActivated`.
 *  - `photos` — v4 invalidates `photos.all`; v5's gallery is hand-rolled
 *    (`PhotosPage.loadInitial`) and re-runs its own load via `onTabActivated`.
 *  - `generate-image` — v4's `characters.all` reaches its TanStack character
 *    list; v5's `GenerateImagePage.loadEntities` is hand-rolled, so the page
 *    ALSO takes an explicit `onTabActivated`. The prefixes below still matter
 *    for the sibling tabs sharing them.
 *  - `character-view` — the three payload-scoped prefixes are v4's exactly; v5
 *    additionally mirrors v4's `dataRefreshKey` bump (stats / conversations /
 *    memories) from inside `CharacterDetail` itself, since those are separate
 *    v5 keys where v4 drove them off one React state counter.
 *  - `settings` — v4's `settings.generalState`, `settings.taboo` and
 *    `plugins.all` have NO v5 analogue: the general-state editor and the Taboo
 *    card fetch outside TanStack Query, and plugins are Rust-core (no client
 *    query). Recorded as a gap rather than invented.
 *
 * **Split key spellings.** Until a consolidation lane unifies them, three v5
 * keys exist in two spellings each and BOTH must be swept:
 *  - `['connectionProfiles']` (home, brahma console, dangerous-content,
 *    image-description, cheap-llm card, connection-profiles card, profile tag
 *    editor) and `['connection-profiles']` (character list/new/detail, salon
 *    conversation, add-character, create-npc, select-llm-profile).
 *  - `['apiKeys']` (api-keys card, connection-profiles card, image-profile
 *    modal) and `['api-keys']` (the chat sidebar's chat section).
 *  - `['chatSettings']` (salon list + conversation, new-chat form, autonomous
 *    room defaults, appearance tab, cheap-llm card, `chatSettingsKeys.all`) and
 *    `['chat-settings']` (the roleplay-templates card).
 *
 * @module workspace/core/tab-refetch
 */

import type { CharacterViewTabPayload, WorkspaceTab } from '../workspace-contract';
import { chatKeys } from '../../chat/chat-keys';
import { AUTONOMOUS_ROOMS_KEY } from '../../core/realtime-topic-map';
import { characterKeys, tagKeys } from '../../screens/characters/characters.api';
import { groupKeys } from '../../screens/groups/groups.api';
import { homeKeys } from '../../screens/home/home.api';
import { imageProfileKeys } from '../../screens/settings/images/image-profiles.api';
import { profileKeys } from '../../screens/profile/profile.api';
import { projectKeys } from '../../screens/prospero/projects.api';
import { templateKeys } from '../../screens/settings/templates/templates.api';

type QueryKeyPrefix = readonly unknown[];

/**
 * v4 `queryKeys.chats.lists` (`['chats','list']`) — "invalidates the collection
 * reads without touching per-chat detail/state/background, which open Salon
 * tabs hold live around a possible in-flight stream".
 *
 * v5 needs no narrowing: its per-chat keys are `['chat', id]` SINGULAR
 * (`['chat', id, 'outfit-summary']`, `['chatFilesList', id]`,
 * `['chatPhotoAlbums', id]`), so the bare `['chats']` prefix already reaches
 * only the collection reads and is detail/state-safe by construction.
 */
const CHAT_LISTS: QueryKeyPrefix = chatKeys.all;

/** Both live spellings of the connection-profile key (see the module doc). */
const CONNECTION_PROFILES: QueryKeyPrefix[] = [['connectionProfiles'], ['connection-profiles']];
/** Both live spellings of the API-keys key. */
const API_KEYS: QueryKeyPrefix[] = [['apiKeys'], ['api-keys']];
/** Both live spellings of the chat-settings key (v4 `queryKeys.settings.chat`). */
const CHAT_SETTINGS: QueryKeyPrefix[] = [['chatSettings'], ['chat-settings']];

/**
 * The query-key prefixes to invalidate when `tab` is re-activated. Empty for
 * tabs that must not be blanket-refreshed (live streams, editors) — see the
 * module doc for the roster.
 */
export function tabActivationQueryKeys(tab: WorkspaceTab): QueryKeyPrefix[] {
  switch (tab.kind) {
    case 'home':
      // The dashboard payload plus the entities it summarizes, so list tabs
      // sharing those prefixes come back fresh alongside it.
      return [homeKeys.all, CHAT_LISTS, projectKeys.all, characterKeys.all];
    case 'salon-list':
      return [
        CHAT_LISTS,
        characterKeys.all,
        ...CONNECTION_PROFILES,
        AUTONOMOUS_ROOMS_KEY,
        ...CHAT_SETTINGS,
      ];
    case 'aurora':
      return [characterKeys.all, groupKeys.all, tagKeys.all];
    case 'character-view': {
      const payload = tab.payload as CharacterViewTabPayload | undefined;
      if (!payload?.characterId) return [];
      return [
        characterKeys.detail(payload.characterId),
        characterKeys.prompts(payload.characterId),
        characterKeys.photos(payload.characterId),
      ];
    }
    case 'prospero':
      // v5's project detail is TanStack-backed under the same prefix, so this
      // one entry covers v4's hand-refetch of detail/chats/files/stores too.
      return [projectKeys.all];
    case 'files':
      return [['files']];
    case 'generate-image':
      return [characterKeys.all, imageProfileKeys.all];
    case 'custom-tools':
      // v5 keeps the per-chat roster, the run presets, the Workbench library
      // and its destinations all under one prefix.
      return [['customTools']];
    case 'profile':
      return [profileKeys.detail];
    case 'settings':
      return [
        ...CHAT_SETTINGS,
        ['textReplacements'],
        ...CONNECTION_PROFILES,
        ['embedding-profiles'],
        imageProfileKeys.all,
        ['providers'],
        ...API_KEYS,
        templateKeys.all,
      ];
    default:
      return [];
  }
}
