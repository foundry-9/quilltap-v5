//! Read-differential test: the character vault WARDROBE read overlay
//! (`readCharacterVaultWardrobe`).
//!
//! Both v4 and the Rust port READ the SAME pre-seeded mount-index fixture and run
//! `readCharacterVaultWardrobe(mountPointId, characterId)` over the SAME cases, so
//! each `{ items } | null` result compares EXACTLY — no normalization (there is no
//! clock mint on this read path; every id/timestamp comes from the shared fixture).
//!
//! Covers (store A — the `Wardrobe/*.md` folder layout): code-unit (Decision-B)
//! ordering, the slug/UUID/unknown/collided-slug component resolution
//! (`05-outfit` → blue-shirt-by-slug + pants-by-UUID + first-claimer hat, ghost
//! dropped), a mutual cycle whose LIVE clear leaves the second item's ref intact
//! (`06-cycle-a` cleared, `07-cycle-b` keeps `cycle-a`), a self-cycle clear
//! (`08-self`), a collided slug addressable only by UUID (`04-hat-secondary`), and
//! an archived item (`09-archived`, archivedAt ← doc.updatedAt). Store B exercises
//! the legacy `wardrobe.json` fallback; store C the empty-vault `null`.
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-vault-wardrobe-read-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-vault-wardrobe-read-fixture.ts
//!   QT_FIXTURE_VAULT_WARDROBE_READ=/tmp/qt-vault-wardrobe-read-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-read.ts \
//!     > /tmp/oracle-vault-wardrobe-read.ndjson
//! Run:
//!   QT_ORACLE_VAULT_WARDROBE_READ=/tmp/oracle-vault-wardrobe-read.ndjson \
//!   QT_FIXTURE_VAULT_WARDROBE_READ=/tmp/qt-vault-wardrobe-read-fixture.db \
//!     cargo test -p quilltap-harness --test vault_wardrobe_read_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::vault_read_overlay::read_character_vault_wardrobe;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Case {
    #[serde(rename = "mountPointId")]
    mount_point_id: String,
    #[serde(rename = "characterId")]
    character_id: String,
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Oracle {
    results: Vec<Value>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/vault-wardrobe-read-tier2.json")
}

#[test]
fn vault_wardrobe_read_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_WARDROBE_READ") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_VAULT_WARDROBE_READ to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_VAULT_WARDROBE_READ") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_VAULT_WARDROBE_READ to the seed fixture .db (see header)."
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");

    assert_eq!(
        spec.cases.len(),
        oracle.results.len(),
        "case count vs oracle result count diverged"
    );

    // Fresh copy so the shared seed fixture stays pristine.
    let work = std::env::temp_dir().join(format!("qt-vwr-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    let repo = writer.doc_mount_documents();

    for (c, want) in spec.cases.iter().zip(oracle.results.iter()) {
        let got = read_character_vault_wardrobe(&repo, &c.mount_point_id, &c.character_id)
            .unwrap_or_else(|e| {
                panic!("read_character_vault_wardrobe({}): {e:?}", c.mount_point_id)
            })
            .unwrap_or(Value::Null);
        assert_eq!(
            &got, want,
            "wardrobe read diverged for mount {} (char {})",
            c.mount_point_id, c.character_id
        );
    }

    let _ = std::fs::remove_file(&work);
    eprintln!(
        "OK: vault wardrobe read matched oracle on {} cases.",
        spec.cases.len()
    );
}
