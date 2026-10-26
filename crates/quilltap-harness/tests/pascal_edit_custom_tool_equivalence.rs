//! `edit_custom_tool` differential (P4.D332; v4 `ea2cb60ab` + `c56fe278c`, pinned
//! `322a5488e`): `tools::edit_custom_tool` + the executor's arm vs v4's REAL
//! `executeEditCustomTool` over the REAL doc layer (not v4's mocks), on a fresh
//! copy of the committed `pascal-run-custom-{main,mount}.db` pair per row.
//!
//! Four row kinds from ONE NDJSON (`QT_ORACLE_PASCAL_EDIT_CUSTOM_TOOL`):
//! - `path`: `tool_definition_path_for` vs `toolDefinitionPathFor`, exact.
//! - `input`: `validate_edit_custom_tool_input` vs `validateEditCustomToolInput`.
//! - `format`: `format_edit_custom_tool_results` vs `formatEditCustomToolResults`.
//! - `handler`: the output object's BYTES, the `Tools/` rows of the mount-index
//!   partition after the call, the Librarian's system `chat_messages`, and the
//!   nine pinned log lines (capture AND silence: a row that v4 stays quiet on
//!   must stay quiet here).
//!
//! Planted `Tools/` files are the oracle row's recorded SQL, run on this side's
//! mount copy before it opens; the row's `sees` (what v4's discovery read in
//! General BEFORE the call) is checked against v5's own discovery, so a plant
//! neither side can read cannot pass vacuously.
//!
//! The one cross-language seam: V8's `JSON.parse` error text (`Unexpected token
//! 'o', "not json {" is not valid JSON`) vs serde's. A `... is not valid JSON:`
//! sentence is compared up to and including the colon — the same rule
//! `pascal_roster_equivalence` applies — and the DEBUG lines are not affected
//! (they carry the stage, not the text).
//!
//! `schema-examples-validate`: v4's own test asserts both worked examples pass
//! `validateToolDefinitionText`. At the pin that holds; on this branch v5's
//! grammar twin lacks the pin's any-of / inventory / log vocabulary until
//! P4.D329 / P4.D331 / P4.D333 land, so the RANGE specimen is expected RED here
//! and GREEN on the union; the DICE specimen is GREEN. Both verdicts are
//! reported by name below (R-2): nothing is carved.
//!
//! Generate the oracle (Node 24; mirror to /tmp — jest ignores `.claude/`): the
//! recipe is in `harness/oracle/cases/pascal-edit-custom-tool.test.ts`.
//! Run:
//!   QT_ORACLE_PASCAL_EDIT_CUSTOM_TOOL=/tmp/oracle-pascal-edit-custom-tool.ndjson \
//!     cargo test -p quilltap-harness --test pascal_edit_custom_tool_equivalence -- --nocapture

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::pascal::custom_tool_schema::{
    CUSTOM_TOOL_SPEC_DICE_JSON, CUSTOM_TOOL_SPEC_JSON,
};
use quilltap_core::pascal::roster::{
    tool_definition_path_for, validate_tool_definition_text, ToolDefinitionValidation,
    ValidationWhere,
};
use quilltap_core::services::librarian_notifications::post_librarian_write_announcement;
use quilltap_core::services::tool_execution::{create_tool_context, ToolCall, ToolRunner};
use quilltap_core::tools::doc_edit::{DocEditToolContext, PendingLibrarianAnnouncement};
use quilltap_core::tools::edit_custom_tool::{
    execute_edit_custom_tool, format_edit_custom_tool_results, validate_edit_custom_tool_input,
    EditCustomToolAction, EditCustomToolOutcome, EditCustomToolOutput,
};
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde::Deserialize;
use serde_json::{json, Map, Value};

const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const GENERAL_MP: &str = "93000000-0000-4000-8000-0000000000aa";

