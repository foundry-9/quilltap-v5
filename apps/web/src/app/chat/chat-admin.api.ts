import type { CoreClient } from '../core/core-client';
import type {
  ChatCreateOutfitSelectionInput,
  SearchReplaceScopeWire,
} from '../core/core-contract';
import type { PreviousOutfitSummary } from '../screens/new-chat/outfit-selector';
import type { AvailableTool } from './tools/tool-settings';

/**
 * The chat-admin verbs the in-chat dialog family reaches for (P4.9E3A's server
 * half, mirrored in `core/core-contract.ts`). The cast verbs live next door in
 * `chat-cast.api.ts`; these are the housekeeping ones — agent mode, titles,
 * merges, re-attribution, hand-run tools.
 *
 * Every function returns the narrow thing its ONE caller needs, read
 * structurally off `dispatchData`: §1 pins the request shapes, not the
 * responses, so nothing here may assume a field it does not check for.
 */

/** What v4's toggle-agent-mode reply drives (`useChatControls.ts:383-390`). */
export interface AgentModeResult {
  /** `data.resolvedAgentModeEnabled ?? data.agentModeEnabled` — the badge's new value. */
  enabled: boolean | null;
  /** v4's toast wording, computed from `resolvedAgentModeEnabled` ALONE. */
  status: 'enabled' | 'disabled' | 'set to inherit';
}

/**
 * §1 `ChatToggleAgentMode` — v4's badge handler
 * (`useChatControls.ts:367-395`).
 *
 * ⚠ **Two arms of a three-armed verb, deliberately.** The column and the verb
 * are tri-state (absent / `null` / boolean), but v4's badge computes
 * `agentModeEnabled === null || !agentModeEnabled` and therefore only ever
 * sends `true` or `false`: from a cleared override it turns the mode ON. There
 * is no v4 affordance that sends `null`, so this helper does not offer one —
 * inventing "back to inherit" here would be a control v4 does not have.
 *
 * The read-back is v4's exactly: `resolvedAgentModeEnabled ?? agentModeEnabled`,
 * which matters because the server DROPS `agentModeEnabled` from the body when
 * the stored column is NULL (`services/chat_admin.rs:325-339`).
 */
export async function toggleAgentMode(
  core: CoreClient,
  chatId: string,
  current: boolean | null,
): Promise<AgentModeResult> {
  const next = current === null || !current;
  const data = await core.dispatchData({ type: 'chatToggleAgentMode', chatId, enabled: next });
  const resolved = data['resolvedAgentModeEnabled'];
  const stored = data['agentModeEnabled'];
  const enabled =
    typeof resolved === 'boolean' ? resolved : typeof stored === 'boolean' ? stored : null;
  const status =
    resolved === true ? 'enabled' : resolved === false ? 'disabled' : 'set to inherit';
  return { enabled, status };
}

/**
 * §1-adjacent — `ChatRegenerateTitle` (v4 `ChatRenameModal.tsx:52-67`).
 *
 * ⚠ **One cheap-LLM call per invocation, in production.** v4 exposes no button
 * of this name: the route fires as a SIDE EFFECT of ticking "Use automatic
 * naming", which is why v5's live verb had no reachable caller until the rename
 * dialog landed (dogfood walk 2026-07-27).
 *
 * Returns the new title, which v4 writes straight into the field it is about to
 * close (`setTitle(data.title)`).
 */
export async function regenerateChatTitle(core: CoreClient, chatId: string): Promise<string> {
  const data = await core.dispatchData({ type: 'chatRegenerateTitle', chatId });
  return String(data['title'] ?? '');
}

/** v4's `roleFilter` values (`BulkCharacterReplaceModal.tsx:49`). */
export type RoleFilter = 'ASSISTANT' | 'USER' | 'both';

/** What v4's bulk-reattribute reply drives (`:172-177`). */
export interface BulkReattributeResult {
  messagesUpdated: number;
  memoriesDeleted: number;
}

/**
 * §1 `ChatBulkReattribute` — v4 `BulkCharacterReplaceModal.tsx:151-158`.
 *
 * `sourceParticipantId` is required and nullable: `null` means "the messages
 * with no participant at all", the operator's own turns. Every memory sourced
 * from a moved message is deleted server-side.
 */
export async function bulkReattribute(
  core: CoreClient,
  params: {
    chatId: string;
    sourceParticipantId: string | null;
    targetParticipantId: string;
    roleFilter: RoleFilter;
  },
): Promise<BulkReattributeResult> {
  const data = await core.dispatchData({ type: 'chatBulkReattribute', ...params });
  return {
    messagesUpdated: Number(data['messagesUpdated'] ?? 0),
    memoriesDeleted: Number(data['memoriesDeleted'] ?? 0),
  };
}

/**
 * §1 `MessageReattribute` — move ONE message to a different participant (v4
 * `ReattributeMessageDialog.tsx:82-90`). Every memory sourced from the message
 * is deleted server-side; the count comes back for the sentence.
 */
