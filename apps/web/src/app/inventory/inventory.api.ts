/**
 * The Inventory's calls — a port of v4 `lib/hooks/use-inventory.ts`
 * (`322a5488e`): one tier's objects, folders and archetypes, an archetype's
 * dependents, an object's pictures, every object / folder / picture write,
 * and what a chat's cast carries.
 *
 * **Recorded mechanism divergences (R-1 / R-M).**
 *  - Every call rides C7's ONE `inventory*` dispatch family carrying the
 *    `container` itself (v4's ONE route factory), where v4 builds REST URLs
 *    per tier. Each function builds the exact request object v4's URL + body
 *    say — an optional key is ABSENT unless v4 would send it (pinned by
 *    `inventory.api.spec.ts`, `toEqual`).
 *  - The upload alone is REST multipart (`/api/v1/inventory/<id>/images?
 *    scope=…[&id=…]&action=upload`), as v4 does and as the wardrobe's
 *    `item-images.api.ts` does; its refusal reads `json.error ?? 'Upload failed'`.
 *  - v4's TanStack reads and `onSettled` invalidations become the signal
 *    stores in `inventory-list-data.ts`, `inventory-folders-data.ts` and
 *    `chat-inventory-data.ts`; whoever runs a write reloads them.
 *  - A refusal arrives as a `CoreDispatchError` carrying `code` and (R-L)
 *    `details` — the `InventoryRequestError` v4 throws; its HTTP `status` has
 *    no twin and nothing branches on it.
 *
 * Every function takes the `CoreClient` first (the v5 data-layer idiom).
 *
 * @module inventory/inventory.api
 */

import { apiUrl } from '../core/api-url';
import type { CoreClient } from '../core/core-client';
import type {
  CarriedEntry,
  CoreRequest,
  InventoryDependent,
  InventoryEquipResult,
  InventoryFolderDto,
  InventoryImageSummary,
  InventoryInheritableField,
  InventoryItemBody,
  InventoryUnequipResult,
  PhotoAlbumOption,
  ResolvedInventoryItem,
  WardrobeItemImageSaveResponse,
} from '../core/core-contract';
import { inventoryImagesUrl, type InventoryContainer } from './inventory-container';

/** v4 `use-inventory.ts:83-90` `InventoryListOptions`. */
export interface InventoryListOptions {
  includeArchived: boolean;
  /** A character's whole reachable pool, each object tagged with its tier. */
  shared?: boolean;
  /** The project tier for a character's pool. */
  projectId?: string | null;
}

/**
 * One tier's objects, resolved (v4 `useInventoryList`'s query function,
 * `:92-112`). `shared` rides ONLY for a character container.
 */
export async function listInventory(
  core: CoreClient,
  container: InventoryContainer,
  opts: InventoryListOptions,
): Promise<ResolvedInventoryItem[]> {
  const shared = !!opts.shared && container.scope === 'character';
  const data = await core.dispatchData({
    type: 'inventoryList',
    container,
    ...(opts.includeArchived ? { includeArchived: true } : {}),
    ...(shared ? { shared: true } : {}),
    ...(opts.projectId ? { projectId: opts.projectId } : {}),
  });
  return (data['items'] as ResolvedInventoryItem[] | undefined) ?? [];
}

/** One tier's folder tree (v4 `useInventoryFolders`, `:115-131`). */
export async function listInventoryFolders(
  core: CoreClient,
  container: InventoryContainer,
): Promise<InventoryFolderDto[]> {
  const data = await core.dispatchData({ type: 'inventoryFolders', container });
  return (data['folders'] as InventoryFolderDto[] | undefined) ?? [];
}

/** The archetypes a tier can reach — the instantiate picker (v4 `:134-150`). */
export async function listInventoryArchetypes(
  core: CoreClient,
  container: InventoryContainer,
  projectId?: string | null,
): Promise<ResolvedInventoryItem[]> {
  const data = await core.dispatchData({
    type: 'inventoryArchetypes',
    container,
    ...(projectId ? { projectId } : {}),
  });
  return (data['items'] as ResolvedInventoryItem[] | undefined) ?? [];
}

/** An archetype's instances, everywhere (v4 `:153-168`). */
export async function listInventoryDependents(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
): Promise<InventoryDependent[]> {
  const data = await core.dispatchData({ type: 'inventoryDependents', container, itemId });
  return (data['dependents'] as InventoryDependent[] | undefined) ?? [];
}

