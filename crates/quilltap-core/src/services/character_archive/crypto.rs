//! Archive bundle encryption (§4.2c of the character-archive spec) — the port
//! of v4 `lib/characters/archive-crypto.ts` (v4 `d553f72a`).
//!
//! `files/` sits outside every encrypted database, so an archive bundle
//! written there in the clear would be the one place a character's mail,
//! photographs and personality live unprotected. Bundles are therefore
//! encrypted with **the same mechanism `.dbkey` uses** — PBKDF2-SHA256 at 600k
//! iterations deriving an AES-256-GCM key from the passphrase — and **never**
//! with the master pepper: backups are logical and the pepper does not travel
//! with them, so a pepper-encrypted bundle would restore onto a new instance
//! byte-perfect and permanently undecryptable. The passphrase (or the
//! [`INTERNAL_PASSPHRASE`] constant on a no-passphrase instance) is knowledge
//! that survives the instance, which is the whole point of the artifact.
//!
//! This is deliberately **parity with the database, not more**: on a
//! no-passphrase instance the bundle is protected against casual filesystem
//! access — a sync client indexing `files/`, a stray copy on a shared disk —
//! and not against someone holding the disk and a copy of Quilltap.
//!
//! On-disk layout (version 1):
//!
//! ```text
//!     bytes 0..7    magic 'QTAPARC1'
//!     bytes 8..11   header length N (uint32 BE)
//!     bytes 12..12+N-1  UTF-8 JSON header
//!     ...           ciphertext
//!     last 16 bytes GCM auth tag
//! ```
//!
//! The header's `keyHash` (sha256 of the derived key, the `pepperHash`
//! convention from `.dbkey`) lets a wrong passphrase be diagnosed as *"this
//! archive predates your passphrase change"* instead of a bare GCM
//! authentication failure.
//!
//! ## Two places this port differs in MECHANICS, never in output
//!
//!   - v4 feeds `createCipheriv` in 8 MiB slices so a huge bundle isn't one
//!     giant native call. Rust's AEAD is one-shot over a slice, and GCM's
//!     output is chunking-independent by construction, so the bytes are
//!     identical either way. The differential asserts that on real vectors
//!     rather than taking it on trust.
//!   - v4's `crypto.createDecipheriv` throws only at `final()`; the `aes-gcm`
//!     crate verifies the tag inside one call. Same arm, same error type —
//!     the ordering against the keyHash check (which runs FIRST on both
//!     sides, before any ciphertext is touched) is what the contract cares
//!     about, and that is preserved exactly.
//!
//! The 16-byte GCM nonce is v4's `IV_LENGTH`, so this reuses
//! [`crate::dbkey`]'s `Aes256Gcm16` shape rather than the crate's 12-byte
//! default alias.

use aes::Aes256;
use aes_gcm::aead::consts::U16;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{AesGcm, Key, Nonce};
use hmac::Hmac;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dbkey::INTERNAL_PASSPHRASE;

/// AES-256-GCM with v4's 16-byte IV (see [`crate::dbkey`]).
type Aes256Gcm16 = AesGcm<Aes256, U16>;

// Parameters mirror `.dbkey` (v4 `lib/startup/dbkey.ts`) exactly.
const ALGORITHM: &str = "aes-256-gcm";
const KEY_LENGTH: usize = 32;
const IV_LENGTH: usize = 16;
const SALT_LENGTH: usize = 32;
const PBKDF2_ITERATIONS: u32 = 600_000;
const PBKDF2_DIGEST: &str = "sha256";
const AUTH_TAG_LENGTH: usize = 16;

const MAGIC: &[u8; 8] = b"QTAPARC1";
const HEADER_LENGTH_BYTES: usize = 4;

/// The bundle header. **Field order is the wire order** — `serde_json`
/// serializes a struct in declaration order and v4 builds the object literal
/// in exactly this sequence, so the header bytes match without a canonicalizer
/// (the standing `json-column-key-order` discipline).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ArchiveCryptoHeader {
    pub version: u32,
    pub algorithm: String,
    pub kdf: String,
    #[serde(rename = "kdfIterations")]
    pub kdf_iterations: u32,
    #[serde(rename = "kdfDigest")]
    pub kdf_digest: String,
    /// PBKDF2 salt, hex. Fresh per bundle.
    pub salt: String,
    /// GCM IV, hex. Fresh per bundle.
    pub iv: String,
    /// sha256 of the derived key, hex — the passphrase-verification hash.
    #[serde(rename = "keyHash")]
    pub key_hash: String,
}

