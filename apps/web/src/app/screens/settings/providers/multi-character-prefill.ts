/**
 * Multi-character turn anchoring — the `[Name]` assistant prefill switch
 * (v4 `lib/llm/multi-character-prefill.ts`, `23af7146`).
 *
 * In a multi-character chat every reply is anchored to the character whose turn
 * it is, by one of two routes:
 *
 *  - **prefill** — the request ends with an assistant message containing
 *    `[Character Name]`, so the model structurally continues only that
 *    character's line;
 *  - **prose** — an instruction is appended to the system message instead,
 *    leaving the conversation ending on a user message.
 *
 * Which route suits a profile is a property of the model on the other end, not
 * of the provider, so it lives on the connection profile as
 * `multiCharacterPrefill`. Anthropic 4.6+ *rejects* a request that ends on an
 * assistant message, which is why Anthropic profiles default to off; Ollama
 * opens a thinking model's reasoning block from the chat template at the start
 * of the assistant turn, so a prefill means `message.thinking` comes back empty
 * however the profile's Enable Thinking box is set (v4 bug 68).
 *
 * **This is the CLIENT half only** (v4 `defaultMultiCharacterPrefill`, its
 * `:41-44`): the seed the profile editor shows and re-seeds on a provider
 * switch. The resolution v4 calls `profileUsesNamePrefill` (`:52-59`) is the
 * server's business — v5 keeps it in the core (P4.D79's
 * `services/multi_character_prefill.rs`), and nothing in the SPA resolves a
 * stored value on its own: the tri-state `null` reaches the form and is
 * replaced by this default so the box reflects the behaviour the server would
 * actually pick.
 */

/** Providers whose models cannot take an assistant prefill at all (v4 `:36`). */
const PREFILL_HOSTILE_PROVIDERS = new Set(['ANTHROPIC']);

/**
 * The value a newly created profile should start with, given its provider. Off
 * for Anthropic (4.6+ hard-rejects an assistant tail), on everywhere else — the
 * historic behaviour. Matched case-insensitively; an absent/empty provider is
 * on (v4 `:41-44`).
 */
export function defaultMultiCharacterPrefill(provider: string | null | undefined): boolean {
  if (!provider) return true;
  return !PREFILL_HOSTILE_PROVIDERS.has(provider.toUpperCase());
}