/** One object's pictures — an instance with none shows its archetype's (v4 `:171-184`). */
export async function listInventoryImages(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
): Promise<{ current: string | null; images: InventoryImageSummary[] }> {
  const data = await core.dispatchData({ type: 'inventoryItemImagesList', container, itemId });
  return {
    current: (data['current'] as string | null | undefined) ?? null,
    images: (data['images'] as InventoryImageSummary[] | undefined) ?? [],
  };
}

/** v4 `create` (`:236-238`) — `projectId` names the project a character's kit may borrow members from. */
export async function createInventoryItem(
  core: CoreClient,
  container: InventoryContainer,
  body: InventoryItemBody,
  projectId?: string | null,
): Promise<ResolvedInventoryItem> {
  const data = await core.dispatchData({
    type: 'inventoryCreate',
    container,
    ...(projectId ? { projectId } : {}),
    body,
  });
  return data['item'] as ResolvedInventoryItem;
}

/** v4 `update` (`:239-241`). */
export async function updateInventoryItem(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
  body: InventoryItemBody,
  projectId?: string | null,
): Promise<ResolvedInventoryItem> {
  const data = await core.dispatchData({
    type: 'inventoryUpdate',
    container,
    itemId,
    ...(projectId ? { projectId } : {}),
    body,
  });
  return data['item'] as ResolvedInventoryItem;
}

/** v4 `clearOverrides` (`:242-244`). */
export async function clearInventoryOverrides(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
  fields: InventoryInheritableField[],
): Promise<ResolvedInventoryItem> {
  const data = await core.dispatchData({ type: 'inventoryClearOverride', container, itemId, fields });
  return data['item'] as ResolvedInventoryItem;
}

/** v4 `detach` (`:245-247`). */
export async function detachInventoryItem(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
): Promise<ResolvedInventoryItem> {
  const data = await core.dispatchData({ type: 'inventoryDetach', container, itemId });
  return data['item'] as ResolvedInventoryItem;
}

/** v4 `remove` (`:248-249`) — `force` rides only when set (`?force=true`). */
export async function deleteInventoryItem(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
  force = false,
): Promise<void> {
  await core.dispatchData({ type: 'inventoryDelete', container, itemId, ...(force ? { force: true } : {}) });
}

/** v4 `instantiate`'s body (`:252`). */
export interface InventoryInstantiateBody {
  archetypeId: string;
  folder?: string;
  projectId?: string | null;
  chatId?: string | null;
}

/** v4 `instantiate` (`:250-253`) — the body travels as given. */
export async function instantiateInventoryItem(
  core: CoreClient,
  container: InventoryContainer,
  body: InventoryInstantiateBody,
): Promise<ResolvedInventoryItem> {
  const data = await core.dispatchData({ type: 'inventoryInstantiate', container, ...body });
  return data['item'] as ResolvedInventoryItem;
}

/** v4 `duplicate` (`:254-255`). */
export async function duplicateInventoryItem(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
): Promise<ResolvedInventoryItem> {
  const data = await core.dispatchData({ type: 'inventoryDuplicate', container, itemId });
  return data['item'] as ResolvedInventoryItem;
}

/** v4 `createFolder` (`:256-257`). */
export async function createInventoryFolder(
  core: CoreClient,
  container: InventoryContainer,
  path: string,
): Promise<InventoryFolderDto> {
  const data = await core.dispatchData({ type: 'inventoryFolderCreate', container, path });
  return data['folder'] as InventoryFolderDto;
}

/** v4 `renameFolder` (`:258-259`). */
export async function renameInventoryFolder(
  core: CoreClient,
  container: InventoryContainer,
  path: string,
  name: string,
): Promise<InventoryFolderDto> {
  const data = await core.dispatchData({ type: 'inventoryFolderRename', container, path, name });
  return data['folder'] as InventoryFolderDto;
}

/** v4 `deleteFolder` (`:260-263`) — both counts through `Number(… ?? 0)`. */
export async function deleteInventoryFolder(
  core: CoreClient,
  container: InventoryContainer,
  path: string,
): Promise<{ movedItems: number; movedFolders: number }> {
  const data = await core.dispatchData({ type: 'inventoryFolderDelete', container, path });
  return { movedItems: Number(data['movedItems'] ?? 0), movedFolders: Number(data['movedFolders'] ?? 0) };
}

/** v4 `fileItems(mode, …)` (`:264-266`) → `data.items ?? []`. */
export async function fileInventoryItems(
  core: CoreClient,
  mode: 'move' | 'copy',
  container: InventoryContainer,
  itemIds: string[],
  folder: string,
): Promise<ResolvedInventoryItem[]> {
  const data = await core.dispatchData({
    type: mode === 'move' ? 'inventoryItemsMove' : 'inventoryItemsCopy',
    container,
    itemIds,
    folder,
  });
  return (data['items'] as ResolvedInventoryItem[] | undefined) ?? [];
}

