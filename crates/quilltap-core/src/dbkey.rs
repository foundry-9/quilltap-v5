//! Read and decrypt the `quilltap.dbkey` file to recover the base64 master
//! pepper that keys every SQLCipher database.
//!
//! This is a faithful port of `lib/startup/dbkey.ts` + `lib/startup/
//! pepper-crypto.ts` from the current app. The `.dbkey` file is
//! self-describing — it carries the algorithm, KDF iterations, and digest —
//! so we read those from the file rather than hardcoding them, exactly as
//! `loadDbKey` does. The only constants we must agree on out-of-band are:
//!
//!   * the internal passphrase used when the user set none, and
//!   * the AES key length (32 bytes — AES-256) and the fact that the cipher
//!     is AES-256-GCM with a 16-byte (128-bit) GCM tag.
//!
//! Decryption procedure (must match pepper-crypto.ts `decryptPepperWithParams`):
//!   1. key  = PBKDF2(passphrase, salt, kdfIterations, 32 bytes, kdfDigest)
//!   2. plaintext = AES-256-GCM-decrypt(key, iv, ciphertext, authTag)
//!   3. verify SHA-256(plaintext) == pepperHash
//!
//! Try the internal passphrase first; only if that fails do we need a
//! user-supplied one — mirroring `loadDbKey`'s control flow.

use std::path::{Path, PathBuf};

use aes::Aes256;
use aes_gcm::aead::consts::U16;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{AesGcm, Key, Nonce};
use hmac::Hmac;

/// AES-256-GCM with a **16-byte** nonce. The current app's pepper-crypto.ts
/// generates a 16-byte IV (`ivLength: 16`), but the `aes-gcm` crate's default
/// `Aes256Gcm` alias hardcodes a 12-byte nonce. We must match the IV length
/// the file was written with, so parameterize the nonce size explicitly.
type Aes256Gcm16 = AesGcm<Aes256, U16>;
use sha2::{Digest, Sha256, Sha512};

/// Internal passphrase used when the user skipped setting one.
/// Must match `INTERNAL_PASSPHRASE` in dbkey.ts / pepper-vault.ts.
const INTERNAL_PASSPHRASE: &str = "__quilltap_no_passphrase__";

/// AES-256 key length in bytes.
const KEY_LEN: usize = 32;

/// On-disk shape of `quilltap.dbkey`. Field names match the JSON written by
/// dbkey.ts exactly (camelCase). Unknown/extra fields (e.g. `version`,
/// `keyLength`) are ignored.
#[derive(serde::Deserialize)]
struct DbKeyFile {
    algorithm: String, // expected: "aes-256-gcm"
    #[serde(rename = "kdfIterations")]
    kdf_iterations: u32,
    #[serde(rename = "kdfDigest")]
    kdf_digest: String, // "sha256" | "sha512"
    salt: String,       // hex
    iv: String,         // hex (GCM nonce)
    ciphertext: String, // hex
    #[serde(rename = "authTag")]
    auth_tag: String, // hex (16-byte GCM tag)
    #[serde(rename = "pepperHash")]
    pepper_hash: String, // hex SHA-256 of the plaintext pepper
}

#[derive(Debug)]
pub enum DbKeyError {
    NotFound(PathBuf),
    Io(std::io::Error),
    Parse(String),
    /// Internal passphrase failed and no (correct) user passphrase was given.
    PassphraseRequired,
    /// A passphrase was supplied but decryption/verification still failed.
    DecryptFailed,
    /// File used an algorithm or digest we don't implement.
    Unsupported(String),
}

impl std::fmt::Display for DbKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbKeyError::NotFound(p) => write!(f, "no quilltap.dbkey at {}", p.display()),
            DbKeyError::Io(e) => write!(f, "io error reading .dbkey: {e}"),
            DbKeyError::Parse(e) => write!(f, "malformed .dbkey: {e}"),
            DbKeyError::PassphraseRequired => {
                write!(
                    f,
                    "this .dbkey is protected by a user passphrase; supply it"
                )
            }
            DbKeyError::DecryptFailed => write!(
                f,
                "decryption/verification failed (wrong passphrase or tampered file)"
            ),
            DbKeyError::Unsupported(s) => write!(f, "unsupported crypto in .dbkey: {s}"),
        }
    }
}
impl std::error::Error for DbKeyError {}

