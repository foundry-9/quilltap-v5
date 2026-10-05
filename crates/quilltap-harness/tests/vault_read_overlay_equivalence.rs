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
//! prompt keep-non-first-default. P4.142: the RENAME plant
//! (`doc_mount_file_links.relativePath` renamed on a per-run copy) — v4's batch
//! reads fall back, so the batch overlay drops every vaulted character with v4's
//! 9 + 2 batch-read lines + the drop lines, and the single overlay throws
//! `CharacterVaultUnavailableError`; compared list + lines (`error=` tails
//! skipped) + the refusal.
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
    /// P4.142 — the RENAME plant (`doc_mount_file_links.relativePath` renamed).
    plant: Plant,
}

#[derive(Deserialize)]
struct Plant {
    characters: Vec<Value>,
    logs: Vec<LogRec>,
    one: PlantOne,
}

#[derive(Deserialize)]
struct PlantOne {
    threw: Option<String>,
    #[serde(default)]
    message: Option<String>,
    logs: Vec<LogRec>,
}

/// One v4 ERROR/WARN recorded off the `Logger` prototype: the context keys in
/// v4's order, `module` and `error` omitted.
#[derive(Deserialize, Debug)]
struct LogRec {
    level: String,
    message: String,
    fields: Vec<(String, String)>,
}

/// The two repository-layer lines (v4's `Repository` logger → `quilltap::db`);
/// every other recorded line is the overlay module's own.
const REPOSITORY_MESSAGES: &[&str] = &[
    "Error finding documents by mount point IDs and path",
    "Error finding documents by mount point IDs and folder",
];

/// v4's record rendered the way v5's capture layer renders the same line, with
/// the `error=` tail dropped on BOTH sides (on a links-rename plant the two
/// sides' messages are each one's own driver sentence — P4.131 finding 3).
fn render_v4(rec: &LogRec) -> String {
    let target = if REPOSITORY_MESSAGES.contains(&rec.message.as_str()) {
        "quilltap::db"
    } else {
        "quilltap_core::db::vault_read_overlay"
    };
    let mut line = format!("{} {target} {}", rec.level.to_uppercase(), rec.message);
    for (k, v) in &rec.fields {
        line.push_str(&format!(" {k}={v}"));
    }
    line
}

