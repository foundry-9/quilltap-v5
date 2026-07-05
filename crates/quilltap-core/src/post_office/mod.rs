//! The Post Office (v4 `lib/post-office/`) — the mailbox storage layer, the shared
//! letter-delivery service, and the agent-facing instruction snippets that the
//! `send_mail` / `list_email` tool handlers sit on. Everything is a content
//! read/write or folder ensure over a character vault's `Mail/` folder (a
//! mount-index document store); no host filesystem, so the whole surface is
//! portable.

pub mod deliver;
pub mod instructions;
pub mod mailbox;
