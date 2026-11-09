//! Tier-1 + tier-3 differential (P4.D367): the ONE image transport budget —
//! `quilltap_core::services::image_transport_budget` vs v4's REAL
//! `lib/chat/image-transport-budget.ts` (`29c1e2690`, at the `624d0d25c` pin;
//! `d439f6dc9`'s per-request cap spent as its count).
//!
//! Both sides read ONE committed corpus, `harness/oracle/fixtures/
//! image-transport-budget.json` (no DB): an attachment's `$dataLen: n` is
//! expanded IN PLACE to `data: "A" x n` on both sides.
//!
//! * **`plan` (tier 1, exact).** v4's `planImageTransport` under each row's
//!   explicit policy (a tsx leaf) vs [`plan_image_transport`] under
//!   [`ImageTransportPolicy::explicit`]: the POSITIONAL verdict grid (v4's
//!   object-identity `Map` projected per message per attachment — the module
//!   note says why positions are identities) and the ranked walk's order. The
//!   leaf throws on a row that misses the verdict it was written for.
//! * **`apply` (tier 3, scripted describer).** v4's `applyImageTransportBudget`
//!   with its two describers doMocked to the row's `scripts` (a `{throw}`
//!   script THROWS; here a scripted [`TransportDescriber`] answers `Err`) and
//!   everything else real — `imageTransportPolicyFor` (so
//!   `resolveMaxImagesPerRequest` and `profileCanReceiveAttachment` are v4's own
//!   and v5's are C2's resolver and `profile_can_receive_attachment`), the
//!   prefix formatter, the logger. Per row: `same` (v4's `result === messages`
//!   ⇔ v5's `None`); the rebuilt messages; the report, BYTE-compared
//!   (`JSON.stringify` vs `serde_json::to_string` — key order is a comparand);
//!   the describer calls in order (each handed attachment tag-free); and every
//!   `[ImageTransport]` line, level + message + every key in order (v4's
//!   `report` object rendered as `reportJson`, §R.5), INCLUDING the silence
//!   legs — a nothing-to-do row logs NOTHING on both sides, a capability-only
//!   row no INFO.
//!   - Messages run through the failover's `StreamMessage`, whose absent-vs-
//!     empty `attachments` narrowing (recorded on its impl) is collapsed on
//!     BOTH sides (`[]` → `null`), as (E) of `file_attachment_tier3` does; rows
//!     marked `formatted` ALSO run through the primary path's `FormattedMsg`,
//!     compared EXACTLY (v4's deleted key ⇔ `None`, a kept `[]` ⇔ `Some([])`).
//!   - A string `data` is projected to `"<n chars>"` on both sides (UTF-16
//!     units): pass-through bytes, kept out of a megabyte NDJSON line.
//!   - The lines are captured with the THREAD-scoped rig: the pass runs on the
//!     caller's thread (a current-thread runtime under the capture), never in a
//!     `Db::write` closure.
//!
//! Generate the oracles (Node 24, from a v4 checkout pinned at `624d0d25c`;
//! STAGE the cases outside any `.claude/` path — v4's jest ignores it):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; TMPO=/tmp/p4d367/itb-oracle-run
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" /tmp/p4d367
//!   cp "$V5W/harness/oracle/cases/image-transport-budget.test.ts" "$V5W/harness/oracle/cases/image-transport-budget-plan.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/image-transport-budget.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   $N/npx tsx "$TMPO/cases/image-transport-budget-plan.ts" > /tmp/p4d367/oracle-itb-plan.ndjson
//!   QT_ORACLE_OUT=/tmp/p4d367/oracle-itb.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- image-transport-budget.test
//! Run:
//!   QT_ORACLE_ITB_PLAN=/tmp/p4d367/oracle-itb-plan.ndjson QT_ORACLE_ITB=/tmp/p4d367/oracle-itb.ndjson \
//!     cargo test -p quilltap-harness --test image_transport_budget_equivalence -- --nocapture

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use quilltap_core::model::stream::StreamMessage;
use quilltap_core::services::file_fallback::{
    FallbackFile, FallbackResult, FallbackType, ProcessingMetadata,
};
use quilltap_core::services::image_transport_budget::{
    apply_image_transport_budget, plan_image_transport, ranked_image_positions,
    ImageTransportPolicy, OverflowReason, TransportDescriber, TransportLogContext,
    TransportVerdict,
};
use quilltap_core::services::message_context::FormattedMsg;
use serde_json::{json, Map, Value};