/// Load and decrypt the pepper from `<data_dir>/quilltap.dbkey`.
///
/// `data_dir` is the instance's `data/` directory (the one that directly
/// contains `quilltap.dbkey`) — same contract as `loadDbKey`'s first argument.
/// Pass `None` for `passphrase` when no user passphrase was set; the internal
/// passphrase is always tried first regardless.
///
/// Returns the **base64** pepper string (identical to what the TS `loadDbKey`
/// returns) — feed it through `pepper_b64_to_key_hex` to get the SQLCipher key.
pub fn load_pepper(data_dir: &Path, passphrase: Option<&str>) -> Result<String, DbKeyError> {
    let path = data_dir.join("quilltap.dbkey");
    if !path.exists() {
        return Err(DbKeyError::NotFound(path));
    }
    let raw = std::fs::read_to_string(&path).map_err(DbKeyError::Io)?;
    let file: DbKeyFile =
        serde_json::from_str(&raw).map_err(|e| DbKeyError::Parse(e.to_string()))?;

    if !file.algorithm.eq_ignore_ascii_case("aes-256-gcm") {
        return Err(DbKeyError::Unsupported(file.algorithm.clone()));
    }

    // 1) Internal passphrase first (the no-user-passphrase case).
    if let Some(p) = try_decrypt(&file, INTERNAL_PASSPHRASE)? {
        return Ok(p);
    }
    // 2) Fall back to the user passphrase, if one was provided.
    match passphrase {
        Some(pass) => match try_decrypt(&file, pass)? {
            Some(p) => Ok(p),
            None => Err(DbKeyError::DecryptFailed),
        },
        None => Err(DbKeyError::PassphraseRequired),
    }
}

/// Attempt one decrypt with a specific passphrase. Returns:
///   Ok(Some(pepper)) on success (and pepperHash verified),
///   Ok(None)         on a clean auth/verification failure (try another pass),
///   Err(..)          on an unsupported digest / malformed hex.
fn try_decrypt(file: &DbKeyFile, passphrase: &str) -> Result<Option<String>, DbKeyError> {
    let salt = hex::decode(&file.salt).map_err(|e| DbKeyError::Parse(format!("salt: {e}")))?;
    let iv = hex::decode(&file.iv).map_err(|e| DbKeyError::Parse(format!("iv: {e}")))?;
    let ct =
        hex::decode(&file.ciphertext).map_err(|e| DbKeyError::Parse(format!("ciphertext: {e}")))?;
    let tag =
        hex::decode(&file.auth_tag).map_err(|e| DbKeyError::Parse(format!("authTag: {e}")))?;

    // PBKDF2 -> 32-byte key, digest selected by the file.
    let mut key = [0u8; KEY_LEN];
    match file.kdf_digest.to_ascii_lowercase().as_str() {
        "sha256" => pbkdf2::pbkdf2::<Hmac<Sha256>>(
            passphrase.as_bytes(),
            &salt,
            file.kdf_iterations,
            &mut key,
        )
        .map_err(|_| DbKeyError::DecryptFailed)?,
        "sha512" => pbkdf2::pbkdf2::<Hmac<Sha512>>(
            passphrase.as_bytes(),
            &salt,
            file.kdf_iterations,
            &mut key,
        )
        .map_err(|_| DbKeyError::DecryptFailed)?,
        other => return Err(DbKeyError::Unsupported(format!("kdfDigest {other}"))),
    }

    // AES-256-GCM expects ciphertext||tag concatenated for the `aes-gcm` crate.
    let mut ct_and_tag = ct;
    ct_and_tag.extend_from_slice(&tag);

    let cipher = Aes256Gcm16::new(Key::<Aes256Gcm16>::from_slice(&key));
    let nonce = Nonce::<U16>::from_slice(&iv); // 16-byte IV, matching pepper-crypto.ts
    let plaintext = match cipher.decrypt(
        nonce,
        Payload {
            msg: &ct_and_tag,
            aad: &[],
        },
    ) {
        Ok(pt) => pt,
        Err(_) => return Ok(None), // auth failure → wrong passphrase; caller tries next
    };
    let pepper = String::from_utf8(plaintext).map_err(|_| DbKeyError::DecryptFailed)?;

    // Verify against pepperHash (SHA-256 of the plaintext pepper).
    let got = hex::encode(Sha256::digest(pepper.as_bytes()));
    if got != file.pepper_hash {
        return Ok(None);
    }
    Ok(Some(pepper))
}

/// Convert the base64 pepper into the lowercase hex string used in the raw-key
/// pragma: `PRAGMA key = "x'<hex>'"`. Mirrors
/// `Buffer.from(pepper, 'base64').toString('hex')` in meta.ts / db-helpers.js.
pub fn pepper_b64_to_key_hex(pepper_b64: &str) -> Result<String, DbKeyError> {
    let bytes = base64_decode(pepper_b64).ok_or(DbKeyError::DecryptFailed)?;
    Ok(hex::encode(bytes))
}

/// Minimal standard-alphabet base64 decoder (no extra dep). Replace with a
/// vetted crate when the core grows one.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut nbits = 0u8;
    let mut out = Vec::new();
    for &c in s.trim().as_bytes() {
        if c == b'=' {
            break;
        }
        let idx = A.iter().position(|&x| x == c)? as u32;
        bits = (bits << 6) | idx;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    Some(out)
}