/// v4's four archive-crypto error classes, each carrying v4's byte-exact
/// sentence through [`std::fmt::Display`] (the Shared contract makes these
/// sentences contractual — they reach the settings UI verbatim).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveCryptoError {
    /// v4 `ArchivePassphraseMismatchError` — the keyHash says this bundle was
    /// written under a different passphrase. Raised BEFORE the ciphertext is
    /// touched.
    PassphraseMismatch { detail: String },
    /// v4 `ArchiveIntegrityError` — the keyHash matched and GCM still failed,
    /// so this is corruption or tampering, not a wrong passphrase.
    Integrity { detail: String },
    /// v4 `ArchiveFormatError` — these bytes are not an encrypted archive in a
    /// format this build knows.
    Format { detail: String },
    /// v4 `ArchiveKeyUnavailableError` — a user passphrase protects this
    /// instance and none has passed through this process yet.
    KeyUnavailable,
}

impl std::fmt::Display for ArchiveCryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // v4 appends ` ${detail}` only when the detail is non-empty.
            ArchiveCryptoError::PassphraseMismatch { detail } => {
                write!(
                    f,
                    "This archive predates your passphrase change: it was encrypted under a \
                     different passphrase than the current one. Unlock it with the passphrase \
                     that was in effect when it was written."
                )?;
                if !detail.is_empty() {
                    write!(f, " {detail}")?;
                }
                Ok(())
            }
            ArchiveCryptoError::Integrity { detail } => {
                write!(f, "Archive bundle failed integrity verification: {detail}")
            }
            ArchiveCryptoError::Format { detail } => {
                write!(f, "Not a readable encrypted archive: {detail}")
            }
            ArchiveCryptoError::KeyUnavailable => write!(
                f,
                "Archive encryption needs the instance passphrase, which this process has not \
                 seen. Lock and unlock the instance (or restart and enter the passphrase) and \
                 try again."
            ),
        }
    }
}

impl std::error::Error for ArchiveCryptoError {}

/// What [`resolve_archive_passphrase`] needs to know about the instance —
/// v4's two module-level globals (`getRuntimePassphrase()` and
/// `getHasUserPassphrase()`) as an explicit argument pair.
///
/// v4 keeps the runtime passphrase on `global.__quilltapRuntimePassphrase` to
/// survive Next.js HMR. v5's process-boundary analog is the engine's own
/// state, so the cache lives there (`api::engine`) and reaches this module as
/// a parameter — no process global, and the archive crypto stays a pure
/// function of its inputs.
#[derive(Debug, Clone, Copy)]
pub struct PassphraseSource<'a> {
    /// The effective passphrase this process last proved, if any.
    pub cached: Option<&'a str>,
    /// Whether a USER passphrase protects this instance.
    pub has_user_passphrase: bool,
}

/// The passphrase archive crypto should use when the caller doesn't supply
/// one: the passphrase this process last proved (deposited by the `.dbkey`
/// flow), or the internal sentinel on a no-passphrase instance — v4
/// `resolveArchivePassphrase`.
pub fn resolve_archive_passphrase(
    source: PassphraseSource<'_>,
) -> Result<String, ArchiveCryptoError> {
    if let Some(cached) = source.cached {
        return Ok(cached.to_string());
    }
    if !source.has_user_passphrase {
        return Ok(INTERNAL_PASSPHRASE.to_string());
    }
    Err(ArchiveCryptoError::KeyUnavailable)
}

/// True when the buffer carries the encrypted-archive magic — v4
/// `isEncryptedArchive`.
pub fn is_encrypted_archive(data: &[u8]) -> bool {
    data.len() >= MAGIC.len() && &data[..MAGIC.len()] == MAGIC
}