/// Every line this order ports, by (target, message).
const HANDLER_TARGET: &str = "quilltap::tools::edit_custom_tool";
const VALIDATOR_TARGET: &str = "quilltap::pascal::custom_tools";
const HANDLER_MESSAGES: [&str; 7] = [
    "edit_custom_tool validation failed",
    "edit_custom_tool: schema requested",
    "edit_custom_tool called",
    "edit_custom_tool: existing definition is invalid",
    "edit_custom_tool: refused an invalid definition",
    "Custom tool definition saved",
    "edit_custom_tool failed",
];
const VALIDATOR_MESSAGES: [&str; 2] = [
    "Custom tool carries keys this build does not know",
    "Custom tool definition failed schema validation",
];

const TOOLS_DUMP_SQL: &str = r#"SELECT l."mountPointId" AS mountPointId, l."relativePath" AS relativePath, l."fileName" AS fileName,
  l."allowEmbed" AS allowEmbed, l."allowCharacterRead" AS allowCharacterRead, l."allowCharacterWrite" AS allowCharacterWrite,
  f."fileType" AS fileType, f."fileSizeBytes" AS fileSizeBytes, f."sha256" AS sha256,
  d."content" AS content, d."contentSha256" AS contentSha256, d."plainTextLength" AS plainTextLength
FROM "doc_mount_file_links" l
JOIN "doc_mount_files" f ON f."id" = l."fileId"
LEFT JOIN "doc_mount_documents" d ON d."fileId" = l."fileId"
WHERE l."relativePath" LIKE 'Tools/%'
ORDER BY l."mountPointId", l."relativePath""#;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/pascal-run-custom.json")
}

/// V8's `JSON.parse` error text is not serde's: compare a `... is not valid
/// JSON:` sentence up to and including its colon.
///
/// P4.D338 residue, lifted at the union: v4 `7263105c6` (bug 256) reworded the
/// path resolver's unknown-store sentence to `No document store "X" is
/// reachable in this context — nothing was read`; `doc_edit/path_resolver.rs`'s
/// two texts are P4.D338's, so on this lane's branch v5 still says `Mount point
/// not found or not accessible in this context`. The v4 sentence is folded onto
/// v5's — by NAME, in this one helper, nowhere else.
fn normalise_json_error(text: &str) -> String {
    const V4_UNKNOWN_STORE_TAIL: &str = " is reachable in this context — nothing was read";
    const V5_UNKNOWN_STORE: &str = "Mount point not found or not accessible in this context";
    let mut text = text.to_string();
    if let Some(start) = text.find("No document store ") {
        if let Some(tail) = text[start..].find(V4_UNKNOWN_STORE_TAIL) {
            let end = start + tail + V4_UNKNOWN_STORE_TAIL.len();
            text.replace_range(start..end, V5_UNKNOWN_STORE);
        }
    }
    match text.find(" is not valid JSON:") {
        Some(i) => text[..i + " is not valid JSON:".len()].to_string(),
        None => text,
    }
}

fn normalise_value(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(normalise_json_error(s)),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), normalise_value(x)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(normalise_value).collect()),
        other => other.clone(),
    }
}

fn canon(v: &Value) -> Value {
    match v {
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15)
            .map(|f| Value::from(f as i64))
            .unwrap_or_else(|| v.clone()),
        Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), canon(x))).collect()),
        _ => v.clone(),
    }
}

/// The system-sender rows (the Librarian's), minted id + clocks left out.
fn system_rows(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .filter(|r| r.get("systemSender").is_some_and(|s| !s.is_null()))
        .map(|r| {
            let get = |k: &str| r.get(k).cloned().unwrap_or(Value::Null);
            let parse_json = |v: Value| match v {
                Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
                other => other,
            };
            json!({
                "role": get("role"),
                "content": get("content"),
                "systemSender": get("systemSender"),
                "systemKind": get("systemKind"),
                "targetParticipantIds": parse_json(get("targetParticipantIds")),
            })
        })
        .collect()
}

