//! P4.d22 — **tier-1 EXACT** differential over v4's
//! `lib/backup/restore/mount-index-coercion.ts`, the module `c1507f47` added to
//! fix restore bug 1 (every `doc_mount_points` / `doc_mount_file_links` row
//! rejected on the way out of an archive).
//!
//! ## Why this family exists at all
//!
//! `system_restore_state` already proves the coercion's EFFECT on the committed
//! archives — but those archives are uniform. Every `enabled`, `allowEmbed`,
//! `allowCharacterRead` and `allowCharacterWrite` in them is `1`, and every
//! pattern column is well-formed JSON text. A port that ignored the integer
//! entirely and hard-coded `true`, or that took the four-extension default
//! whenever the column was text, would pass the state diff on all four archives.
//!
//! The arms that matter are precisely the ones no fixture reaches: INTEGER `0`
//! (a store the user disabled, a document they made read-only or withheld from
//! the embedder), the empty string, `null`, unparseable text, and text that
//! parses to something that is not an array. Getting any of those wrong
//! **silently loosens a policy the user set**, which is the worst shape of
//! restore bug: no error, no warning, just a permission the archive said `no` to
//! coming back `yes`.
//!
//! So the corpus is written rather than sampled: one row per storage shape,
//! driven through v4's own two exported functions, compared **whole-row and
//! exactly** — which also pins the pass-through columns and v4's key order
//! (its `{...row, a, b, c}` keeps an existing key in place and appends a new
//! one; `serde_json`'s `preserve_order` does the same, so the orders must
//! match).
//!
//! Generate the oracle (see `harness/oracle/cases/backup-mount-index-coercion.test.ts`), then:
//!   QT_ORACLE_MOUNT_INDEX_COERCION=/tmp/oracle-mount-index-coercion.ndjson \
//!     cargo test -p quilltap-harness --test backup_mount_index_coercion_equivalence -- --nocapture

use quilltap_core::services::backup::restore::mount_index_coercion::{
    coerce_doc_mount_file_link_row, coerce_doc_mount_point_row,
};
use serde_json::Value;

/// Both storage shapes for both row kinds, plus the fallback arms. A hand-
/// written count, deliberately: a truncated oracle must not pass silently
/// (`harness-corpus-shape-constants-rot`).
const EXPECTED_CASES: usize = 20;

/// The two arms no committed archive can reach. If the corpus ever loses them,
/// this family stops testing the thing it was written for — so name them.
const MUST_COVER: &[&str] = &[
    "mp_enabled_integer_zero",
    "link_flags_all_zero",
    "link_flags_mixed",
    "mp_patterns_empty_json_text",
    "mp_patterns_unparseable",
];

#[test]
fn mount_index_coercion_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_MOUNT_INDEX_COERCION") else {
        eprintln!("SKIP: set QT_ORACLE_MOUNT_INDEX_COERCION (see the test header).");
        return;
    };
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));
    let cases: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
        .collect();

    let mut failures: Vec<String> = Vec::new();
    let mut names: Vec<&str> = Vec::new();

    for case in &cases {
        let name = case["name"].as_str().expect("case has a name");
        names.push(name);
        let input = &case["input"];
        let want = &case["output"];
        let got = match case["kind"].as_str() {
            Some("mountPoint") => coerce_doc_mount_point_row(input),
            Some("fileLink") => coerce_doc_mount_file_link_row(input),
            other => {
                failures.push(format!("{name}: unknown oracle kind {other:?}"));
                continue;
            }
        };
        if got != *want {
            failures.push(format!(
                "{name}: coerced row differs\n  rust:   {got}\n  oracle: {want}"
            ));
            continue;
        }
        // Key ORDER too — `got != want` compares an IndexMap by contents, not
        // by order, so serialize and compare the text as well.
        let (gs, ws) = (got.to_string(), want.to_string());
        if gs != ws {
            failures.push(format!(
                "{name}: same values, different key ORDER\n  rust:   {gs}\n  oracle: {ws}"
            ));
            continue;
        }
        println!("OK {name}");
    }

    assert!(
        failures.is_empty(),
        "{} coercion difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        cases.len(),
        EXPECTED_CASES,
        "oracle carries {} cases, expected {EXPECTED_CASES} — a truncated corpus must not pass \
         silently; if the corpus grew on purpose, move the constant",
        cases.len()
    );
    for must in MUST_COVER {
        assert!(
            names.contains(must),
            "the corpus lost `{must}` — that arm is unreachable from every committed archive, \
             so nothing else in the tree covers it"
        );
    }
    println!(
        "OK mount_index_coercion: {} cases, whole-row exact, key order included",
        cases.len()
    );
}
