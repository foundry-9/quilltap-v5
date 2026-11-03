//! The Host's off-scene message kinds, and which of them reach a model (v4
//! `lib/services/host-notifications/off-scene-kinds.ts`, `d68c1af88`).
//!
//! Kept apart from the writer (`host_notifications` / `off_scene`) so the
//! history filters — [`crate::services::message_context::is_record_only_message`]
//! and [`crate::chat_tasks::to_visible_conversation_message`] — can ask the
//! question without importing the writer and everything it pulls in.

/// A "spoken of" introduction: posted the first time an absent character was
/// named (no longer posted since v4 `9a88f862f`; old chats still carry them).
pub const HOST_KIND_OFF_SCENE_CHARACTERS: &str = "off-scene-characters";

/// A left-behind notice (Continue Elsewhere). Its own kind because, unlike a
/// "spoken of" introduction, it stays in the model's history: it marks where the
/// old company split, at the tail of the carried-over transcript (bug 171).
/// Notices posted before this kind existed carry `off-scene-characters` and are
/// told apart by [`LEFT_BEHIND_INTRO_PREFIX`].
pub const HOST_KIND_OFF_SCENE_LEFT_BEHIND: &str = "off-scene-left-behind";

/// How every left-behind notice opens; the legacy rows' only distinguishing mark.
pub const LEFT_BEHIND_INTRO_PREFIX: &str = "The Host observes that";

/// v4 `isOffSceneMentionIntroduction(m)` — true for a Host "spoken of"
/// introduction. The Salon shows it, but it is a record only: the model hears
/// about absent people from the referent ledger's trailing section on every turn
/// they are named, not from a card wedged into the history where it can sever a
/// line from the reply that leans on it. Left-behind notices are not records;
/// they stay in the history. An absent `content` reads as `''` (v4
/// `m.content ?? ''`).
pub fn is_off_scene_mention_introduction(
    system_sender: Option<&str>,
    system_kind: Option<&str>,
    content: Option<&str>,
) -> bool {
    system_sender == Some("host")
        && system_kind == Some(HOST_KIND_OFF_SCENE_CHARACTERS)
        && !content.unwrap_or("").starts_with(LEFT_BEHIND_INTRO_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_shapes() {
        let card = Some("The Host begs leave to introduce a person");
        let legacy = Some("The Host observes that one member of the previous company");
        assert!(is_off_scene_mention_introduction(
            Some("host"),
            Some("off-scene-characters"),
            card
        ));
        assert!(is_off_scene_mention_introduction(
            Some("host"),
            Some("off-scene-characters"),
            None
        ));
        assert!(!is_off_scene_mention_introduction(
            Some("host"),
            Some("off-scene-characters"),
            legacy
        ));
        assert!(!is_off_scene_mention_introduction(
            Some("host"),
            Some("off-scene-left-behind"),
            legacy
        ));
        assert!(!is_off_scene_mention_introduction(
            Some("lantern"),
            Some("off-scene-characters"),
            card
        ));
    }
}
