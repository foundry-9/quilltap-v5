//! P4.9E3B search-replace differential: `services::search_replace` vs v4's
//! REAL `POST /api/v1/search-replace` route over a FRESH copy of the committed
//! `chat-dialogs-{main,mount}.db` fixture per case. Response body + the
//! post-mutation dumps (SR_CHAT chat row + messages, every memory) are diffed.
//!
//! ## Minted values
//!
//! The execute path re-writes matched memories through `update_memory`
//! (P4.D298, bug 233), minting `updatedAt` ONCE per touched row. The differential compares
//! WHICH rows were touched (the `updatedAt != seed` boolean must agree), then
//! asserts v4's stamp is at-or-after the frozen base and v5's differs from the
//! seed, and only then strips the key — the difference is proven, not
//! normalized away. Message rows and the chat row carry no minted values.
//!
//! ## Two RECORDED-ONLY v4 arms (no v5 counterpart, deliberately)
//!
//! `unknown_action` / `missing_action` are v4 `withActionDispatch` middleware
//! furniture — v5 carries no action string (the §1 verbs ARE the action
//! selection, and no REST edge exists, matching the message-op precedent).
//! Their oracle rows are asserted by shape so upstream copy drift is caught.
//!
//! ## The poisoned message table (P4.109)
//!
//! `preview_poisoned` / `execute_poisoned` rename `chat_messages` away on the
//! per-case copy (never the committed pair). v4's count and replace never
//! reach their own `safeQuery` arms: `getMessages` swallows first (`[]` + ERROR
//! `Failed to get messages for chat {chatId}`), so preview answers 200 with
//! zero message matches and execute 200 with zero messages changed, while the
//! memory half of each still runs. The oracle WARMS the repository's messages
//! collection before the rename (v4 would otherwise re-create an empty table
//! on first touch and answer the same zeros with NO error) and records the
//! route's ERROR/WARN lines; [`compare_poisoned_logs`] holds v5's to them,
//! through a PROCESS-GLOBAL capture (execute's read runs on the writer
//! thread). Red-first at `a2db63da7` with `get_messages` still rethrowing:
//! preview 500 where v4 answers 200; execute 200 with a non-empty `errors`
//! where v4's is `[]`; both missing the ERROR line.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-search-replace.ndjson npx jest -- chat-dialogs-search-replace
//! Run:
//!   QT_ORACLE_SEARCH_REPLACE=/tmp/oracle-search-replace.ndjson \
//!     cargo test -p quilltap-harness --test search_replace_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::search_replace::{plan_memory_replacement, search_replace, Action};
use serde::Deserialize;
use serde_json::{json, Value};

