//! Tier-1 differential: v4's per-provider memory embedding text at the pin
//! (`83334b9a5`) — `lib/memory/index-store/embedding-text.ts` +
//! `buildMemoryAnchorLine` (`lib/memory/episodic.ts`) against
//! `crate::memory_embedding_text` (P4.D289, C1-A item 7, LR-12).
//!
//! Every source row of the case is rendered in five explicit zones (`UTC`,
//! `America/Chicago`, `Asia/Kolkata`, `Pacific/Chatham`, `Asia/Tokyo`) through
//! the anchor line, the canonical (neural) text and the lexical (BUILTIN)
//! text; all three compare as exact strings. The corpus carries
//! `memory-anchor-zone.test.ts`'s Tokyo row, `builtin-lexical-text.test.ts`'s
//! rows, local midnights east and west of UTC that the pre-port UTC slice read
//! a day off, DST-edge instants (Chicago, Chatham), unparseable `occurredAt`s
//! falling to `slice(0, 10)` (matching and not), empty / whitespace / null
//! anchors, non-string / blank / five entities, padded `narrativeTime`, and
//! the unpadded-year stamps that lose their `when` part. Entities arrays ride
//! through `AnchorEntities::Json` (v4's `typeof e === 'string'` arm); an
//! all-string array is ALSO driven through `AnchorEntities::Strings` and must
//! agree.
//!
//! The case runs under `TZ=UTC` (the `ZONELESS_PARSE_AS_UTC` convention,
//! LR-10 — the `zoneless-datetime` row agrees only on that leg).
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/memory-embedding-text.ts" > /tmp/oracle-memory-embedding-text.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_MEMORY_EMBEDDING_TEXT=/tmp/oracle-memory-embedding-text.ndjson \
//!     cargo test -p quilltap-harness --test memory_embedding_text_equivalence -- --nocapture

use std::collections::BTreeMap;

use quilltap_core::memory_embedding_text::{
    embedding_text_for, lexical_embedding_text_for, memory_anchor_line, AnchorEntities,
    EmbeddingTextSource, LEXICAL_TEXT_SHAPE_VERSION,
};
use serde_json::Value;

/// The source rows of the case (each rendered in every zone).
const SOURCE_ROWS: usize = 51;
const ZONES: usize = 5;

// The legacy-`Date.parse` rows (`July 14, 2026`, `2026/07/14`, `2026-7-14`,
// `Tue Jul 14 2026`, `2026-07-14 12:00`, full-width digits, `+010000-…`) were
// pinned here as the P4.D289 finding `JS_DATE_PARSE_ISO_SUBSET` until v5
// ported V8's whole parser (`quilltap_core::v8_date_parse`, the human's ruling
// 2026-10-14); they now compare like every other row.

/// Compare the three renderings; every mismatch is collected (not panicked
/// on) so one run names every red row.
fn check(src: &EmbeddingTextSource<'_>, tz: &str, row: &Value, ctx: &str, bad: &mut Vec<String>) {
    let pairs = [
        ("anchor line", memory_anchor_line(src, tz), "anchorLine"),
        ("embeddingTextFor", embedding_text_for(src, tz), "embedding"),
        (
            "lexicalEmbeddingTextFor",
            lexical_embedding_text_for(src),
            "lexical",
        ),
    ];
    for (what, got, key) in pairs {
        let want = row[key].as_str().unwrap();
        if got != want {
            bad.push(format!("{ctx}: {what}: rust={got:?} oracle={want:?}"));
        }
    }
}

#[test]
fn memory_embedding_text_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MEMORY_EMBEDDING_TEXT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MEMORY_EMBEDDING_TEXT to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut strings_arm = 0usize;
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    let mut bad: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap().to_string();
        *counts.entry(kind.clone()).or_default() += 1;
        match kind.as_str() {
            "text" => {
                let id = row["id"].as_str().unwrap();
                let tz = row["tz"].as_str().unwrap();
                *ids.entry(id.to_string()).or_default() += 1;
                let ctx = format!("{id} @ {tz}");
                let s = &row["source"];
                let entities_json: Option<&Vec<Value>> =
                    s.get("entities").and_then(Value::as_array);
                let base = EmbeddingTextSource {
                    summary: s["summary"].as_str().unwrap(),
                    content: s["content"].as_str().unwrap(),
                    occurred_at: s.get("occurredAt").and_then(Value::as_str),
                    narrative_time: s.get("narrativeTime").and_then(Value::as_str),
                    entities: entities_json
                        .map_or(AnchorEntities::None, |v| AnchorEntities::Json(v.as_slice())),
                };
                check(&base, tz, &row, &ctx, &mut bad);

                // The typed rows' arm must agree wherever it can carry the array.
                if let Some(v) = entities_json {
                    if v.iter().all(Value::is_string) {
                        let owned: Vec<String> =
                            v.iter().map(|e| e.as_str().unwrap().to_string()).collect();
                        let typed = EmbeddingTextSource {
                            entities: (&owned).into(),
                            ..base
                        };
                        check(&typed, tz, &row, &format!("{ctx} [Strings]"), &mut bad);
                        strings_arm += 1;
                    }
                }
            }
            "constant" => {
                assert_eq!(
                    row["LEXICAL_TEXT_SHAPE_VERSION"].as_i64().unwrap(),
                    LEXICAL_TEXT_SHAPE_VERSION
                );
            }
            other => panic!("unknown row kind {other}"),
        }
    }

    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );

    // v4's own jest expectations, restated against the port.
    let tokyo = EmbeddingTextSource {
        summary: "Lighthouse picnic",
        content: "We ate sandwiches at the lighthouse.",
        occurred_at: Some("2026-07-13T15:00:00.000Z"),
        ..Default::default()
    };
    assert!(embedding_text_for(&tokyo, "Asia/Tokyo").contains("when: 2026-07-14"));
    let garden = EmbeddingTextSource {
        summary: "Garden",
        content: "We sat outside.\n(place: the garden)",
        ..Default::default()
    };
    assert_eq!(
        lexical_embedding_text_for(&garden),
        "Garden\n\nWe sat outside.\n(place: the garden)"
    );

    assert_eq!(
        ids.len(),
        SOURCE_ROWS,
        "source rows drifted from the corpus"
    );
    assert!(
        ids.values().all(|n| *n == ZONES),
        "every source renders in every zone: {ids:?}"
    );
    assert_eq!(
        counts.into_iter().collect::<Vec<_>>(),
        vec![
            ("constant".to_string(), 1),
            ("text".to_string(), SOURCE_ROWS * ZONES)
        ],
        "oracle row counts drifted from the corpus"
    );
    eprintln!(
        "OK: memory-embedding-text matched oracle ({} text rows, {strings_arm} also through the Strings arm).",
        SOURCE_ROWS * ZONES
    );
}
