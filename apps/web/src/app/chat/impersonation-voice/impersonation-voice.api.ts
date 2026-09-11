import type { CoreClient } from '../../core/core-client';

/**
 * The In Their Own Words verb client — the sibling of `previewAnnouncement`
 * (`chat/post-office/post-office.api.ts:144-172`), for the IN-scene rehearsal.
 *
 * v4 POSTs `/api/v1/chats/{id}?action=impersonation-voice-preview` and reads a
 * four-key body. v5 serves no `POST /api/v1/chats/{id}?action=` REST family at
 * all — `announcement-preview` has been dispatch-only since P4.D38 — so the new
 * action follows its sibling onto the dispatch boundary (§B: "No REST edge this
 * round"; P4.D180 defines the verb).
 *
 * @module chat/impersonation-voice/impersonation-voice.api
 */

/** v4's success body (`in-scene-voiced` route, `NextResponse.json` key order). */
export interface ImpersonationVoicePreview {
  proposedMarkdown: string;
  profileName: string;
  modelName: string;
  /**
   * v4's `if (data.profileName || data.modelName)` gate on the RAW body
   * (`useImpersonationVoice.ts:183`), decided here because the coercion below
   * would otherwise make it unanswerable: a raw `0` is falsy and must NOT set
   * the resolved voice, but `String(0 ?? '')` is the truthy `'0'`.
   */
  hasResolvedVoice: boolean;
}

/**
 * Ask the seat's own model to restate the draft. Persists nothing.
 *
 * The two optional ids follow v4's `profileId || undefined` spelling: an empty
 * picker value OMITS the key rather than sending `null` — the wire shape §B
 * pins, and the same normalization `previewAnnouncement` already does for
 * `systemPromptId`.
 */
export async function previewImpersonationVoice(
  core: CoreClient,
  args: {
    chatId: string;
    participantId: string;
    seedMarkdown: string;
    connectionProfileId?: string | null;
    systemPromptId?: string | null;
  },
): Promise<ImpersonationVoicePreview> {
  const data = await core.dispatchData({
    type: 'chatImpersonationVoicePreview',
    chatId: args.chatId,
    participantId: args.participantId,
    // v4 `seedMarkdown: seed.trim()` (`useImpersonationVoice.ts:164`).
    seedMarkdown: args.seedMarkdown.trim(),
    ...(args.connectionProfileId ? { connectionProfileId: args.connectionProfileId } : {}),
    ...(args.systemPromptId ? { systemPromptId: args.systemPromptId } : {}),
  });
  return {
    // v4 `String(data.proposedMarkdown || '').trim()` (`:181`) — `||`, not `??`,
    // so a falsy-but-present value (0, false, '') lands as the empty string and
    // the dialog shows its "Nothing came back" line.
    proposedMarkdown: String(data['proposedMarkdown'] || '').trim(),
    // v4 `String(data.profileName ?? '')` / `String(data.modelName ?? '')`
    // (`:184-185`) — `??` here, which is v4's own inconsistency, carried.
    profileName: String(data['profileName'] ?? ''),
    modelName: String(data['modelName'] ?? ''),
    hasResolvedVoice: Boolean(data['profileName'] || data['modelName']),
  };
}