const LOG_TARGET: &str = "quilltap_core::services::image_transport_budget";

fn corpus() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../harness/oracle/fixtures/image-transport-budget.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("read the corpus"))
        .expect("parse the corpus")
}

/// `$dataLen: n` → `data: "A" x n`, in place.
fn expand_data(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(expand_data).collect()),
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, val) in m {
                if k == "$dataLen" {
                    out.insert(
                        "data".into(),
                        Value::String("A".repeat(val.as_u64().unwrap() as usize)),
                    );
                } else {
                    out.insert(k.clone(), expand_data(val));
                }
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn oracle_rows(env: &str) -> Option<HashMap<String, Value>> {
    let path = match std::env::var(env) {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!("SKIP: {env} not set");
            return None;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut rows = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("oracle line");
        rows.insert(v["label"].as_str().unwrap().to_string(), v);
    }
    assert!(!rows.is_empty(), "{path} is empty");
    Some(rows)
}

fn stream_messages(messages: &Value) -> Vec<StreamMessage> {
    messages
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let content = m["content"].as_str().unwrap_or("").to_string();
            match m["role"].as_str().unwrap() {
                "system" => StreamMessage::system(content),
                "assistant" => StreamMessage::assistant(content),
                "user" => StreamMessage::User {
                    content,
                    cache_control: None,
                    attachments: m
                        .get("attachments")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                    name: m.get("name").and_then(Value::as_str).map(str::to_string),
                },
                r => panic!("unexpected role {r}"),
            }
        })
        .collect()
}

fn formatted_messages(messages: &Value) -> Vec<FormattedMsg> {
    messages
        .as_array()
        .unwrap()
        .iter()
        .map(|m| FormattedMsg {
            role: m["role"].as_str().unwrap().to_string(),
            content: m["content"].as_str().unwrap_or("").to_string(),
            name: m.get("name").and_then(Value::as_str).map(str::to_string),
            thought_signature: None,
            attachments: m.get("attachments").and_then(Value::as_array).cloned(),
        })
        .collect()
}

fn verdict_json(v: &Option<TransportVerdict>) -> Value {
    match v {
        None => Value::Null,
        Some(TransportVerdict::Keep) => json!({"kind": "keep"}),
        Some(TransportVerdict::Capability) => json!({"kind": "capability"}),
        Some(TransportVerdict::Overflow(OverflowReason::Count)) => {
            json!({"kind": "overflow", "reason": "count"})
        }
        Some(TransportVerdict::Overflow(OverflowReason::Bytes)) => {
            json!({"kind": "overflow", "reason": "bytes"})
        }
    }
}

