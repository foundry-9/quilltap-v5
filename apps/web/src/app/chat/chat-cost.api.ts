/**
 * The chat cost/token-aggregate client surface (the P4.6ap round's Shared
 * contract §1). v4's `ChatCostSummary` fetches
 * `GET /api/v1/chats/{chatId}?action=cost` directly; v5 goes through the
 * `chatGetCost` dispatch verb, which lane A (P4.6ao) provides.
 *
 * `ChatGetCostRequest` was folded into the `CoreRequest` union at unification
 * (name-for-name against `types.rs`, pinned by `p4_6ao_wire_contract.rs`); it
 * is re-exported from here so consumers keep one import site.
 *
 * @module chat/chat-cost.api
 */

import type { CoreClient } from '../core/core-client';
import type { ChatGetCostRequest } from '../core/core-contract';

export type { ChatGetCostRequest } from '../core/core-contract';

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
  const data = await core.dispatchData(req);
  return data as unknown as ChatCostDto;
}
