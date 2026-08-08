import type { CoreClient } from '../../../core/core-client';
import type {
  BrahmaConsoleSettingsRequest,
  BrahmaConsoleSettingsUpdateRequest,
} from '../../../core/core-contract';

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
 * consumed here. The two request variants live in `core/core-contract.ts`
 * (folded into the union at unification, mirroring `api/types.rs`
 * name-for-name — P4.D57's Shared contract).
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

/** Read `maxAgentTurns` defensively, falling back to the default (v4's own load guard). */
function readTurns(data: Record<string, unknown>, fallback: number): number {
  return typeof data['maxAgentTurns'] === 'number' ? (data['maxAgentTurns'] as number) : fallback;
}

/** v4 GET `/settings/brahma-console` → the budget `{maxAgentTurns}`. */
export async function getBrahmaConsoleSettings(core: CoreClient): Promise<BrahmaConsoleSettingsDto> {
  const req: BrahmaConsoleSettingsRequest = { type: 'brahmaConsoleSettings' };
  const data = await core.dispatchData(req);
  return { maxAgentTurns: readTurns(data, DEFAULT_MAX_AGENT_TURNS) };
}

/** v4 PUT `/settings/brahma-console` → the STORED echo (throws on validation). */
export async function updateBrahmaConsoleSettings(
  core: CoreClient,
  maxAgentTurns: number,
): Promise<BrahmaConsoleSettingsDto> {
  const req: BrahmaConsoleSettingsUpdateRequest = { type: 'brahmaConsoleSettingsUpdate', maxAgentTurns };
  const data = await core.dispatchData(req);
  return { maxAgentTurns: readTurns(data, maxAgentTurns) };
}
