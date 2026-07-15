/**
 * The chat cost/token-aggregate client surface (the P4.6ap round's Shared
 * contract §1). v4's `ChatCostSummary` fetches
 * `GET /api/v1/chats/{chatId}?action=cost` directly; v5 goes through the
 * `chatGetCost` dispatch verb, which lane A (P4.6ao) provides.
 *
 * The types live here — in a lane-owned api file — rather than in
 * `core-contract.ts`, per the round's §1: the request `type` is bridged with a
 * cast at the dispatch call below until the unifier folds `ChatGetCostRequest`
 * into the `CoreRequest` union name-for-name against `types.rs`. This is the
 * established P4.6am / P4.d3 precedent for a cross-lane verb.
 *
 * @module chat/chat-cost.api
 */

import type { CoreClient } from '../core/core-client';
import type { CoreRequest } from '../core/core-contract';

/** The `priceSource` union, verbatim from the Shared contract §1. */
export type CostPriceSource =
  | 'openrouter'
  | 'registry'
  | 'fallback'
  | 'openrouter-estimate'
  | 'unavailable';

/**
 * v4's non-detailed cost breakdown (`lib/services/cost-estimation.service.ts:
 * 139-190`). `estimatedCostUSD` is null when no pricing data could be resolved.
 *
 * Note the REST edge returns this object RAW (v4 `NextResponse.json(breakdown)`
 * — NOT the successResponse envelope); over the dispatch boundary it arrives as
 * the response `data` body, so `dispatchData` is the right reader.
 */
export interface ChatCostDto {
  totalTokens: number;
  promptTokens: number;
  completionTokens: number;
  estimatedCostUSD: number | null;
  priceSource: CostPriceSource;
}

/** The dispatch request for the cost breakdown (§1). */
export interface ChatGetCostRequest {
  type: 'chatGetCost';
  chatId: string;
  /**
   * Absent means false. v5 never sends it: the detailed arm
   * (`messageBreakdown` / `systemEventBreakdown`) has no v4 client either — a
   * named deferral of this order, not an omission.
   */
  detailed?: boolean;
}

export const chatCostKeys = {
  /**
   * Per-chat cost key. `refreshKey` is v4's `refreshKey` prop (the Salon passes
   * `messages.length`): v4 re-fetches when it changes, so it belongs IN the key
   * rather than in a manual invalidation.
   */
  cost: (chatId: string, refreshKey: number | string) =>
    ['chat', chatId, 'cost', refreshKey] as const,
};

/** GET the chat's token/cost aggregate (v4 `?action=cost`, non-detailed). */
export async function fetchChatCost(core: CoreClient, chatId: string): Promise<ChatCostDto> {
  const req: ChatGetCostRequest = { type: 'chatGetCost', chatId };
  const data = await core.dispatchData(req as unknown as CoreRequest);
  return data as unknown as ChatCostDto;
}
