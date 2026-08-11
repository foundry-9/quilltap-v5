//! P4.D65 — the archive re-encryption sweep, tier 2 (the D63 unit-7 wire's
//! differential).
//!
//! Drives `quilltap_core::services::character_archive::reencrypt::
//! reencrypt_archive_bundles` through the SAME six case sequences the oracle
//! drives v4's real `lib/characters/archive-reencrypt.ts` through, over a FRESH
//! copy of the committed `character-archive-{main,mount}.db` pair per case.
//!
//! ⚠ **Ciphertext is never compared** — every write draws a fresh salt and IV on
//! both sides, so the persisted bytes differ run to run within one engine, let
//! alone across two. What IS comparable, and what the sweep is actually for, is
//! **which passphrase each artifact now answers to**: the `opens` comparand
//! records, per bundle, whether it carries an encryption header and whether it
//! decrypts under the new / old passphrase. A port that "re-encrypted" a bundle
//! into unopenable bytes passes a byte-blind check and fails this one.
//!
//! `files.size` moves when a bundle is rewritten and must NOT move when the
//! attempt failed, so it is diffed too — that is the cheapest proof that a
//! failed file was left strictly alone.
//!
//! ## Why one case archives for real and the rest plant rows
//!
//! `sweep_real_bundle` archives Sable through the real service, so the sweep
//! meets a bundle the writer itself produced (and sweeps from the internal
//! sentinel, which is what `resolveArchivePassphrase` answers on a
//! passphrase-less instance). The other shapes cannot be produced that way at
//! all: a PLAINTEXT pre-encryption bundle, one sealed under a THIRD passphrase
//! (the survivor of an earlier half-finished change), and a row whose storage
//! key names nothing. Those three are the entire reason the result carries
//! per-file failures instead of throwing, and `sweep_mixed_library` proves the
//! design claim directly — the failing bundle is planted FIRST, so a port that
//! stops at the first error reports one success fewer.
//!
//! Generate the oracle (Node 24, from the v4 checkout — jest ignores `.claude/`
//! venues, so the case + spec are copied to a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-archive-reencrypt-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/archive-reencrypt-tier2.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/character-archive.json"       "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_ARCHIVE_MAIN=$V5W/crates/quilltap-web/tests/fixtures/character-archive-main.db \
//!   QT_FIXTURE_ARCHIVE_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/character-archive-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-archive-reencrypt.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- archive-reencrypt-tier2
//!
//! Run:
//!   QT_ORACLE_ARCHIVE_REENCRYPT=/tmp/oracle-archive-reencrypt.ndjson \
//!     cargo test -p quilltap-harness --test archive_reencrypt_tier2_equivalence -- --nocapture
//!
//! Skips (does not fail) when the env var is unset — the standing gated-
//! differential discipline.

use std::path::PathBuf;
use std::sync::Arc;

use quilltap_core::db::files::{FileEntry, FilesRepository};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::character_archive::crypto::{
    decrypt_archive, encrypt_archive, is_encrypted_archive, PassphraseSource,
};
use quilltap_core::services::character_archive::reencrypt::reencrypt_archive_bundles;
use quilltap_core::services::character_archive::service::{archive_character, ArchiveSeams};
use quilltap_core::services::file_storage::StorageBackend;
use quilltap_host::files_store::LocalStorageBackend;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    seed_timestamp: String,
    sable: String,
}

const OLD_PASSPHRASE: &str = "the-old-brass-key";
const NEW_PASSPHRASE: &str = "a-newer-brass-key";
const THIRD_PASSPHRASE: &str = "a-third-key";

const F_PLAINTEXT: &str = "f0000000-0000-4000-8000-0000000000a1";
const F_FOREIGN: &str = "f0000000-0000-4000-8000-0000000000a2";
const F_NO_BYTES: &str = "f0000000-0000-4000-8000-0000000000a3";

/// The oracle's one-line NDJSON body, byte for byte (`JSON.stringify` of the
/// same object, `+ '\n'`). It has to match exactly: it is what the plaintext
/// bundles' `size` is measured against.
const BUNDLE_NDJSON: &str = "{\"kind\":\"__envelope__\",\"format\":\"qtap-ndjson\",\"version\":1,\"manifest\":{\"exportType\":\"characters\",\"counts\":{}}}\n";

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/character-archive.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

struct Work {
    _dir: tempfile::TempDir,
    db: Db,
    backend: Arc<dyn StorageBackend>,
}

fn open_work(spec: &Spec) -> Work {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.db");
    let mount = dir.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("character-archive-main.db"), &main).expect("copy main");
    std::fs::copy(fixtures_dir().join("character-archive-mount.db"), &mount).expect("copy mount");
    std::fs::create_dir_all(dir.path().join("files")).expect("files dir");
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    let backend: Arc<dyn StorageBackend> =
        Arc::new(LocalStorageBackend::new(dir.path().join("files")));
    Work {
        _dir: dir,
        db,
        backend,
    }
}

