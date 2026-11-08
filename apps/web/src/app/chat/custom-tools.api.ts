import type { CoreClient } from '../core/core-client';
import type {
  ChatCustomToolPresetsRequest,
  CustomToolListing,
  CustomToolPreset,
  CustomToolPresetPool,
  CustomToolPresetStore,
  CustomToolPresetUnread,
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
  /**
   * The whole custom-tools namespace (P4.D346 R-5; v4 `queryKeys.customTools
   * .all`) — what an unscoped `customToolInterventions` hint and the reconnect
   * sweep invalidate. NEW in v5: nothing before it needed the bare prefix.
   */
  all: ['customTools'] as const,
  /** The roster for one chat — the same key the popup refetches fresh on open. */
  byChat: (chatId: string) => ['customTools', chatId] as const,
  /**
   * Run presets for one tool as one character sees them —
   * `Tools/{toolName}.{preset}.settings.json` across the character → group →
   * project → General walk, listed by the run dialog. Invalidated after a
   * save; refetched per dialog open, same doctrine as `byChat` (the files
   * live in stores the user edits). (v4 `lib/query/keys.ts:215-224` at
   * `624d0d25c`, on v5's `customTools` prefix — R-5.) It sits under
   * `byChat(chatId)`, so a roster invalidation reaches it, as in v4.
   *
   * v5 has no central keys module — keys are per-feature — so this sits beside
   * `byChat` rather than in v4's `lib/query/keys.ts`.
   */
  presets: (chatId: string, asCharacterId: string | null, toolName: string) =>
    ['customTools', chatId, 'presets', asCharacterId ?? '', toolName] as const,
  /**
   * The runs held for the operator in one chat (operator intervention, v4
   * `839575d89`). Refreshed by the `customToolInterventions` realtime topic.
   * v4 spells the prefix `custom-tools`; v5 keeps its own `customTools` (R-5).
   */
  interventions: (chatId: string) => ['customTools', chatId, 'interventions'] as const,
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
  /** State paths this tool's effects may WRITE. Absent on rosters from an older server. */
  stateWrites?: string[];
  /** Metadata keys this tool's effects may WRITE on the rolling character. Absent on older servers. */
  metadataWrites?: string[];
  /**
   * Progression IDS this tool reads — ids rather than `<id>.<field>` keys, so
   * the panel says "consults your cannon" and never edges toward the odds the
   * roster deliberately withholds. Absent on older servers.
   */
  progress?: string[];
  /** Progression ids this tool's effects may WRITE. Absent on older servers. */
  progressWrites?: string[];
  /** The tool quotes `{{now}}`. Absent on older servers. */
  now?: boolean;
  /** The tool quotes `{{character}}`. Absent on older servers. */
  character?: boolean;
  /** Item references this tool's wardrobe effects may put on, verbatim. Absent on older servers. */
  wears?: string[];
  /** Item references this tool's wardrobe effects may take off, verbatim. Absent on older servers. */
  takesOff?: string[];
  /** Slots this tool's wardrobe effects may clear. Absent on older servers. */
  clearsSlots?: string[];
  // === P4.D330: the inventory vocabulary (v4 `e1872c94f`) ===
  /** Inventory object references this tool reads (tests, quotes, draws from). Absent on older servers. */
  inventoryReads?: string[];
  /** `inventoryAggregate` keys this tool tests (`equipped.names`, …). Absent on older servers. */
  inventoryAggregates?: string[];
  /** References this tool's inventory effects may equip, verbatim. Absent on older servers. */
  equips?: string[];
  /** References this tool's inventory effects may unequip, verbatim. Absent on older servers. */
  unequips?: string[];
  /** Archetype references this tool's inventory effects may pick up, verbatim. Absent on older servers. */
  picksUp?: string[];
  // === end P4.D330 ===
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

/** One preset store as the wire carries it — every field read defensively. */
function readPresetStore(raw: unknown): CustomToolPresetStore {
  const r = (raw ?? {}) as Record<string, unknown>;
  return {
    tier: typeof r['tier'] === 'string' ? r['tier'] : '',
    mountPointId: typeof r['mountPointId'] === 'string' ? r['mountPointId'] : '',
    mountName: typeof r['mountName'] === 'string' ? r['mountName'] : '',
  };
}

const asArray = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);

/**
 * The run dialog's preset pool for one tool as one character sees it (v4
 * `fd165fce0` — `GET /api/v1/chats/{chatId}/custom-tools?action=presets&tool=…
 * &asCharacterId=…`, served here by the `chatCustomToolPresets` dispatch,
 * P4.D379 / contract C11). `presets` is the nearest preset of each name, sorted
 * by name; `stores` every store on the walk, nearest first; `unread` the stores
 * that could not be looked in. Read defensively: an absent array is `[]`, an
 * absent `shadows` is `[]`.
 */
export async function fetchCustomToolPresets(
  core: CoreClient,
  chatId: string,
  toolName: string,
  asCharacterId: string,
): Promise<CustomToolPresetPool> {
  const req: ChatCustomToolPresetsRequest = {
    type: 'chatCustomToolPresets',
    chatId,
    tool: toolName,
    asCharacterId,
  };
  const data = await core.dispatchData(req);
  return {
    presets: asArray(data['presets']).map((raw): CustomToolPreset => {
      const r = (raw ?? {}) as Record<string, unknown>;
      return {
        ...readPresetStore(r),
        name: typeof r['name'] === 'string' ? r['name'] : '',
        path: typeof r['path'] === 'string' ? r['path'] : '',
        shadows: asArray(r['shadows']).map(readPresetStore),
      };
    }),
    stores: asArray(data['stores']).map(readPresetStore),
    unread: asArray(data['unread']).map((raw): CustomToolPresetUnread => {
      const r = (raw ?? {}) as Record<string, unknown>;
      return { ...readPresetStore(r), reason: typeof r['reason'] === 'string' ? r['reason'] : '' };
    }),
  };
}
