//! Tier-1 differential test: the legacy `wardrobe.json` parser
//! (`parseLegacyWardrobeJson`).
//!
//! Exact-equality against the v4 oracle. Unlike the two JSON projection parsers,
//! this validates an array of full `WardrobeItemSchema` items, so it exercises
//! the Zod `z.uuid()` / `z.iso.datetime()` string formats (leap years, Z-only
//! zone, trailing-newline rejection), the `.default()` materialization
//! (componentItemIds/isDefault/replace), unknown-key stripping (root `presets`,
//! per-item extras, in-`outfit` extras), and the discard-but-still-validate
//! handling of `outfit`. Any single bad item nulls the whole result.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-legacy-wardrobe.ts \
//!     > /tmp/oracle-vault-legacy-wardrobe.ndjson
//! Run:
//!   QT_ORACLE_VAULT_LEGACY_WARDROBE=/tmp/oracle-vault-legacy-wardrobe.ndjson \
//!     cargo test -p quilltap-harness --test vault_legacy_wardrobe_equivalence

use quilltap_core::vault_overlay::parse_legacy_wardrobe_json;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Row {
    id: String,
    raw: String,
    out: Value,
}

#[test]
fn legacy_wardrobe_parser_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_LEGACY_WARDROBE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_VAULT_LEGACY_WARDROBE to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        let got = parse_legacy_wardrobe_json(&row.raw)
            .map(|w| serde_json::to_value(w).unwrap())
            .unwrap_or(Value::Null);
        assert_eq!(got, row.out, "[{}] parse_legacy_wardrobe_json", row.id);
        n += 1;
    }
    assert!(n >= 39, "expected the full corpus, saw {n} rows");
    eprintln!("OK: legacy wardrobe parser matched oracle on {n} cases.");
}
