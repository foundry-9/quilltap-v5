//! P4.6w pure tier-1 differential: `documents::compute_rename_target` vs v4's
//! REAL `computeRenameTarget` (`lib/documents/operator-doc-actions.ts`). The
//! oracle emits the exact `RenameTarget` object per case; the Rust side rebuilds
//! the same JSON shape from its enum and compares — extension inheritance,
//! directory preservation, JS `trim`, and the rejection arms are all verified.
//!
//! Generate the oracle output (from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/documents-rename-target.ts \
//!     > /tmp/oracle-documents-rename-target.ndjson
//! Run:
//!   QT_ORACLE_DOCUMENTS_RENAME=/tmp/oracle-documents-rename-target.ndjson \
//!     cargo test -p quilltap-harness --test documents_rename_target_equivalence

use quilltap_core::documents::{compute_rename_target, RenameTarget};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Row {
    id: String,
    current: String,
    title: String,
    out: Value,
}

/// Convert the Rust enum into v4's `RenameTarget` JSON shape.
fn to_v4_json(t: &RenameTarget) -> Value {
    match t {
        RenameTarget::Ok {
            new_file_path,
            new_display_title,
        } => json!({
            "ok": true,
            "newFilePath": new_file_path,
            "newDisplayTitle": new_display_title,
        }),
        RenameTarget::Err { reason } => json!({ "ok": false, "reason": reason }),
    }
}

#[test]
fn rename_target_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_DOCUMENTS_RENAME") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_DOCUMENTS_RENAME to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let got = to_v4_json(&compute_rename_target(&row.current, &row.title));
        assert_eq!(
            got, row.out,
            "computeRenameTarget '{}' (current={:?}, title={:?})",
            row.id, row.current, row.title
        );
        count += 1;
    }
    assert!(count > 0, "oracle file looks empty");
    eprintln!("OK: computeRenameTarget matched oracle ({count} rows).");
}
