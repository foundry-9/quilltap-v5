/**
 * The groups-vertical data layer (v4 `app/aurora/hooks/useGroups.ts` +
 * `useGroupMembers` + `useGroupMountPoints`): TanStack query keys + thin
 * `CoreClient` dispatch helpers shared by the Characters-page groups section and
 * the routed group editor.
 *
 * Every op reads through {@link CoreClient.dispatchData} (raw `data`, throwing a
 * `CoreDispatchError` on the error envelope) — the p4.6k Shared contract pins the
 * response BODIES via its differentials, not a narrowed Rust `Response` type.
 */

import type { CoreClient } from '../../core/core-client';
import type {
  DocumentStoreSummary,
  GroupMemberSummary,
  GroupSummary,
} from '../../core/core-contract';

/** Query keys for the groups vertical (v4 `queryKeys.groups`). */
export const groupKeys = {
  all: ['groups'] as const,
  list: () => ['groups', 'list'] as const,
  detail: (id: string) => ['groups', 'detail', id] as const,
  members: (id: string) => ['groups', 'members', id] as const,
  stores: (id: string) => ['groups', 'stores', id] as const,
  /** v4 `queryKeys.groups.state(id)` — the group's OWN persistent state tier. */
  state: (id: string) => ['groups', id, 'state'] as const,
};

/** A group as the card consumes it: the row plus the mapped `memberCount`. */
export interface GroupCardModel extends GroupSummary {
  memberCount: number;
}

/**
 * Fetch the group list and map `_count.members` → `memberCount` (v4 `useGroups`
 * `mappedGroups`). Lane A pins `groupList` → `{groups: [...]}` (createdAt desc).
 */
export async function fetchGroups(core: CoreClient): Promise<GroupCardModel[]> {
  const data = await core.dispatchData({ type: 'groupList' });
  const groups = (data['groups'] as GroupSummary[]) ?? [];
  return groups.map((g) => ({ ...g, memberCount: g._count?.members ?? 0 }));
}

export async function fetchGroup(core: CoreClient, groupId: string): Promise<GroupSummary> {
  const data = await core.dispatchData({ type: 'groupGet', groupId });
  // v4 accepts either `{group}` or a bare row; unwrap defensively.
  return (data['group'] as GroupSummary) ?? (data as unknown as GroupSummary);
}

export async function createGroup(
  core: CoreClient,
  name: string,
  description?: string | null,
): Promise<GroupSummary> {
  const data = await core.dispatchData({ type: 'groupCreate', name, description });
  return (data['group'] as GroupSummary) ?? (data as unknown as GroupSummary);
}

export async function updateGroup(
  core: CoreClient,
  groupId: string,
  patch: {
    name?: string;
    description?: string | null;
    color?: string | null;
    icon?: string | null;
  },
): Promise<GroupSummary> {
  // The patch rides a nested `group` bag (the server's pinned shape).
  const data = await core.dispatchData({ type: 'groupUpdate', groupId, group: patch });
  return (data['group'] as GroupSummary) ?? (data as unknown as GroupSummary);
}

export async function deleteGroup(core: CoreClient, groupId: string): Promise<void> {
  await core.dispatchData({ type: 'groupDelete', groupId });
}

// --- Members ---

export async function fetchGroupMembers(
  core: CoreClient,
  groupId: string,
): Promise<GroupMemberSummary[]> {
  const data = await core.dispatchData({ type: 'groupMembers', groupId });
  return (data['members'] as GroupMemberSummary[]) ?? [];
}

export async function addGroupMember(
  core: CoreClient,
  groupId: string,
  characterId: string,
): Promise<void> {
  await core.dispatchData({ type: 'groupMemberAdd', groupId, characterId });
}

export async function removeGroupMember(
  core: CoreClient,
  groupId: string,
  characterId: string,
): Promise<void> {
  await core.dispatchData({ type: 'groupMemberRemove', groupId, characterId });
}

// --- Linked document stores (the Scriptorium card) ---

export async function fetchGroupStores(
  core: CoreClient,
  groupId: string,
): Promise<DocumentStoreSummary[]> {
  const data = await core.dispatchData({ type: 'groupMountPointList', groupId });
  return (data['mountPoints'] as DocumentStoreSummary[]) ?? [];
}

export async function unlinkGroupStore(
  core: CoreClient,
  groupId: string,
  mountPointId: string,
): Promise<void> {
  await core.dispatchData({ type: 'groupMountPointUnlink', groupId, mountPointId });
}

// --- State (§A of the `c53510c7` round) — the group's OWN tier, no cascade ---

/** GET the group's persistent state (v4 `?action=get-state`). */
export async function fetchGroupState(
  core: CoreClient,
  groupId: string,
): Promise<Record<string, unknown>> {
  const data = await core.dispatchData({ type: 'groupStateGet', groupId });
  return (data['state'] as Record<string, unknown>) ?? {};
}

/** PUT the group's state (REPLACES wholesale); returns the persisted object. */
export async function setGroupState(
  core: CoreClient,
  groupId: string,
  state: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const data = await core.dispatchData({ type: 'groupStateSet', groupId, state });
  return (data['state'] as Record<string, unknown>) ?? {};
}

/** DELETE the group's state; returns the `previousState` the server reports. */
export async function resetGroupState(
  core: CoreClient,
  groupId: string,
): Promise<Record<string, unknown>> {
  const data = await core.dispatchData({ type: 'groupStateReset', groupId });
  return (data['previousState'] as Record<string, unknown>) ?? {};
}
