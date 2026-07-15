/**
 * The LLM-logs client surface (the P4.6ar/as round's Shared contract §1) — a port
 * of v4 `app/salon/[id]/hooks/useLLMLogs.ts` plus the log DTOs the Inspector
 * renders (v4 `lib/schemas/llm-log.types.ts`).
 *
 * v4's hook fetches `GET /api/v1/llm-logs?chatId=…&includeMessages=true`; v5 goes
 * through the `llmLogsList` dispatch verb, which lane A (P4.6ar) provides.
 *
 * LOCAL REQUEST TYPE, BY ORDER: this round's SPA lanes must not touch
 * `core-contract.ts` — {@link LlmLogsListRequest} is declared here and cast at the
 * dispatch call site; the unifier folds it into the `CoreRequest` union
 * name-for-name against `types.rs` (the P4.6ao `chatGetCost` precedent).
 *
 * @module chat/llm-logs.api
 */

import type { CoreClient } from '../core/core-client';
import type { CoreRequest } from '../core/core-contract';

/**
 * The nineteen log types (v4 `LLMLogTypeEnum`, `llm-log.types.ts:17-37`).
 *
 * Note the Inspector's badge/filter tables cover only twelve of them — the other
 * seven fall through to the unknown-type arms BY DESIGN (see
 * {@link import('./llm-inspector-entry').LLMInspectorEntry}); v4 is the same.
 */
export type LlmLogType =
  | 'CHAT_MESSAGE'
  | 'TOOL_CONTINUATION'
  | 'MEMORY_EXTRACTION'
  | 'TITLE_GENERATION'
  | 'CONTEXT_COMPRESSION'
  | 'SUMMARIZATION'
  | 'IMAGE_PROMPT_CRAFTING'
  | 'CHARACTER_WIZARD'
  | 'IMAGE_DESCRIPTION'
  | 'DANGER_CLASSIFICATION'
  | 'APPEARANCE_RESOLUTION'
  | 'AI_IMPORT'
  | 'SCENE_STATE_TRACKING'
  | 'CHARACTER_OPTIMIZER'
  | 'EXTERNAL_PROMPT'
  | 'AUTO_CONFIGURE'
  | 'IMAGE_GENERATION'
  | 'WARDROBE_IMAGE_ANALYSIS'
  | 'ANSWER_CONFIRMATION';

/** One prompt message in a logged request (v4 `LLMLogMessageSummarySchema` :44-49). */
export interface LlmLogMessageSummary {
  role: string;
  /** The FULL message content. */
  content: string;
  /** Deprecated in v4: the old truncated field, kept for backward compat. */
  contentPreview?: string;
  contentLength: number;
  hasAttachments: boolean;
}

/** The logged request summary (v4 `LLMLogRequestSummarySchema` :57-63). */
export interface LlmLogRequestSummary {
  messageCount: number;
  messages: LlmLogMessageSummary[];
  temperature?: number | null;
  maxTokens?: number | null;
  toolCount: number;
}

/** The logged response summary (v4 `LLMLogResponseSummarySchema` :71-82). */
export interface LlmLogResponseSummary {
  /** The FULL response content. */
  content: string;
  /** Deprecated in v4: the old truncated field, kept for backward compat. */
  contentPreview?: string;
  contentLength: number;
  /** Deprecated in v4: kept for backward compat with old log entries. */
  fullContent?: string | null;
  error?: string | null;
}

/** Token usage (v4 `LLMLogTokenUsageSchema` :93-96). */
export interface LlmLogTokenUsage {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}

/**
 * Prompt-cache usage (v4 `LLMLogCacheUsageSchema` :104-106). BOTH keys are
 * `.optional()` and NOT nullable — which is exactly why the Usage tab tests each
 * with `!== undefined` rather than truthiness: a real zero must still render.
 */
export interface LlmLogCacheUsage {
  cacheCreationInputTokens?: number;
  cacheReadInputTokens?: number;
}

/** One LLM log row (v4 `LLMLogSchema` :131-178, the fields the Inspector reads). */
export interface LlmLogDto {
  id: string;
  userId: string;
  type: LlmLogType | string;
  messageId?: string | null;
  chatId?: string | null;
  characterId?: string | null;
  autonomousRunId?: string | null;
  provider: string;
  modelName: string;
  request: LlmLogRequestSummary;
  response: LlmLogResponseSummary;
  usage?: LlmLogTokenUsage | null;
  cacheUsage?: LlmLogCacheUsage | null;
  durationMs?: number | null;
  createdAt: string;
  updatedAt: string;
}

/**
 * The `llmLogsList` verb (Shared contract §1). Every field but `type` is optional
 * on the wire; the Inspector sends only `chatId` + `includeMessages`, which is
 * exactly what v4's hook puts in its query string (`useLLMLogs.ts:28`).
 *
 * `limit`/`offset` are RAW STRINGS by contract — the handler ports v4's
 * parseInt/Math.min body (including the NaN no-slice quirk) so dispatch and REST
 * agree. The Inspector sends neither.
 */
export interface LlmLogsListRequest {
  type: 'llmLogsList';
  messageId?: string;
  chatId?: string;
  characterId?: string;
  /** v4's `?type=` query param; the wire name avoids colliding with the envelope key. */
  logType?: string;
  standalone?: boolean;
  includeMessages?: boolean;
  limit?: string;
  offset?: string;
}

/**
 * The `llmLogsList` response body (Shared contract §1): v4's exact envelope —
 * `total` is pre-slice, `count` post-slice.
 */
export interface LlmLogsListResponse {
  logs: LlmLogDto[];
  count: number;
  total: number;
  limit: number;
  offset: number;
}

export const llmLogKeys = {
  /** v4 `queryKeys.llmLogs.byChat(chatId)`. */
  byChat: (chatId: string) => ['llmLogs', 'byChat', chatId] as const,
};

/**
 * Fetch every log for a chat (v4 `useLLMLogs.ts:25-30` — the combined endpoint
 * with `includeMessages=true`).
 *
 * The cast at the dispatch call site is the round's ownership rule, not a type
 * hole: `LlmLogsListRequest` matches lane A's `Request::LlmLogsList` field for
 * field, and the unifier replaces the cast with the folded union member.
 */
export async function fetchChatLlmLogs(core: CoreClient, chatId: string): Promise<LlmLogDto[]> {
  const req: LlmLogsListRequest = { type: 'llmLogsList', chatId, includeMessages: true };
  const data = await core.dispatchData(req as unknown as CoreRequest);
  return (data as unknown as LlmLogsListResponse).logs ?? [];
}

/**
 * Which messages have logs (v4 `useLLMLogs.ts:35-41`): the set of `messageId`s
 * present on the chat's logs. Drives the per-message "View LLM logs" icon.
 *
 * v4 filters on truthiness before mapping, so a log with `messageId: ''` or null
 * contributes nothing — a chat-level log (title generation, compression) has no
 * message to point at.
 */
export function deriveMessagesWithLogs(logs: LlmLogDto[]): Set<string> {
  return new Set(logs.filter((log) => log.messageId).map((log) => log.messageId!));
}