fn derive_key(
    passphrase: &str,
    salt: &[u8],
    iterations: u32,
    digest: &str,
) -> Result<[u8; KEY_LENGTH], ArchiveCryptoError> {
    let mut key = [0u8; KEY_LENGTH];
    match digest {
        "sha256" => {
            pbkdf2::pbkdf2::<Hmac<Sha256>>(passphrase.as_bytes(), salt, iterations, &mut key)
                .map_err(|_| ArchiveCryptoError::Format {
                    detail: "PBKDF2 refused the requested key length".to_string(),
                })?;
        }
        "sha512" => {
            pbkdf2::pbkdf2::<Hmac<sha2::Sha512>>(passphrase.as_bytes(), salt, iterations, &mut key)
                .map_err(|_| ArchiveCryptoError::Format {
                    detail: "PBKDF2 refused the requested key length".to_string(),
                })?;
        }
        // The header names its own digest (the `.dbkey` convention), so an
        // unknown one is a format problem, not a passphrase problem.
        other => {
            return Err(ArchiveCryptoError::Format {
                detail: format!("unsupported kdfDigest {other}"),
            })
        }
    }
    Ok(key)
}

fn hash_key(key: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(key);
    hex::encode(h.finalize())
}

/// Encrypt an archive bundle — v4 `encryptArchive`. Fresh salt and IV per
/// call.
///
/// `salt_iv` is the randomness seam: production passes `None` (a fresh
/// CSPRNG draw, v4's `crypto.randomBytes`); the differential passes the
/// oracle's mocked salt+IV so both sides produce byte-identical bundles.
pub fn encrypt_archive(
    plaintext: &[u8],
    passphrase: &str,
    salt_iv: Option<(&[u8], &[u8])>,
) -> Result<Vec<u8>, ArchiveCryptoError> {
    let mut fresh_salt = [0u8; SALT_LENGTH];
    let mut fresh_iv = [0u8; IV_LENGTH];
    let (salt, iv) = match salt_iv {
        Some((s, i)) => (s.to_vec(), i.to_vec()),
        None => {
            getrandom::getrandom(&mut fresh_salt).map_err(|e| ArchiveCryptoError::Format {
                detail: format!("csprng unavailable: {e}"),
            })?;
            getrandom::getrandom(&mut fresh_iv).map_err(|e| ArchiveCryptoError::Format {
                detail: format!("csprng unavailable: {e}"),
            })?;
            (fresh_salt.to_vec(), fresh_iv.to_vec())
        }
    };

    let key = derive_key(passphrase, &salt, PBKDF2_ITERATIONS, PBKDF2_DIGEST)?;

    let header = ArchiveCryptoHeader {
        version: 1,
        algorithm: ALGORITHM.to_string(),
        kdf: "pbkdf2".to_string(),
        kdf_iterations: PBKDF2_ITERATIONS,
        kdf_digest: PBKDF2_DIGEST.to_string(),
        salt: hex::encode(&salt),
        iv: hex::encode(&iv),
        key_hash: hash_key(&key),
    };
    let header_json = serde_json::to_vec(&header).map_err(|e| ArchiveCryptoError::Format {
        detail: format!("header serialize: {e}"),
    })?;

    let cipher = Aes256Gcm16::new(Key::<Aes256Gcm16>::from_slice(&key));
    let nonce = Nonce::<U16>::from_slice(&iv);
    // The crate returns ciphertext||tag concatenated, which is exactly the
    // trailing layout v4 assembles by hand from `update()`+`getAuthTag()`.
    let ct_and_tag = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: &[],
            },
        )
        .map_err(|_| ArchiveCryptoError::Integrity {
            detail: "encryption failed".to_string(),
        })?;

    let mut out = Vec::with_capacity(
        MAGIC.len() + HEADER_LENGTH_BYTES + header_json.len() + ct_and_tag.len(),
    );
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(header_json.len() as u32).to_be_bytes());
    out.extend_from_slice(&header_json);
    out.extend_from_slice(&ct_and_tag);
    Ok(out)
}

