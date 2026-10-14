//! P4.D288 unit 2 — tier-2 differential: the memory settings façade
//! (`memory_settings::resolve_memory_settings`, contract C1-5 — the ONE home
//! P4.D296 / P4.D297 consume) against v4's REAL `resolveMemorySettings`
//! (`lib/memory/settings.ts:99-130`) at the `83334b9a5` pin, over the same
//! planted `instance_settings` rows.
//!
//! Comparands per case: the resolved knobs (`extraction.limits` / `.mode`, the
//! stored recall settings + `minCosine`, the maintenance settings with
//! `phases` defaulted on when the stored value has none, `consolidation`,
//! `coldRetentionDays`, and `capFor` over a probe list); the DEBUG
//! `[MemorySettings] Resolved memory settings` line's BYTES (field order,
//! `userId` `null` when absent, `capOverrides` the COUNT); each getter's WARN
//! by message + `error` presence (the text is V8's / Zod's, not serde's — the
//! `instance_settings_json_warns` rule), in v4's order.
//!
//! Regenerate + run (a tsx oracle over its own scratch DB):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-memory-settings-resolve.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/memory-settings-resolve.ts \
//!     > /tmp/oracle-memory-settings-resolve.ndjson
//!   cd $V5W
//!   QT_ORACLE_MEMORY_SETTINGS_RESOLVE=/tmp/oracle-memory-settings-resolve.ndjson \
//!     cargo test -p quilltap-harness --test memory_settings_resolve_equivalence -- --nocapture

use quilltap_core::memory_settings::resolve_memory_settings;
use serde_json::{json, Map, Value};

fn conn() -> rusqlite::Connection {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch(
        "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL)",
    )
    .unwrap();
    c
}

/// A captured `<LEVEL> <target> <message> k=v …` line as `(level, rest)`.
fn split(line: &str) -> (String, String) {
    let mut it = line.splitn(3, ' ');
    let level = it.next().unwrap().to_string();
    let _target = it.next();
    (level, it.next().unwrap_or("").to_string())
}

fn v4_line(l: &Value) -> (String, String) {
    let mut rest = l["message"].as_str().unwrap().to_string();
    for pair in l["fields"].as_array().unwrap() {
        let v = match &pair[1] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        rest.push_str(&format!(" {}={v}", pair[0].as_str().unwrap()));
    }
    (l["level"].as_str().unwrap().to_ascii_uppercase(), rest)
}

/// A WARN keeps its message and whether it carried an `error`; a DEBUG stays whole.
fn comparable((level, rest): (String, String)) -> (String, String) {
    if level == "WARN" {
        match rest.find(" error=") {
            Some(i) => (level, format!("{} error=<present>", &rest[..i])),
            None => (level, rest),
        }
    } else {
        (level, rest)
    }
}

#[test]
fn memory_settings_resolve_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_MEMORY_SETTINGS_RESOLVE") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_SETTINGS_RESOLVE to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("oracle");
    let mut failures = Vec::new();
    let mut cases = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line");
        cases += 1;
        let name = row["name"].as_str().unwrap();
        let c = conn();
        for (key, value) in row["rows"].as_object().unwrap() {
            c.execute(
                "INSERT INTO instance_settings (key, value) VALUES (?1, ?2)",
                [key.as_str(), value.as_str().unwrap()],
            )
            .unwrap();
        }
        let probes: Vec<String> = row["resolved"]["maintenance"]["capFor"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let (s, lines) = quilltap_core::test_support::captured_with(|| {
            resolve_memory_settings(&c, row["userId"].as_str())
        });
        let mut cap_for = Map::new();
        for id in &probes {
            cap_for.insert(id.clone(), s.maintenance.cap_for(id).into());
        }
        let got = json!({
            "extraction": { "limits": s.extraction.limits.to_json(), "mode": s.extraction.mode.to_json() },
            "recall": {
                "scopePolicy": s.recall.settings.scope_policy,
                "expandRelated": s.recall.settings.expand_related,
                "perTurnConversationSummaries": s.recall.settings.per_turn_conversation_summaries,
                "minCosine": { "neural": s.recall.min_cosine.neural, "tfidf": s.recall.min_cosine.tfidf },
            },
            "maintenance": {
                "settings": s.maintenance.settings.to_json(),
                "consolidation": s.maintenance.consolidation.to_json(),
                "coldRetentionDays": s.maintenance.cold_retention_days,
                "capFor": cap_for,
            },
        });
        let (g, w) = (
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&row["resolved"]).unwrap(),
        );
        if g != w {
            failures.push(format!("{name}: resolved\n  v5 {g}\n  v4 {w}"));
        }
        let got_lines: Vec<(String, String)> = lines
            .iter()
            .map(|l| comparable(split(l)))
            .filter(|(level, _)| level == "WARN" || level == "DEBUG")
            .collect();
        let want_lines: Vec<(String, String)> = row["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| comparable(v4_line(l)))
            .collect();
        if got_lines != want_lines {
            failures.push(format!(
                "{name}: lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(cases, 3, "the three cases");
    eprintln!("OK: resolve_memory_settings matched v4 over {cases} cases.");
}