fn dump_sql_rows(conn: &rusqlite::Connection, sql: &str) -> Vec<Value> {
    let mut stmt = conn.prepare(sql).unwrap();
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt
        .query_map([], |row| {
            let mut m = Map::new();
            for (i, n) in names.iter().enumerate() {
                let v: rusqlite::types::Value = row.get(i)?;
                m.insert(
                    n.clone(),
                    match v {
                        rusqlite::types::Value::Null => Value::Null,
                        rusqlite::types::Value::Integer(i) => json!(i),
                        rusqlite::types::Value::Real(f) => json!(f),
                        rusqlite::types::Value::Text(s) => json!(s),
                        rusqlite::types::Value::Blob(b) => json!(hex::encode(b)),
                    },
                );
            }
            Ok(Value::Object(m))
        })
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

fn output_from_value(v: &Value) -> EditCustomToolOutput {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    EditCustomToolOutput {
        success: v["success"].as_bool().unwrap_or(false),
        action: match v.get("action").and_then(Value::as_str) {
            Some("saved") => Some(EditCustomToolOutcome::Saved),
            Some("validated") => Some(EditCustomToolOutcome::Validated),
            Some("schema") => Some(EditCustomToolOutcome::Schema),
            _ => None,
        },
        path: s("path"),
        uri: s("uri"),
        name: s("name"),
        schema: v.get("schema").cloned(),
        examples: v.get("examples").and_then(Value::as_array).cloned(),
        error: s("error"),
    }
}

fn capture_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| {
            l.split_once(' ').is_some_and(|(_, rest)| {
                let hit = |target: &str, messages: &[&str]| {
                    rest.strip_prefix(target)
                        .and_then(|r| r.strip_prefix(' '))
                        .is_some_and(|m| {
                            messages
                                .iter()
                                .any(|p| m == *p || m.starts_with(&format!("{p} ")))
                        })
                };
                hit(HANDLER_TARGET, &HANDLER_MESSAGES) || hit(VALIDATOR_TARGET, &VALIDATOR_MESSAGES)
            })
        })
        .cloned()
        .collect()
}

fn want_lines(logs: &Value) -> Vec<String> {
    logs.as_array()
        .expect("oracle row has `logs`")
        .iter()
        .map(|l| {
            let message = l["message"].as_str().unwrap_or("");
            let target = if VALIDATOR_MESSAGES.contains(&message) {
                VALIDATOR_TARGET
            } else {
                HANDLER_TARGET
            };
            common::v4_log_as_capture_line(target, l)
        })
        .collect()
}

fn runner(db: &Db) -> BuiltInToolRunner {
    BuiltInToolRunner::new(
        db.clone(),
        SelfInventoryEnv {
            version: String::new(),
            runtime_mode: "local-dev".to_string(),
            client_shell: ClientShell::Browser,
            mount_index_degraded: false,
            release_notes: None,
            changelog: None,
            model_info: Vec::new(),
            fallback_pricing: Vec::new(),
            registry_default_context: 8192,
        },
    )
}

