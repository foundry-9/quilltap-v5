//! Tier-1 (exact) differential: `services::impersonation_voice_legacy` vs
//! v4's REAL `lib/chat/impersonation-voice-legacy.ts` (NEW at `07b8f0209`,
//! P4.D251) — the retired `impersonationVoiceRewrite` boolean translated into
//! `impersonationVoiceMode`.
//!
//! Two ops over the oracle's fixed corpus:
//!   - `fromLegacy`: the scalar rule (`true`/`1` → `'ask'`, else `'off'`),
//!     incl. `1.0` (one JS number), `2`, and the strings `'1'` / `'true'`.
//!   - `withLegacy`: the record rule — `same` (v4's reference identity) is
//!     compared as the `Cow` variant; the output record as JSON WITH key order
//!     (v4's `{...rest, impersonationVoiceMode}` keeps an existing mode in its
//!     slot and appends a new one last). An `undefined` input value is spelled
//!     `{"undefined": true}` by the oracle; here it becomes a present `null`
//!     (JSON's nearest) — both translate to `'off'`, and key PRESENCE is what
//!     the record rule reads.
//!
//! Generate (Node 24, from the v4 checkout or a pinned worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/impersonation-voice-legacy.ts \
//!     > /tmp/oracle-impersonation-voice-legacy.ndjson
//! Run:
//!   QT_ORACLE_IMPERSONATION_VOICE_LEGACY=/tmp/oracle-impersonation-voice-legacy.ndjson \
//!     cargo test -p quilltap-harness --test impersonation_voice_legacy_equivalence -- --nocapture

use std::borrow::Cow;

use quilltap_core::services::impersonation_voice_legacy::{
    impersonation_voice_mode_from_legacy, with_impersonation_voice_mode_from_legacy,
};
use serde_json::Value;

/// The oracle's `{"undefined": true}` sentinel → JSON's nearest, `null`.
fn unspell(v: &Value) -> Value {
    match v {
        Value::Object(o) if o.len() == 1 && o.get("undefined") == Some(&Value::Bool(true)) => {
            Value::Null
        }
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), unspell(v))).collect()),
        other => other.clone(),
    }
}

#[test]
fn legacy_translation_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_IMPERSONATION_VOICE_LEGACY") else {
        eprintln!("SKIP: set QT_ORACLE_IMPERSONATION_VOICE_LEGACY (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap();
    let (mut scalars, mut records) = (Vec::new(), Vec::new());
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let label = row["label"].as_str().unwrap();
        match row["op"].as_str().unwrap() {
            "fromLegacy" => {
                // `JSON.stringify(1.0)` is `1`, which serde reads as an INTEGER —
                // so the oracle's `one_point_zero` row would re-drive the `one`
                // arm. Feed the FLOAT variant v4's `=== 1` must also accept (one
                // JS number type), after checking the oracle saw the number 1.
                let value = if label == "one_point_zero" {
                    assert_eq!(row["value"].as_f64(), Some(1.0), "[fromLegacy {label}]");
                    Value::from(1.0_f64)
                } else {
                    unspell(&row["value"])
                };
                let got = impersonation_voice_mode_from_legacy(&value);
                assert_eq!(
                    got.as_str(),
                    row["result"].as_str().unwrap(),
                    "[fromLegacy {label}]"
                );
                scalars.push(label.to_string());
            }
            "withLegacy" => {
                let input = unspell(&row["input"]);
                let got = with_impersonation_voice_mode_from_legacy(&input);
                let same = matches!(got, Cow::Borrowed(_));
                assert_eq!(
                    same,
                    row["same"].as_bool().unwrap(),
                    "[withLegacy {label}] same reference"
                );
                assert_eq!(
                    serde_json::to_string(&*got).unwrap(),
                    serde_json::to_string(&row["result"]).unwrap(),
                    "[withLegacy {label}] output record (key order included)"
                );
                records.push(label.to_string());
            }
            other => panic!("unknown op {other}"),
        }
    }
    // The label SETS, not counts — a duplicated row cannot mask a missing one.
    assert_eq!(
        scalars,
        [
            "true",
            "one",
            "false",
            "zero",
            "null",
            "undefined",
            "one_point_zero",
            "two",
            "string_one",
            "string_true",
        ],
        "scalar rows — regenerate the oracle"
    );
    assert_eq!(
        records,
        [
            "no_key",
            "key_true_no_mode",
            "key_true_mode_always",
            "key_false_mode_null",
            "key_true_mode_null",
            "key_present_undefined",
            "key_one_mode_off",
        ],
        "record rows — regenerate the oracle"
    );
    eprintln!(
        "OK: {} scalar + {} record rows match v4.",
        scalars.len(),
        records.len()
    );
}
