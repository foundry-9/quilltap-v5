//! Tier-1 differential: the retired project background display modes (v4
//! `70505745a`, `lib/schemas/project.types.ts`). Exact equality against v4's
//! REAL `normalizeBackgroundDisplayMode` / `RETIRED_BACKGROUND_DISPLAY_MODES` /
//! `ProjectPropertiesSchema`, one row per assertion in v4's own
//! `project-background-display-mode.test.ts` plus the shapes that test does not
//! state.
//!
//! The `parse` rows drive v5's [`ProjectEntity::parse_properties`] — the single
//! chokepoint the overlay READ, the write overlay's read-modify-write, and
//! `write_managed_fields` on create all pass through — so they pin the coercion
//! on every one of those paths at once. `ok: false` is a comparand, not a
//! crash: v4's `.default('theme')` short-circuits only on `undefined`, so an
//! explicit JSON `null` reaches the preprocess, becomes `undefined`, and then
//! fails the enum. v5's `String` field refuses the same `null` at
//! deserialization; the arm asserts the refusal, not its wording.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/project-background-display-mode.ts \
//!     > /tmp/oracle-project-background-mode.ndjson
//! Run:
//!   QT_ORACLE_PROJECT_BACKGROUND_MODE=/tmp/oracle-project-background-mode.ndjson \
//!     cargo test -p quilltap-harness --test project_background_display_mode_equivalence

use quilltap_core::db::document_store_overlay::StoreEntity;
use quilltap_core::db::projects::{
    normalize_background_display_mode, ProjectEntity, RETIRED_BACKGROUND_DISPLAY_MODES,
};
use serde::Deserialize;
use serde_json::Value;

/// `undefined` is not JSON; the oracle emits it as this sentinel.
const UNDEFINED: &str = "<undefined>";

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "normalize")]
    Normalize {
        id: String,
        input: Value,
        out: Value,
    },
    #[serde(rename = "retiredList")]
    RetiredList { id: String, out: Vec<String> },
    #[serde(rename = "parse")]
    Parse {
        id: String,
        input: Value,
        ok: bool,
        mode: Option<String>,
    },
}

#[test]
fn project_background_display_mode_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PROJECT_BACKGROUND_MODE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PROJECT_BACKGROUND_MODE to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut normalize_rows = 0usize;
    let mut parse_rows = 0usize;
    let mut retired_rows = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Normalize { id, input, out } => {
                normalize_rows += 1;
                // The sentinel stands for v4's `undefined` on BOTH sides of the
                // row: as an input it means "the key was absent", which is
                // `None` here; as an output it means "left to the default".
                let arg = match &input {
                    Value::String(s) if s == UNDEFINED => None,
                    other => Some(other),
                };
                let got = normalize_background_display_mode(arg);
                let got_json = match got {
                    None => Value::String(UNDEFINED.to_string()),
                    Some(s) => Value::String(s.to_string()),
                };
                assert_eq!(got_json, out, "normalize '{id}' (input {input})");
            }
            Row::RetiredList { id, out } => {
                retired_rows += 1;
                assert_eq!(
                    RETIRED_BACKGROUND_DISPLAY_MODES.to_vec(),
                    out,
                    "retired list '{id}'"
                );
            }
            Row::Parse {
                id,
                input,
                ok,
                mode,
            } => {
                parse_rows += 1;
                match ProjectEntity::parse_properties(&input) {
                    Ok(props) => {
                        assert!(ok, "parse '{id}': v5 accepted where v4 refused");
                        assert_eq!(
                            Some(props.background_display_mode.clone()),
                            mode,
                            "parse '{id}' mode"
                        );
                    }
                    Err(e) => {
                        assert!(!ok, "parse '{id}': v5 refused where v4 accepted ({e})");
                    }
                }
            }
        }
    }

    // Shape guard (the harness-corpus-constants idiom): a truncated or
    // silently-shrunk oracle must not read as a pass.
    assert!(
        normalize_rows >= 9,
        "expected at least 9 normalize rows, got {normalize_rows} — stale oracle?"
    );
    assert!(
        parse_rows >= 8,
        "expected at least 8 parse rows, got {parse_rows} — stale oracle?"
    );
    // The retired-list row is the family's v4-constant tripwire (a v4-side
    // addition shows up as a row, not as silence) — so its ABSENCE must not
    // read as a pass either (unification review, 2026-09-02).
    assert!(
        retired_rows == 1,
        "expected exactly one retiredList row, got {retired_rows} — stale oracle?"
    );
    eprintln!(
        "project-background-display-mode: {normalize_rows} normalize + {parse_rows} parse rows OK."
    );
}
