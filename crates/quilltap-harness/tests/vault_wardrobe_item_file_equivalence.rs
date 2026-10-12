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
//! P4.D279 (v4 `4a38222e6`, wardrobe folders): every parsed item carries
//! `folder` — DERIVED from the file's path (`Wardrobe/Formal/Evening/Gown.md` →
//! `Formal/Evening`; a root file → `''`; a frontmatter `folder:` key is IGNORED
//! — the path wins). The `docpath` rows grow to the RECURSIVE predicate (items
//! at any depth; only the FIRST-level `images/` reserved; `instructions.md`
//! excluded at any depth; an empty segment never an item), and two NEW
//! `kind`-tagged families pin the new exports the projection's `ownsFile`
//! sweep and the folder rename's id stamping call: `isdoc`
//! (`isWardrobeItemDocument` — item path + a valid `types` list, `null`
//! content as `''`) and `stableid` (`hasStableWardrobeItemId` — a UUID-shaped
//! frontmatter `id`).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-item-file.ts \
//!     > /tmp/oracle-vault-wardrobe-item-file.ndjson
//! Run:
//!   QT_ORACLE_VAULT_WARDROBE_ITEM_FILE=/tmp/oracle-vault-wardrobe-item-file.ndjson \
//!     cargo test -p quilltap-harness --test vault_wardrobe_item_file_equivalence

use quilltap_core::vault_overlay::{
    has_stable_wardrobe_item_id, is_wardrobe_item_document, is_wardrobe_item_document_path,
    parse_wardrobe_item_file, resolve_wardrobe_item_id, wardrobe_item_id_for_document, VaultDoc,
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
    let mut folder_rows = 0;
    let mut nested_docpaths = 0;
    let mut isdoc_rows = 0;
    let mut stableid_rows = 0;
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
                "docpath" => {
                    if s("path").matches('/').count() > 1 {
                        nested_docpaths += 1;
                    }
                    Value::Bool(is_wardrobe_item_document_path(&s("path")))
                }
                // P4.D279: the two NEW exports. The oracle records a `null`
                // content for v4's `null` AND `undefined` (both `?? ''`).
                "isdoc" => {
                    isdoc_rows += 1;
                    Value::Bool(is_wardrobe_item_document(
                        &s("relativePath"),
                        raw["content"].as_str(),
                    ))
                }
                "stableid" => {
                    stableid_rows += 1;
                    Value::Bool(has_stable_wardrobe_item_id(raw["content"].as_str()))
                }
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
        // P4.D279: every parsed item carries `folder`, and the nested rows
        // carry a NON-root one (the oracle's own shape, asserted so a corpus
        // that stopped exercising folders cannot pass quietly).
        if let Some(obj) = row.out.as_object() {
            assert!(
                obj.get("folder").is_some_and(Value::is_string),
                "[{}] a parsed item must carry a string `folder`",
                row.id
            );
            if row.doc.relative_path.matches('/').count() > 1 {
                assert!(
                    obj["folder"].as_str().is_some_and(|f| !f.is_empty()),
                    "[{}] a nested item's folder must not be the root",
                    row.id
                );
                folder_rows += 1;
            }
        }
        n += 1;
    }
    assert!(n >= 31, "expected the full corpus, saw {n} rows");
    assert!(
        extractions >= 72,
        "expected the P4.D255 + P4.D279 extraction rows, saw {extractions}"
    );
    assert!(
        folder_rows >= 4,
        "expected the nested-folder parse rows, saw {folder_rows}"
    );
    assert!(
        nested_docpaths >= 10,
        "expected the recursive docpath rows, saw {nested_docpaths}"
    );
    assert!(
        isdoc_rows >= 17,
        "expected the isdoc rows, saw {isdoc_rows}"
    );
    assert!(
        stableid_rows >= 14,
        "expected the stableid rows, saw {stableid_rows}"
    );
    eprintln!("OK: wardrobe item file parser matched oracle on {n} cases.");
}
