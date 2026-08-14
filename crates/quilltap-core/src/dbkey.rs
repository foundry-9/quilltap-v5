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
pub const INTERNAL_PASSPHRASE: &str = "__quilltap_no_passphrase__";

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

// ============================================================================
// The write path (v4 dbkey.ts saveDbKey / encryptPepper / generatePepper)
// ============================================================================

/// PBKDF2 iteration count for NEWLY WRITTEN `.dbkey` files (v4
/// `PBKDF2_ITERATIONS` — "upgraded from 100k in pepper-vault to 600k"). The
/// read path takes the count from the file itself, so older files keep
/// decrypting after future upgrades.
const PBKDF2_ITERATIONS: u32 = 600_000;

/// Salt length for newly written files (v4 `SALT_LENGTH`).
const SALT_LEN: usize = 32;

/// GCM IV length (v4 `IV_LENGTH` — 16 bytes, hence the custom
/// [`Aes256Gcm16`] type instead of the crate's 12-byte default).
const IV_LEN: usize = 16;

/// SHA-256 hex of the plaintext pepper (v4 `hashPepper`) — stored as
/// `pepperHash` in the file and compared against an env-supplied pepper at
/// provisioning time.
pub fn hash_pepper(pepper: &str) -> String {
    hex::encode(Sha256::digest(pepper.as_bytes()))
}

/// Read just the stored `pepperHash` from `<data_dir>/quilltap.dbkey`,
/// tolerantly: `None` if the file is missing, unreadable, or malformed —
/// v4 `readDbKeyFile`'s null-on-error contract, which provisioning treats
/// as "no file".
pub fn read_pepper_hash(data_dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(data_dir.join("quilltap.dbkey")).ok()?;
    let file: DbKeyFile = serde_json::from_str(&raw).ok()?;
    Some(file.pepper_hash)
}

/// Generate a new pepper (v4 `generatePepper`): 32 CSPRNG bytes encoded as
/// base64 — a 44-character string.
pub fn generate_pepper() -> Result<String, DbKeyError> {
    use base64::Engine as _;
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| DbKeyError::Parse(format!("csprng unavailable: {e}")))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// The on-disk shape v4's `encryptPepper` writes, in its exact JSON field
/// order (`{version, algorithm, kdf, kdfIterations, kdfDigest, ...bundle,
/// pepperHash}` — the bundle is `salt, iv, ciphertext, authTag`).
#[derive(serde::Serialize)]
struct DbKeyFileOut<'a> {
    version: u32,
    algorithm: &'a str,
    kdf: &'a str,
    #[serde(rename = "kdfIterations")]
    kdf_iterations: u32,
    #[serde(rename = "kdfDigest")]
    kdf_digest: &'a str,
    salt: String,
    iv: String,
    ciphertext: String,
    #[serde(rename = "authTag")]
    auth_tag: String,
    #[serde(rename = "pepperHash")]
    pepper_hash: String,
}

/// Encrypt `pepper` under `passphrase` and write `<data_dir>/quilltap.dbkey`,
/// exactly as v4's `saveDbKey` does: PBKDF2-HMAC-SHA256 × 600 000 over a
/// 32-byte salt, AES-256-GCM with a 16-byte IV, 2-space-pretty JSON, file
/// mode 0o600 (POSIX), parent directory created if missing.
///
/// An **empty** `passphrase` means "no user passphrase" and encrypts under
/// the internal passphrase, mirroring v4 `setupDbKey`'s
/// `hasUserPassphrase = passphrase.length > 0` convention. Round-trip
/// verified against [`load_pepper`] (the Friday-verified reader).
pub fn save_dbkey(data_dir: &Path, pepper: &str, passphrase: &str) -> Result<(), DbKeyError> {
    let actual_pass = if passphrase.is_empty() {
        INTERNAL_PASSPHRASE
    } else {
        passphrase
    };
    let content = encrypt_dbkey_json(pepper, actual_pass)?;
    std::fs::create_dir_all(data_dir).map_err(DbKeyError::Io)?;
    write_dbkey_file(&data_dir.join("quilltap.dbkey"), &content)
}

