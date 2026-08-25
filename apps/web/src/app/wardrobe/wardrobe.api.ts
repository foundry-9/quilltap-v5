/**
 * The wardrobe-dialog data layer: the §1 verb types this lane consumes and the
 * four-tier item loader (v4 `lib/hooks/use-character-wardrobe-items.ts`).
 *
 * §1 (the P4.9f1/P4.9f2 Shared contract): the request types for lane P4.9f1's
 * NEW verbs — `chatOutfitGet`, `chatEquip` (all seven modes, v4's enum order,
 * incl. the deprecated `equip` alias for `wear`), `wardrobeTransferDestinations`
 * / `wardrobeTransferApply`, the GLOBAL archetype tier `wardrobeList` /
 * `wardrobeCreate` / `wardrobeUpdate` / `wardrobeDelete`, and (tier 2)
 * `wardrobePreviewAvatar` + `chatRegenerateAvatar` — were FOLDED into
 * `core-contract.ts` at the round's unification and the cast retired; the
 * name-for-name wire diff ran there against `api/types.rs` and every §1 name
 * and payload matched. They are re-exported below so this module stays the
 * wardrobe data-layer entry point. Field names are v4's route bodies verbatim.
 *
 * Landed verbs consumed directly (no §1 dependency):
 * `characterWardrobe{List,Create,Get,Update,Delete}`, `projectWardrobe*`,
 * `characterList`, `imageProfileList`, `chatGet`.
 *
 * §2 (the P4.D112/P4.D113 Shared contract): the FIVE `groupWardrobe*` verbs and
 * the two `wardrobeTransferApply` body additions (`source`, `components`) are
 * declared HERE, in the same idiom §1 used before it was folded — lane P4.D112
 * delivers them in `api/types.rs`, this lane mirrors them name-for-name, and the
 * unifier folds them into `core-contract.ts` and runs the wire diff. Until then
 * `dispatchWardrobe` carries the §2 cast. Field names are the contract's
 * verbatim.
 */

import type { CoreClient } from '../core/core-client';
import type {
  ChatEquipRequest,
  ChatOutfitGetRequest,
  ChatRegenerateAvatarRequest,
  CharacterWardrobeListRequest,
  CharacterWardrobeCreateRequest,
  CharacterWardrobeUpdateRequest,
  CharacterWardrobeDeleteRequest,
  ProjectWardrobeListRequest,
  ProjectWardrobeCreateRequest,
  ProjectWardrobeUpdateRequest,
  ProjectWardrobeDeleteRequest,
  WardrobeCreateRequest,
  WardrobeDeleteRequest,
  WardrobeListRequest,
  WardrobePreviewAvatarRequest,
  WardrobeTransferDestinationsRequest,
  WardrobeTransferScope,
  WardrobeUpdateRequest,
  WardrobeItemDto,
  CoreRequest,
} from '../core/core-contract';
import type { EquippedSlots } from './equipped-slots';
import type { WardrobeContainer, WardrobeContainerScope } from './wardrobe-container';

// ---------------------------------------------------------------------------
// §1 — lane P4.9f1's request types (FOLDED into core-contract at unification)
// ---------------------------------------------------------------------------

export type {
  ChatOutfitGetRequest,
  ChatEquipMode,
  ChatEquipRequest,
  WardrobeTransferDestinationsRequest,
  WardrobeTransferScope,
  WardrobeListRequest,
  WardrobeCreateRequest,
  WardrobeUpdateRequest,
  WardrobeDeleteRequest,
  WardrobePreviewAvatarRequest,
  ChatRegenerateAvatarRequest,
} from '../core/core-contract';

// ---------------------------------------------------------------------------
// §2 — the P4.D112/P4.D113 Shared contract (mirrored here; folded at unification)
// ---------------------------------------------------------------------------

/**
 * The group tier's CRUD, §Shared contract item 1 — field shapes mirror
 * `ProjectWardrobe*` exactly. v4 `app/api/v1/groups/[id]/wardrobe[/itemId]`
 * (`d7263f39`); before that commit the group tier had no endpoints at all.
 */
