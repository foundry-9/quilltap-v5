import type { CoreClient } from '../../../core/core-client';
import type { CoreRequest } from '../../../core/core-contract';

/**
 * The Brahma Console agent-turn-budget client surface (v4 `6452e2c3`, the SPA
 * half of P4.D59). The instance-wide setting `instance_settings['brahmaConsole']`
 * caps how many tool-use turns the Console (and the one-shot `@Brahma` path) may
 * take before it must answer.
 *
 * This is an INSTANCE setting, so — like Data Retention and Taboo — it fetches
 * for itself over the dedicated `brahmaConsoleSettings` /
 * `brahmaConsoleSettingsUpdate` dispatch verbs (P4.D57's Shared contract), NOT
 * the per-user `chatSettings` blob.
 *
 * The response is read DEFENSIVELY through `CoreClient.dispatchData` (the
 * settings precedent — data-retention, taboo, text-replacements): the server
 * pins the body `{ maxAgentTurns }`, so no narrowed `CoreResponse` variant is
 * consumed here. P4.D57 owns the Rust dispatch + the wire contract; this lane
 * (P4.D59) owns only the SPA card + its wiring, so the request `type` is bridged
 * with the `as unknown as CoreRequest` cast rather than folding the two request
 * variants into the union (which lives in `core/core-contract.ts`, outside this
 * lane's ownership). The verb NAMES mirror the contract exactly.
 *
 * @module screens/settings/chat/brahma-console-settings.api
 */

/** v4 `DEFAULT_MAX_AGENT_TURNS` — the fallback when the setting is unset/unreadable. */
export const DEFAULT_MAX_AGENT_TURNS = 50;
/** v4 `MIN_TURNS` — the lower bound of the input. */
export const MIN_TURNS = 5;
/** v4 `MAX_TURNS` — the upper bound of the input. */
export const MAX_TURNS = 200;

/** The instance-wide Brahma Console budget (v4 `brahmaConsole` instance setting). */
export interface BrahmaConsoleSettingsDto {
  /** Tool-use turns the Console may take before it must answer (default 50; bounds 5–200). */
  maxAgentTurns: number;
}

/** v4 GET `/api/v1/settings/brahma-console` (§ Shared contract). */
interface BrahmaConsoleSettingsRequest {
  type: 'brahmaConsoleSettings';
}

/** v4 PUT `/api/v1/settings/brahma-console` — merge + validate + echo the STORED value. */
interface BrahmaConsoleSettingsUpdateRequest {
  type: 'brahmaConsoleSettingsUpdate';
  maxAgentTurns: number;
}

/** Read `maxAgentTurns` defensively, falling back to the default (v4's own load guard). */
function readTurns(data: Record<string, unknown>, fallback: number): number {
  return typeof data['maxAgentTurns'] === 'number' ? (data['maxAgentTurns'] as number) : fallback;
}

/** v4 GET `/settings/brahma-console` → the budget `{maxAgentTurns}`. */
export async function getBrahmaConsoleSettings(core: CoreClient): Promise<BrahmaConsoleSettingsDto> {
  const req: BrahmaConsoleSettingsRequest = { type: 'brahmaConsoleSettings' };
  const data = await core.dispatchData(req as unknown as CoreRequest);
  return { maxAgentTurns: readTurns(data, DEFAULT_MAX_AGENT_TURNS) };
}

/** v4 PUT `/settings/brahma-console` → the STORED echo (throws on validation). */
export async function updateBrahmaConsoleSettings(
  core: CoreClient,
  maxAgentTurns: number,
): Promise<BrahmaConsoleSettingsDto> {
  const req: BrahmaConsoleSettingsUpdateRequest = { type: 'brahmaConsoleSettingsUpdate', maxAgentTurns };
  const data = await core.dispatchData(req as unknown as CoreRequest);
  return { maxAgentTurns: readTurns(data, maxAgentTurns) };
}