/// Re-wrap `pepper` under `actual_passphrase` INTO an existing `.dbkey` object:
/// the ten fields v5 models are replaced in place (so they keep their
/// positions), and every field v5 does not model is preserved (P4.46).
///
/// The field that matters today is `minServerVersion` — v4's version guard
/// writes it into `quilltap.dbkey` by read-modify-write so the shell can refuse
/// a too-old binary before opening the database at all
/// (`lib/startup/version-guard.ts:235-258`). v5 has no version guard and never
/// writes it, so a passphrase change that dropped it would remove the floor
/// permanently from a v4-authored instance.
///
/// **A deliberate divergence from v4, measured** (`verify-dbkey-crosscompat.ts`
/// pins it from v4's own code): v4's `changePassphrase` hands `writeDbKeyFile`
/// a freshly built `encryptPepper` object, so it DROPS `minServerVersion` too.
/// In v4 the loss self-heals — `storeCurrentVersion()` rewrites the field on
/// the next startup — and in v5 nothing ever would. Preserving is the only
/// behavior that is correct on both sides of the fence.
fn rewrap_dbkey_json(
    existing: &str,
    pepper: &str,
    actual_passphrase: &str,
) -> Result<String, DbKeyError> {
    let fresh = encrypt_dbkey_json(pepper, actual_passphrase)?;
    let fresh: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&fresh).map_err(|e| DbKeyError::Parse(e.to_string()))?;
    let mut out: serde_json::Map<String, serde_json::Value> = match serde_json::from_str(existing) {
        // A `.dbkey` that is not a JSON object cannot have reached here
        // (it parsed as `DbKeyFile` upstream), but fall back to the fresh
        // object rather than inventing a failure mode.
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    // serde_json is built with `preserve_order`, so inserting an existing key
    // updates it in place and a new key appends — the file keeps its shape.
    for (key, value) in fresh {
        out.insert(key, value);
    }
    serde_json::to_string_pretty(&out).map_err(|e| DbKeyError::Parse(e.to_string()))
}

/// Encrypt `pepper` under `actual_passphrase` (already resolved — empty→internal
/// is the caller's decision) into the 2-space-pretty `.dbkey` JSON string in v4's
/// exact field order. Fresh random salt+IV per call.
fn encrypt_dbkey_json(pepper: &str, actual_passphrase: &str) -> Result<String, DbKeyError> {
    let mut salt = [0u8; SALT_LEN];
    getrandom::getrandom(&mut salt)
        .map_err(|e| DbKeyError::Parse(format!("csprng unavailable: {e}")))?;
    let mut iv = [0u8; IV_LEN];
    getrandom::getrandom(&mut iv)
        .map_err(|e| DbKeyError::Parse(format!("csprng unavailable: {e}")))?;

    let mut key = [0u8; KEY_LEN];
    pbkdf2::pbkdf2::<Hmac<Sha256>>(
        actual_passphrase.as_bytes(),
        &salt,
        PBKDF2_ITERATIONS,
        &mut key,
    )
    .map_err(|_| DbKeyError::DecryptFailed)?;

    // The `aes-gcm` crate returns ciphertext||tag; v4 stores them as separate
    // hex fields, so split the trailing 16-byte tag back off.
    let cipher = Aes256Gcm16::new(Key::<Aes256Gcm16>::from_slice(&key));
    let nonce = Nonce::<U16>::from_slice(&iv);
    let ct_and_tag = cipher
        .encrypt(
            nonce,
            Payload {
                msg: pepper.as_bytes(),
                aad: &[],
            },
        )
        .map_err(|_| DbKeyError::DecryptFailed)?;
    let split = ct_and_tag.len() - 16;
    let (ct, tag) = ct_and_tag.split_at(split);

    let out = DbKeyFileOut {
        version: 1,
        algorithm: "aes-256-gcm",
        kdf: "pbkdf2",
        kdf_iterations: PBKDF2_ITERATIONS,
        kdf_digest: "sha256",
        salt: hex::encode(salt),
        iv: hex::encode(iv),
        ciphertext: hex::encode(ct),
        auth_tag: hex::encode(tag),
        pepper_hash: hash_pepper(pepper),
    };
    serde_json::to_string_pretty(&out).map_err(|e| DbKeyError::Parse(e.to_string()))
}

