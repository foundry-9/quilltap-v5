import type { CoreClient } from '../core/core-client';
import type {
  CustomToolListing,
  CustomToolRunData,
  CustomToolsRosterData,
} from '../core/core-contract';

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

/**
 * What a tool actually quotes (the drift round's Shared contract §1, emitted by
 * lane P4.d19's `collectToolVocabulary`). Mirrors v4's `ToolVocabulary`
 * (`lib/pascal/tool-vocabulary.ts`) — **every field is an OCCURRENCE, not an
 * availability**, so a tool that rolls dice but never writes `{{dice}}` reports
 * `dice: false`.
 *
 * All seven keys are always present on the wire. The type is declared HERE
 * rather than on `CustomToolListing` because §1 is P4.d19's to emit and this
 * lane's to render: until that lane lands, the key is simply absent, which the
 * consumer must read as "quotes nothing" rather than as an error.
 */
export interface CustomToolReferences {
  /** The tool quotes `{{value}}`. */
  value: boolean;
  /** The tool quotes `{{roll}}`. */
  roll: boolean;
  /** The tool quotes `{{dice}}`. */
  dice: boolean;
  /** The tool quotes `{{llm}}`. */
  llm: boolean;
  /** Declared parameters the tool quotes as `{{params.name}}`. */
  params: string[];
  /** Keys of the invoking character's `metadata.json` this tool reads. */
  metadata: string[];
  /** Paths into the merged persistent state this tool reads. */
  state: string[];
}

/** A roster entry, plus §1's `references` (absent on a roster from an older server). */
export interface CustomToolRosterEntry extends CustomToolListing {
  references?: CustomToolReferences;
}

/** The roster body as the run dialog reads it. */
export interface CustomToolsRoster extends Omit<CustomToolsRosterData, 'tools'> {
  tools: CustomToolRosterEntry[];
}

/** GET the roster (`handleList`), resolved FRESH on every dialog open — no cache. */
export async function fetchCustomToolsRoster(
  core: CoreClient,
  chatId: string,
): Promise<CustomToolsRoster> {
  const data = await core.dispatchData({ type: 'chatCustomToolsList', chatId });
  return data as unknown as CustomToolsRoster;
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