/// Parse and validate the header — v4 `parseHeader`. Returns the header and
/// the ciphertext's start offset.
pub fn parse_header(data: &[u8]) -> Result<(ArchiveCryptoHeader, usize), ArchiveCryptoError> {
    if !is_encrypted_archive(data) {
        return Err(ArchiveCryptoError::Format {
            detail: "missing the QTAPARC1 magic — this is not an encrypted archive".to_string(),
        });
    }
    let length_start = MAGIC.len();
    let json_start = length_start + HEADER_LENGTH_BYTES;
    if data.len() < json_start {
        return Err(ArchiveCryptoError::Format {
            detail: "truncated before the header length".to_string(),
        });
    }
    let header_length = u32::from_be_bytes([
        data[length_start],
        data[length_start + 1],
        data[length_start + 2],
        data[length_start + 3],
    ]) as usize;
    // v4 computes `bodyStart` before the bounds check, so an absurd length
    // simply fails the same check; saturating keeps that shape without an
    // overflow panic.
    let body_start = json_start.saturating_add(header_length);
    if header_length == 0 || data.len() < body_start.saturating_add(AUTH_TAG_LENGTH) {
        return Err(ArchiveCryptoError::Format {
            detail: "truncated header or missing auth tag".to_string(),
        });
    }

    let header: ArchiveCryptoHeader = match serde_json::from_slice(&data[json_start..body_start]) {
        Ok(h) => h,
        // v4 separates "not JSON" (its try/catch) from "JSON of the wrong
        // shape" (the field checks below). serde collapses both into one
        // failure, so re-run the parse as untyped to tell them apart and keep
        // both sentences reachable.
        Err(_) => {
            let untyped: Result<serde_json::Value, _> =
                serde_json::from_slice(&data[json_start..body_start]);
            return Err(match untyped {
                Err(_) => ArchiveCryptoError::Format {
                    detail: "header is not valid JSON".to_string(),
                },
                Ok(v) => ArchiveCryptoError::Format {
                    detail: format!(
                        "unsupported header (version {})",
                        // v4's `String(header.version)` over an absent key is
                        // the literal "undefined".
                        match v.get("version") {
                            None | Some(serde_json::Value::Null) => "undefined".to_string(),
                            Some(other) => match other {
                                serde_json::Value::String(s) => s.clone(),
                                _ => other.to_string(),
                            },
                        }
                    ),
                },
            });
        }
    };
    if header.version != 1 || header.kdf != "pbkdf2" {
        return Err(ArchiveCryptoError::Format {
            detail: format!("unsupported header (version {})", header.version),
        });
    }
    Ok((header, body_start))
}

