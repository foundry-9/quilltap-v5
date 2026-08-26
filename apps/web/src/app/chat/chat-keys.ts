/**
 * The chat query keys, in one place.
 *
 * v5 has no central `lib/query/keys.ts` (v4's 28-namespace module); keys live
 * beside the feature that reads them. The chat family was the one exception —
 * it had no const at all, only the two raw spellings `['chats']` and
 * `['chat', id]` typed out at every call site (the Salon alone carried
 * twenty-seven of them).
 *
 * P4.D125 gives them a home, because the realtime topic map now has to name
 * them: a topic → query-key table that quotes a spelling nobody else imports is
 * a drift waiting to happen (v4's decision 8 — "topics mirror the query-key
 * namespaces" — assumes the namespaces are written down exactly once).
 *
 * The two shapes, and why the prefix rule works:
 *  - {@link chatKeys.all} `['chats']` — the collection reads. The Salon list and
 *    the merge picker both key on `['chats', {…filters}]`, so this bare prefix
 *    reaches every one of them.
 *  - {@link chatKeys.detail} `['chat', id]` SINGULAR — one conversation, and the
 *    prefix of every per-chat sub-key (`['chat', id, 'background']`,
 *    `['chat', id, 'outfit-summary']`, `['chat', id, 'cost', …]`). Invalidating
 *    it therefore reaches the whole row, and — because the collection key is the
 *    PLURAL word — never touches the lists.
 *
 * `detail` accepts a nullable id on purpose: several Salon handlers pass
 * `this.chatId()` straight through, and `['chat', null]` is the key those sites
 * already produced. Narrowing the parameter would change behavior, not just
 * types.
 *
 * @module chat/chat-keys
 */

export const chatKeys = {
  /** The chat-list collection reads (`['chats', {…filters}]` all sit under it). */
  all: ['chats'] as const,
  /** One conversation, and the prefix of every per-chat sub-key. */
  detail: (chatId: string | null | undefined) => ['chat', chatId] as const,
};
