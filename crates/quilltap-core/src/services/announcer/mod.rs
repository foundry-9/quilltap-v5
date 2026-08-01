//! The Announcer (v4 `lib/services/announcer/`) — the operator's in-chat
//! announcement paths:
//!
//!   - [`audience`] — `resolveAnnouncementAudience`, the whisper audience the
//!     operator names in the dialog's "Who hears it" section, re-verified
//!     server-side against the chat's CURRENT participants.
//!   - [`writer`] — `postAdhocAnnouncement`, the Insert Announcement composer
//!     button's persisted bubble (Staff / off-scene character / free custom
//!     name), public by default and whispered when an audience resolved.
//!   - [`character_voiced`] — `generateCharacterVoicedAnnouncement`, the
//!     in-character rewrite the dialog offers before the operator posts.
//!     Persists nothing.
//!
//! Both sit behind the P4.9E2A dispatch verbs in [`crate::api::chat_post_office`].

pub mod audience;
pub mod character_voiced;
pub mod writer;
