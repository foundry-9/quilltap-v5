//! Tier-2 differential (P4.D345): operator intervention's four actions —
//! `GET ?action=interventions`, `POST ?action=intervention-preview |
//! intervention-reask | intervention-resolve` — against v4's REAL route module
//! (`interventions.ts`), registry, recompute and engine at the `236c04e35` pin.
//!
//! Each oracle row is one SCENE (v4's `custom-tools-interventions.test.ts`
//! `it`s, widened): holds are registered through the REAL registry with a
//! baseline the REAL engine deals from a fixed draw, then the actions run in
//! order and every step's `{status, body}` is compared as BYTES — the hold
//! view, the preview, the consult list, the refusal sentence. A `decision`
//! step compares what the waiting turn receives (the server's own recompute,
//! the re-ask count); `status` / `consults` steps read the registry. The
//! registry / route lines (`context` `pascal.intervention.registry` /
//! `api.v1.chats.custom-tools.interventions` / `pascal.custom-tools`) are
//! compared too, level + message + every field (an array on its `…Json` name,
//! §R.5).
//!
//! Minted hold ids are rewritten `<hold:N>` (registration order) and each
//! `createdAt` `<at:N>` — on BOTH sides. The model call is a scripted invoker
//! on both sides (v4: `buildCustomToolLlmInvoker` doubled); the prompts it was
//! posed are compared. The edge's own `The request body is not JSON` (a
//! `postRaw` step) is the web route's — asserted here against the constant the
//! edge answers, and driven for real by `quilltap-web`'s route test.
//!
//! Generate the oracle: the recipe is in
//! `harness/oracle/cases/pascal-intervention-route.test.ts`.
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   M=/tmp/qt-pascal-intervention-route-mirror; rm -rf $M; mkdir -p $M/cases
//!   cp $V5W/harness/oracle/cases/pascal-intervention-route.test.ts $M/cases/
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-intervention-route.ndjson \
//!     npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" \
//!       --roots "$M/cases" --runTestsByPath "$M/cases/pascal-intervention-route.test.ts"
//! Run:
//!   QT_ORACLE_PASCAL_INTERVENTION_ROUTE=/tmp/oracle-pascal-intervention-route.ndjson \
//!     cargo test -p quilltap-harness --test pascal_intervention_route_equivalence

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use quilltap_core::api::custom_tool_interventions::{
    chat_custom_tool_intervention_preview, chat_custom_tool_intervention_reask_with_invoker,
    chat_custom_tool_intervention_resolve, chat_custom_tool_interventions_list,
};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::tiered_mount_pool::MountTier;
use quilltap_core::pascal::custom_tool_types::{safe_parse, QtapCustomTool};
use quilltap_core::pascal::custom_tools::{
    execute_custom_tool, LlmConsultResult, LlmInvokeOptions, LlmInvokeResult, LlmInvoker,
    RecordedDraw, ReplayInputs,
};
use quilltap_core::pascal::dice::FixedBytes;
use quilltap_core::pascal::intervention::recompute::RunSnapshot;
use quilltap_core::pascal::intervention::registry::{
    get_intervention, hold, intervention_status, reset_intervention_registry_for_tests, Entrance,
    InterventionDecision, NewHold,
};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Map, Value};
use tokio::sync::oneshot;

/// The scripted model (v4's doubled `buildCustomToolLlmInvoker`): answers in
/// order, records every prompt posed.
struct Scripted {
    answers: Mutex<VecDeque<Value>>,
    prompts: Mutex<Vec<String>>,
}

impl LlmInvoker for Scripted {
    fn invoke<'a>(
        &'a self,
        prompt: &'a str,
        _options: LlmInvokeOptions,
    ) -> Pin<Box<dyn Future<Output = LlmInvokeResult> + Send + 'a>> {
        Box::pin(async move {
            self.prompts.lock().unwrap().push(prompt.to_string());
            let next = self
                .answers
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| json!({"ok": false, "reason": "no scripted answer"}));
            if next["ok"].as_bool() == Some(true) {
                LlmInvokeResult::Answered {
                    output: next["output"].as_str().unwrap().to_string(),
                    provider: None,
                    model: None,
                }
            } else {
                LlmInvokeResult::Failed {
                    reason: next["reason"].as_str().unwrap().to_string(),
                }
            }
        })
    }
}

