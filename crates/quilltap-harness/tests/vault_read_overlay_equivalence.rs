//! Read-differential test: the character vault read overlay (`hydrateOne` +
//! `applyDocumentStoreOverlay`).
//!
//! Both v4 and the Rust port READ the SAME pre-seeded mount-index fixture and run
//! `applyDocumentStoreOverlay` over the SAME input characters, so the hydrated
//! list compares EXACTLY — except the physicalDescription mint branch, whose
//! createdAt/updatedAt are minted at hydration time and are placeholdered for the
//! ids in `mintCharacterIds`.
//!
//! Covers: pass-through (no linked vault), full overlay (all single files +
//! physical base-reuse + multi-default prompt demotion + scenario), DROP on a
//! missing properties.json keystone, partial overlay (properties only → identity
//! kept, systemPrompts/scenarios replaced with []), physical MINT (no existing
//! physicalDescription), empty identity.md → null + prompt promote-first, and
//! prompt keep-non-first-default.
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-vault-read-overlay-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-vault-read-overlay-fixture.ts
//!   QT_FIXTURE_VAULT_READ_OVERLAY=/tmp/qt-vault-read-overlay-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-read-overlay.ts \
//!     > /tmp/oracle-vault-read-overlay.ndjson
//! Run:
//!   QT_ORACLE_VAULT_READ_OVERLAY=/tmp/oracle-vault-read-overlay.ndjson \
//!   QT_FIXTURE_VAULT_READ_OVERLAY=/tmp/qt-vault-read-overlay-fixture.db \
//!     cargo test -p quilltap-harness --test vault_read_overlay_equivalence

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use quilltap_core::db::vault_read_overlay::{
    apply_document_store_overlay, apply_document_store_overlay_one, OverlayOneError,
};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "mintCharacterIds")]
    mint_character_ids: Vec<String>,
    characters: Vec<Value>,
}

#[derive(Deserialize)]
struct Oracle {
    characters: Vec<Value>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/vault-read-overlay-tier2.json")
}

/// Placeholder the minted `physicalDescription.createdAt`/`updatedAt` (the read
/// overlay's only nondeterminism) for characters whose physical base was minted.
fn normalize_mint(chars: &mut [Value], mint_ids: &HashSet<String>) {
    for c in chars.iter_mut() {
        let id = c
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !mint_ids.contains(&id) {
            continue;
        }
        if let Some(pd) = c
            .get_mut("physicalDescription")
            .and_then(Value::as_object_mut)
        {
            for k in ["createdAt", "updatedAt"] {
                if pd.contains_key(k) {
                    pd.insert(k.to_string(), Value::String("<ts>".into()));
                }
            }
        }
    }
}

#[test]
fn vault_read_overlay_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_READ_OVERLAY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_VAULT_READ_OVERLAY to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_VAULT_READ_OVERLAY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_VAULT_READ_OVERLAY to the seed fixture .db (see header)."
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let mint_ids: HashSet<String> = spec.mint_character_ids.iter().cloned().collect();

    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");

    // Fresh copy so the shared seed fixture stays pristine.
    let work = std::env::temp_dir().join(format!("qt-vro-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // The batched overlay over the same input characters.
    let mut got = {
        let repo = writer.doc_mount_documents();
        apply_document_store_overlay(&repo, spec.characters.clone())
            .unwrap_or_else(|e| panic!("apply_document_store_overlay: {e:?}"))
    };
    let mut want = oracle.characters.clone();

    normalize_mint(&mut got, &mint_ids);
    normalize_mint(&mut want, &mint_ids);

    assert_eq!(
        got.len(),
        want.len(),
        "character count diverged (got {}, want {})",
        got.len(),
        want.len()
    );
    for (g, w) in got.iter().zip(want.iter()) {
        assert_eq!(g, w, "hydrated character diverged for {:?}", w.get("id"));
    }

    // The single-character overlay throws (Err) on the dropped character's vault.
    let dropped: Value = spec
        .characters
        .iter()
        .find(|c| {
            c["characterDocumentMountPointId"].as_str()
                == Some("5700000e-0000-4000-8000-0000000000b2")
        })
        .cloned()
        .expect("the drop-case character");
    let repo = writer.doc_mount_documents();
    match apply_document_store_overlay_one(&repo, Some(dropped)) {
        Err(OverlayOneError::Unavailable(_)) => {}
        other => panic!("apply_…_one should be Unavailable on the broken vault, got {other:?}"),
    }

    let _ = std::fs::remove_file(&work);
    eprintln!(
        "OK: vault read overlay matched oracle on {} characters (+ the …One throw).",
        got.len()
    );
}
