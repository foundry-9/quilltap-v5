/**
 * The Host's off-scene message kinds — a TS twin of v4
 * `lib/services/host-notifications/off-scene-kinds.ts` (`9a88f862f`, at
 * `236c04e35`). The Salon's feed filter reads {@link isOffSceneMentionIntroduction}
 * (the one line P4.D344 adds to `salon-conversation.ts` — this lane's HANDOFF):
 * the old Host "spoken of" cards leave the FEED, replaced by the name chips; they
 * stay in the transcript. The Rust side is P4.D347's.
 *
 * @module chat/absent-mentions/off-scene-kinds
 */

/** A "spoken of" introduction: posted the first time an absent character is named. */
export const HOST_KIND_OFF_SCENE_CHARACTERS = 'off-scene-characters';

/**
 * A left-behind notice (Continue Elsewhere). Its own kind because, unlike a
 * "spoken of" introduction, it stays in the model's history: it marks where the
 * old company split. Notices posted before this kind existed carry
 * `off-scene-characters` and are told apart by {@link LEFT_BEHIND_INTRO_PREFIX}.
 */
export const HOST_KIND_OFF_SCENE_LEFT_BEHIND = 'off-scene-left-behind';

/** How every left-behind notice opens; the legacy rows' only distinguishing mark. */
export const LEFT_BEHIND_INTRO_PREFIX = 'The Host observes that';

/**
 * True for a Host "spoken of" introduction — a record only. Left-behind notices
 * are not records; they stay in the history.
 */
export function isOffSceneMentionIntroduction(m: {
  systemSender?: string | null;
  systemKind?: string | null;
  content?: string | null;
}): boolean {
  return (
    m.systemSender === 'host' &&
    m.systemKind === HOST_KIND_OFF_SCENE_CHARACTERS &&
    !(m.content ?? '').startsWith(LEFT_BEHIND_INTRO_PREFIX)
  );
}