/// Plant an ARCHIVE `files` row, with or without bytes on disk — the oracle's
/// `plant` helper, key for key.
async fn plant(work: &Work, spec: &Spec, id: &str, bytes: Option<&[u8]>, filename: &str) {
    let storage_key = match bytes {
        Some(_) => format!("archives/{id}/{filename}"),
        None => format!("archives/{id}/gone.qtap"),
    };
    if let Some(b) = bytes {
        work.backend
            .upload(&storage_key, b, "application/octet-stream")
            .expect("upload planted bytes");
    }
    let row = quilltap_core::db::files::FileCreate {
        user_id: spec.user_id.clone(),
        sha256: sha256_hex(bytes.unwrap_or(&[])),
        original_filename: filename.to_string(),
        mime_type: "application/octet-stream".to_string(),
        size: bytes.map(|b| b.len()).unwrap_or(0) as f64,
        width: None,
        height: None,
        is_plain_text: None,
        linked_to: Vec::new(),
        source: "GENERATED".to_string(),
        category: "ARCHIVE".to_string(),
        generation_prompt: None,
        generation_model: None,
        generation_revised_prompt: None,
        description: None,
        tags: Vec::new(),
        project_id: None,
        folder_path: None,
        storage_key: Some(storage_key),
        file_status: "ok".to_string(),
    };
    let opts = quilltap_core::db::files::CreateOptions {
        id: id.to_string(),
        created_at: spec.seed_timestamp.clone(),
        updated_at: spec.seed_timestamp.clone(),
    };
    work.db
        .write(move |ws| FilesRepository::new(ws.main().connection()).create(&row, &opts))
        .await
        .expect("plant files row");
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn seams<'a>(work: &Work) -> ArchiveSeams<'a> {
    ArchiveSeams {
        backend: Some(Arc::clone(&work.backend)),
        passphrase: PassphraseSource {
            cached: None,
            has_user_passphrase: false,
        },
        // No bundle byte length is compared in this family (the sweep's own
        // rewrite decides `size`), so the manifest's version string is free.
        app_version: "0.0.0".to_string(),
        codec: None,
        extractor: quilltap_core::services::mount_index::converters::default_text_extractor(),
    }
}

/// The `opens` comparand: for each ARCHIVE row in id order, whether the
/// persisted bytes carry a header and which passphrase they answer to.
fn opens_and_files(work: &Work, spec: &Spec, old_passphrase: &str) -> (Value, Value) {
    let uid = spec.user_id.clone();
    let mut rows = work
        .db
        .read_main(move |c| FilesRepository::new(c).find_by_category(&uid, "ARCHIVE"))
        .expect("read archive rows");
    rows.sort_by(|a, b| a.id.cmp(&b.id));

    let mut opens = Vec::new();
    let mut files = Vec::new();
    for file in rows {
        let entry = FileEntry {
            id: file.id.clone(),
            sha256: file.sha256.clone(),
            original_filename: file.original_filename.clone(),
            mime_type: file.mime_type.clone(),
            size: file.size,
            width: file.width,
            height: file.height,
            category: file.category.clone(),
            generation_prompt: None,
            generation_model: None,
            generation_revised_prompt: None,
            description: file.description.clone(),
            storage_key: file.storage_key.clone(),
        };
        let bytes = quilltap_core::services::file_storage::download_file(
            &work.db,
            work.backend.as_ref(),
            &entry,
        );
        let (encrypted, with_new, with_old) = match bytes {
            Ok(b) => {
                let enc = is_encrypted_archive(&b);
                let open = |pass: &str| enc && decrypt_archive(&b, pass).is_ok();
                (
                    Value::Bool(enc),
                    Value::Bool(open(NEW_PASSPHRASE)),
                    Value::Bool(open(old_passphrase)),
                )
            }
            // A row with no bytes cannot be opened at all — that IS the answer,
            // and the oracle records the same three nulls.
            Err(_) => (Value::Null, Value::Null, Value::Null),
        };
        opens.push(json!({
            "originalFilename": file.original_filename,
            "encrypted": encrypted,
            "opensWithNew": with_new,
            "opensWithOld": with_old,
        }));
        files.push(json!({
            "originalFilename": file.original_filename,
            "size": file.size,
        }));
    }
    (Value::Array(opens), Value::Array(files))
}

/// The ARCHIVE bundle's filename carries a minted, `:`/`.`-scrubbed ISO stamp,
/// so the real-bundle case's name can never agree across two runs; collapse it
/// on both sides. Everything else in this corpus is a planted, pinned name.
fn blind_minted_archive_name(v: &mut Value) {
    let Some(arr) = v.as_array_mut() else { return };
    for row in arr {
        let is_minted = row
            .get("originalFilename")
            .and_then(Value::as_str)
            .map(|s| s.ends_with("-character-archive.qtap"))
            .unwrap_or(false);
        if is_minted {
            if let Some(m) = row.as_object_mut() {
                m.insert(
                    "originalFilename".into(),
                    Value::String("<minted>-character-archive.qtap".into()),
                );
            }
        }
    }
}

/// The real bundle's byte length depends on the manifest's `appVersion` (a
/// version string neither side is asserting here), so its `size` is blinded —
/// the PLANTED bundles' sizes stay real comparands, and they are the ones that
/// prove "rewritten" vs "left alone".
fn blind_minted_archive_size(v: &mut Value) {
    let Some(arr) = v.as_array_mut() else { return };
    for row in arr {
        let is_minted = row
            .get("originalFilename")
            .and_then(Value::as_str)
            .map(|s| s.contains("-character-archive.qtap"))
            .unwrap_or(false);
        if is_minted {
            if let Some(m) = row.as_object_mut() {
                m.insert("size".into(), Value::String("<minted-size>".into()));
            }
        }
    }
}

#[derive(Deserialize)]
struct OracleCase {
    name: String,
    #[serde(default, rename = "oldPassphrase")]
    old_passphrase: String,
    #[serde(default)]
    result: Value,
    #[serde(default)]
    opens: Value,
    #[serde(default)]
    files: Value,
}

#[tokio::test(flavor = "multi_thread")]
async fn archive_reencrypt_tier2_equivalence() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_ARCHIVE_REENCRYPT") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec")).expect("spec");
    let oracle: Vec<OracleCase> = std::fs::read_to_string(&oracle_path)
        .expect("oracle ndjson")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert_eq!(oracle.len(), 6, "the corpus is six cases");

    let mut mismatches: Vec<String> = Vec::new();

    for case in &oracle {
        let work = open_work(&spec);
        let plaintext = BUNDLE_NDJSON.as_bytes();
        let foreign = encrypt_archive(plaintext, THIRD_PASSPHRASE, None).expect("seal foreign");

        let mut old_passphrase = OLD_PASSPHRASE.to_string();
        match case.name.as_str() {
            "sweep_empty" => {}
            "sweep_real_bundle" => {
                let s = seams(&work);
                archive_character(&work.db, &spec.user_id, &spec.sable, &s)
                    .await
                    .expect("archive");
                old_passphrase = quilltap_core::dbkey::INTERNAL_PASSPHRASE.to_string();
            }
            "sweep_plaintext_bundle" => {
                plant(
                    &work,
                    &spec,
                    F_PLAINTEXT,
                    Some(plaintext),
                    "pre-encryption.qtap",
                )
                .await;
            }
            "sweep_foreign_passphrase" => {
                plant(&work, &spec, F_FOREIGN, Some(&foreign), "foreign.qtap").await;
            }
            "sweep_missing_bytes" => {
                plant(&work, &spec, F_NO_BYTES, None, "vanished.qtap").await;
            }
            "sweep_mixed_library" => {
                plant(&work, &spec, F_FOREIGN, Some(&foreign), "foreign.qtap").await;
                plant(
                    &work,
                    &spec,
                    F_PLAINTEXT,
                    Some(plaintext),
                    "pre-encryption.qtap",
                )
                .await;
                plant(&work, &spec, F_NO_BYTES, None, "vanished.qtap").await;
            }
            other => panic!("unknown oracle case: {other}"),
        }
        assert_eq!(
            old_passphrase, case.old_passphrase,
            "case {}: the two sides must sweep from the same passphrase",
            case.name
        );

        let result = reencrypt_archive_bundles(
            &work.db,
            work.backend.as_ref(),
            &spec.user_id,
            &old_passphrase,
            NEW_PASSPHRASE,
        )
        .await
        .expect("sweep");

        let (mut opens, mut files) = opens_and_files(&work, &spec, &old_passphrase);
        let mut want_opens = case.opens.clone();
        let mut want_files = case.files.clone();
        for v in [&mut opens, &mut want_opens] {
            blind_minted_archive_name(v);
        }
        for v in [&mut files, &mut want_files] {
            blind_minted_archive_name(v);
            blind_minted_archive_size(v);
        }

        let got_result = serde_json::to_value(&result).expect("serialize result");
        for (label, got, want) in [
            ("result", &got_result, &case.result),
            ("opens", &opens, &want_opens),
            ("files", &files, &want_files),
        ] {
            if got != want {
                mismatches.push(format!(
                    "case {} section {label}:\n  got  {got}\n  want {want}",
                    case.name
                ));
            }
        }
        println!(
            "case {:<26} result={} opens={} files={}",
            case.name,
            if got_result == case.result {
                "ok"
            } else {
                "DIFF"
            },
            if opens == want_opens { "ok" } else { "DIFF" },
            if files == want_files { "ok" } else { "DIFF" },
        );
    }

    assert!(
        mismatches.is_empty(),
        "archive re-encryption divergences:\n{}",
        mismatches.join("\n\n")
    );
}
