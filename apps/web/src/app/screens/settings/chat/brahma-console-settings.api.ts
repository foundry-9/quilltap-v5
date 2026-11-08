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
 * v4 `c539c82e0` adds the inline `@Brahma` engine,
 * `brahmaConsole.inlineConnectionProfileId`. The PUT merges over the stored
 * settings, so sending only this field leaves the turn budget alone — and the
 * turn-budget dial likewise sends only its own field (v4
 * `BrahmaConsoleSettings.tsx:17-22`).
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
  /**
   * The connection profile the inline `@Brahma:` / `@Brahma?` answerer uses in
   * Salon chats, or `null` for the user's default (v4 `c539c82e0`). A server
   * that has not learned the key yet omits it — read as `null`.
   */
  inlineConnectionProfileId: string | null;
}

/** Read `inlineConnectionProfileId` defensively: a string, else `null`. */
function readInline(data: Record<string, unknown>): string | null {
  const v = data['inlineConnectionProfileId'];
  return typeof v === 'string' ? v : null;
}

/** Read `maxAgentTurns` defensively, falling back to the default (v4's own load guard). */
function readTurns(data: Record<string, unknown>, fallback: number): number {
  return typeof data['maxAgentTurns'] === 'number' ? (data['maxAgentTurns'] as number) : fallback;
}

/** v4 GET `/settings/brahma-console` → the budget `{maxAgentTurns}`. */
export async function getBrahmaConsoleSettings(core: CoreClient): Promise<BrahmaConsoleSettingsDto> {
  const req: BrahmaConsoleSettingsRequest = { type: 'brahmaConsoleSettings' };
  const data = await core.dispatchData(req);
  return {
    maxAgentTurns: readTurns(data, DEFAULT_MAX_AGENT_TURNS),
    inlineConnectionProfileId: readInline(data),
  };
}

/**
 * v4 PUT `/settings/brahma-console` with the budget ALONE → the STORED echo
 * (throws on validation). It never carries `inlineConnectionProfileId`, so a
 * budget save never touches the engine.
 */
export async function updateBrahmaConsoleSettings(
  core: CoreClient,
  maxAgentTurns: number,
): Promise<BrahmaConsoleSettingsDto> {
  const req: BrahmaConsoleSettingsUpdateRequest = { type: 'brahmaConsoleSettingsUpdate', maxAgentTurns };
  const data = await core.dispatchData(req);
  return { maxAgentTurns: readTurns(data, maxAgentTurns), inlineConnectionProfileId: readInline(data) };
}

/**
 * v4 `InlineBrahmaProfileSetting`'s PUT (`c539c82e0`): `{inlineConnectionProfileId}`
 * ALONE — `null` clears it, an id sets it; the budget is absent, so it is kept.
 * Answers the STORED settings.
 */
export async function updateInlineBrahmaProfile(
  core: CoreClient,
  profileId: string | null,
): Promise<BrahmaConsoleSettingsDto> {
  const req: BrahmaConsoleSettingsUpdateRequest = {
    type: 'brahmaConsoleSettingsUpdate',
    inlineConnectionProfileId: profileId,
  };
  const data = await core.dispatchData(req);
  return {
    maxAgentTurns: readTurns(data, DEFAULT_MAX_AGENT_TURNS),
    inlineConnectionProfileId: readInline(data),
  };
}
