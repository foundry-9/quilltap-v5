/**
 * The dice button's request and the shape of the pending-result chip it
 * produces (v4 `components/chat/rng-request.ts`, `4877fc2da`). Shared by the
 * RNG dropdown and the composer's `/d6`, `/d20`, `/coin`, `/bottle` commands:
 * one request, one chip.
 *
 * v4's `requestRngDirect` (the legacy non-preview roll that posts the message
 * itself) is NOT ported: v5 always previews — the dropdown's `pendingResult` is
 * an unconditional output the Salon always listens to (`rng-dropdown.ts`), so
 * the legacy branch has no v5 caller.
 *
 * @module chat/rng-request
 */

import { coreErrorMessage, type CoreClient } from '../core/core-client';
import { rollRng, type RngKind } from './chat-cast.api';

/**
 * What to roll: a die with this many sides, a coin, or the bottle. v4's name
 * for exactly the `chatRng` verb's `RngKind` union (measured identical).
 */
export type RngRollType = RngKind;

export interface RngRequestArgs {
  type: RngRollType;
  rolls: number;
}

/**
 * A rolled result waiting in the composer (v4 `RngPendingResult`, moved here
 * from the dropdown). The Salon turns it into a `pendingToolResults` entry on
 * the next send, which the spine writes as a TOOL message ahead of the user's.
 */
export interface RngPendingResult {
  tool: 'rng';
  displayName: string;
  icon: string;
  summary: string;
  formattedResult: string;
  requestPrompt: string;
  arguments: Record<string, unknown>;
  success: boolean;
}

/**
 * Roll in preview mode and shape the result as the composer's pending chip.
 * Nothing is posted to the chat; the chip rides along with the next send.
 *
 * v4 throws `data.message || data.error || 'RNG failed'` on a refused request;
 * the core client already throws the server's text, so a refusal surfaces as
 * that text with v4's fallback for a throw that carries none.
 */
export async function requestRngPendingResult(
  core: CoreClient,
  chatId: string,
  args: RngRequestArgs,
): Promise<RngPendingResult> {
  let result: Awaited<ReturnType<typeof rollRng>>;
  try {
    result = await rollRng(core, chatId, args.type, args.rolls, true);
  } catch (err) {
    throw new Error(coreErrorMessage(err, 'RNG failed'));
  }
  if (!result) {
    throw new Error('RNG did not return a preview');
  }
  return {
    tool: 'rng',
    displayName: 'Random Number Generator',
    icon: '🎲',
    summary: result.summary,
    formattedResult: result.formattedText,
    requestPrompt: result.requestPrompt,
    arguments: result.arguments,
    success: true,
  };
}