fn status_body(resp: Response) -> (u64, Value) {
    match resp {
        Response::CustomToolInterventions(v)
        | Response::CustomToolInterventionPreview(v)
        | Response::CustomToolInterventionReask(v)
        | Response::CustomToolInterventionResolve(v) => (200, v),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                other => panic!("unexpected error kind {other:?}: {}", e.message),
            };
            (status, json!({ "error": e.message }))
        }
        other => panic!("unexpected response {other:?}"),
    }
}

fn consult_of(v: &Value) -> Option<LlmConsultResult> {
    let o = v.as_object()?;
    let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
    Some(LlmConsultResult {
        ok: o["ok"].as_bool().unwrap(),
        output: s("output").unwrap(),
        prompt: s("prompt").unwrap(),
        reason: s("reason"),
        provider: s("provider"),
        model: s("model"),
    })
}

/// `(level, message, fields in order)`.
type Line = (String, String, Vec<(String, String)>);

/// A rig line (`LEVEL target message k=v …`) → `(level, message, fields)`,
/// for the three messages-and-contexts this family compares.
fn rust_line(line: &str, messages: &[&str]) -> Option<Line> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let message = messages.iter().find(|m| rest.starts_with(**m))?;
    let tail = rest[message.len()..].trim_start();
    // A field runs to the next ` <ident>=`.
    let mut fields = Vec::new();
    let mut cursor = tail;
    while !cursor.is_empty() {
        let (key, after) = cursor.split_once('=')?;
        let mut end = after.len();
        let mut search = 0;
        while let Some(off) = after[search..].find(' ') {
            let at = search + off;
            let next = &after[at + 1..];
            if let Some(eq) = next.find('=') {
                if next[..eq]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    end = at;
                    break;
                }
            }
            search = at + 1;
        }
        fields.push((key.to_string(), after[..end].to_string()));
        cursor = after[end..].trim_start();
    }
    Some((level.to_string(), (*message).to_string(), fields))
}

fn oracle_line(v: &Value) -> Line {
    let fields = v["meta"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, val)| match val {
            Value::String(s) => (k.clone(), s.clone()),
            Value::Array(_) | Value::Object(_) => (format!("{k}Json"), val.to_string()),
            other => (k.clone(), other.to_string()),
        })
        .collect();
    (
        v["level"].as_str().unwrap().to_string(),
        v["message"].as_str().unwrap().to_string(),
        fields,
    )
}

const MESSAGES: &[&str] = &[
    "Custom tool run held for the operator",
    "Custom tool hold resolve refused",
    "Custom tool hold resolved",
    "Custom tool holds listed",
    "Custom tool hold previewed",
    "Custom tool hold re-asked",
    "Custom tool hold re-ask failed",
    "Custom tool LLM consult failed",
    "Custom tool table dealt ahead of the consult",
];