fn strip_error_tail(line: &str) -> String {
    match line.find(" error=") {
        Some(i) => line[..i].to_string(),
        None => line.to_string(),
    }
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
    // A scratch dir removed on drop — with the TRUNCATE-mode `-journal`
    // the writable open leaves beside the DB.
    let scratch = tempfile::Builder::new()
        .prefix("qt-vro-rust-")
        .tempdir()
        .expect("tempdir");
    let work = scratch.path().join("work.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // The batched overlay over the same input characters, with the log captured:
    // P4.D172 ported v4's two drop lines (`read-overlay.ts:346-362`), which v5
    // had been swallowing under a bare `Err(_) => { /* drop */ }`. That silence
    // mattered little while this was a list read; bug 131 puts it on the hot turn
    // path, where a seat vanishing from the rotation now says why.
    let (mut got, overlay_lines) = quilltap_core::test_support::captured_with(|| {
        let repo = writer.doc_mount_documents();
        apply_document_store_overlay(&repo, spec.characters.clone())
            .unwrap_or_else(|e| panic!("apply_document_store_overlay: {e:?}"))
    });
    {
        let per_character: Vec<&String> = overlay_lines
            .iter()
            .filter(|l| l.contains("Dropping character from list — vault unavailable"))
            .collect();
        assert_eq!(
            per_character.len(),
            1,
            "expected v4's per-character ERROR once, got {overlay_lines:?}"
        );
        // v4's bag: characterId, characterDocumentMountPointId, detail (the
        // `CharacterVaultUnavailableError` message).
        let line = per_character[0];
        assert!(
            line.contains("characterDocumentMountPointId=5700000e-0000-4000-8000-0000000000b2"),
            "{line}"
        );
        assert!(line.contains("has no usable vault"), "{line}");
        assert!(line.contains("properties.json missing"), "{line}");

        let summary: Vec<&String> = overlay_lines
            .iter()
            .filter(|l| {
                l.contains("applyDocumentStoreOverlay dropped characters with unavailable vaults")
            })
            .collect();
        assert_eq!(
            summary.len(),
            1,
            "expected v4's summary WARN once, got {overlay_lines:?}"
        );
        assert!(summary[0].contains("dropped=1"), "{}", summary[0]);
        assert!(
            summary[0].contains(&format!("total={}", spec.characters.len())),
            "{}",
            summary[0]
        );
    }
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

    // The SILENCE arm: a list with nothing broken in it logs neither line. Without
    // this the assertions above would pass on a logger that shouts about
    // everything.
    let clean: Vec<Value> = spec
        .characters
        .iter()
        .filter(|c| {
            c["characterDocumentMountPointId"].as_str()
                != Some("5700000e-0000-4000-8000-0000000000b2")
        })
        .cloned()
        .collect();
    let (_, clean_lines) = quilltap_core::test_support::captured_with(|| {
        let repo = writer.doc_mount_documents();
        apply_document_store_overlay(&repo, clean.clone())
            .unwrap_or_else(|e| panic!("apply_document_store_overlay (clean): {e:?}"))
    });
    assert!(
        !clean_lines
            .iter()
            .any(|l| l.contains("Dropping character from list")
                || l.contains("dropped characters with unavailable vaults")),
        "a list with no broken vault must log neither drop line: {clean_lines:?}"
    );

    // ── P4.142 — the RENAME plant, on a SECOND per-run copy. v4's batch reads
    // are fallback `withRawDb([])`s, so the batch overlay drops every vaulted
    // character (9 path lines, 2 folder lines, one drop ERROR per vaulted
    // character, 1 summary WARN) and the single overlay throws
    // `CharacterVaultUnavailableError` (`properties.json missing`) after its own
    // 9 + 2 lines.
    // A scratch dir removed on drop — with the TRUNCATE-mode `-journal`
    // the writable open leaves beside the DB.
    let plant_scratch = tempfile::Builder::new()
        .prefix("qt-vro-rust-plant-")
        .tempdir()
        .expect("tempdir");
    let plant_work = plant_scratch.path().join("work.db");
    std::fs::copy(&fixture, &plant_work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let plant_writer = Writer::open_writable(&plant_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open plant copy: {e}"));
    plant_writer
        .connection()
        .execute_batch(
            "ALTER TABLE doc_mount_file_links RENAME COLUMN relativePath TO relativePath_x",
        )
        .expect("plant the rename");
    let (plant_got, plant_lines) = quilltap_core::test_support::captured_with(|| {
        let repo = plant_writer.doc_mount_documents();
        apply_document_store_overlay(&repo, spec.characters.clone())
    });
    let mut plant_got = plant_got.unwrap_or_else(|e| {
        panic!("the batch overlay must DROP on the plant (v4), got Err({e:?})")
    });
    let mut plant_want = oracle.plant.characters.clone();
    normalize_mint(&mut plant_got, &mint_ids);
    normalize_mint(&mut plant_want, &mint_ids);
    assert_eq!(plant_got, plant_want, "the plant's hydrated list diverged");
    let want_lines: Vec<String> = oracle.plant.logs.iter().map(render_v4).collect();
    let got_lines: Vec<String> = plant_lines
        .iter()
        .filter(|l| l.starts_with("ERROR ") || l.starts_with("WARN "))
        .map(|l| strip_error_tail(l))
        .collect();
    assert_eq!(
        got_lines, want_lines,
        "the plant's batch-overlay line sequence diverged"
    );
    assert_eq!(
        want_lines.len(),
        9 + 2 + 6 + 1,
        "the oracle's plant must hold 9 path + 2 folder + 6 drops + 1 WARN: {want_lines:#?}"
    );

    let ada: Value = spec
        .characters
        .iter()
        .find(|c| c["name"].as_str() == Some("Ada"))
        .cloned()
        .expect("Ada");
    let (one, one_lines) = quilltap_core::test_support::captured_with(|| {
        let repo = plant_writer.doc_mount_documents();
        apply_document_store_overlay_one(&repo, Some(ada))
    });
    assert_eq!(
        oracle.plant.one.threw.as_deref(),
        Some("CharacterVaultUnavailableError"),
        "v4's single overlay throws on the plant"
    );
    match one {
        Err(OverlayOneError::Unavailable(u)) => assert_eq!(
            Some(u.message()),
            oracle.plant.one.message,
            "the single overlay's refusal text diverged"
        ),
        other => panic!(
            "the single overlay must answer Unavailable (v4 CharacterVaultUnavailableError) on the plant, got {other:?}"
        ),
    }
    let want_one: Vec<String> = oracle.plant.one.logs.iter().map(render_v4).collect();
    let got_one: Vec<String> = one_lines
        .iter()
        .filter(|l| l.starts_with("ERROR ") || l.starts_with("WARN "))
        .map(|l| strip_error_tail(l))
        .collect();
    assert_eq!(
        got_one, want_one,
        "the single overlay's plant lines diverged"
    );
    assert_eq!(want_one.len(), 9 + 2, "{want_one:#?}");
    drop(plant_writer);

    eprintln!(
        "OK: vault read overlay matched oracle on {} characters (+ the …One throw, + the rename plant: {} lines).",
        got.len(),
        want_lines.len()
    );
}