/** v4 `transfer` (`:267-273`). */
export async function transferInventoryItem(
  core: CoreClient,
  body: { action: 'move' | 'copy'; itemId: string; from: InventoryContainer; to: InventoryContainer },
): Promise<ResolvedInventoryItem> {
  const data = await core.dispatchData({ type: 'inventoryTransfer', ...body });
  return data['item'] as ResolvedInventoryItem;
}

/** v4 Pictures section `generate` (`inventory-item-editor.tsx:493`). */
export async function generateInventoryItemImage(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
): Promise<void> {
  await core.dispatchData({ type: 'inventoryItemImageGenerate', container, itemId });
}

/** v4 Pictures section `set-current` (`:520`). */
export async function setCurrentInventoryItemImage(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
  fileId: string,
): Promise<void> {
  await core.dispatchData({ type: 'inventoryItemImageSetCurrent', container, itemId, fileId });
}

/** v4 Pictures section `delete-image` (`:528`). */
export async function deleteInventoryItemImage(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
  fileId: string,
): Promise<void> {
  await core.dispatchData({ type: 'inventoryItemImageDelete', container, itemId, fileId });
}

/**
 * v4 Pictures section `upload` (`:473-479`): multipart `file` to the REST
 * route; a refusal throws `json.error ?? 'Upload failed'`.
 */
export async function uploadInventoryItemImage(
  container: InventoryContainer,
  itemId: string,
  file: File,
): Promise<void> {
  const form = new FormData();
  form.set('file', file);
  const res = await fetch(apiUrl(inventoryImagesUrl(container, itemId, 'upload')), { method: 'POST', body: form });
  if (!res.ok) {
    throw new Error(((await res.json().catch(() => ({}))) as { error?: string }).error ?? 'Upload failed');
  }
}

/** The save dialog's albums for an Inventory picture (v4 `SaveImageDialog.tsx:57-61`). */
export async function listInventorySaveTargets(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
): Promise<PhotoAlbumOption[]> {
  const data = await core.dispatchData({ type: 'inventoryItemImageSaveTargets', container, itemId });
  return (data['albums'] as PhotoAlbumOption[] | undefined) ?? [];
}

/** File an Inventory picture in a document store — a blank caption is OMITTED. */
export async function saveInventoryImageToStore(
  core: CoreClient,
  container: InventoryContainer,
  itemId: string,
  body: { fileId: string; mountPointId: string; caption?: string },
): Promise<WardrobeItemImageSaveResponse> {
  const data = await core.dispatchData({
    type: 'inventoryItemImageSaveToStore',
    container,
    itemId,
    fileId: body.fileId,
    mountPointId: body.mountPointId,
    ...(body.caption !== undefined ? { caption: body.caption } : {}),
  });
  return data as unknown as WardrobeItemImageSaveResponse;
}

/** v4 `useChatInventory`'s read (`:285-293`) — absent keys read as none. */
export async function getChatInventory(
  core: CoreClient,
  chatId: string,
): Promise<{ inventory: Record<string, CarriedEntry>; projectId: string | null }> {
  const data = await core.dispatchData({ type: 'chatInventoryGet', chatId });
  return {
    inventory: (data['inventory'] as Record<string, CarriedEntry> | undefined) ?? {},
    projectId: (data['projectId'] as string | null | undefined) ?? null,
  };
}

/** v4 `equip` (`:316`). */
export async function equipChatInventory(
  core: CoreClient,
  chatId: string,
  characterId: string,
  ids: string[],
): Promise<InventoryEquipResult> {
  return (await core.dispatchData({
    type: 'chatInventoryEquip',
    chatId,
    characterId,
    ids,
  })) as unknown as InventoryEquipResult;
}

/** v4 `unequip` (`:317`). */
export async function unequipChatInventory(
  core: CoreClient,
  chatId: string,
  characterId: string,
  ids: string[],
): Promise<InventoryUnequipResult> {
  return (await core.dispatchData({
    type: 'chatInventoryUnequip',
    chatId,
    characterId,
    ids,
  })) as unknown as InventoryUnequipResult;
}

/** v4 `resetToDefaults` (`:318`). */
export async function resetChatInventoryDefaults(
  core: CoreClient,
  chatId: string,
  characterId: string,
): Promise<{ equipped: ResolvedInventoryItem[] }> {
  const request: CoreRequest = { type: 'chatInventoryResetDefaults', chatId, characterId };
  return (await core.dispatchData(request)) as unknown as { equipped: ResolvedInventoryItem[] };
}