export interface GroupWardrobeListRequest {
  type: 'groupWardrobeList';
  groupId: string;
}
export interface GroupWardrobeCreateRequest {
  type: 'groupWardrobeCreate';
  groupId: string;
  item: Record<string, unknown>;
}
export interface GroupWardrobeGetRequest {
  type: 'groupWardrobeGet';
  groupId: string;
  itemId: string;
}
export interface GroupWardrobeUpdateRequest {
  type: 'groupWardrobeUpdate';
  groupId: string;
  itemId: string;
  item: Record<string, unknown>;
}
export interface GroupWardrobeDeleteRequest {
  type: 'groupWardrobeDelete';
  groupId: string;
  itemId: string;
}

/** What travels with a composite outfit (§Shared contract item 2). */
export type WardrobeTransferComponentMode = 'move' | 'copy' | 'none';

/**
 * `wardrobeTransferApply` WIDENED by §Shared contract item 2 — this supersedes
 * the `core-contract.ts` shape, which the unifier updates in place:
 *
 *  - `source?: { scope, id? }` — the explicit home container, sent when the
 *    dialog browses a shared container and the item's tier is known exactly.
 *    The server then resolves straight from it, no character probing.
 *  - `components?` — for a composite, what to do with its same-container
 *    components (all or nothing).
 *  - `sourceCharacterId` becomes OPTIONAL: v4 omits the key entirely when
 *    there is no selected character (`:163`, the `...(x ? {x} : {})` spread).
 *    At least one of `sourceCharacterId` / `source` is required — v4's refine.
 */
export interface WardrobeTransferApplyRequest {
  type: 'wardrobeTransferApply';
  action: 'move' | 'copy';
  itemId: string;
  sourceCharacterId?: string;
  sourceProjectId: string | null;
  source?: { scope: WardrobeContainerScope; id?: string };
  components?: WardrobeTransferComponentMode;
  destination: { scope: WardrobeTransferScope; id?: string };
}

/** The §1 + §2 union — the §2 arms ride the cast in `dispatchWardrobe`. */
export type WardrobeLaneRequest =
  | ChatOutfitGetRequest
  | ChatEquipRequest
  | WardrobeTransferDestinationsRequest
  | WardrobeTransferApplyRequest
  | WardrobeListRequest
  | WardrobeCreateRequest
  | WardrobeUpdateRequest
  | WardrobeDeleteRequest
  | WardrobePreviewAvatarRequest
  | ChatRegenerateAvatarRequest
  | GroupWardrobeListRequest
  | GroupWardrobeCreateRequest
  | GroupWardrobeGetRequest
  | GroupWardrobeUpdateRequest
  | GroupWardrobeDeleteRequest;

/**
 * The wardrobe dispatch choke-point. The §1 cast was retired when those verbs
 * were folded into `core-contract.ts`; the §2 cast is back for the same reason
 * and retires the same way — `groupWardrobe*` and the widened
 * `wardrobeTransferApply` are not in `CoreRequest` until the unifier folds them.
 */
export function dispatchWardrobe(
  core: CoreClient,
  request: WardrobeLaneRequest | WardrobeContainerRequest,
): Promise<Record<string, unknown>> {
  return core.dispatchData(request as unknown as CoreRequest);
}

// ---------------------------------------------------------------------------
// Transfer payload shapes (v4 `WardrobeTransferDialog.tsx:11-21`)
// ---------------------------------------------------------------------------

export interface TransferDestinationOption {
  id: string;
  name: string;
}

/** v4 `WardrobeTransferDialog.tsx:16-21` / `transfers/route.ts:236-260`. */
export interface TransferDestinationsPayload {
  general: { available: boolean; label: string };
  projects: TransferDestinationOption[];
  groups: TransferDestinationOption[];
  users: TransferDestinationOption[];
}

// ---------------------------------------------------------------------------
// The container → verb router (v4 `wardrobeCollectionUrl` / `wardrobeItemUrl`)
// ---------------------------------------------------------------------------

/**
 * v4's two URL helpers return REST endpoints:
 *
 *     character → /api/v1/characters/{id}/wardrobe[/{itemId}]
 *     project   → /api/v1/projects/{id}/wardrobe[/{itemId}]
 *     group     → /api/v1/groups/{id}/wardrobe[/{itemId}]
 *     general   → /api/v1/wardrobe[/{itemId}]
 *
 * v5 dispatches verbs instead, so the same four-way routing lives here. The
 * `character` and `general` arms carry the container id / the singleton;
 * `project` and `group` carry theirs. A `character`/`project`/`group`
 * container always has an id (`decodeWardrobeContainer` refuses one without),
 * so the non-null assertions below are the decoder's invariant, not a guess —
 * `containerId` throws loudly rather than silently addressing the wrong tier.
 */