fn explicit_policy(p: &Value) -> ImageTransportPolicy {
    let receivable: HashSet<String> = p["canReceive"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .collect();
    ImageTransportPolicy::explicit(
        p["maxImages"].as_i64().unwrap(),
        p["maxTotalBase64"].as_u64().unwrap() as usize,
        move |m| receivable.contains(m),
    )
}

#[test]
fn plan_matches_oracle() {
    let Some(oracle) = oracle_rows("QT_ORACLE_ITB_PLAN") else {
        return;
    };
    let corpus = corpus();
    let mut ran = 0;
    for row in corpus["plan"].as_array().unwrap() {
        let label = row["label"].as_str().unwrap();
        let messages = stream_messages(&expand_data(&row["messages"]));
        let policy = explicit_policy(&row["policy"]);
        let grid = plan_image_transport(&messages, &policy);
        let got = json!({
            "kind": "plan",
            "label": label,
            "grid": grid
                .iter()
                .map(|m| m.iter().map(verdict_json).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            "ranked": ranked_image_positions(&messages, &policy)
                .into_iter()
                .map(|(mi, ai)| json!([mi, ai]))
                .collect::<Vec<_>>(),
        });
        let want = oracle
            .get(label)
            .unwrap_or_else(|| panic!("oracle missing plan row {label}"));
        assert_eq!(
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(want).unwrap(),
            "plan row {label} diverged ({})",
            row["rule"]
        );
        ran += 1;
    }
    assert_eq!(ran, oracle.len(), "every oracle row is a corpus row");
    eprintln!("plan: {ran} rows equal");
}

// ===========================================================================
// apply
// ===========================================================================

/// The scripted describer — v4's doMock, script for script.
struct ScriptedDescriber {
    scripts: Value,
    calls: Mutex<Vec<Value>>,
}

impl ScriptedDescriber {
    fn answer(&self, which: &str, file: &FallbackFile) -> Result<FallbackResult, String> {
        self.calls.lock().unwrap().push(json!({
            "fn": which,
            "id": file.id,
            // v5's `FallbackFile` has no slot for a tag at all; v4 records
            // `'transport' in fileAttachment`.
            "hasTransport": false,
        }));
        let script = self.scripts.get(&file.id).and_then(|s| s.get(which));
        let default =
            json!({"type": "image_description", "text": format!("A description of {}.", file.id)});
        let s = script.unwrap_or(&default);
        if let Some(t) = s.get("throw").and_then(Value::as_str) {
            return Err(t.to_string());
        }
        let meta = ProcessingMetadata {
            original_filename: file.filename.clone(),
            original_mime_type: file.mime_type.clone(),
            ..Default::default()
        };
        let text = s.get("text").and_then(Value::as_str).map(str::to_string);
        Ok(match s["type"].as_str().unwrap() {
            "image_description" => FallbackResult {
                type_: FallbackType::ImageDescription,
                text_content: None,
                image_description: text,
                processing_metadata: Some(meta),
                error: None,
            },
            "text" => FallbackResult {
                type_: FallbackType::Text,
                text_content: text,
                image_description: None,
                processing_metadata: Some(meta),
                error: None,
            },
            _ => FallbackResult {
                type_: FallbackType::Unsupported,
                text_content: None,
                image_description: None,
                processing_metadata: Some(meta),
                error: s.get("error").and_then(Value::as_str).map(str::to_string),
            },
        })
    }
}

impl TransportDescriber for ScriptedDescriber {
    async fn fallback(
        &self,
        file: &FallbackFile,
        _profile: &Value,
    ) -> Result<FallbackResult, String> {
        self.answer("fallback", file)
    }
    async fn describe(&self, file: &FallbackFile) -> Result<FallbackResult, String> {
        self.answer("describe", file)
    }
}

fn project_attachments(atts: &[Value]) -> Value {
    Value::Array(
        atts.iter()
            .map(|a| match a {
                Value::Object(m) => {
                    let mut o = Map::new();
                    for (k, v) in m {
                        match (k.as_str(), v) {
                            ("data", Value::String(s)) => {
                                o.insert(
                                    k.clone(),
                                    json!(format!("<{} chars>", s.encode_utf16().count())),
                                );
                            }
                            _ => {
                                o.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    Value::Object(o)
                }
                other => other.clone(),
            })
            .collect(),
    )
}

fn project_stream(ms: &[StreamMessage]) -> Value {
    Value::Array(
        ms.iter()
            .map(|m| {
                let mut o = Map::new();
                o.insert("role".into(), json!(m.role_str()));
                o.insert("content".into(), json!(m.content()));
                if let StreamMessage::User { name: Some(n), .. } = m {
                    o.insert("name".into(), json!(n));
                }
                let att = m.attachments();
                o.insert(
                    "attachments".into(),
                    if att.is_empty() {
                        Value::Null
                    } else {
                        project_attachments(att)
                    },
                );
                Value::Object(o)
            })
            .collect(),
    )
}

fn project_formatted(ms: &[FormattedMsg]) -> Value {
    Value::Array(
        ms.iter()
            .map(|m| {
                let mut o = Map::new();
                o.insert("role".into(), json!(m.role));
                o.insert("content".into(), json!(m.content));
                if let Some(n) = &m.name {
                    o.insert("name".into(), json!(n));
                }
                o.insert(
                    "attachments".into(),
                    match &m.attachments {
                        None => Value::Null,
                        Some(a) => project_attachments(a),
                    },
                );
                Value::Object(o)
            })
            .collect(),
    )
}

/// v4's RAW projection with the `StreamMessage` narrowing applied: `[]` → null.
fn collapse_empty(v: &Value) -> Value {
    let mut v = v.clone();
    for m in v.as_array_mut().unwrap() {
        if m["attachments"].as_array().is_some_and(Vec::is_empty) {
            m["attachments"] = Value::Null;
        }
    }
    v
}

/// v4 `{level, message, fields}` → the capture rig's rendering.
fn render_v4_line(line: &Value) -> String {
    let level = line["level"].as_str().unwrap().to_uppercase();
    let mut out = format!("{level} {LOG_TARGET} {}", line["message"].as_str().unwrap());
    if let Some(fields) = line["fields"].as_object() {
        for (k, v) in fields {
            match v {
                Value::String(s) => out.push_str(&format!(" {k}={s}")),
                Value::Object(_) | Value::Array(_) => {
                    out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
                }
                other => out.push_str(&format!(" {k}={other}")),
            }
        }
    }
    out
}

fn log_context(v: &Value) -> TransportLogContext {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    TransportLogContext {
        chat_id: s("chatId"),
        character_participant_id: s("characterParticipantId"),
        kind: s("kind"),
    }
}

fn current_thread() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

#[test]
fn apply_matches_oracle() {
    let Some(oracle) = oracle_rows("QT_ORACLE_ITB") else {
        return;
    };
    let corpus = corpus();
    let mut ran = 0;
    let mut formatted_ran = 0;
    for row in corpus["apply"].as_array().unwrap() {
        let label = row["label"].as_str().unwrap();
        let want = oracle
            .get(label)
            .unwrap_or_else(|| panic!("oracle missing apply row {label}"));
        let messages_json = expand_data(&row["messages"]);
        let profile = &row["profile"];
        let ctx = log_context(&row["logContext"]);

        // --- StreamMessage (the failover's shape) ---
        let messages = stream_messages(&messages_json);
        let describer = ScriptedDescriber {
            scripts: row["scripts"].clone(),
            calls: Mutex::new(Vec::new()),
        };
        let policy = row.get("policy").map(explicit_policy);
        let ((adapted, report), lines) = quilltap_core::test_support::captured_with(|| {
            current_thread().block_on(apply_image_transport_budget(
                &messages, profile, &describer, &ctx, policy,
            ))
        });
        let got_lines: Vec<String> = lines
            .iter()
            .filter(|l| l.contains("[ImageTransport]"))
            .cloned()
            .collect();
        let want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4_line)
            .collect();
        assert_eq!(got_lines, want_lines, "apply row {label}: the lines");
        assert_eq!(
            json!(adapted.is_none()),
            want["same"],
            "apply row {label}: same-reference"
        );
        assert_eq!(
            serde_json::to_string(&report).unwrap(),
            want["report"].as_str().unwrap(),
            "apply row {label}: the report"
        );
        assert_eq!(
            serde_json::to_string(&Value::Array(describer.calls.lock().unwrap().clone())).unwrap(),
            serde_json::to_string(&want["calls"]).unwrap(),
            "apply row {label}: the describer calls"
        );
        assert_eq!(
            serde_json::to_string(&project_stream(adapted.as_deref().unwrap_or(&messages)))
                .unwrap(),
            serde_json::to_string(&collapse_empty(&want["messages"])).unwrap(),
            "apply row {label}: the messages (StreamMessage)"
        );

        // --- FormattedMsg (the primary path's shape), EXACT ---
        if row["formatted"].as_bool() == Some(true) {
            let messages = formatted_messages(&messages_json);
            let describer = ScriptedDescriber {
                scripts: row["scripts"].clone(),
                calls: Mutex::new(Vec::new()),
            };
            let policy = row.get("policy").map(explicit_policy);
            let (adapted, _report) = current_thread().block_on(apply_image_transport_budget(
                &messages, profile, &describer, &ctx, policy,
            ));
            assert_eq!(
                json!(adapted.is_none()),
                want["same"],
                "apply row {label}: same (FormattedMsg)"
            );
            assert_eq!(
                serde_json::to_string(&project_formatted(adapted.as_deref().unwrap_or(&messages)))
                    .unwrap(),
                serde_json::to_string(&want["messages"]).unwrap(),
                "apply row {label}: the messages (FormattedMsg, exact)"
            );
            formatted_ran += 1;
        }
        ran += 1;
    }
    assert_eq!(ran, oracle.len(), "every oracle row is a corpus row");
    assert!(
        formatted_ran >= 5,
        "the FormattedMsg leg ran {formatted_ran} rows"
    );
    eprintln!("apply: {ran} rows equal ({formatted_ran} also as FormattedMsg)");
}
