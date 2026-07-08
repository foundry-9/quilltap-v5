//! Pepper provisioning — the control-flow port of v4 `provisionDbKey`
//! (`lib/startup/dbkey.ts`), the resolution that runs before anything else
//! at startup and decides whether the engine can boot.
//!
//! Resolution order (v4's, verbatim):
//!
//! 1. env pepper set AND `.dbkey` exists: SHA-256 hash match → `resolved`;
//!    mismatch → a hard typed error (v4 `process.exit(1)` — the pepper was
//!    changed externally and proceeding would strand all encrypted data).
//! 2. env pepper set, no `.dbkey` → `needs-vault-storage` (**operational**;
//!    file storage recommended).
//! 3. no env pepper, `.dbkey` exists: the internal passphrase decrypts →
//!    `resolved`; else `needs-passphrase` (a user passphrase was set).
//! 4. neither → `needs-setup` (first run). An unreadable/malformed `.dbkey`
//!    also lands here, as v4's `readDbKeyFile` null-on-error does.
//!
//! **Differential status:** this is the minimal boot core of the P4.4
//! "unlock / pepper-vault service" backfill unit, which ports the full
//! service (setup/store/change-passphrase, the 3-attempt limit, auto-lock
//! resume) with its differential and subsumes these internals. The crypto
//! this composes (`dbkey::load_pepper`) is already Friday-verified.

use std::path::Path;

use crate::dbkey::{self, DbKeyError};

use super::types::PepperState;

/// The provisioning outcome: the state, the pepper when operational, and
/// whether a user passphrase protects the vault (v4
/// `__quilltapHasUserPassphrase`).
#[derive(Debug, Clone, PartialEq)]
pub struct Provisioned {
    pub state: PepperState,
    /// The base64 pepper — present exactly when `state.is_operational()`.
    pub pepper: Option<String>,
    pub has_user_passphrase: bool,
}

/// The one hard provisioning failure (v4's FATAL exit): the env pepper does
/// not match the `.dbkey` file's stored hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PepperHashMismatch;

impl std::fmt::Display for PepperHashMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // v4's FATAL log message, condensed: proceeding would make all
        // encrypted data unreadable.
        write!(
            f,
            "ENCRYPTION_MASTER_PEPPER does not match the stored pepper hash in the .dbkey file. \
             Restore the original pepper or delete the .dbkey file to re-initialize."
        )
    }
}
impl std::error::Error for PepperHashMismatch {}