function containerId(container: WardrobeContainer): string {
  if (!container.id) {
    throw new Error(`Wardrobe container of scope "${container.scope}" has no id`);
  }
  return container.id;
}

/**
 * What the four routers produce: the §2 group arms plus the character /
 * project / General verbs that already exist. Every one of these is a real
 * dispatch verb — the routing is the only thing this lane adds.
 */
export type WardrobeContainerRequest =
  | CharacterWardrobeListRequest
  | CharacterWardrobeCreateRequest
  | CharacterWardrobeUpdateRequest
  | CharacterWardrobeDeleteRequest
  | ProjectWardrobeListRequest
  | ProjectWardrobeCreateRequest
  | ProjectWardrobeUpdateRequest
  | ProjectWardrobeDeleteRequest
  | GroupWardrobeListRequest
  | GroupWardrobeCreateRequest
  | GroupWardrobeUpdateRequest
  | GroupWardrobeDeleteRequest
  | WardrobeListRequest
  | WardrobeCreateRequest
  | WardrobeUpdateRequest
  | WardrobeDeleteRequest;

/** v4 `wardrobeCollectionUrl(container)` read with GET. */
export function containerListRequest(container: WardrobeContainer): WardrobeContainerRequest {
  switch (container.scope) {
    case 'character':
      return { type: 'characterWardrobeList', characterId: containerId(container) };
    case 'project':
      return { type: 'projectWardrobeList', projectId: containerId(container) };
    case 'group':
      return { type: 'groupWardrobeList', groupId: containerId(container) };
    case 'general':
      return { type: 'wardrobeList' };
  }
}

/** v4 `wardrobeCollectionUrl(container)` written with POST. */
export function containerCreateRequest(
  container: WardrobeContainer,
  item: Record<string, unknown>,
): WardrobeContainerRequest {
  switch (container.scope) {
    case 'character':
      return { type: 'characterWardrobeCreate', characterId: containerId(container), item };
    case 'project':
      return { type: 'projectWardrobeCreate', projectId: containerId(container), item };
    case 'group':
      return { type: 'groupWardrobeCreate', groupId: containerId(container), item };
    case 'general':
      return { type: 'wardrobeCreate', item };
  }
}

/** v4 `wardrobeItemUrl(container, itemId)` written with PUT. */
export function containerUpdateRequest(
  container: WardrobeContainer,
  itemId: string,
  item: Record<string, unknown>,
): WardrobeContainerRequest {
  switch (container.scope) {
    case 'character':
      return {
        type: 'characterWardrobeUpdate',
        characterId: containerId(container),
        itemId,
        item,
      };
    case 'project':
      return { type: 'projectWardrobeUpdate', projectId: containerId(container), itemId, item };
    case 'group':
      return { type: 'groupWardrobeUpdate', groupId: containerId(container), itemId, item };
    case 'general':
      return { type: 'wardrobeUpdate', itemId, item };
  }
}

/** v4 `wardrobeItemUrl(container, itemId)` with DELETE. */
export function containerDeleteRequest(
  container: WardrobeContainer,
  itemId: string,
): WardrobeContainerRequest {
  switch (container.scope) {
    case 'character':
      return { type: 'characterWardrobeDelete', characterId: containerId(container), itemId };
    case 'project':
      return { type: 'projectWardrobeDelete', projectId: containerId(container), itemId };
    case 'group':
      return { type: 'groupWardrobeDelete', groupId: containerId(container), itemId };
    case 'general':
      return { type: 'wardrobeDelete', itemId };
  }
}

// ---------------------------------------------------------------------------
// The shared-container loader (v4 `lib/hooks/use-wardrobe-container-items.ts`)
// ---------------------------------------------------------------------------

export interface WardrobeContainerLoadResult {
  /** Items that live in the container itself — the editable set. */
  items: WardrobeItemDto[];
  /** `items` plus General archetypes, for resolving composite components. */
  resolutionItems: WardrobeItemDto[];
}