/// Write a `.dbkey` JSON string to `path` with mode 0600 (POSIX), creating the
/// parent directory if needed.
fn write_dbkey_file(path: &Path, content: &str) -> Result<(), DbKeyError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(DbKeyError::Io)?;
    }
    std::fs::write(path, content).map_err(DbKeyError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(DbKeyError::Io)?;
    }
    Ok(())
}

/// v4 `changePassphrase` (`lib/startup/dbkey.ts`): re-wrap the pepper under a new
/// passphrase without touching the databases. Decrypts `quilltap.dbkey` with the
/// old passphrase (empty string = the internal no-passphrase sentinel, exactly as
/// v4), re-encrypts, and writes the new wrapping to `quilltap.dbkey` — the
/// instance's one and only `.dbkey` file. Returns the recovered pepper.
///
/// There is a single pepper per instance, wrapped once here. Every database —
/// main, LLM logs, and mount index — opens with it. Quilltap once wrote a second
/// copy at `quilltap-llm-logs.dbkey`, the remnant of a per-database-key design
/// that was never built; nothing ever read it, and it could hold a stale
/// wrapping, so v4 removed the write in 4.8.1 (bug 60) and v5 follows. Instances
/// that changed a passphrase before then may still have that file on disk; it is
/// inert, deliberately left alone (deleting key material on the user's behalf is
/// the wrong instinct even when it is known redundant), and safe to delete.
///
/// Fails `DecryptFailed` when the old passphrase is wrong (v4's `unauthorized`
/// "Current passphrase is incorrect"). Unlike [`load_pepper`], this does NOT try
/// the internal passphrase first — it decrypts with exactly the resolved old
/// passphrase, matching v4.
pub fn change_passphrase(
    data_dir: &Path,
    old_passphrase: &str,
    new_passphrase: &str,
) -> Result<String, DbKeyError> {
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

    let actual_old = if old_passphrase.is_empty() {
        INTERNAL_PASSPHRASE
    } else {
        old_passphrase
    };
    let pepper = match try_decrypt(&file, actual_old)? {
        Some(p) => p,
        None => return Err(DbKeyError::DecryptFailed),
    };

    let actual_new = if new_passphrase.is_empty() {
        INTERNAL_PASSPHRASE
    } else {
        new_passphrase
    };
    // Read-modify-write, not replace: fields v5 does not model (v4's
    // `minServerVersion`) survive the re-wrap. See [`rewrap_dbkey_json`].
    let content = rewrap_dbkey_json(&raw, &pepper, actual_new)?;
    write_dbkey_file(&path, &content)?;
    Ok(pepper)
}

/// Convert the base64 pepper into the lowercase hex string used in the raw-key
/// pragma: `PRAGMA key = "x'<hex>'"`. Mirrors
/// `Buffer.from(pepper, 'base64').toString('hex')` in meta.ts / db-helpers.js.
pub fn pepper_b64_to_key_hex(pepper_b64: &str) -> Result<String, DbKeyError> {
    let bytes = base64_decode(pepper_b64).ok_or(DbKeyError::DecryptFailed)?;
    Ok(hex::encode(bytes))
}

