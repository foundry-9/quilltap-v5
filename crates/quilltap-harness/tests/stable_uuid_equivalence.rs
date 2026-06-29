//! Tier-1 differential test: `stableUuidFromString` (the vault-overlay id leaf).
//!
//! Exact-equality check against the v4 oracle: for every source string in the
//! corpus, `quilltap_core::vault_overlay::stable_uuid_from_string` must produce
//! the byte-identical UUID v4's real `stableUuidFromString` produces. The corpus
//! includes the real `prompt:`/`scenario:`/`wardrobe-item:` prefixed forms, an
//! empty string, and a non-ASCII path (SHA-256 over UTF-8 bytes — must agree).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/stable-uuid.ts \
//!     > /tmp/oracle-stable-uuid.ndjson
//! Run:
//!   QT_ORACLE_STABLE_UUID=/tmp/oracle-stable-uuid.ndjson \
//!     cargo test -p quilltap-harness --test stable_uuid_equivalence

use quilltap_core::vault_overlay::stable_uuid_from_string;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    id: String,
    source: String,
    out: String,
}

#[test]
fn stable_uuid_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_STABLE_UUID") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_STABLE_UUID to the oracle NDJSON (see header).");
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        let got = stable_uuid_from_string(&row.source);
        assert_eq!(
            got, row.out,
            "[{}] stable_uuid_from_string({:?}) = {got}, oracle = {}",
            row.id, row.source, row.out
        );
        n += 1;
    }
    assert!(n >= 8, "expected the full corpus, saw {n} rows");
    eprintln!("OK: stable_uuid_from_string matched oracle on {n} sources.");
}
