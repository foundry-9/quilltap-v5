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
