import type { CoreClient } from '../core/core-client';

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