#[test]
fn intervention_routes_match_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_INTERVENTION_ROUTE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_INTERVENTION_ROUTE (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    global_capture::install();

    let mut docs: BTreeMap<String, Value> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let mut scenes = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let scene = row["id"].as_str().unwrap().to_string();
        if scene == "__docs__" {
            docs = row["docs"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            continue;
        }
        scenes += 1;
        reset_intervention_registry_for_tests();
        let scripted = Scripted {
            answers: Mutex::new(VecDeque::new()),
            prompts: Mutex::new(Vec::new()),
        };
        let mut ids: BTreeMap<String, String> = BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        let mut receivers: BTreeMap<String, oneshot::Receiver<InterventionDecision>> =
            BTreeMap::new();
        let mut clock = 1_000i64;
        let mut check = |what: String, got: &Value, want: &Value| {
            let (g, w) = (
                serde_json::to_string(got).unwrap(),
                serde_json::to_string(want).unwrap(),
            );
            if g != w {
                failures.push(format!("{scene} {what}\n  v5: {g}\n  v4: {w}"));
            }
        };

        let (_, lines) = rt.block_on(global_capture::capture_async(async {
            for (i, step) in row["steps"].as_array().unwrap().iter().enumerate() {
                let mask = |v: &Value, ids: &BTreeMap<String, String>, order: &[String]| {
                    let mut t = serde_json::to_string(v).unwrap();
                    for (n, h) in order.iter().enumerate() {
                        t = t.replace(&ids[h], &format!("<hold:{n}>"));
                        let stamp = get_intervention(&ids[h])
                            .map(|p| p.created_at)
                            .unwrap_or_default();
                        t = t.replace(
                            &format!("\"createdAt\":{stamp}"),
                            &format!("\"createdAt\":\"<at:{n}>\""),
                        );
                    }
                    serde_json::from_str::<Value>(&t).unwrap()
                };
                let subst = |v: &Value, ids: &BTreeMap<String, String>| {
                    let mut t = serde_json::to_string(v).unwrap();
                    for (h, id) in ids {
                        t = t.replace(&format!("\"${h}\""), &format!("\"{id}\""));
                    }
                    serde_json::from_str::<Value>(&t).unwrap()
                };
                if let Some(answers) = step.get("answers") {
                    *scripted.answers.lock().unwrap() =
                        answers.as_array().unwrap().iter().cloned().collect();
                } else if let Some(h) = step.get("hold").and_then(Value::as_str) {
                    let definition: QtapCustomTool =
                        safe_parse(&docs[step["def"].as_str().unwrap()])
                            .unwrap_or_else(|_| panic!("{scene}: definition refused"));
                    let draw = match step["draw"]["form"].as_str().unwrap() {
                        "dice" => RecordedDraw::Dice {
                            faces: step["draw"]["faces"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|f| f.as_i64().unwrap())
                                .collect(),
                        },
                        _ => RecordedDraw::Range {
                            unit: step["draw"]["unit"].as_f64().unwrap(),
                        },
                    };
                    let consult = consult_of(&step["consult"]);
                    let character_name = step["character"]["name"].as_str().map(str::to_string);
                    let snapshot = RunSnapshot {
                        params: None,
                        private: None,
                        metadata: Map::new(),
                        progress: Map::new(),
                        now_ms: 1_000,
                        character: character_name.clone(),
                        state: json!({}),
                        inventory: None,
                    };
                    let baseline = execute_custom_tool(
                        &definition,
                        None,
                        None,
                        Some(&snapshot.metadata),
                        Some(&snapshot.state),
                        &mut FixedBytes::new(vec![]),
                        None,
                        Some(&snapshot.progress),
                        snapshot.now_ms,
                        snapshot.character.as_deref(),
                        None,
                        Some(&ReplayInputs {
                            draw: Some(&draw),
                            consult: consult.as_ref(),
                            pins: None,
                        }),
                    )
                    .await
                    .unwrap_or_else(|e| panic!("{scene}: baseline refused: {}", e.0));
                    let trigger = step["trigger"].as_str().map(str::to_string);
                    clock += 3;
                    let (id, rx) = hold(
                        NewHold {
                            chat_id: step["chat"].as_str().unwrap().to_string(),
                            user_id: "user-1".into(),
                            entrance: if trigger.is_some() {
                                Entrance::Trigger
                            } else {
                                Entrance::Llm
                            },
                            character_id: step["character"]["id"].as_str().map(str::to_string),
                            character_name,
                            trigger_phrase: trigger,
                            definition,
                            definition_tier: MountTier::Global,
                            definition_mount_id: "mount-1".into(),
                            snapshot,
                            draw: baseline.draw.clone(),
                            consults: consult.iter().cloned().collect(),
                            baseline,
                        },
                        clock,
                    );
                    ids.insert(h.to_string(), id);
                    order.push(h.to_string());
                    receivers.insert(h.to_string(), rx);
                } else if let Some(chat) = step.get("get").and_then(Value::as_str) {
                    let (status, body) = status_body(chat_custom_tool_interventions_list(chat));
                    check(
                        format!("step {i} GET status"),
                        &json!(status),
                        &step["status"],
                    );
                    check(
                        format!("step {i} GET body"),
                        &mask(&body, &ids, &order),
                        &step["body"],
                    );
                } else if let Some(chat) = step.get("post").and_then(Value::as_str) {
                    let input = subst(&step["input"], &ids);
                    let action = step["action"].as_str().unwrap();
                    let field = |k: &str| input.as_object().and_then(|o| o.get(k)).cloned();
                    let resp = if !input.is_object() {
                        // The edge answers a non-object body from the core's
                        // own parse — exactly what `intervention_post` does.
                        use quilltap_core::pascal::intervention::overrides::{
                            join_messages, parse_preview_body, parse_resolve_body,
                        };
                        let issues = if action == "intervention-resolve" {
                            parse_resolve_body(&input).err()
                        } else {
                            parse_preview_body(&input).err()
                        };
                        Response::error(
                            ErrorKind::BadRequest,
                            join_messages(&issues.unwrap_or_default()),
                        )
                    } else {
                        match action {
                            "intervention-preview" => {
                                chat_custom_tool_intervention_preview(
                                    chat,
                                    field("id"),
                                    field("overrides"),
                                )
                                .await
                            }
                            "intervention-reask" => {
                                chat_custom_tool_intervention_reask_with_invoker(
                                    chat,
                                    field("id"),
                                    field("overrides"),
                                    Some(&scripted),
                                )
                                .await
                            }
                            "intervention-resolve" => {
                                chat_custom_tool_intervention_resolve(
                                    chat,
                                    field("id"),
                                    field("decision"),
                                )
                                .await
                            }
                            other => panic!("action {other}"),
                        }
                    };
                    let (status, body) = status_body(resp);
                    check(
                        format!("step {i} {action} status"),
                        &json!(status),
                        &step["status"],
                    );
                    check(
                        format!("step {i} {action} body"),
                        &mask(&body, &ids, &order),
                        &step["body"],
                    );
                } else if step.get("postRaw").is_some() {
                    // The edge's own sentence for a body that is not JSON.
                    let raw = step["raw"].as_str().unwrap();
                    assert!(
                        serde_json::from_str::<Value>(raw).is_err(),
                        "{scene}: raw parses"
                    );
                    check(
                        format!("step {i} postRaw status"),
                        &json!(400),
                        &step["status"],
                    );
                    check(
                        format!("step {i} postRaw body"),
                        &json!({ "error": "The request body is not JSON" }),
                        &step["body"],
                    );
                } else if let Some(h) = step.get("decision").and_then(Value::as_str) {
                    let rx = receivers.get_mut(h).unwrap();
                    let got = match rx.try_recv() {
                        Ok(InterventionDecision::Deal {
                            overrides,
                            run,
                            reasks,
                        }) => json!({
                            "kind": "deal",
                            "overrides": overrides.to_wire(),
                            "reasks": reasks,
                            "message": run.result.message,
                            "outcomeIndex": run.result.outcome_index,
                            "presentation": run.presentation.to_wire(),
                            "staleAnswer": run.stale_answer,
                        }),
                        Ok(InterventionDecision::AsRolled) => json!({ "kind": "as-rolled" }),
                        Ok(InterventionDecision::Veto { cause }) => {
                            json!({ "kind": "veto", "cause": cause.as_str() })
                        }
                        Err(e) => json!({ "unsettled": e.to_string() }),
                    };
                    check(format!("step {i} decision"), &got, &step["out"]);
                } else if let Some(h) = step.get("status").and_then(Value::as_str) {
                    let got = json!(intervention_status(&ids[h]).as_str());
                    check(format!("step {i} status"), &got, &step["out"]);
                } else if let Some(h) = step.get("consults").and_then(Value::as_str) {
                    let got = get_intervention(&ids[h])
                        .map(|p| {
                            Value::Array(
                                p.hold
                                    .consults
                                    .iter()
                                    .map(|c| Value::Object(c.to_wire()))
                                    .collect(),
                            )
                        })
                        .unwrap_or(Value::Null);
                    check(format!("step {i} consults"), &got, &step["out"]);
                }
            }
        }));

        let prompts = scripted.prompts.lock().unwrap().clone();
        check("promptsSeen".into(), &json!(prompts), &row["promptsSeen"]);

        let mut got_lines: Vec<Line> = Vec::new();
        for l in &lines {
            if let Some(mut parsed) = rust_line(l, MESSAGES) {
                for (n, h) in order.iter().enumerate() {
                    for f in parsed.2.iter_mut() {
                        f.1 = f.1.replace(&ids[h], &format!("<hold:{n}>"));
                    }
                }
                got_lines.push(parsed);
            }
        }
        let want_lines: Vec<_> = row["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(oracle_line)
            .collect();
        if got_lines != want_lines {
            failures.push(format!(
                "{scene} lines\n  v5: {got_lines:?}\n  v4: {want_lines:?}"
            ));
        }
    }
    reset_intervention_registry_for_tests();
    eprintln!("pascal_intervention_route scenes: {scenes}");
    assert!(scenes > 0, "no scenes ran");
    assert!(
        failures.is_empty(),
        "{} comparand(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
