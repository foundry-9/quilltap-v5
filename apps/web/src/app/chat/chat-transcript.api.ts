import type { CoreClient } from '../core/core-client';
import type { ChatTranscriptDto } from '../core/core-contract';

/**
 * The Salon's cheap conditional transcript read (P4.D183 §C.2) — v4
 * `GET /api/v1/messages?chatId=…&action=transcript[&knownVersion=N]`,
 * dispatched as the `chatTranscript` verb over v5's one boundary.
 *
 * This is the authoritative delivery path for every message in the room. The
 * event stream still carries tokens for the turn being generated, but it no
 * longer decides what the room *contains*: a reply persisted while the stream
 * was dropped, an Aurora wardrobe note, a Lantern backdrop announcement or a
 * Commonplace whisper written from the writer task all land here, on the hint
 * the write already published.
 *
 * The read hands back the version it last saw, so the common case — a hint
 * that fired for something other than a message — costs a round trip and the
 * word "unchanged", not a re-serialized conversation. That matters because one
 * busy turn fires wardrobe, backdrop, whisper and memory hints at the same
 * `chats` topic.
 *
 * v4 sends `cache: 'no-store'` here; v5 dispatches over POST `/api/dispatch`,
 * which no cache answers, so there is nothing to suppress (recorded
 * NO-COUNTERPART).
 *
 * @param knownVersion The last applied counter, or `null` for "ask for
 *   everything" — which is right on the first read and after any response we
 *   could not make sense of.
 * @module chat/chat-transcript.api
 */
export async function fetchChatTranscript(
  core: CoreClient,
  chatId: string,
  knownVersion: number | null,
): Promise<ChatTranscriptDto> {
  const data = await core.dispatchData({
    type: 'chatTranscript',
    chatId,
    ...(knownVersion !== null ? { knownVersion } : {}),
  });
  return data as unknown as ChatTranscriptDto;
}