/// Decrypt an archive bundle — v4 `decryptArchive`.
///
/// The KDF parameters come from the header itself (the `.dbkey` convention),
/// so older bundles keep decrypting after future parameter upgrades. A wrong
/// passphrase is diagnosed by the key-verification hash *before* touching the
/// ciphertext and yields [`ArchiveCryptoError::PassphraseMismatch`]; a hash
/// match followed by GCM failure is corruption,
/// [`ArchiveCryptoError::Integrity`].
pub fn decrypt_archive(data: &[u8], passphrase: &str) -> Result<Vec<u8>, ArchiveCryptoError> {
    let (header, body_start) = parse_header(data)?;

    let salt = hex::decode(&header.salt).map_err(|_| ArchiveCryptoError::Format {
        detail: "header salt is not hex".to_string(),
    })?;
    let iv = hex::decode(&header.iv).map_err(|_| ArchiveCryptoError::Format {
        detail: "header iv is not hex".to_string(),
    })?;
    let key = derive_key(passphrase, &salt, header.kdf_iterations, &header.kdf_digest)?;

    if hash_key(&key) != header.key_hash {
        return Err(ArchiveCryptoError::PassphraseMismatch {
            detail: String::new(),
        });
    }

    // v4 slices the tag off the end and hands the middle to the decipher; the
    // crate wants them concatenated, which is the shape the file already has.
    let ct_and_tag = &data[body_start..];
    if header.algorithm != ALGORITHM {
        return Err(ArchiveCryptoError::Format {
            detail: format!("unsupported algorithm {}", header.algorithm),
        });
    }
    if iv.len() != IV_LENGTH {
        return Err(ArchiveCryptoError::Format {
            detail: format!("unsupported iv length {}", iv.len()),
        });
    }

    let cipher = Aes256Gcm16::new(Key::<Aes256Gcm16>::from_slice(&key));
    let nonce = Nonce::<U16>::from_slice(&iv);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ct_and_tag,
                aad: &[],
            },
        )
        .map_err(|_| ArchiveCryptoError::Integrity {
            // v4's catch is `error instanceof Error ? error.message :
            // 'authentication failed'`, and the OBSERVED v4 message on a
            // corrupt tag is the fallback — recorded by the differential, not
            // guessed (an earlier draft of this port assumed Node's
            // "Unsupported state or unable to authenticate data" and the
            // corpus refuted it on its first run).
            detail: "authentication failed".to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SALT: [u8; SALT_LENGTH] = [7u8; SALT_LENGTH];
    const IV: [u8; IV_LENGTH] = [9u8; IV_LENGTH];

    fn seal(plain: &[u8], pass: &str) -> Vec<u8> {
        encrypt_archive(plain, pass, Some((&SALT, &IV))).expect("encrypt")
    }

    #[test]
    fn round_trips() {
        let bundle = seal(b"{\"kind\":\"character\"}\n", "correct horse");
        assert!(is_encrypted_archive(&bundle));
        assert_eq!(
            decrypt_archive(&bundle, "correct horse").expect("decrypt"),
            b"{\"kind\":\"character\"}\n"
        );
    }

    #[test]
    fn a_fresh_call_draws_a_new_salt_and_iv() {
        let a = encrypt_archive(b"x", "p", None).expect("a");
        let b = encrypt_archive(b"x", "p", None).expect("b");
        assert_ne!(a, b, "each bundle must carry its own salt+IV");
    }

    #[test]
    fn wrong_passphrase_is_diagnosed_before_the_ciphertext() {
        let bundle = seal(b"secret", "the old one");
        let err = decrypt_archive(&bundle, "the new one").expect_err("must refuse");
        assert!(matches!(err, ArchiveCryptoError::PassphraseMismatch { .. }));
        assert!(err
            .to_string()
            .starts_with("This archive predates your passphrase change:"));
    }

    #[test]
    fn a_corrupt_tag_is_integrity_not_passphrase() {
        let mut bundle = seal(b"secret", "pass");
        let last = bundle.len() - 1;
        bundle[last] ^= 0xff;
        let err = decrypt_archive(&bundle, "pass").expect_err("must refuse");
        assert!(matches!(err, ArchiveCryptoError::Integrity { .. }), "{err}");
    }

    #[test]
    fn bad_magic_and_truncation_are_format_errors() {
        let err = decrypt_archive(b"not an archive at all", "p").expect_err("magic");
        assert_eq!(
            err.to_string(),
            "Not a readable encrypted archive: missing the QTAPARC1 magic — this is not an encrypted archive"
        );

        let err = decrypt_archive(b"QTAPARC1", "p").expect_err("length");
        assert_eq!(
            err.to_string(),
            "Not a readable encrypted archive: truncated before the header length"
        );

        let mut truncated = seal(b"secret", "pass");
        truncated.truncate(MAGIC.len() + HEADER_LENGTH_BYTES + 4);
        let err = decrypt_archive(&truncated, "pass").expect_err("header");
        assert_eq!(
            err.to_string(),
            "Not a readable encrypted archive: truncated header or missing auth tag"
        );
    }

    #[test]
    fn resolve_prefers_the_cache_then_the_sentinel_then_refuses() {
        assert_eq!(
            resolve_archive_passphrase(PassphraseSource {
                cached: Some("proved"),
                has_user_passphrase: true
            })
            .expect("cached"),
            "proved"
        );
        assert_eq!(
            resolve_archive_passphrase(PassphraseSource {
                cached: None,
                has_user_passphrase: false
            })
            .expect("sentinel"),
            INTERNAL_PASSPHRASE
        );
        let err = resolve_archive_passphrase(PassphraseSource {
            cached: None,
            has_user_passphrase: true,
        })
        .expect_err("must refuse");
        assert_eq!(err, ArchiveCryptoError::KeyUnavailable);
        assert!(err
            .to_string()
            .starts_with("Archive encryption needs the instance passphrase"));
    }
}
