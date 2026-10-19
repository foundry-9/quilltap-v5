//! Chat-row helpers that mirror v4's `lib/chat/` modules one for one (P4.D317).
//!
//! v4 keeps a small family of pure chat helpers under `lib/chat/`; v5's older
//! ports of that family live as flat `chat_*` modules beside this one. New
//! ports of `lib/chat/<name>.ts` land here as `chat::<name>`.

pub mod character_keyed_fields;
