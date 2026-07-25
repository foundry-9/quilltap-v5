//! The Announcer (v4 `lib/services/announcer/`) — the operator's in-chat
//! announcement paths:
//!
//!   - [`writer`] — `postAdhocAnnouncement`, the Insert Announcement composer
//!     button's persisted broadcast bubble (Staff / off-scene character / free
//!     custom name).
//!   - [`character_voiced`] — `generateCharacterVoicedAnnouncement`, the
//!     in-character rewrite the dialog offers before the operator posts.
//!     Persists nothing.
//!
//! Both sit behind the P4.9E2A dispatch verbs in [`crate::api::chat_post_office`].

pub mod character_voiced;
pub mod writer;
