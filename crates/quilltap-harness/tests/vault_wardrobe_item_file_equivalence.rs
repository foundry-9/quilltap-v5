//! Tier-1 differential test: the vault `Wardrobe/*.md` parser
//! (`parseWardrobeItemFile`).
//!
//! Exact-equality against the v4 oracle. Exercises the title fallback chain
//! (frontmatter `title` → `# heading` → filename), the required `types`
//! (parseWardrobeTypesField → skip on invalid), the id sanity check
//! (`/^[0-9a-f-]{36}$/i` else `stableUuidFromString`, incl. a 36-char non-hex id
//! that must fall back), the non-empty-string / boolean-flag / archived field
//! logic, the raw `componentItems`, the frontmatter-vs-doc timestamp precedence,
//! the always-present nullable fields, and the body→description rule (empty body
//! → null description, NOT a skip).
//!
//! P4.D255 (v4 `3ee3b1342` + `7c8572869`): every parsed item now carries
//! `imageFileId` (a non-empty string, else `null`), and three `kind`-tagged
//! row families pin the parser extractions the carriers share —
//! `resolveWardrobeItemId`, `isWardrobeItemDocumentPath`,
//! `wardrobeItemIdForDocument` — through their REAL exports.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-item-file.ts \
//!     > /tmp/oracle-vault-wardrobe-item-file.ndjson
//! Run:
//!   QT_ORACLE_VAULT_WARDROBE_ITEM_FILE=/tmp/oracle-vault-wardrobe-item-file.ndjson \
//!     cargo test -p quilltap-harness --test vault_wardrobe_item_file_equivalence

use quilltap_core::vault_overlay::{
    is_wardrobe_item_document_path, parse_wardrobe_item_file, resolve_wardrobe_item_id,
    wardrobe_item_id_for_document, VaultDoc,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Doc {
    content: String,
    #[serde(rename = "mountPointId")]
    mount_point_id: String,
    #[serde(rename = "relativePath")]
    relative_path: String,
    #[serde(rename = "fileName")]
    file_name: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    doc: Doc,
    out: Value,
}

const CID: &str = "char-1";

#[test]
fn wardrobe_item_file_parser_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_WARDROBE_ITEM_FILE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_VAULT_WARDROBE_ITEM_FILE to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n = 0;
    let mut extractions = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        // P4.D255: the three parser extractions (tier-1, their REAL exports).
        let raw: Value = serde_json::from_str(line).expect("parse oracle row");
        if let Some(kind) = raw.get("kind").and_then(Value::as_str) {
            let s = |k: &str| raw[k].as_str().unwrap_or_default().to_string();
            let got = match kind {
                "resolve" => Value::String(resolve_wardrobe_item_id(
                    raw["frontmatterId"].as_str(),
                    "mp-1",
                    "Wardrobe/Coat.md",
                )),
                "docpath" => Value::Bool(is_wardrobe_item_document_path(&s("path"))),
                "docid" => Value::String(wardrobe_item_id_for_document(
                    &s("mountPointId"),
                    &s("relativePath"),
                    &s("content"),
                )),
                other => panic!("unknown oracle row kind {other}"),
            };
            assert_eq!(got, raw["out"], "[{kind}:{}]", s("id"));
            extractions += 1;
            continue;
        }
        let row: Row = serde_json::from_value(raw).expect("parse oracle row");
        let d = &row.doc;
        let doc = VaultDoc {
            content: &d.content,
            mount_point_id: &d.mount_point_id,
            relative_path: &d.relative_path,
            file_name: &d.file_name,
            created_at: &d.created_at,
            updated_at: &d.updated_at,
        };
        let got = parse_wardrobe_item_file(&doc, CID)
            .map(|w| serde_json::to_value(w).unwrap())
            .unwrap_or(Value::Null);
        assert_eq!(got, row.out, "[{}] parse_wardrobe_item_file", row.id);
        n += 1;
    }
    assert!(n >= 24, "expected the full corpus, saw {n} rows");
    assert!(
        extractions >= 28,
        "expected the P4.D255 extraction rows, saw {extractions}"
    );
    eprintln!("OK: wardrobe item file parser matched oracle on {n} cases.");
}
