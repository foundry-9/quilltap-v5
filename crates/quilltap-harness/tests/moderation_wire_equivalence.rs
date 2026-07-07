//! Differential (W4.7f): the OpenAI moderation wire
//! (`quilltap_core::services::dangerous_content::moderation_wire`) vs v4's REAL
//! `moderationPlugin.moderate`. Diffs `build_moderation_request` (method / url /
//! body) + `parse_moderation_response` (the full `{flagged, categories:[{category,
//! flagged, score}]}` OR the exact thrown error string) over committed wire
//! payloads (flagged / clean / missing-score / empty-results / HTTP error /
//! base-url override).
//!
//! The fixture is committed (no env var); regenerate by driving
//! `record-moderation-wire.mjs` from the openai plugin dir.

use std::path::{Path, PathBuf};

use quilltap_core::model::wire::WireResponse;
use quilltap_core::services::dangerous_content::moderation_wire::{
    build_moderation_request, parse_moderation_response,
};
use serde_json::Value;

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/moderation-wire/moderation-wire.recorded.ndjson")
}

#[test]
fn moderation_wire_matches_v4() {
    let text = std::fs::read_to_string(corpus_path()).expect("committed moderation-wire NDJSON");
    let mut rows = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line).unwrap();
        let case = row["case"].as_str().unwrap();
        let content = row["content"].as_str().unwrap();
        let base_url = row["baseUrl"].as_str();
        rows += 1;

        // 1. Request.
        let req = build_moderation_request(content, base_url);
        let want = &row["request"];
        assert_eq!(
            req.method,
            want["method"].as_str().unwrap(),
            "{case} method"
        );
        assert_eq!(req.url, want["url"].as_str().unwrap(), "{case} url");
        assert_eq!(
            req.body_string(),
            want["body"].as_str().unwrap(),
            "{case} body"
        );

        // 2. Parse.
        let wire = &row["wire"];
        let resp = WireResponse::new(
            wire["status"].as_u64().unwrap() as u16,
            wire["body"].as_str().unwrap().to_string(),
        );
        let parsed = parse_moderation_response(&resp);
        match row["outcome"].as_str().unwrap() {
            "ok" => {
                let got = parsed.unwrap_or_else(|e| panic!("{case}: expected ok, got err {e}"));
                let want = &row["result"];
                assert_eq!(
                    got.flagged,
                    want["flagged"].as_bool().unwrap(),
                    "{case} flagged"
                );
                let want_cats = want["categories"].as_array().unwrap();
                assert_eq!(got.categories.len(), want_cats.len(), "{case} cat count");
                for (g, w) in got.categories.iter().zip(want_cats) {
                    assert_eq!(
                        g.category,
                        w["category"].as_str().unwrap(),
                        "{case} category"
                    );
                    assert_eq!(
                        g.flagged,
                        w["flagged"].as_bool().unwrap(),
                        "{case} cat flagged"
                    );
                    let ws = w["score"].as_f64().unwrap();
                    assert!(
                        (g.score - ws).abs() < 1e-12,
                        "{case} score {} vs {}",
                        g.score,
                        ws
                    );
                }
            }
            "thrown" => {
                let err = parsed.expect_err(&format!("{case}: expected thrown"));
                assert_eq!(err, row["thrown"].as_str().unwrap(), "{case} thrown");
            }
            other => panic!("{case}: unknown outcome {other}"),
        }
    }
    assert!(rows >= 6, "expected the full moderation corpus, got {rows}");
}
