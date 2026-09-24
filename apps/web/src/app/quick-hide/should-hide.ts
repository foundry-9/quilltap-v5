import {
  type ConciergeState,
  conciergeStateUsesUncensoredRoute,
} from '../chat/concierge-state';

/**
 * The quick-hide predicates, transcribed from v4
 * `components/providers/quick-hide-provider.tsx` (`shouldHideByIds` `:183-196`
 * and `shouldHideChat` `:203-215`).
 *
 * Kept pure and separate from the service so every consumer's tag-collection
 * semantics can be unit-pinned without standing up Angular DI.
 *
 * The P4.9d non-port ruling on `shouldHideChat` is RETIRED. It read v4 right at
 * the time: the method took a `chat.isDangerous` that no real payload carried,
 * so its dangerous arm could never fire, and every v4 consumer inlined the
 * check on the REAL field instead. v4 `c43d3b1b4` fixed exactly that — the
 * method now takes the DERIVED `conciergeState`, all four inline filters call
 * it, and it is THE rule rather than a dead twin of one. So v5 lands it.
 */

/**
 * v4 `:184-193`: false when there are no ids at all; true when ANY non-empty id
 * is in the hidden set. Nullish entries are skipped, not treated as matches
 * (v4 `:189` guards with `if (tagId && …)`).
 */
export function shouldHideByIds(
  hiddenTagIds: ReadonlySet<string>,
  tagIds?: ReadonlyArray<string | null | undefined>,
): boolean {
  if (!tagIds || tagIds.length === 0) {
    return false;
  }
  for (const tagId of tagIds) {
    if (tagId && hiddenTagIds.has(tagId)) {
      return true;
    }
  }
  return false;
}

/**
 * THE quick-hide rule for a chat, in one place (v4 `:203-215`). "Dangerous
 * Chats" hides whatever takes the uncensored route — Flagged (the Concierge's
 * verdict) and Uncensored (the operator's) — never a Vouched Safe chat that
 * merely carries a preserved label underneath.
 *
 * Note the arm order against v5's four pre-lane filters: v4 asks the TAG
 * question first and the danger question second, where three of v5's four
 * inline filters asked danger first. Both arms are pure and neither
 * short-circuits anything observable, so the reordering is behaviour-neutral —
 * recorded rather than reasoned about again.
 */
export function shouldHideChat(
  hiddenTagIds: ReadonlySet<string>,
  hideDangerousChats: boolean,
  chat: { characterTags?: ReadonlyArray<string | null | undefined>; conciergeState?: ConciergeState },
): boolean {
  if (shouldHideByIds(hiddenTagIds, chat.characterTags)) {
    return true;
  }
  if (hideDangerousChats && chat.conciergeState && conciergeStateUsesUncensoredRoute(chat.conciergeState)) {
    return true;
  }
  return false;
}
