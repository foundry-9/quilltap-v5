/**
 * The wardrobe-dialog data layer: the §1 verb types this lane consumes and the
 * three-tier item loader (v4 `lib/hooks/use-character-wardrobe-items.ts`).
 *
 * §1 (the P4.9f1/P4.9f2 Shared contract): the request types for lane P4.9f1's
 * NEW verbs — `chatOutfitGet`, `chatEquip` (all seven modes, v4's enum order,
 * incl. the deprecated `equip` alias for `wear`), `wardrobeTransferDestinations`
 * / `wardrobeTransferApply`, the GLOBAL archetype tier `wardrobeList` /
 * `wardrobeCreate` / `wardrobeUpdate` / `wardrobeDelete`, and (tier 2)
 * `wardrobePreviewAvatar` + `chatRegenerateAvatar` — live HERE as a LOCAL typed
 * module behind an `as unknown as CoreRequest` cast (the established
 * `home.api.ts` / `photos.api.ts` pattern). The unifier folds them into
 * `core-contract.ts` and runs the name-for-name wire diff. Field names are
 * v4's route bodies verbatim; do NOT rename without editing the sibling order
 * in the same commit (§2 — a divergence is a round-breaking defect).
 *
 * Landed verbs consumed directly (no §1 dependency):
 * `characterWardrobe{List,Create,Get,Update,Delete}`, `projectWardrobe*`,
 * `characterList`, `imageProfileList`, `chatGet`.
 */

import type { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import type { EquippedSlots } from './equipped-slots';

// ---------------------------------------------------------------------------
// §1 — lane P4.9f1's request types (LOCAL until the unify fold)
// ---------------------------------------------------------------------------

/** v4 `GET /api/v1/chats/{id}?action=outfit` (`outfit.ts` `handleGetOutfit`). */
export interface ChatOutfitGetRequest {
  type: 'chatOutfitGet';
  chatId: string;
}

/**
 * The seven equip modes, in v4's own enum order
 * (`app/api/v1/chats/[id]/actions/outfit.ts:37-79`). v4's comment: "`wear`
 * honors the item's `replace` flag; `replace` force-swaps the slots it covers.
 * `equip` is a deprecated alias for `wear`" — the alias is the easy-to-miscount
 * seventh; it is carried faithfully, not collapsed into `wear`.
 */
export type ChatEquipMode =
  | 'wear'
  | 'replace'
  | 'equip'
  | 'add_to_slot'
  | 'remove_from_slot'
  | 'clear_slot'
  | 'set_all';

/**
 * v4 `POST /api/v1/chats/{id}?action=equip` (`outfit.ts:35-79` schema,
 * `handleEquipSlot:193`). `slot` is required for `add_to_slot` /
 * `remove_from_slot` / `clear_slot`; `itemId` for `wear` / `replace` / `equip`
 * / `add_to_slot`; `slots` for `set_all` (the superRefine arms — the server
 * enforces them; the client sends exactly what v4's callers send).
 */
export interface ChatEquipRequest {
  type: 'chatEquip';
  chatId: string;
  characterId: string;
  mode: ChatEquipMode;
  slot?: 'top' | 'bottom' | 'footwear' | 'accessories';
  itemId?: string | null;
  slots?: EquippedSlots;
}

/** v4 `GET /api/v1/wardrobe/transfers` (`transfers/route.ts:236-260`). */
export interface WardrobeTransferDestinationsRequest {
  type: 'wardrobeTransferDestinations';
}

/** v4 `WardrobeTransferDialog.tsx:9` / `transfers/route.ts:47-57`. */
export type WardrobeTransferScope = 'general' | 'project' | 'group' | 'character';

/** v4 `POST /api/v1/wardrobe/transfers` (`transfers/route.ts:47-57` schema). */
export interface WardrobeTransferApplyRequest {
  type: 'wardrobeTransferApply';
  action: 'move' | 'copy';
  itemId: string;
  sourceCharacterId: string;
  sourceProjectId: string | null;
  destination: { scope: WardrobeTransferScope; id?: string };
}

/** v4 `GET /api/v1/wardrobe` (the global archetype tier). */
export interface WardrobeListRequest {
  type: 'wardrobeList';
}

/** v4 `POST /api/v1/wardrobe` — the `item` bag mirrors `projectWardrobeCreate`. */
export interface WardrobeCreateRequest {
  type: 'wardrobeCreate';
  item: Record<string, unknown>;
}

/** v4 `PUT /api/v1/wardrobe/{itemId}`. */
export interface WardrobeUpdateRequest {
  type: 'wardrobeUpdate';
  itemId: string;
  item: Record<string, unknown>;
}

/** v4 `DELETE /api/v1/wardrobe/{itemId}`. */
export interface WardrobeDeleteRequest {
  type: 'wardrobeDelete';
  itemId: string;
}

/** v4 `POST /api/v1/wardrobe/preview-avatar` (tier 2; non-persisting —
 *  touches neither `avatarOverrides` nor any chat, the route's `:1-15` note). */
export interface WardrobePreviewAvatarRequest {
  type: 'wardrobePreviewAvatar';
  characterId: string;
  equippedSlots: EquippedSlots;
  imageProfileId?: string;
}

/** v4 `POST /api/v1/chats/{id}?action=regenerate-avatar` (tier 2; QUEUES a
 *  background job — the fitting-room slots ride as a one-shot override). */
export interface ChatRegenerateAvatarRequest {
  type: 'chatRegenerateAvatar';
  chatId: string;
  characterId: string;
  equippedSlots: EquippedSlots;
  imageProfileId?: string;
}

/** The §1 union — cast through this at the dispatch sites. */
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
  | ChatRegenerateAvatarRequest;

/** One choke-point for the §1 cast (folded away at unification). */
export function dispatchWardrobe(
  core: CoreClient,
  request: WardrobeLaneRequest,
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
// The three-tier item loader (v4 `lib/hooks/use-character-wardrobe-items.ts`)
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
 * Load a character's wearable garments across the tri-tier wardrobe model and
 * merge them, de-duped by id with the nearer tier winning on collision:
 * personal vault → project store → Quilltap General
 * (v4 `use-character-wardrobe-items.ts:57-114`).
 *
 * An explicit `projectId` wins; otherwise the project tier is derived from
 * `chatId` (v4 `:66-77` fetches the chat solely to read `projectId` — here one
 * `chatGet` serves the same purpose). Each tier read fails soft, exactly as
 * v4's per-response `.ok` checks do — a missing tier (including the global
 * tier before lane P4.9f1 lands) simply isn't folded in.
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

  // The three tier reads, in parallel (v4 :80-86); each fails soft.
  const [personal, project, archetype] = await Promise.all([
    core
      .dispatchData({ type: 'characterWardrobeList', characterId })
      .catch(() => null),
    projectTierId
      ? core
          .dispatchData({ type: 'projectWardrobeList', projectId: projectTierId })
          .catch(() => null)
      : Promise.resolve(null),
    dispatchWardrobe(core, { type: 'wardrobeList' }).catch(() => null),
  ]);

  // Merge with precedence: personal > project > general (v4 :88-106).
  const collected: WardrobeItemDto[] = [];
  const push = (data: Record<string, unknown> | null): void => {
    const list = (data?.['wardrobeItems'] as WardrobeItemDto[] | undefined) ?? [];
    for (const w of list) {
      if (!collected.some((c) => c.id === w.id)) collected.push(w);
    }
  };
  push(personal);
  push(project);
  push(archetype);

  return { items: collected, projectId: projectTierId };
}

