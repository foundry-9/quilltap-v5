//! The Announcer (v4 `lib/services/announcer/`) — the operator's in-chat
//! announcement paths:
//!
//!   - [`audience`] — `resolveAnnouncementAudience`, the whisper audience the
//!     operator names in the dialog's "Who hears it" section, re-verified
//!     server-side against the chat's CURRENT participants.
//!   - [`writer`] — `postAdhocAnnouncement`, the Insert Announcement composer
//!     button's persisted bubble (Staff / off-scene character / free custom
//!     name), public by default and whispered when an audience resolved.
//!   - [`voice_rewrite_core`] — what the two "say it in the character's own
//!     voice" rehearsals share: the Commonplace recall against the draft, the
//!     cheap-LLM call, and the never-throws result shape (v4 `686954937`).
//!   - [`in_scene_voiced`] — `generateInSceneVoicedLine`, the IN-SCENE rewrite
//!     for a seat the operator is impersonating ("In Their Own Words"). The
//!     character is in the room and it is their turn. Persists nothing.
//!   - [`character_voiced`] — `generateCharacterVoicedAnnouncement`, the
//!     OFF-SCENE rewrite the Insert Announcement dialog offers before the
//!     operator posts. Persists nothing.
//!
//! Both sit behind the P4.9E2A dispatch verbs in [`crate::api::chat_post_office`].

pub mod audience;
pub mod character_voiced;
pub mod in_scene_voiced;
pub mod voice_rewrite_core;
pub mod writer;