const NORA: &str = "a2000000-0000-4000-8000-000000000001";
const SR_CHAT: &str = "c2000000-0000-4000-8000-000000000003";
const MISSING_ID: &str = "99999999-9999-4999-8999-999999999999";
/// The fixture's seed timestamp — what an untouched `updatedAt` still reads.
const SEED_ISO: &str = "2026-05-01T00:00:00.000Z";
/// The oracle's ticking frozen-clock base.
const FROZEN_NOW_ISO: &str = "2026-05-05T00:00:00.000Z";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-dialogs-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}
fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// A fixture copy in a scratch dir; the caller keeps the `TempDir` alive past
/// the `Db` (dropping it removes the dir).
fn fresh_db(spec: &Spec, tag: &str) -> (Db, tempfile::TempDir) {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-sr-{tag}-"))
        .tempdir()
        .expect("tempdir");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-dialogs-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-dialogs-mount.db"), &mount).unwrap();
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

// === P4.D298: bug 233's memory arm (append-only) ===
/// The twin of jest.setup's embedding stub (`generateEmbeddingForUser` →
/// `[0.1, 0.2, 0.3]`, model `test-model`, provider `TEST`), which the oracle's
/// `updateMemory` embeds through — the chat-dialogs pair has no embedding
/// profile, so v4's search-replace runs on it.
struct JestStubEmbedding;
impl quilltap_core::model::embedding::EmbeddingProvider for JestStubEmbedding {
    async fn generate_embedding_for_user(
        &self,
        _text: &str,
        _user_id: &str,
        _profile_id: Option<&str>,
        _opts: quilltap_core::model::embedding::EmbeddingCallOptions,
    ) -> Result<
        quilltap_core::model::embedding::EmbeddingResult,
        quilltap_core::model::embedding::EmbeddingError,
    > {
        Ok(quilltap_core::model::embedding::EmbeddingResult {
            embedding: vec![0.1, 0.2, 0.3],
            model: "test-model".into(),
            dimensions: 3,
            provider: "TEST".into(),
        })
    }
}

/// The oracle case whose embedding service REJECTS; v5 drives it with no
/// provider (`NoEmbeddingProvider`, L-2). The provider's message after `Could
/// not embed the edited memory: ` is MASKED.
const EMBEDDING_FAILS: &[&str] = &["execute_memory_embedding_failed_left_unchanged"];

/// The service lines bug 233 moved or v5 never logged — the oracle's
/// `isServiceLine` twin.
fn is_service_message(message: &str) -> bool {
    matches!(
        message,
        "Executing search/replace"
            | "Search/replace execution complete"
            | "Replaced text in memory and re-embedded it"
            | "Memory skipped during replace"
    ) || message.contains(" left unchanged: its new text could not be embedded (")
        || message.starts_with("Failed to update memory ")
        || message.starts_with("Failed to update messages in chat ")
}

/// `Could not embed the edited memory: <provider message>)` → masked.
fn mask_provider_message(s: &str) -> String {
    const PREFIX: &str = "Could not embed the edited memory: ";
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(PREFIX) {
        out.push_str(&rest[..i + PREFIX.len()]);
        out.push_str("<provider message>");
        let after = &rest[i + PREFIX.len()..];
        // The sentence's OWN closing paren — the provider message may carry
        // parens of its own: the one followed by a field, a JSON quote, or the
        // end of the text.
        let end = [") memoryId=", ")\"", ")"]
            .iter()
            .filter_map(|pat| {
                if *pat == ")" {
                    after.ends_with(')').then(|| after.len() - 1)
                } else {
                    after.find(pat)
                }
            })
            .min();
        rest = match end {
            Some(j) => &after[j..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// v4 `{level, message, fields}` → `<LEVEL> <message> k=v …` (the target
/// stripped; array / object fields as `…Json`, §R.5).
fn render_v4_service_line(line: &Value) -> String {
    let mut out = format!(
        "{} {}",
        line["level"].as_str().unwrap().to_uppercase(),
        line["message"].as_str().unwrap()
    );
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
    mask_provider_message(&out)
}

/// v5's captured service lines (`<LEVEL> <target> <message> k=v …`) in the
/// same rendering, the target stripped.
fn v5_service_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next()?;
            let target = parts.next()?;
            let rest = parts.next()?;
            (target == "quilltap_core::services::search_replace"
                && is_service_message(
                    rest.split(" memoryId=")
                        .next()
                        .unwrap_or(rest)
                        .split(" messagesUpdated=")
                        .next()
                        .unwrap_or(rest)
                        .split(" scopeJson=")
                        .next()
                        .unwrap_or(rest),
                ))
            .then(|| mask_provider_message(&format!("{level} {rest}")))
        })
        .collect()
}
// === end ===

/// P4.109's plant — byte-identical to the oracle case's.
const POISON_SQL: &str = r#"ALTER TABLE "chat_messages" RENAME TO "chat_messages_p4105_poisoned""#;

/// v4's BACKEND-level ERROR lines, logged beneath the repository by the
/// SQLite backend before it rethrows — never ported (P4.109 Tier 3 item 10,
/// named). Dropped from v4's side of the log comparison by name, nothing else.
const UNPORTED_BACKEND_LINES: &[&str] = &["Raw query failed", "SQLite find error", "findOne error"];

/// A PROCESS-GLOBAL capture: the execute path's message read runs inside
/// `db.write`, on the WRITER thread, where a thread-scoped subscriber sees
/// nothing. This binary holds one test, so a global default steals nothing.
fn install_global_capture() -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    use tracing_subscriber::layer::SubscriberExt;
    let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let subscriber =
        tracing_subscriber::registry().with(quilltap_core::test_support::CaptureLayer(buf.clone()));
    tracing::subscriber::set_global_default(subscriber).expect("one global capture per binary");
    buf
}

/// The poisoned arms' proof that the swallow FIRED: v4's recorded ERROR/WARN
/// lines (less [`UNPORTED_BACKEND_LINES`]) against v5's, by level, message and
/// `chatId`. A zero without its line is the warm-up trap on v4's side.
fn compare_poisoned_logs(name: &str, want: &Value, got: &[String]) -> Vec<String> {
    let v4: Vec<&Value> = want["logs"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|l| !UNPORTED_BACKEND_LINES.contains(&l["message"].as_str().unwrap_or("")))
                .collect()
        })
        .unwrap_or_default();
    if !v4.iter().any(|l| l["level"] == "error") {
        eprintln!("[{name}] v4 logged no repository ERROR — the warm-up trap: {want:?}");
        return vec![format!("{name}_v4_no_error_line")];
    }
    let v5: Vec<&String> = got
        .iter()
        .filter(|l| l.starts_with("ERROR ") || l.starts_with("WARN "))
        .collect();
    if v5.len() != v4.len() {
        eprintln!("[{name}] LOG COUNT — v4 {v4:?}\n  rust {v5:?}");
        return vec![format!("{name}_log_count")];
    }
    let mut failed = Vec::new();
    for (w, g) in v4.iter().zip(&v5) {
        let level = w["level"].as_str().unwrap_or("").to_uppercase();
        let mut ok = g.starts_with(&format!("{level} quilltap::db"))
            && g.contains(w["message"].as_str().unwrap_or("\u{0}"));
        if let Some(chat_id) = w["chatId"].as_str() {
            ok &= g.contains(&format!("chatId={chat_id}"));
        }
        if !ok {
            eprintln!("[{name}] LOG LINE — v4 {w}\n  rust {g}");
            failed.push(format!("{name}_log_line"));
        } else {
            eprintln!("[{name}] log line OK: {g}");
        }
    }
    failed
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatDialog(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                ErrorKind::Internal => 500,
                _ => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

/// The oracle's `readTables` mirror: SR_CHAT chat row + messages + every memory
/// reduced to the same columns.
fn dump_tables(db: &Db) -> Value {
    let chat = db
        .read_main(|c| quilltap_core::db::chats_read::find_by_id(c, SR_CHAT))
        .unwrap()
        .unwrap_or(Value::Null);
    let messages = Value::Array(
        db.read_main(|c| quilltap_core::db::chats_messages_read::get_messages(c, SR_CHAT))
            .unwrap(),
    );
    let rows = db
        .read_main(quilltap_core::db::memories_read::find_all)
        .unwrap();
    let mut memories: Vec<Value> = rows
        .into_iter()
        .map(|m| {
            json!({
                "id": m.get("id").cloned().unwrap_or(Value::Null),
                "characterId": m.get("characterId").cloned().unwrap_or(Value::Null),
                "chatId": m.get("chatId").cloned().unwrap_or(Value::Null),
                "sourceMessageId": m.get("sourceMessageId").cloned().unwrap_or(Value::Null),
                "content": m.get("content").cloned().unwrap_or(Value::Null),
                "summary": m.get("summary").cloned().unwrap_or(Value::Null),
                "keywords": m.get("keywords").cloned().unwrap_or(Value::Null),
                "embedding": m.get("embedding").cloned().unwrap_or(Value::Null),
                "updatedAt": m.get("updatedAt").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    memories.sort_by_key(|v| {
        v.get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    });
    json!({ "chat": chat, "messages": messages, "memories": Value::Array(memories) })
}

/// Prove-then-strip the minted memory `updatedAt`s: the touched-row SETS must
/// agree, v4's stamps sit at-or-after the frozen base, v5's differ from the
/// seed. Returns failure labels.
fn reconcile_memory_clocks(name: &str, got: &mut Value, want: &mut Value) -> Vec<String> {
    let mut failed = Vec::new();
    let (Some(g), Some(w)) = (
        got.get_mut("memories").and_then(Value::as_array_mut),
        want.get_mut("memories").and_then(Value::as_array_mut),
    ) else {
        return vec![format!("{name}_memories_shape")];
    };
    if g.len() != w.len() {
        return vec![format!("{name}_memories_len")];
    }
    for (gm, wm) in g.iter_mut().zip(w.iter_mut()) {
        let gid = gm.get("id").and_then(Value::as_str).unwrap_or("");
        let gu = gm
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let wu = wm
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let g_touched = gu != SEED_ISO;
        let w_touched = wu != SEED_ISO;
        if g_touched != w_touched {
            eprintln!(
                "[{name}] memory {gid}: touched-set drift (v5 touched={g_touched}, v4 touched={w_touched})"
            );
            failed.push(format!("{name}_{gid}_touched"));
        }
        if w_touched && wu.as_str() < FROZEN_NOW_ISO {
            eprintln!("[{name}] memory {gid}: v4 stamp {wu:?} before the frozen base");
            failed.push(format!("{name}_{gid}_v4_clock"));
        }
        if g_touched && gu.is_empty() {
            failed.push(format!("{name}_{gid}_v5_clock"));
        }
        gm.as_object_mut().unwrap().remove("updatedAt");
        wm.as_object_mut().unwrap().remove("updatedAt");
    }
    failed
}

#[test]
fn search_replace_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SEARCH_REPLACE") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert!(
        !oracle.is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();

    let chat_scope = json!({ "type": "chat", "chatId": SR_CHAT });
    let char_scope = json!({ "type": "character", "characterId": NORA });

    struct Case<'a> {
        name: &'a str,
        action: Action,
        scope: Value,
        search: &'a str,
        replace: &'a str,
        include_messages: Option<bool>,
        include_memories: Option<bool>,
        dump: bool,
        poison: bool,
    }
    let cases = [
        Case {
            name: "preview_chat_scope",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: false,
            poison: false,
        },
        Case {
            name: "preview_character_scope",
            action: Action::Preview,
            scope: char_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: false,
            poison: false,
        },
        Case {
            name: "preview_messages_only",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: Some(false),
            dump: false,
            poison: false,
        },
        Case {
            name: "preview_memories_only",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: Some(false),
            include_memories: None,
            dump: false,
            poison: false,
        },
        Case {
            name: "preview_no_match",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "gryphon",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: false,
            poison: false,
        },
        Case {
            name: "preview_chat_missing",
            action: Action::Preview,
            scope: json!({ "type": "chat", "chatId": MISSING_ID }),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: false,
            poison: false,
        },
        Case {
            name: "execute_chat_scope",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: true,
            poison: false,
        },
        Case {
            name: "execute_character_scope",
            action: Action::Execute,
            scope: char_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: true,
            poison: false,
        },
        Case {
            name: "execute_case_asymmetry",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "Lantern",
            replace: "Beacon",
            include_messages: None,
            include_memories: None,
            dump: true,
            poison: false,
        },
        Case {
            name: "execute_memories_only",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: Some(false),
            include_memories: None,
            dump: true,
            poison: false,
        },
        Case {
            name: "execute_no_match",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "gryphon",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: true,
            poison: false,
        },
        Case {
            name: "execute_invalid_scope",
            action: Action::Execute,
            scope: json!({ "type": "project", "projectId": SR_CHAT }),
            search: "x",
            replace: "y",
            include_messages: None,
            include_memories: None,
            dump: false,
            poison: false,
        },
        Case {
            name: "preview_empty_search",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "",
            replace: "y",
            include_messages: None,
            include_memories: None,
            dump: false,
            poison: false,
        },
        Case {
            name: "preview_poisoned",
            action: Action::Preview,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: false,
            poison: true,
        },
        Case {
            name: "execute_memory_keyword_only",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "lantern-",
            replace: "lamp-",
            include_messages: Some(false),
            include_memories: None,
            dump: true,
            poison: false,
        },
        Case {
            name: "execute_memory_case_only_skipped",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "LANTERN",
            replace: "x",
            include_messages: Some(false),
            include_memories: None,
            dump: true,
            poison: false,
        },
        Case {
            name: "execute_memory_embedding_failed_left_unchanged",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: true,
            poison: false,
        },
        Case {
            name: "execute_poisoned",
            action: Action::Execute,
            scope: chat_scope.clone(),
            search: "lantern",
            replace: "beacon",
            include_messages: None,
            include_memories: None,
            dump: true,
            poison: true,
        },
    ];

    let log_buf = install_global_capture();
    for c in cases {
        let (db, _scratch) = fresh_db(&spec, c.name);
        if c.poison {
            rt.block_on(db.write(|w| {
                w.main().connection().execute_batch(POISON_SQL)?;
                Ok(())
            }))
            .expect("plant the poison");
        }
        log_buf.lock().unwrap().clear();
        let stub = JestStubEmbedding;
        let r = rt.block_on(search_replace(
            &db,
            (!EMBEDDING_FAILS.contains(&c.name)).then_some(&stub),
            &spec.user_id,
            "UTC",
            c.action,
            &c.scope,
            c.search,
            c.replace,
            c.include_messages,
            c.include_memories,
        ));
        driven.insert(c.name.to_string());
        let Some(want) = oracle.get(c.name) else {
            failed.push(format!("{}_MISSING_FROM_ORACLE", c.name));
            continue;
        };
        let route_lines: Vec<String> = std::mem::take(&mut *log_buf.lock().unwrap());
        if c.poison {
            failed.extend(compare_poisoned_logs(c.name, want, &route_lines));
        }
        // P4.D298: the service lines, in order, against v4's.
        if let Some(want_lines) = want["serviceLines"].as_array() {
            let want_lines: Vec<String> = want_lines.iter().map(render_v4_service_line).collect();
            let got_lines = v5_service_lines(&route_lines);
            if got_lines != want_lines {
                eprintln!(
                    "[{}] SERVICE LINES:\n--- v4 ---\n{}\n--- v5 ---\n{}",
                    c.name,
                    want_lines.join("\n"),
                    got_lines.join("\n")
                );
                failed.push(format!("{}_service_lines", c.name));
            } else {
                eprintln!("[{}] service lines OK ({}).", c.name, got_lines.len());
            }
        }
        let (status, mut body) = status_body(&r);
        let mut want = want.clone();
        if EMBEDDING_FAILS.contains(&c.name) {
            for errors in [&mut body["errors"], &mut want["body"]["errors"]] {
                if let Some(a) = errors.as_array_mut() {
                    for e in a.iter_mut() {
                        *e = json!(mask_provider_message(e.as_str().unwrap_or("")));
                    }
                }
            }
        }
        let want = &want;
        if u64::from(status) != want["status"].as_u64().unwrap_or(0) {
            eprintln!("[{}] STATUS {status} != {}", c.name, want["status"]);
            failed.push(format!("{}_status", c.name));
        }
        if norm(&body) != norm(&want["body"]) {
            eprintln!(
                "[{}] BODY MISMATCH:\n{}",
                c.name,
                first_diff(&norm(&body), &norm(&want["body"]))
            );
            failed.push(c.name.to_string());
        } else {
            eprintln!("[{}] body OK.", c.name);
        }
        if c.dump {
            let mut got_tables = dump_tables(&db);
            let mut want_tables = want["tables"].clone();
            failed.extend(reconcile_memory_clocks(
                c.name,
                &mut got_tables,
                &mut want_tables,
            ));
            if norm(&got_tables) != norm(&want_tables) {
                eprintln!(
                    "[{} tables] MISMATCH:\n{}",
                    c.name,
                    first_diff(&norm(&got_tables), &norm(&want_tables))
                );
                failed.push(format!("{}_tables", c.name));
            } else {
                eprintln!("[{} tables] OK.", c.name);
            }
        }
    }

    // P4.D298: tier-1 — v4's REAL exported `planMemoryReplacement` over the
    // oracle's rows, against `plan_memory_replacement` (the patch's keys in v4's
    // order, `null` for no plan).
    {
        let name = "plan_memory_replacement_rows";
        driven.insert(name.to_string());
        match oracle.get(name).and_then(|r| r["rows"].as_array()) {
            None => failed.push(format!("{name}_MISSING_FROM_ORACLE")),
            Some(rows) => {
                let mut bad = 0;
                for row in rows {
                    let m = &row["memory"];
                    let keywords: Option<Vec<String>> = m["keywords"].as_array().map(|a| {
                        a.iter()
                            .filter_map(|k| k.as_str().map(str::to_string))
                            .collect()
                    });
                    let plan = plan_memory_replacement(
                        m["content"].as_str().unwrap(),
                        m["summary"].as_str().unwrap(),
                        keywords.as_deref(),
                        row["search"].as_str().unwrap(),
                        row["replace"].as_str().unwrap(),
                    );
                    let got = match plan {
                        None => Value::Null,
                        Some(p) => {
                            let mut o = serde_json::Map::new();
                            if let Some(c) = p.content {
                                o.insert("content".into(), json!(c));
                            }
                            if let Some(s) = p.summary {
                                o.insert("summary".into(), json!(s));
                            }
                            if let Some(k) = p.keywords {
                                o.insert("keywords".into(), json!(k));
                            }
                            Value::Object(o)
                        }
                    };
                    if serde_json::to_string(&got).unwrap()
                        != serde_json::to_string(&row["plan"]).unwrap()
                    {
                        eprintln!("[{name}] {row}: v5 {got}");
                        bad += 1;
                    }
                }
                if bad > 0 || rows.len() < 11 {
                    failed.push(name.to_string());
                } else {
                    eprintln!("[{name}] OK ({} rows).", rows.len());
                }
            }
        }
    }

    // The two RECORDED-ONLY middleware arms (module header): assert shape, no
    // v5 drive.
    for (name, want_status, want_error) in [
        ("unknown_action", 400, "Unknown action: rename"),
        (
            "missing_action",
            400,
            "Action parameter required: execute or preview",
        ),
    ] {
        driven.insert(name.to_string());
        let Some(want) = oracle.get(name) else {
            failed.push(format!("{name}_MISSING_FROM_ORACLE"));
            continue;
        };
        let ok = want["status"].as_u64() == Some(want_status)
            && want["body"]["error"].as_str() == Some(want_error);
        if !ok {
            eprintln!(
                "[{name}] the recorded middleware arm no longer holds: {:?}",
                want
            );
            failed.push(format!("{name}_recorded"));
        } else {
            eprintln!("[{name}] recorded v4 arm OK (no v5 counterpart by design).");
        }
        if name == "unknown_action"
            && want["body"]["availableActions"] != json!(["execute", "preview"])
        {
            failed.push(format!("{name}_available_actions"));
        }
    }

    let expected: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = expected.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case-set drift — oracle-only: {missing:?}; driven-only: {extra:?}"
    );
    assert!(failed.is_empty(), "search-replace mismatches: {failed:?}");
}
