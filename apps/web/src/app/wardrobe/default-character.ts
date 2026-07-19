/**
 * The shell footer button's chat-scope resolution — a port of v4
 * `components/layout/left-sidebar/sidebar-footer.tsx:39-101`.
 */

import type { CoreClient } from '../core/core-client';

/**
 * Match a UUID immediately following /salon/ (v4 `SALON_CHAT_PATH_RE`,
 * `sidebar-footer.tsx:39`). If the user is reading a chat, the sidebar's
 * Wardrobe button should pass that chat id along so the dialog can show
 * Wearing now / Wear this against the right scope.
 */
export const SALON_CHAT_PATH_RE =
  /^\/salon\/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})/;

interface ParticipantLite {
  id: string;
  controlledBy?: string | null;
  displayOrder?: number | null;
  character?: { id?: string | null } | null;
}

interface MessageLite {
  role?: string | null;
  participantId?: string | null;
}

/**
 * Resolve the wardrobe dialog's default character for a chat
 * (v4 `resolveDefaultCharacterForChat`, `sidebar-footer.tsx:60-101`). Priority:
 *   1. Most recent assistant message authored by a non-user-controlled
 *      participant.
 *   2. First non-user-controlled CHARACTER participant in chat order.
 *   3. null — the caller lets the dialog fall back to alphabetical first.
 *
 * v5 adaptation: v4 issues two fetches (the chat + `/api/v1/messages`); v5's
 * `chatGet` returns participants AND messages in one payload, so a single
 * dispatch serves both priorities. The message role is v5's uppercase
 * `'ASSISTANT'` (v4's client messages API lowercases it).
 */
export async function resolveDefaultCharacterForChat(
  core: CoreClient,
  chatId: string,
): Promise<string | null> {
  try {
    const data = await core.dispatchData({ type: 'chatGet', chatId });
    const chat = data['chat'] as
      | { participants?: ParticipantLite[]; messages?: MessageLite[] }
      | undefined;
    const participants = chat?.participants ?? [];
    const eligible = participants.filter((p) => p.controlledBy !== 'user' && p.character?.id);
    if (eligible.length === 0) return null;
    const eligibleIds = new Set(eligible.map((p) => p.character?.id ?? ''));

    // Priority 1: most recent assistant message attribution — scan from the
    // end; messages arrive in chronological order (v4 :80-92).
    const messages = chat?.messages ?? [];
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i];
      if (m.role !== 'ASSISTANT' || !m.participantId) continue;
      const p = eligible.find((pp) => pp.id === m.participantId);
      const cid = p?.character?.id;
      if (cid && eligibleIds.has(cid)) return cid;
    }

    // Priority 2: first non-user-controlled participant in chat order (v4 :94-97).
    const sortedByOrder = [...eligible].sort(
      (a, b) => (a.displayOrder ?? 0) - (b.displayOrder ?? 0),
    );
    return sortedByOrder[0]?.character?.id ?? null;
  } catch {
    return null;
  }
}
