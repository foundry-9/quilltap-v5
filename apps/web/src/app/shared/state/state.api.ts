/**
 * The four-entity state dispatch layer (v4 `components/state/StateEditorModal`'s
 * `endpointConfig` + fetchers), shared by the Prospero card, the Group editor,
 * and the General State settings card.
 *
 * Each tier reads through {@link CoreClient.dispatchData} (the §A verbs); the
 * `c53510c7` Shared contract pins the response BODIES. Only the CHAT tier
 * surfaces the inherited cascade beneath its own state — the others edit a
 * single tier directly.
 *
 * @module shared/state/state.api
 */

import type { CoreClient } from '../../core/core-client';
import type { GroupTier } from '../../core/core-contract';
import { fetchGroupState, resetGroupState, setGroupState } from '../../screens/groups/groups.api';
import {
  fetchProjectState,
  resetProjectState,
  setProjectState,
} from '../../screens/prospero/projects.api';

/** The four tiers the shared editor spans. `general` is instance-wide (no id). */
export type StateEntityType = 'chat' | 'project' | 'group' | 'general';

/**
 * A state read, normalized across tiers. Only a CHAT read fills the inherited
 * slices + `groupTier` + `projectId`; the others carry just `state`. A tier
 * slice that is present but empty is omitted by the server, so `undefined` here
 * means "no keys at that tier" — exactly what the inherited-layers note wants.
 */
export interface StateFetchResult {
  state: Record<string, unknown>;
  chatState?: Record<string, unknown>;
  projectState?: Record<string, unknown>;
  groupState?: Record<string, unknown>;
  generalState?: Record<string, unknown>;
  groupTier?: GroupTier;
  projectId?: string;
}

/** The human label per tier (v4 `endpointConfig().label`). */
export function stateLabel(entityType: StateEntityType): string {
  switch (entityType) {
    case 'chat':
      return 'Chat';
    case 'project':
      return 'Project';
    case 'group':
      return 'Group';
    case 'general':
      return 'General';
  }
}

/** GET the tier's state (the chat tier also brings the inherited cascade). */
export async function fetchState(
  core: CoreClient,
  entityType: StateEntityType,
  entityId: string,
): Promise<StateFetchResult> {
  switch (entityType) {
    case 'chat': {
      const data = await core.dispatchData({ type: 'chatStateGet', chatId: entityId });
      return {
        state: (data['state'] as Record<string, unknown>) ?? {},
        chatState: data['chatState'] as Record<string, unknown> | undefined,
        projectState: data['projectState'] as Record<string, unknown> | undefined,
        groupState: data['groupState'] as Record<string, unknown> | undefined,
        generalState: data['generalState'] as Record<string, unknown> | undefined,
        groupTier: data['groupTier'] as GroupTier | undefined,
        projectId: data['projectId'] as string | undefined,
      };
    }
    case 'project':
      return { state: await fetchProjectState(core, entityId) };
    case 'group':
      return { state: await fetchGroupState(core, entityId) };
    case 'general': {
      const data = await core.dispatchData({ type: 'generalStateGet' });
      return { state: (data['state'] as Record<string, unknown>) ?? {} };
    }
  }
}

/** PUT the tier's state (REPLACES wholesale); returns the persisted object. */
export async function setState(
  core: CoreClient,
  entityType: StateEntityType,
  entityId: string,
  state: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  switch (entityType) {
    case 'project':
      return setProjectState(core, entityId, state);
    case 'group':
      return setGroupState(core, entityId, state);
    case 'chat': {
      const data = await core.dispatchData({ type: 'chatStateSet', chatId: entityId, state });
      return (data['state'] as Record<string, unknown>) ?? {};
    }
    case 'general': {
      const data = await core.dispatchData({ type: 'generalStateSet', state });
      return (data['state'] as Record<string, unknown>) ?? {};
    }
  }
}

/** DELETE the tier's state; returns the `previousState` the server reports. */
export async function resetState(
  core: CoreClient,
  entityType: StateEntityType,
  entityId: string,
): Promise<Record<string, unknown>> {
  switch (entityType) {
    case 'project':
      return resetProjectState(core, entityId);
    case 'group':
      return resetGroupState(core, entityId);
    case 'chat': {
      const data = await core.dispatchData({ type: 'chatStateReset', chatId: entityId });
      return (data['previousState'] as Record<string, unknown>) ?? {};
    }
    case 'general': {
      const data = await core.dispatchData({ type: 'generalStateReset' });
      return (data['previousState'] as Record<string, unknown>) ?? {};
    }
  }
}