/// v4 `provisionDbKey`, minus the process-global mutation: pure inputs
/// (`data_dir`, the env pepper if any) to a [`Provisioned`] outcome. An empty
/// env pepper counts as unset (v4 `process.env.X || ''` falsiness).
pub fn provision(
    data_dir: &Path,
    env_pepper: Option<&str>,
) -> Result<Provisioned, PepperHashMismatch> {
    let env_pepper = env_pepper.filter(|p| !p.is_empty());
    let stored_hash = dbkey::read_pepper_hash(data_dir);

    if let Some(pepper) = env_pepper {
        return match stored_hash {
            // Case 1: env var + .dbkey — verify the hash, fatal on mismatch.
            Some(hash) => {
                if dbkey::hash_pepper(pepper) == hash {
                    Ok(Provisioned {
                        state: PepperState::Resolved,
                        pepper: Some(pepper.to_string()),
                        has_user_passphrase: false,
                    })
                } else {
                    Err(PepperHashMismatch)
                }
            }
            // Case 2: env var only — operational, storage recommended.
            None => Ok(Provisioned {
                state: PepperState::NeedsVaultStorage,
                pepper: Some(pepper.to_string()),
                has_user_passphrase: false,
            }),
        };
    }

    // Case 3: .dbkey only — internal passphrase first (v4's silent resolve).
    if stored_hash.is_some() {
        return match dbkey::load_pepper(data_dir, None) {
            Ok(pepper) => Ok(Provisioned {
                state: PepperState::Resolved,
                pepper: Some(pepper),
                has_user_passphrase: false,
            }),
            Err(DbKeyError::PassphraseRequired) => Ok(Provisioned {
                state: PepperState::NeedsPassphrase,
                pepper: None,
                has_user_passphrase: true,
            }),
            // Unreadable/malformed mid-read → needs-setup, as v4's outer
            // catch does (read_pepper_hash already filtered most of these).
            Err(_) => Ok(Provisioned {
                state: PepperState::NeedsSetup,
                pepper: None,
                has_user_passphrase: false,
            }),
        };
    }

    // Case 4: neither — first run.
    Ok(Provisioned {
        state: PepperState::NeedsSetup,
        pepper: None,
        has_user_passphrase: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn env_pepper_without_file_is_needs_vault_storage() {
        let dir = tempdir().unwrap();
        let p = provision(dir.path(), Some("cGVwcGVy")).unwrap();
        assert_eq!(p.state, PepperState::NeedsVaultStorage);
        assert_eq!(p.pepper.as_deref(), Some("cGVwcGVy"));
        assert!(!p.has_user_passphrase);
        assert!(p.state.is_operational());
    }

    #[test]
    fn empty_env_pepper_counts_as_unset() {
        let dir = tempdir().unwrap();
        let p = provision(dir.path(), Some("")).unwrap();
        assert_eq!(p.state, PepperState::NeedsSetup);
    }

    #[test]
    fn env_pepper_matching_file_hash_resolves() {
        let dir = tempdir().unwrap();
        let pepper = dbkey::generate_pepper().unwrap();
        dbkey::save_dbkey(dir.path(), &pepper, "secret").unwrap();
        let p = provision(dir.path(), Some(&pepper)).unwrap();
        assert_eq!(p.state, PepperState::Resolved);
        assert_eq!(p.pepper.as_deref(), Some(pepper.as_str()));
    }

    #[test]
    fn env_pepper_hash_mismatch_is_fatal() {
        let dir = tempdir().unwrap();
        let pepper = dbkey::generate_pepper().unwrap();
        dbkey::save_dbkey(dir.path(), &pepper, "").unwrap();
        let other = dbkey::generate_pepper().unwrap();
        assert_eq!(provision(dir.path(), Some(&other)), Err(PepperHashMismatch));
    }

    #[test]
    fn file_without_user_passphrase_resolves_silently() {
        let dir = tempdir().unwrap();
        let pepper = dbkey::generate_pepper().unwrap();
        dbkey::save_dbkey(dir.path(), &pepper, "").unwrap();
        let p = provision(dir.path(), None).unwrap();
        assert_eq!(p.state, PepperState::Resolved);
        assert_eq!(p.pepper.as_deref(), Some(pepper.as_str()));
        assert!(!p.has_user_passphrase);
    }

    #[test]
    fn file_with_user_passphrase_needs_passphrase() {
        let dir = tempdir().unwrap();
        let pepper = dbkey::generate_pepper().unwrap();
        dbkey::save_dbkey(dir.path(), &pepper, "open sesame").unwrap();
        let p = provision(dir.path(), None).unwrap();
        assert_eq!(p.state, PepperState::NeedsPassphrase);
        assert_eq!(p.pepper, None);
        assert!(p.has_user_passphrase);
        assert!(!p.state.is_operational());
    }

    #[test]
    fn nothing_at_all_is_needs_setup() {
        let dir = tempdir().unwrap();
        let p = provision(dir.path(), None).unwrap();
        assert_eq!(p.state, PepperState::NeedsSetup);
    }

    #[test]
    fn malformed_dbkey_file_is_needs_setup() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("quilltap.dbkey"), "not json").unwrap();
        let p = provision(dir.path(), None).unwrap();
        assert_eq!(p.state, PepperState::NeedsSetup);
    }
}
