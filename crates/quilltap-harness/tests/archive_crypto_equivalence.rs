//! P4.D63 — the archive-crypto differential (tier 1, EXACT).
//!
//! Compares `quilltap_core::services::character_archive::crypto` against v4's
//! REAL `lib/characters/archive-crypto.ts` at `d553f72a`, with the oracle's
//! `crypto.randomBytes` mocked so salt and IV are pinned. That makes
//! `encryptArchive` a pure function and lets this diff the whole bundle
//! byte-for-byte (magic, big-endian header length, header key ORDER, salt, IV,
//! keyHash, ciphertext and GCM tag) rather than settling for "it round-trips".
//! (Prose here must not start a line with a shell keyword — the recipe-sweep
//! extractor once mistook a wrapped "for byte**" line for script.)
//!
//! It also diffs, in both directions:
//!   - every refusal arm's error `name` AND `message` (the four typed errors'
//!     sentences are contractual across the three lanes of this round),
//!   - **cross-decryption**: v5 opens v4's bundle and v4's recorded round-trip
//!     equals v5's own, so neither side is merely self-consistent,
//!   - the `isEncryptedArchive` magic probe.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout; jest ignores
//! `.claude/` venues, so the case is copied to a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-archive-crypto-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/archive-crypto.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-archive-crypto.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- archive-crypto
//!
//! Run:
//!   QT_ORACLE_ARCHIVE_CRYPTO=/tmp/oracle-archive-crypto.ndjson \
//!     cargo test -p quilltap-harness --test archive_crypto_equivalence -- --nocapture
//!
//! Skips (does not fail) when the env var is unset — the standing gated-
//! differential discipline.

use quilltap_core::services::character_archive::crypto::{
    decrypt_archive, encrypt_archive, is_encrypted_archive, ArchiveCryptoError,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct EncryptInput {
    #[serde(rename = "plaintextHex")]
    plaintext_hex: String,
    passphrase: String,
    #[serde(rename = "saltHex")]
    salt_hex: String,
    #[serde(rename = "ivHex")]
    iv_hex: String,
}

#[derive(Deserialize)]
struct WireError {
    name: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "encrypt")]
    Encrypt {
        name: String,
        input: EncryptInput,
        #[serde(rename = "bundleHex")]
        bundle_hex: String,
        #[serde(rename = "headerJson")]
        header_json: String,
        #[serde(rename = "roundTripHex")]
        round_trip_hex: String,
    },
    #[serde(rename = "decrypt_error")]
    DecryptError {
        name: String,
        #[serde(rename = "bytesHex")]
        bytes_hex: String,
        passphrase: String,
        error: WireError,
    },
    #[serde(rename = "is_encrypted")]
    IsEncrypted {
        name: String,
        hex: String,
        result: bool,
    },
}

/// v4's `Error.name` for each of our typed variants — the pairing that makes
/// the classes, not just the sentences, comparable.
fn error_name(e: &ArchiveCryptoError) -> &'static str {
    match e {
        ArchiveCryptoError::PassphraseMismatch { .. } => "ArchivePassphraseMismatchError",
        ArchiveCryptoError::Integrity { .. } => "ArchiveIntegrityError",
        ArchiveCryptoError::Format { .. } => "ArchiveFormatError",
        ArchiveCryptoError::KeyUnavailable => "ArchiveKeyUnavailableError",
    }
}

#[test]
fn archive_crypto_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_ARCHIVE_CRYPTO") else {
        eprintln!("SKIP: set QT_ORACLE_ARCHIVE_CRYPTO to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read the archive-crypto oracle");
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse an oracle row"))
        .collect();
    assert!(
        rows.len() >= 16,
        "the oracle looks truncated ({} rows) — regenerate it",
        rows.len()
    );

    let mut failed: Vec<String> = Vec::new();
    let mut encrypt_cases = 0;
    let mut error_cases = 0;

    for row in &rows {
        match row {
            Row::Encrypt {
                name,
                input,
                bundle_hex,
                header_json,
                round_trip_hex,
            } => {
                encrypt_cases += 1;
                let plaintext = hex::decode(&input.plaintext_hex).expect("plaintext hex");
                let salt = hex::decode(&input.salt_hex).expect("salt hex");
                let iv = hex::decode(&input.iv_hex).expect("iv hex");

                let got = encrypt_archive(&plaintext, &input.passphrase, Some((&salt, &iv)))
                    .expect("v5 encrypt");
                let got_hex = hex::encode(&got);
                if &got_hex != bundle_hex {
                    // Name the header separately: a header-key-order or
                    // field-value drift is a different bug from a ciphertext
                    // drift, and the whole-bundle hex hides which one it is.
                    let hl = u32::from_be_bytes([got[8], got[9], got[10], got[11]]) as usize;
                    let got_header = String::from_utf8_lossy(&got[12..12 + hl]).to_string();
                    if &got_header != header_json {
                        eprintln!(
                            "[{name}] HEADER MISMATCH:\n  got : {got_header}\n  want: {header_json}"
                        );
                    } else {
                        eprintln!("[{name}] BUNDLE MISMATCH (header identical → ciphertext/tag)");
                    }
                    failed.push(name.clone());
                    continue;
                }

                // v5 reads v4's bytes (they are the same bytes, but this also
                // exercises the header-driven KDF parameter path), and the
                // plaintext must be what v4 recovered.
                let round_trip = decrypt_archive(&got, &input.passphrase).expect("v5 decrypt");
                if hex::encode(&round_trip) != *round_trip_hex {
                    eprintln!("[{name}] ROUND-TRIP MISMATCH");
                    failed.push(format!("{name}_roundtrip"));
                    continue;
                }
                eprintln!("[{name}] OK ({} bundle bytes).", got.len());
            }

            Row::DecryptError {
                name,
                bytes_hex,
                passphrase,
                error,
            } => {
                error_cases += 1;
                let bytes = hex::decode(bytes_hex).expect("bytes hex");
                match decrypt_archive(&bytes, passphrase) {
                    Ok(_) => {
                        eprintln!("[{name}] expected a refusal, got success");
                        failed.push(name.clone());
                    }
                    Err(e) => {
                        let got_name = error_name(&e);
                        let got_message = e.to_string();
                        if got_name != error.name || got_message != error.message {
                            eprintln!(
                                "[{name}] ERROR MISMATCH:\n  got : {got_name}: {got_message}\n  want: {}: {}",
                                error.name, error.message
                            );
                            failed.push(name.clone());
                        } else {
                            eprintln!("[{name}] OK ({got_name}).");
                        }
                    }
                }
            }

            Row::IsEncrypted { name, hex, result } => {
                let bytes = hex::decode(hex).expect("probe hex");
                let got = is_encrypted_archive(&bytes);
                if got != *result {
                    eprintln!("[{name}] MISMATCH: got {got} want {result}");
                    failed.push(name.clone());
                } else {
                    eprintln!("[{name}] OK ({got}).");
                }
            }
        }
    }

    // Shape assertions rather than hand counts (the standing
    // `harness-corpus-shape-constants-rot` rule): a corpus that quietly lost
    // its refusal arms would otherwise pass with flying colours.
    assert!(
        encrypt_cases >= 6,
        "expected the encrypt arms (got {encrypt_cases}) — regenerate the oracle"
    );
    assert!(
        error_cases >= 6,
        "expected every refusal arm (got {error_cases}) — regenerate the oracle"
    );
    assert!(failed.is_empty(), "archive-crypto FAILED: {failed:?}");
}