/// Minimal standard-alphabet base64 decoder (predates the `base64` dep; kept —
/// it is on the verified read path).
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// A written no-passphrase `.dbkey` round-trips through the ported
    /// (Friday-verified) reader with NO passphrase supplied — the internal
    /// passphrase decrypts it, as v4's silent-resolve path expects.
    #[test]
    fn save_then_load_no_passphrase() {
        let dir = tempdir().unwrap();
        let pepper = generate_pepper().unwrap();
        assert_eq!(pepper.len(), 44); // 32 bytes base64
        save_dbkey(dir.path(), &pepper, "").unwrap();
        let loaded = load_pepper(dir.path(), None).unwrap();
        assert_eq!(loaded, pepper);
    }

    /// `change_passphrase` re-wraps the pepper: the new passphrase unlocks, the
    /// old no longer does, the pepper is preserved, and ONLY `quilltap.dbkey` is
    /// written — the phantom `quilltap-llm-logs.dbkey` copy is gone (v4 4.8.1,
    /// bug 60). The output is byte-format-identical to `save_dbkey` (same
    /// `encrypt_dbkey_json`), which is Friday-verified against v4, so a
    /// v5-rewrapped file is v4-readable (and the reverse — v5 reads any v4
    /// `.dbkey` — is the ported read path).
    #[test]
    fn change_passphrase_round_trip() {
        let dir = tempdir().unwrap();
        let pepper = generate_pepper().unwrap();
        save_dbkey(dir.path(), &pepper, "first").unwrap();

        // Wrong old passphrase → DecryptFailed (does NOT try the internal first).
        assert!(matches!(
            change_passphrase(dir.path(), "wrong", "second"),
            Err(DbKeyError::DecryptFailed)
        ));

        // Correct old → re-wrapped; returns the recovered pepper.
        let recovered = change_passphrase(dir.path(), "first", "second").unwrap();
        assert_eq!(recovered, pepper);
        assert!(load_pepper(dir.path(), Some("first")).is_err());
        assert_eq!(load_pepper(dir.path(), Some("second")).unwrap(), pepper);
        // One pepper, one file: the per-database key file is NOT created.
        assert!(!dir.path().join("quilltap-llm-logs.dbkey").exists());
    }

    /// A stale `quilltap-llm-logs.dbkey` left by a pre-4.8.1 passphrase change
    /// is inert and deliberately left alone: `change_passphrase` neither
    /// rewrites nor deletes it (v4 parity — the docs tell users it is safe to
    /// remove; the code never reaps key material on their behalf).
    #[test]
    fn change_passphrase_leaves_stale_llm_logs_file_untouched() {
        let dir = tempdir().unwrap();
        let pepper = generate_pepper().unwrap();
        save_dbkey(dir.path(), &pepper, "first").unwrap();

        // Plant a stale second file (arbitrary bytes — nothing ever parses it).
        let stale_path = dir.path().join("quilltap-llm-logs.dbkey");
        let stale_bytes = b"{\"stale\":\"pre-4.8.1 wrapping, never read\"}".to_vec();
        std::fs::write(&stale_path, &stale_bytes).unwrap();

        change_passphrase(dir.path(), "first", "second").unwrap();

        // The stale file survives byte-identical; the real file was re-wrapped.
        assert_eq!(std::fs::read(&stale_path).unwrap(), stale_bytes);
        assert_eq!(load_pepper(dir.path(), Some("second")).unwrap(), pepper);
    }

    /// P4.46: a passphrase change PRESERVES fields v5 does not model. v4's
    /// version guard writes `minServerVersion` into `quilltap.dbkey` by
    /// read-modify-write (`version-guard.ts:235-258`) so an older binary is
    /// refused before the database is even opened; v5 has no guard and never
    /// rewrites it, so a full-replace re-wrap would strip the floor from a
    /// v4-authored instance for good. The ten modelled fields keep their
    /// positions and the unknown key keeps its own — the file's shape survives.
    #[test]
    fn change_passphrase_preserves_unknown_fields() {
        let dir = tempdir().unwrap();
        let pepper = generate_pepper().unwrap();
        save_dbkey(dir.path(), &pepper, "first").unwrap();
        let path = dir.path().join("quilltap.dbkey");

        // Plant the field the way v4's guard does: parse, set, 2-space pretty.
        let mut planted: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        planted.insert("minServerVersion".into(), serde_json::json!("4.9.0-dev.0"));
        planted.insert("aFieldV5HasNeverHeardOf".into(), serde_json::json!(7));
        std::fs::write(&path, serde_json::to_string_pretty(&planted).unwrap()).unwrap();
        let keys_before: Vec<String> = planted.keys().cloned().collect();

        assert_eq!(
            change_passphrase(dir.path(), "first", "second").unwrap(),
            pepper
        );

        let after: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            after.get("minServerVersion"),
            Some(&serde_json::json!("4.9.0-dev.0")),
            "the version floor did not survive the re-wrap"
        );
        assert_eq!(
            after.get("aFieldV5HasNeverHeardOf"),
            Some(&serde_json::json!(7))
        );
        assert_eq!(
            after.keys().cloned().collect::<Vec<String>>(),
            keys_before,
            "the re-wrap reordered the file"
        );
        // And it is still a working .dbkey under the new passphrase.
        assert_eq!(load_pepper(dir.path(), Some("second")).unwrap(), pepper);
        // The crypto fields really were replaced (a re-wrap, not a no-op).
        assert_ne!(after.get("salt"), planted.get("salt"));
        assert_ne!(after.get("ciphertext"), planted.get("ciphertext"));
    }

    /// The empty-string old/new passphrases are the internal no-passphrase
    /// sentinel: a no-passphrase vault re-wraps to no-passphrase and back.
    #[test]
    fn change_passphrase_empty_sentinel() {
        let dir = tempdir().unwrap();
        let pepper = generate_pepper().unwrap();
        save_dbkey(dir.path(), &pepper, "").unwrap(); // no passphrase

        // old="" (internal) → set a real passphrase.
        assert_eq!(change_passphrase(dir.path(), "", "secret").unwrap(), pepper);
        assert!(load_pepper(dir.path(), None).is_err()); // internal no longer works
        assert_eq!(load_pepper(dir.path(), Some("secret")).unwrap(), pepper);

        // new="" (internal) → back to no passphrase; None unlocks again.
        assert_eq!(change_passphrase(dir.path(), "secret", "").unwrap(), pepper);
        assert_eq!(load_pepper(dir.path(), None).unwrap(), pepper);
    }

    /// A passphrase-protected file requires the passphrase: `None` errors with
    /// `PassphraseRequired`, a wrong passphrase with `DecryptFailed`, and the
    /// right one recovers the pepper.
    #[test]
    fn save_then_load_with_passphrase() {
        let dir = tempdir().unwrap();
        let pepper = generate_pepper().unwrap();
        save_dbkey(dir.path(), &pepper, "open sesame").unwrap();

        assert!(matches!(
            load_pepper(dir.path(), None),
            Err(DbKeyError::PassphraseRequired)
        ));
        assert!(matches!(
            load_pepper(dir.path(), Some("wrong")),
            Err(DbKeyError::DecryptFailed)
        ));
        assert_eq!(
            load_pepper(dir.path(), Some("open sesame")).unwrap(),
            pepper
        );
    }

    /// The written JSON carries v4's exact field set (and mode 0600 on POSIX).
    #[test]
    fn written_file_shape_matches_v4() {
        let dir = tempdir().unwrap();
        let pepper = generate_pepper().unwrap();
        save_dbkey(dir.path(), &pepper, "").unwrap();
        let path = dir.path().join("quilltap.dbkey");
        let raw = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "version",
                "algorithm",
                "kdf",
                "kdfIterations",
                "kdfDigest",
                "salt",
                "iv",
                "ciphertext",
                "authTag",
                "pepperHash"
            ]
        );
        assert_eq!(v["algorithm"], "aes-256-gcm");
        assert_eq!(v["kdfIterations"], 600000);
        assert_eq!(v["pepperHash"], hash_pepper(&pepper));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}