// ---------------------------------------------------------------------------
// The tier-routed row mutations (v4 `wardrobe-control-dialog.tsx`)
// ---------------------------------------------------------------------------

/**
 * Toggle `isDefault` on an item (v4 `handleToggleDefault`,
 * `wardrobe-control-dialog.tsx:357-380`): character-owned items PUT the
 * character route (`:361`), shared archetypes PUT the global route (`:362`) —
 * the pair choice is the item's tier (`item.characterId`).
 */
export async function toggleItemDefault(core: CoreClient, item: WardrobeItemDto): Promise<void> {
  const body = { isDefault: !item.isDefault };
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
 * Delete an item over the same tier pair (v4 `handleDelete`,
 * `wardrobe-control-dialog.tsx:386-388`).
 */
export async function deleteWardrobeItem(core: CoreClient, item: WardrobeItemDto): Promise<void> {
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
 * Duplicate a character-owned item (v4 `handleDuplicate`,
 * `wardrobe-control-dialog.tsx:404-448`). Duplicate is only offered for
 * character-owned items (the kebab hides it for shared archetypes), so it
 * always POSTs the character route (`:420-421`); composite references are
 * copied verbatim — member items are not cloned (v4's `:416` debug note).
 */
export async function duplicateWardrobeItem(
  core: CoreClient,
  characterId: string,
  item: WardrobeItemDto,
  newTitle: string,
): Promise<void> {
  await core.dispatchData({
    type: 'characterWardrobeCreate',
    characterId,
    item: {
      title: newTitle,
      description: item.description ?? null,
      types: item.types,
      appropriateness: item.appropriateness ?? null,
      isDefault: item.isDefault,
      componentItemIds: item.componentItemIds ?? [],
      replace: item.replace,
    },
  });
}