export async function reattributeMessage(
  core: CoreClient,
  messageId: string,
  newParticipantId: string,
): Promise<{ memoriesDeleted: number }> {
  const data = await core.dispatchData({ type: 'messageReattribute', messageId, newParticipantId });
  return { memoriesDeleted: Number(data['memoriesDeleted'] ?? 0) };
}

/**
 * §1 `ChatOutfitSummary` — what each character was wearing at the end of a
 * source chat (v4 `MergeConversationModal.tsx:102-110`), for the "Same as last
 * conversation" preview.
 *
 * v4 reads `data.summary`; a body without it yields `null`, which the selector
 * renders as no preview rather than failing.
 */
export async function readOutfitSummary(
  core: CoreClient,
  chatId: string,
): Promise<PreviousOutfitSummary | null> {
  const data = await core.dispatchData({ type: 'chatOutfitSummary', chatId });
  const summary = data['summary'];
  return summary && typeof summary === 'object' ? (summary as PreviousOutfitSummary) : null;
}

/**
 * §1 `ChatMergeConversation` — fold a source conversation's cast + recap into
 * this one (v4 `MergeConversationModal.tsx:172-181`). `chatId` is the TARGET.
 *
 * Returns the server's merged count (`data.merge.mergedCharacterIds.length`,
 * `:187`) or `null` when the body does not carry it — v4 then falls back to the
 * client's own count.
 */
export async function mergeConversation(
  core: CoreClient,
  params: {
    chatId: string;
    sourceChatId: string;
    characterIds: string[];
    outfitSelections: ChatCreateOutfitSelectionInput[];
  },
): Promise<number | null> {
  const data = await core.dispatchData({ type: 'chatMergeConversation', ...params });
  const merge = data['merge'] as { mergedCharacterIds?: unknown } | undefined;
  const ids = merge?.mergedCharacterIds;
  return Array.isArray(ids) ? ids.length : null;
}

/**
 * §1 `ToolsList` — the tool inventory (v4 `GET /api/v1/tools`).
 *
 * `chatId` is what makes the reply carry per-chat availability;
 * `includeSchemas` adds each tool's `parameters` JSON Schema and is only asked
 * for by the Run Tool picker (v4 `RunToolModal.tsx:80`), since it is a much
 * larger body.
 */
export async function listTools(
  core: CoreClient,
  params: { chatId?: string; includeSchemas?: boolean } = {},
): Promise<AvailableTool[]> {
  const data = await core.dispatchData({ type: 'toolsList', ...params });
  const tools = data['tools'];
  return Array.isArray(tools) ? (tools as AvailableTool[]) : [];
}

/**
 * §1-adjacent — `ChatRunTool` (v4 `RunToolModal.tsx:134-142`). v4 treats a body
 * without `success: true` as a failure even on a 200, so the same check is made
 * here rather than trusting the status alone.
 */
export async function runTool(
  core: CoreClient,
  params: {
    chatId: string;
    toolName: string;
    arguments: Record<string, unknown>;
    characterId?: string;
    private: boolean;
  },
): Promise<void> {
  const data = await core.dispatchData({ type: 'chatRunTool', ...params });
  if (data['success'] === false) {
    throw new Error(String(data['error'] ?? data['message'] ?? 'Tool execution failed'));
  }
}

/** §1 `SearchReplacePreview` — the dry-run counts (v4 `useSearchReplace.ts:75-93`). */
export async function searchReplacePreview(
  core: CoreClient,
  params: {
    scope: SearchReplaceScopeWire;
    searchText: string;
    replaceText: string;
    includeMessages: boolean;
    includeMemories: boolean;
  },
): Promise<{
  messageMatches: number;
  memoryMatches: number;
  affectedChats: number;
  affectedMemories: number;
}> {
  const data = await core.dispatchData({ type: 'searchReplacePreview', ...params });
  return {
    messageMatches: Number(data['messageMatches'] ?? 0),
    memoryMatches: Number(data['memoryMatches'] ?? 0),
    affectedChats: Number(data['affectedChats'] ?? 0),
    affectedMemories: Number(data['affectedMemories'] ?? 0),
  };
}

/** §1 `SearchReplaceExecute` — the irreversible write (v4 `:133-146`). */
export async function searchReplaceExecute(
  core: CoreClient,
  params: {
    scope: SearchReplaceScopeWire;
    searchText: string;
    replaceText: string;
    includeMessages: boolean;
    includeMemories: boolean;
  },
): Promise<{
  messagesUpdated: number;
  memoriesUpdated: number;
  chatsAffected: number;
  errors: string[];
}> {
  const data = await core.dispatchData({ type: 'searchReplaceExecute', ...params });
  const errors = data['errors'];
  return {
    messagesUpdated: Number(data['messagesUpdated'] ?? 0),
    memoriesUpdated: Number(data['memoriesUpdated'] ?? 0),
    chatsAffected: Number(data['chatsAffected'] ?? 0),
    errors: Array.isArray(errors) ? errors.map(String) : [],
  };
}
