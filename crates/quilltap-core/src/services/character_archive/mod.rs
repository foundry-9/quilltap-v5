//! The character-archive services (v4 `lib/characters/archive-*.ts`, v4
//! `d553f72a`).
//!
//! **This round (P4.D63) ports the CRYPTO half only.** The archive service
//! itself — prune, bundle, rehydrate, the participant flips — is round 2 of
//! the character-archive catch-up; the `characterArchive` /
//! `characterRehydrate` dispatch verbs exist and refuse loudly by name until
//! it lands.
//!
//! What is here:
//!   - [`crypto`] — the bundle format: PBKDF2-SHA256 → AES-256-GCM under the
//!     instance **passphrase** (never the pepper, which does not travel with a
//!     logical backup), and the four typed errors whose sentences the settings
//!     UI shows verbatim.
//!   - [`reencrypt`] — the passphrase-change sweep that rewrites every ARCHIVE
//!     bundle from the old passphrase to the new one, never aborting.

pub mod crypto;
pub mod reencrypt;
