//! Tier-1 differential: wardrobe pronoun guidance (v4 `eb90d9ed8` —
//! `lib/characters/pronoun-presets.ts` + `lib/wardrobe/pronoun-guidance.ts`).
//!
//! Exact equality against v4's REAL modules over one corpus: the presets and
//! labels, `normalizePronounGuidance` / `formatPronounGuidance` over raw inputs
//! (a list in any order, a single string, a preset label, junk, non-strings,
//! `undefined`), `standardPronounSubjectOf` + `fitsPronounGuidance` over every
//! (guidance, pronouns) pair (case, padding, partial sets, custom, unset), and
//! `filterByPronounGuidance` over a fixed item list with and without `keepIds`.
//!
//! Generate the oracle output:
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/pronoun-guidance.ts \
//!     > /tmp/oracle-pronoun-guidance.ndjson
//! Run:
//!   QT_ORACLE_PRONOUN_GUIDANCE=/tmp/oracle-pronoun-guidance.ndjson \
//!     cargo test -p quilltap-harness --test pronoun_guidance_equivalence

use std::collections::HashSet;

use quilltap_core::characters::pronoun_presets::{
    standard_pronoun_label, standard_pronoun_subject_of, STANDARD_PRONOUNS,
    STANDARD_PRONOUN_SUBJECTS,
};
use quilltap_core::wardrobe::pronoun_guidance::{
    filter_by_pronoun_guidance, fits_pronoun_guidance, format_pronoun_guidance,
    has_no_pronoun_guidance, normalize_pronoun_guidance,
};
use serde_json::{json, Value};

fn triple(p: &Value) -> Option<(&str, &str, &str)> {
    let o = p.as_object()?;
    Some((
        o.get("subject")?.as_str()?,
        o.get("object")?.as_str()?,
        o.get("possessive")?.as_str()?,
    ))
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn pronoun_guidance_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_PRONOUN_GUIDANCE") else {
        eprintln!("SKIP: QT_ORACLE_PRONOUN_GUIDANCE not set");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let mut rows = 0;
    let mut failures = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle row");
        let id = row["id"].as_str().unwrap().to_string();
        let got: Value = match row["kind"].as_str().unwrap() {
            "presets" => json!({
                "presets": (STANDARD_PRONOUNS.iter().map(|p| json!({
                    "label": p.label, "subject": p.subject,
                    "object": p.object, "possessive": p.possessive,
                })).collect::<Vec<_>>()),
                "subjects": STANDARD_PRONOUN_SUBJECTS,
                "labels": (["he", "she", "they", "it", "xe", ""]
                    .iter().map(|s| standard_pronoun_label(s)).collect::<Vec<_>>()),
            }),
            "normalize" => {
                let raw = (!row["absent"].as_bool().unwrap()).then(|| row["raw"].clone());
                // v4 `formatPronounGuidance(raw ?? [])` normalises first, so the
                // twin is format(normalize(raw)) — the same input set.
                let normalized = normalize_pronoun_guidance(raw.as_ref());
                json!({
                    "normalize": normalized,
                    "format": format_pronoun_guidance(&normalized),
                })
            }
            "subjectOf" => {
                json!({ "subjectOf": standard_pronoun_subject_of(triple(&row["pronouns"])) })
            }
            "fits" => {
                let g = strings(&row["guidance"]);
                json!({
                    "hasNone": has_no_pronoun_guidance(&g),
                    "fits": fits_pronoun_guidance(&g, triple(&row["pronouns"])),
                })
            }
            "filter" => {
                let items: Vec<(String, Vec<String>)> = vec![
                    ("gown".into(), vec!["she".into()]),
                    ("suit".into(), vec!["he".into()]),
                    ("boots".into(), vec![]),
                    ("cloak".into(), vec!["they".into(), "it".into()]),
                ];
                let keep: Option<HashSet<String>> = row["keepIds"]
                    .as_array()
                    .map(|a| strings(&Value::Array(a.clone())).into_iter().collect());
                let (kept, hidden) = filter_by_pronoun_guidance(
                    &items,
                    triple(&row["pronouns"]),
                    keep.as_ref(),
                    |i| i.0.as_str(),
                    |i| i.1.as_slice(),
                );
                json!({
                    "kept": kept.iter().map(|i| i.0.clone()).collect::<Vec<_>>(),
                    "hiddenCount": hidden,
                })
            }
            other => panic!("unknown kind {other}"),
        };
        let want: Value = got
            .as_object()
            .unwrap()
            .keys()
            .map(|k| (k.clone(), row[k].clone()))
            .collect::<serde_json::Map<_, _>>()
            .into();
        if serde_json::to_string(&got).unwrap() != serde_json::to_string(&want).unwrap() {
            failures.push(format!("{id}: got {got} want {want}"));
        }
        rows += 1;
    }
    assert!(rows >= 100, "oracle too small: {rows} rows");
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("OK: pronoun guidance exact for {rows} rows.");
}