/**
 * Load one *shared* container's wardrobe — Quilltap General, a project store,
 * or a group store — with NO tier merging: the list is exactly what lives in
 * that container, which is what the dialog shows (and lets you edit) when
 * browsing it directly. The character scope stays with
 * {@link loadCharacterWardrobeItems}, whose job is the opposite: merge every
 * tier a character can reach.
 *
 * Alongside the container's own list, the General archetypes are fetched as a
 * *resolution pool* so composite rows can display components that bundle a
 * General archetype — those stay read-only in the dialog because they don't
 * live in the viewed container (v4 `use-wardrobe-container-items.ts:14-17`).
 *
 * A character-scoped (or null) container is a no-op, as v4's `active` guard is
 * (`:50`); a failed read empties BOTH lists and warns, exactly as v4's catch
 * does (`:81-85`) — a half-loaded container would offer edits into a place we
 * could not read.
 */
export async function loadWardrobeContainerItems(
  core: CoreClient,
  container: WardrobeContainer | null,
): Promise<WardrobeContainerLoadResult> {
  if (!container || container.scope === 'character') {
    return { items: [], resolutionItems: [] };
  }
  try {
    // v4 `:66-70` — the container read and (unless it IS General) the
    // archetype read, in parallel.
    const [containerData, generalData] = await Promise.all([
      dispatchWardrobe(core, containerListRequest(container)),
      container.scope === 'general'
        ? Promise.resolve(null)
        : dispatchWardrobe(core, { type: 'wardrobeList' }).catch(() => null),
    ]);
    const own = (containerData['wardrobeItems'] as WardrobeItemDto[] | undefined) ?? [];
    const pool = [...own];
    // v4 `:75-79` — General archetypes append, de-duped by id; the container's
    // own copy wins because it was pushed first.
    for (const w of (generalData?.['wardrobeItems'] as WardrobeItemDto[] | undefined) ?? []) {
      if (!pool.some((c) => c.id === w.id)) pool.push(w);
    }
    return { items: own, resolutionItems: pool };
  } catch (err) {
    console.warn('[loadWardrobeContainerItems] Failed to load container wardrobe', err);
    return { items: [], resolutionItems: [] };
  }
}

// ---------------------------------------------------------------------------
// The four-tier item loader (v4 `lib/hooks/use-character-wardrobe-items.ts`)
// ---------------------------------------------------------------------------

export interface CharacterWardrobeLoadResult {
  items: WardrobeItemDto[];
  /**
   * The project tier this load resolved (from an explicit `projectId` or
   * derived from `chatId`), or null when there is none — lets callers offer a
   * "create in this project" destination without re-resolving the chat
   * (v4 `use-character-wardrobe-items.ts:27-32`).
   */
  projectId: string | null;
}

/**
 * Load a character's wearable garments across EVERY wardrobe tier and merge
 * them, de-duped by id with the nearer tier winning on collision:
 * personal vault → the character's groups → project store → Quilltap General
 * (v4 `use-character-wardrobe-items.ts:57-118`).
 *
 * The group tier arrived at 4.8.2 (`8600c83f` — items moved into a group were
 * invisible to everyone, because nothing ever read them back). Precedence
 * mirrors the server's `findArchetypes`: **character > group > project >
 * general**, so a group's livery shadows a project's copy of the same item and
 * a character's personal copy shadows both — including the `isDefault: false`
 * personal copy that is how a character opts out of a shared default. Group
 * stores follow the CHARACTER, not the chat.
 *
 * An explicit `projectId` wins; otherwise the project tier is derived from
 * `chatId` (v4 `:66-77` fetches the chat solely to read `projectId` — here one
 * `chatGet` serves the same purpose). Each tier read fails soft, exactly as
 * v4's per-response `.ok` checks do — a missing tier simply isn't folded in,
 * which is also what keeps this working against a server that predates the
 * `scope=group` arm (lane P4.D71's).
 */
