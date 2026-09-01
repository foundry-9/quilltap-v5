/**
 * The client twin of the server's chat-activity chokepoint (v4
 * `lib/chat/chat-activity.ts`, `735d9408c` — bug 112). v4's components import
 * `chatActivityAt` straight from that module; v5's SPA cannot reach into the
 * Rust core, so this is its transcription — kept to the one exported function
 * the client actually uses, character for character.
 *
 * A chat's `updatedAt` moves whenever anything about the row changes — a
 * generated story background landing, a context summary being folded, a
 * Concierge reroute, a token-cost tally. None of that is the conversation
 * moving forward, so none of it belongs on the card the reader scans to find
 * where they left off.
 */

/** A chat as far as the date readout is concerned. */
export interface ChatActivityShape {
  lastMessageAt?: string | null;
  createdAt: string;
}

/**
 * The timestamp to sort and display a chat by: when a character last posted,
 * falling back to when the chat was created.
 *
 * The fallback is `createdAt`, **not** `updatedAt` — a chat where only the Staff
 * has ever spoken has had no conversational activity at all, and dating it by
 * the last background image regenerated is the very drift this exists to stop.
 * `createdAt` is the honest, and stable, answer.
 *
 * `??` is deliberate (v4's own operator): nullish, not falsy.
 */
export function chatActivityAt(chat: ChatActivityShape): string {
  return chat.lastMessageAt ?? chat.createdAt;
}
