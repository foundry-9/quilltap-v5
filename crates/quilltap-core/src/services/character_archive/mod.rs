//! The character-archive services (v4 `lib/characters/archive-*.ts`, v4
//! `d553f72a`).
//!
//! Completed by round 2 (P4.D65) — the whole lifecycle is here:
//!   - [`crypto`] — the bundle format: PBKDF2-SHA256 → AES-256-GCM under the
//!     instance **passphrase** (never the pepper, which does not travel with a
//!     logical backup), and the four typed errors whose sentences the settings
//!     UI shows verbatim.
//!   - [`reencrypt`] — the passphrase-change sweep that rewrites every ARCHIVE
//!     bundle from the old passphrase to the new one, never aborting.
//!   - [`service`] — `archiveCharacter` / `rehydrateCharacter`: the bundle, the
//!     verification gate, the tombstone commit, the in-place vault prune and
//!     the preserve-ids restore.

pub mod crypto;
pub mod reencrypt;
pub mod service;
