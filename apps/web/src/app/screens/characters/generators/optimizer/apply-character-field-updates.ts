/**
 * Shared save-dispatch for character field updates (v4
 * `components/characters/apply-character-field-updates.ts`, 98 lines,
 * transcribed over EXISTING v5 verbs). v4's `PUT /api/v1/characters/[id]`
 * body accepts simple scalars, the scenarios array, and the single
 * physicalDescription object, but STRIPS `systemPrompts` — system prompts
 * persist through their own dedicated endpoints. This helper centralises that
 * fan-out (`characterPromptUpdate` / `characterPromptCreate` / `characterUpdate`,
 * `core-contract.ts:1127-1152` / `:1026-1031`) so the optimizer's apply step
 * routes updates the same way v4's does, with the same partial-failure
 * handling: it never throws on a dispatch failure — failures are collected and
 * returned so the caller can decide how to surface a partial apply (there is
 * no transaction across endpoints, and each write is idempotent, so a re-run
 * is safe).
 *
 * v5 has NO existing twin of this helper under any name (`details-tab.ts`'s
 * `runTemplateSave` inlines the same fan-out for its own two callers rather
 * than sharing it — that duplication predates this lane and is out of scope
 * here); this file is the first shared home, named for its optimizer caller.
 */

import type { CoreClient } from '../../../../core/core-client';

export interface CharacterFieldUpdates {
  /** Fields that map onto the main `characterUpdate` dispatch. */
  mainUpdates: Record<string, unknown>;
  /** Existing system prompts to refine via `characterPromptUpdate`. */
  promptUpdates?: Array<{ id: string; content: string }>;
  /** New system prompts to create via `characterPromptCreate`. */
  promptCreates?: Array<{ name: string; content: string }>;
  /** Optional fallback error strings (used only when the server returns none). */
  messages?: {
    promptUpdateFailed?: string;
    promptCreateFailed?: (name: string) => string;
    mainPutFailed?: string;
  };
}

const DEFAULT_MESSAGES = {
  promptUpdateFailed: 'A system prompt could not be saved.',
  promptCreateFailed: (name: string) => `The system prompt "${name}" could not be saved.`,
  mainPutFailed: 'The character could not be updated.',
};

/**
 * Dispatches character field updates across `characterUpdate` and the
 * dedicated system-prompt verbs. Fires prompt refinements, then prompt
 * creations, then the main update (only when there is something to send).
 * Returns the collected error messages (empty array on full success); never
 * rejects on a dispatch error.
 */
export async function applyCharacterFieldUpdates(
  core: CoreClient,
  characterId: string,
  updates: CharacterFieldUpdates,
): Promise<{ errors: string[] }> {
  const { mainUpdates, promptUpdates = [], promptCreates = [], messages } = updates;
  const msg = {
    promptUpdateFailed: messages?.promptUpdateFailed ?? DEFAULT_MESSAGES.promptUpdateFailed,
    promptCreateFailed: messages?.promptCreateFailed ?? DEFAULT_MESSAGES.promptCreateFailed,
    mainPutFailed: messages?.mainPutFailed ?? DEFAULT_MESSAGES.mainPutFailed,
  };
  const errors: string[] = [];

  for (const { id, content } of promptUpdates) {
    try {
      await core.dispatchData({
        type: 'characterPromptUpdate',
        characterId,
        promptId: id,
        content,
      });
    } catch (err) {
      errors.push(err instanceof Error && err.message ? err.message : msg.promptUpdateFailed);
    }
  }

  for (const { name, content } of promptCreates) {
    try {
      await core.dispatchData({ type: 'characterPromptCreate', characterId, name, content });
    } catch (err) {
      errors.push(err instanceof Error && err.message ? err.message : msg.promptCreateFailed(name));
    }
  }

  if (Object.keys(mainUpdates).length > 0) {
    try {
      await core.dispatchData({ type: 'characterUpdate', characterId, character: mainUpdates });
    } catch (err) {
      errors.push(err instanceof Error && err.message ? err.message : msg.mainPutFailed);
    }
  }

  return { errors };
}