fn open_scratch(spec: &Spec, tag: &str, plant_sql: &[String]) -> (Db, PathBuf) {
    let scratch = std::env::temp_dir().join(format!("qt-edit-ct-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("pascal-run-custom-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("pascal-run-custom-mount.db"), &mount).unwrap();
    if !plant_sql.is_empty() {
        let writer = Writer::open_writable(&mount, &spec.test_pepper_base64)
            .expect("open mount for the plant");
        for sql in plant_sql {
            writer
                .connection()
                .execute_batch(sql)
                .unwrap_or_else(|e| panic!("row '{tag}' plant {sql}: {e}"));
        }
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    (db, scratch)
}

fn doc_ctx(spec: &Spec, want: &Value) -> DocEditToolContext {
    DocEditToolContext {
        chat_id: CHAT.to_string(),
        user_id: spec.user_id.clone(),
        project_id: want["projectId"].as_str().map(str::to_string),
        character_id: want["characterId"].as_str().map(str::to_string),
        operator_override: false,
        files_dir: None,
        blob_webp:
            quilltap_core::services::mount_index::normalize_blob_image::SharedBlobWebp::default(),
        mount_pool: None,
    }
}

fn discovered(db: &Db) -> Value {
    let (found, errors) = db
        .read_mount_index(|mount| {
            Ok(quilltap_core::pascal::roster::load_tools_from_mount(
                mount,
                GENERAL_MP,
                quilltap_core::db::tiered_mount_pool::MountTier::Global,
            ))
        })
        .unwrap();
    let mut f: Vec<String> = found
        .iter()
        .map(|e| format!("{}={}", e.definition_path, e.definition.name))
        .collect();
    f.sort();
    let mut er: Vec<String> = errors.iter().map(|e| e.definition_path.clone()).collect();
    er.sort();
    json!({ "found": f, "errors": er })
}

#[test]
fn edit_custom_tool_matches_oracle() {
    // The capture rig must own this binary's subscriber before any callsite is
    // reached (memory note `capture-rig-writer-thread`).
    quilltap_core::test_support::global_capture::install();
    let Ok(path) = std::env::var("QT_ORACLE_PASCAL_EDIT_CUSTOM_TOOL") else {
        eprintln!("SKIP: set QT_ORACLE_PASCAL_EDIT_CUSTOM_TOOL (see test header).");
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(
        !rows.is_empty(),
        "the oracle NDJSON is empty — regenerate it"
    );

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut saw_saved = false;
    let mut saw_silence = false;
    let mut saw_policy_refusal = false;

    for row in &rows {
        let kind = row["kind"].as_str().unwrap();
        *counts
            .entry(match kind {
                "path" => "path",
                "input" => "input",
                "format" => "format",
                _ => "handler",
            })
            .or_default() += 1;
        match kind {
            "path" => {
                let file = row["file"].as_str().unwrap();
                let got = tool_definition_path_for(file);
                assert_eq!(
                    got.as_deref(),
                    row["path"].as_str(),
                    "toolDefinitionPathFor({file:?})"
                );
            }
            "input" => {
                let got = validate_edit_custom_tool_input(&row["input"]).map(|p| {
                    let mut m = Map::new();
                    if let Some(EditCustomToolAction::Schema) = p.action {
                        m.insert("action".into(), json!("schema"));
                    }
                    if let Some(v) = p.mount_point {
                        m.insert("mount_point".into(), json!(v));
                    }
                    if let Some(v) = p.file {
                        m.insert("file".into(), json!(v));
                    }
                    if let Some(v) = p.content {
                        m.insert("content".into(), v);
                    }
                    Value::Object(m)
                });
                assert_eq!(
                    canon(&got.unwrap_or(Value::Null)),
                    canon(&row["parsed"]),
                    "validateEditCustomToolInput({})",
                    row["input"]
                );
            }
            "format" => {
                let out = output_from_value(&row["output"]);
                assert_eq!(
                    format_edit_custom_tool_results(&out),
                    row["text"].as_str().unwrap(),
                    "formatEditCustomToolResults row '{}'",
                    row["name"]
                );
            }
            "handler" => {
                let name = row["name"].as_str().unwrap();
                let plant_sql: Vec<String> = row["plantSql"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap().to_string())
                    .collect();
                let (db, scratch) = open_scratch(&spec, name, &plant_sql);

                // The plant self-check: v5's discovery must see exactly what v4's did.
                assert_eq!(
                    discovered(&db),
                    row["sees"],
                    "row '{name}': discovery before the call"
                );

                // The handler runs on THIS thread over test-owned writer
                // connections: a `Db::write` closure runs on the writer thread,
                // invisible to the thread-scoped capture rig (memory note
                // `capture-rig-writer-thread`). The executor's own closure path is
                // pinned by the tool-result test below. The Librarian's
                // announcement is posted exactly as the executor posts it.
                let ctx = doc_ctx(&spec, row);
                let main_w =
                    Writer::open_writable(&scratch.join("main.db"), &spec.test_pepper_base64)
                        .expect("open main for the handler");
                let mount_w =
                    Writer::open_writable(&scratch.join("mount.db"), &spec.test_pepper_base64)
                        .expect("open mount for the handler");
                let ((out, pending), lines) =
                    quilltap_core::test_support::global_capture::capture(|| {
                        execute_edit_custom_tool(
                            main_w.connection(),
                            mount_w.connection(),
                            &row["input"],
                            &ctx,
                        )
                    });
                drop(main_w);
                drop(mount_w);
                if let Some(pending) = pending {
                    match pending {
                        PendingLibrarianAnnouncement::Write(a) => {
                            rt.block_on(post_librarian_write_announcement(&db, &a));
                        }
                        other => panic!("row '{name}': an unexpected announcement kind {other:?}"),
                    }
                }

                // The output object's BYTES (V8's JSON.parse text normalised).
                let got_json = serde_json::to_string(&out).unwrap();
                let want_json = row["outputJson"].as_str().unwrap();
                assert_eq!(
                    normalise_json_error(&got_json),
                    normalise_json_error(want_json),
                    "row '{name}': the output object"
                );

                // The Tools/ rows after the call.
                let got_after = db
                    .read_mount_index(|m| Ok(dump_sql_rows(m, TOOLS_DUMP_SQL)))
                    .unwrap();
                assert_eq!(
                    canon(&Value::Array(got_after)),
                    canon(&row["toolsAfter"]),
                    "row '{name}': the Tools/ rows after the call"
                );
                // ... and discovery sees what v4's saw.
                assert_eq!(
                    discovered(&db),
                    row["seesAfter"],
                    "row '{name}': discovery after the call"
                );

                // The Librarian's posted system rows.
                let msgs = db
                    .read_main(|c| dump_table_json_conn(c, "chat_messages", "id"))
                    .unwrap();
                let got_rows = msgs["rows"].as_array().cloned().unwrap_or_default();
                let want_rows = row["rows"].as_array().cloned().unwrap_or_default();
                assert_eq!(
                    got_rows.len(),
                    want_rows.len(),
                    "row '{name}': chat_messages row count"
                );
                assert_eq!(
                    canon(&Value::Array(system_rows(&got_rows))),
                    canon(&Value::Array(system_rows(&want_rows))),
                    "row '{name}': the Librarian's system rows"
                );

                // The pinned lines, line for line (capture AND silence).
                let got_lines = capture_lines(&lines);
                let want = want_lines(&row["logs"]);
                assert_eq!(
                    normalise_json_error_lines(&got_lines),
                    normalise_json_error_lines(&want),
                    "row '{name}': log lines"
                );
                if want.is_empty() {
                    saw_silence = true;
                }
                if out.action == Some(EditCustomToolOutcome::Saved) {
                    saw_saved = true;
                }
                if out
                    .error
                    .as_deref()
                    .is_some_and(|e| e.contains("read-only to characters"))
                {
                    saw_policy_refusal = true;
                }

                drop(db);
                let _ = std::fs::remove_dir_all(&scratch);
            }
            other => panic!("unknown row kind {other}"),
        }
    }

    eprintln!("rows by kind: {counts:?}");
    assert!(counts["path"] >= 19 && counts["input"] >= 15 && counts["format"] >= 7);
    assert!(counts["handler"] >= 25, "oracle looked short: {counts:?}");
    // The floors: each was a way for the family to be green while proving nothing.
    assert!(saw_saved, "no row actually saved a definition");
    assert!(
        saw_policy_refusal,
        "no row hit the per-document write policy"
    );
    assert!(saw_silence, "no row proved a quiet path stays quiet");
}

fn normalise_json_error_lines(lines: &[String]) -> Vec<String> {
    lines.iter().map(|l| normalise_json_error(l)).collect()
}

/// The executor's `result` object: v4's literal key order (`formattedText,
/// action, path, uri, name, schema, examples`), absent members dropped, and a
/// failure's `result` is `null` with the handler's `error` carried through.
#[test]
fn the_executor_arm_shapes_the_tool_result_like_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_PASCAL_EDIT_CUSTOM_TOOL") else {
        eprintln!("SKIP: set QT_ORACLE_PASCAL_EDIT_CUSTOM_TOOL (see test header).");
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    for name in [
        "saving-validates-then-writes-through-doc-write-file",
        "validating-the-file-as-it-stands-and-writes-nothing",
        "schema-returns-the-published-schema-and-specimens",
        "saving-refuses-text-that-is-not-json",
    ] {
        let row = rows
            .iter()
            .find(|r| r["kind"] == "handler" && r["name"] == name)
            .unwrap_or_else(|| panic!("oracle missing row {name}"));
        let plant_sql: Vec<String> = row["plantSql"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect();
        let (db, scratch) = open_scratch(&spec, &format!("exec-{name}"), &plant_sql);
        let runner = runner(&db);
        let ctx = create_tool_context(
            CHAT,
            spec.user_id.clone(),
            row["characterId"]
                .as_str()
                .unwrap_or("a1000000-0000-4000-8000-00000000000a"),
            "e1000000-0000-4000-8000-00000000000a",
            None,
            None,
            None,
            None,
            None,
        );
        let result = rt.block_on(runner.run(
            &ToolCall {
                name: "edit_custom_tool".into(),
                arguments: row["input"].clone(),
                call_id: Some("call-1".into()),
            },
            &ctx,
        ));
        let want = output_from_value(&row["output"]);
        if want.success {
            assert!(result.success, "row '{name}': {:?}", result.error);
            let mut expect = Map::new();
            expect.insert(
                "formattedText".into(),
                json!(format_edit_custom_tool_results(&want)),
            );
            for key in ["action", "path", "uri", "name", "schema", "examples"] {
                if let Some(v) = row["output"].get(key) {
                    expect.insert(key.into(), v.clone());
                }
            }
            assert_eq!(
                serde_json::to_string(&result.result).unwrap(),
                serde_json::to_string(&Value::Object(expect)).unwrap(),
                "row '{name}': the tool result's keys, in v4's order"
            );
        } else {
            assert!(!result.success);
            assert_eq!(result.result, Value::Null, "a failure's result is null");
            assert_eq!(
                normalise_json_error(result.error.as_deref().unwrap_or("")),
                normalise_json_error(want.error.as_deref().unwrap_or("")),
                "row '{name}': the error text"
            );
        }
        drop(runner);
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);
    }
    let _ = normalise_value; // kept for symmetry with sibling families' helpers
}

/// v4's `returns the published JSON Schema and specimens that pass the runtime
/// validator` (R-2): the Rust twin of its second assertion. RED for the range
/// specimen on this lane's own branch (the grammar twins are P4.D329 / P4.D331 /
/// P4.D333's) and GREEN on the union; the dice specimen is GREEN. Both verdicts
/// are PRINTED by name and BOTH asserted: the range red is the union
/// obligation (§S.5 wire 9), carved nowhere.
#[test]
fn schema_examples_validate() {
    let verdict = |label: &str, text: &str| -> bool {
        match validate_tool_definition_text(text, &ValidationWhere::default()) {
            ToolDefinitionValidation::Ok { .. } => {
                eprintln!("schema-examples-validate[{label}]: GREEN");
                true
            }
            ToolDefinitionValidation::Invalid { stage, reason } => {
                eprintln!(
                    "schema-examples-validate[{label}]: RED at {} — {reason}",
                    stage.as_str()
                );
                false
            }
        }
    };
    let range = verdict("range", CUSTOM_TOOL_SPEC_JSON);
    let dice = verdict("dice", CUSTOM_TOOL_SPEC_DICE_JSON);
    // Nothing is carved (R-2): the RANGE red is the union obligation's tripwire.
    assert!(
        dice,
        "the dice specimen must validate against the runtime validator"
    );
    assert!(
        range,
        "schema-examples-validate[range]: the pin's CUSTOM_TOOL_SPEC.json does not validate \
         against v5's grammar twin — EXPECTED RED on this lane's own branch (R-2 / R-E): it flips \
         green when P4.D329 / P4.D331 / P4.D333 land (§S.5 wire 9)"
    );
}
