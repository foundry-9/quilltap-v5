import type { CoreClient } from '../core/core-client';
import type { CustomToolRunData, CustomToolsRosterData } from '../core/core-contract';

/**
 * The composer popup's read/run surface over the two Pascal custom-tools verbs
 * (§4, OWNER: lane P4.6ay). The response `type` string is lane-AY-owned, so both
 * bodies are read STRUCTURALLY via `dispatchData` (the autonomous-room
 * precedent) rather than a pinned response variant — the request verbs and the
 * data shapes are what the wire mirror diffs.
 *
 * @module chat/custom-tools.api
 */

export const customToolsKeys = {
  /** The roster for one chat — the same key the popup refetches fresh on open. */
  byChat: (chatId: string) => ['customTools', chatId] as const,
};

/** GET the roster (`handleList`), resolved FRESH on every popup open — no cache. */
export async function fetchCustomToolsRoster(
  core: CoreClient,
  chatId: string,
): Promise<CustomToolsRosterData> {
  const data = await core.dispatchData({ type: 'chatCustomToolsList', chatId });
  return data as unknown as CustomToolsRosterData;
}

/** Arguments for a manual run (v4 `?action=run` body). */
export interface RunCustomToolArgs {
  tool: string;
  parameters?: Record<string, number | string | boolean>;
  private?: boolean;
  asCharacterId?: string;
}

/** POST one tool at the operator's behest (`?action=run`). */
export async function runCustomTool(
  core: CoreClient,
  chatId: string,
  args: RunCustomToolArgs,
): Promise<CustomToolRunData> {
  const data = await core.dispatchData({
    type: 'chatCustomToolRun',
    chatId,
    tool: args.tool,
    parameters: args.parameters,
    private: args.private,
    asCharacterId: args.asCharacterId,
  });
  return data as unknown as CustomToolRunData;
}
