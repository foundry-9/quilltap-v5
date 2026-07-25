//! The Announcer (v4 `lib/services/announcer/`) — the operator's in-chat
//! announcement paths:
//!
//!   - [`writer`] — `postAdhocAnnouncement`, the Insert Announcement composer
//!     button's persisted broadcast bubble (Staff / off-scene character / free
//!     custom name).
//!
//! Sits behind the P4.9E2A dispatch verbs in [`crate::api::chat_post_office`].

pub mod writer;