export async function loadCharacterWardrobeItems(
  core: CoreClient,
  characterId: string | null,
  opts?: { projectId?: string | null; chatId?: string | null },
): Promise<CharacterWardrobeLoadResult> {
  if (!characterId) {
    return { items: [], projectId: opts?.projectId ?? null };
  }

  let projectTierId = opts?.projectId ?? null;
  if (!projectTierId && opts?.chatId) {
    try {
      const data = await core.dispatchData({ type: 'chatGet', chatId: opts.chatId });
      const chat = data['chat'] as { projectId?: string | null } | undefined;
      projectTierId = chat?.projectId ?? null;
    } catch {
      /* project tier simply won't be folded in (v4 :74-76) */
    }
  }

  // The four tier reads, in parallel (v4 :93-100); each fails soft.
  const [personal, group, project, archetype] = await Promise.all([
    core
      .dispatchData({ type: 'characterWardrobeList', characterId })
      .catch(() => null),
    core
      .dispatchData({ type: 'characterWardrobeList', characterId, scope: 'group' })
      .catch(() => null),
    projectTierId
      ? core
          .dispatchData({ type: 'projectWardrobeList', projectId: projectTierId })
          .catch(() => null)
      : Promise.resolve(null),
    dispatchWardrobe(core, { type: 'wardrobeList' }).catch(() => null),
  ]);

  // Merge with precedence: personal > group > project > general (v4 :102-124).
  const collected: WardrobeItemDto[] = [];
  const push = (data: Record<string, unknown> | null): void => {
    const list = (data?.['wardrobeItems'] as WardrobeItemDto[] | undefined) ?? [];
    for (const w of list) {
      if (!collected.some((c) => c.id === w.id)) collected.push(w);
    }
  };
  push(personal);
  push(group);
  push(project);
  push(archetype);

  return { items: collected, projectId: projectTierId };
}

// ---------------------------------------------------------------------------
// The tier-routed row mutations (v4 `wardrobe-control-dialog.tsx`)
// ---------------------------------------------------------------------------

/**
 * Toggle `isDefault` on an item (v4 `handleToggleDefault`,
 * `wardrobe-control-dialog.tsx:495-505` at `d7263f39`). Two arms, exactly as
 * v4's ternary has them:
 *
 *  - **Character view** (`container.scope === 'character'`): the pair choice is
 *    the item's own tier — character-owned items go to the character route,
 *    shared archetypes to the global one. Only character-owned items reach this
 *    handler anyway (kebab gating), but the fallback is v4's.
 *  - **Browsing a shared container**: the item lives THERE, so the update goes
 *    to that container's own route.
 */
export async function toggleItemDefault(
  core: CoreClient,
  item: WardrobeItemDto,
  container: WardrobeContainer,
): Promise<void> {
  const body = { isDefault: !item.isDefault };
  if (container.scope !== 'character') {
    await dispatchWardrobe(core, containerUpdateRequest(container, item.id, body));
    return;
  }
  if (item.characterId) {
    await core.dispatchData({
      type: 'characterWardrobeUpdate',
      characterId: item.characterId,
      itemId: item.id,
      item: body,
    });
  } else {
    await dispatchWardrobe(core, { type: 'wardrobeUpdate', itemId: item.id, item: body });
  }
}

/**
 * Delete an item over the same two arms (v4 `handleDelete`,
 * `wardrobe-control-dialog.tsx:512-518` at `d7263f39`).
 */
export async function deleteWardrobeItem(
  core: CoreClient,
  item: WardrobeItemDto,
  container: WardrobeContainer,
): Promise<void> {
  if (container.scope !== 'character') {
    await dispatchWardrobe(core, containerDeleteRequest(container, item.id));
    return;
  }
  if (item.characterId) {
    await core.dispatchData({
      type: 'characterWardrobeDelete',
      characterId: item.characterId,
      itemId: item.id,
    });
  } else {
    await dispatchWardrobe(core, { type: 'wardrobeDelete', itemId: item.id });
  }
}

/**
 * Duplicate a manageable item (v4 `handleDuplicate`,
 * `wardrobe-control-dialog.tsx:521-590` at `d7263f39`). Duplicate is only
 * offered for manageable items (kebab gating), so the copy stays where the
 * original lives: the selected character's vault in the character view, or the
 * browsed shared container itself.
 *
 * The payload carries `imagePrompt` since `d7263f39` — the Portrait Cue used to
 * be dropped on every duplicate, so a copied garment came back describing
 * nothing. Composite references are copied verbatim; member items are not
 * cloned (v4's own debug note).
 */
export async function duplicateWardrobeItem(
  core: CoreClient,
  target: WardrobeContainer,
  item: WardrobeItemDto,
  newTitle: string,
): Promise<void> {
  await dispatchWardrobe(
    core,
    containerCreateRequest(target, {
      title: newTitle,
      description: item.description ?? null,
      imagePrompt: item.imagePrompt ?? null,
      types: item.types,
      appropriateness: item.appropriateness ?? null,
      isDefault: item.isDefault,
      componentItemIds: item.componentItemIds ?? [],
      replace: item.replace,
    }),
  );
}
