/**
 * Which system prompt does a character start with? — the CLIENT twin of v4
 * `lib/characters/default-system-prompt.ts` (`baa85e19b`, bug 154).
 *
 * The answer is recorded twice — the character's `defaultSystemPromptId`
 * column and the `isDefault` flag on the prompt itself — and it was resolved
 * by hand at five call sites, three of them disagreeing about what to do when
 * the column named a prompt the character no longer had. Two quietly gave up
 * and started the chat with no prompt at all; v5 reproduced exactly those two
 * (`screens/new-chat/new-chat.logic.ts`, `screens/new-chat/new-chat.state.ts`).
 *
 * One order, stated once: the column when it names a prompt that exists, then
 * the flag, then the first prompt. Structurally typed and free of imports, so
 * every client picker can share it.
 *
 * **Why a twin at all.** v4's module is structurally typed precisely so the
 * client picker and the server's chat initializer can import the SAME file;
 * v5 cannot do that across the Rust boundary, so the read order lives twice —
 * here and in `quilltap-core`'s `default_system_prompt` (P4.D201) — each
 * transcribed independently from v4's file at `baa85e19b`, and each pinned by
 * v4's own five test vectors (`__tests__/unit/lib/characters/
 * default-system-prompt.test.ts`, same sha).
 *
 * The write side of the same fact is the server's `setDefaultSystemPrompt`.
 *
 * ⚠ **NO-COUNTERPART, recorded not ported:** the rest of v4's `useSystemPrompts.
 * ts` hunk at `baa85e19b` — the star's move off `?action=update-prompt` onto
 * the prompt's own PUT route. v5's star never had the dead route: it posts the
 * `characterPromptSetDefault` dispatch verb, so only the optimistic-cache-write
 * and rollback halves of that hunk port (`system-prompts-tab.ts`).
 */

/** The shape the resolver needs of a prompt: an id, and maybe a default flag. */
export interface DefaultablePrompt {
  id: string;
  isDefault?: boolean;
}

/** The shape the resolver needs of a character. */
export interface HasSystemPrompts<P extends DefaultablePrompt> {
  systemPrompts?: P[] | null;
  defaultSystemPromptId?: string | null;
}

/** The prompt a character starts with, or null when they have none at all. */
export function resolveDefaultSystemPrompt<P extends DefaultablePrompt>(
  character: HasSystemPrompts<P>,
): P | null {
  const prompts = character.systemPrompts ?? [];
  if (prompts.length === 0) return null;

  // JS truthiness, deliberately: an empty-string column falls through to the
  // flag exactly as a null one does (v4 `if (character.defaultSystemPromptId)`).
  if (character.defaultSystemPromptId) {
    const named = prompts.find((p) => p.id === character.defaultSystemPromptId);
    if (named) return named;
  }

  return prompts.find((p) => p.isDefault) ?? prompts[0] ?? null;
}

/** The id of {@link resolveDefaultSystemPrompt}, for the pickers that store one. */
export function resolveDefaultSystemPromptId<P extends DefaultablePrompt>(
  character: HasSystemPrompts<P>,
): string | null {
  return resolveDefaultSystemPrompt(character)?.id ?? null;
}
