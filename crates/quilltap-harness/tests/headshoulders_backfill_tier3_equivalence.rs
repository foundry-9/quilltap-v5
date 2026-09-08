//! P4.82 TIER-3 (mocked-LLM) differential for the
//! `CHARACTER_HEADSHOULDERS_BACKFILL` job handler:
//! `services::headshoulders_backfill_job::handle_headshoulders_backfill` vs
//! v4's REAL `handleCharacterHeadShouldersBackfill`. Both sides run over a
//! FRESH copy of the committed `headshoulders-{main,mount}.db` pair per case,
//! with the SAME canned cheap-LLM answer injected at the model boundary.
//!
//! Five comparands per case:
//!   * `threw` — v4 returns on every skip arm and only `generateField` throws,
//!     so this is what separates "job succeeded doing nothing" from "job
//!     failed and will be retried";
//!   * `calls` — the whole request: provider / baseUrl / model / temperature /
//!     maxTokens / profileParameters / messages. The prompts ARE the port, so
//!     `buildContextPrompt`'s seed placement and
//!     `HEAD_AND_SHOULDERS_PHYSICAL_PROMPT` are byte comparands, and so is the
//!     ABSENT tenth `generateField` argument (v4 never passes
//!     `profileParameters` here — it must be null on both sides);
//!   * `physicalDescription` as the overlay READS it back, plus the vault
//!     store's rendered `physical-prompts.json` BYTES (key order is v4's
//!     `renderPhysicalPromptsJson`) — the happy-path character carries all five
//!     other tiers, so dropping the `...pd` spread reddens it;
//!   * the `CHARACTER_WIZARD` `llm_logs` projection;
//!   * the six `[HeadShouldersBackfill]` log lines with their bags.
//!
//! **The oracle's `apiKey` field is deliberately NOT a comparand.** v4 resolves
//! the key and hands it to `generateField`, which passes it to
//! `provider.sendMessage(params, key)`; v5's [`CompletionProvider`] boundary
//! resolves the key BELOW itself, so no key reaches the seam. The key GATE is
//! measured instead by the two cases that turn on it — `no_api_key` (a
//! non-local selection whose profile has no key row: warn, no call) and
//! `local_selection` (an OLLAMA selection, whose key resolves to the empty
//! string: the call proceeds).
//!
//! **`physicalDescription.createdAt`/`updatedAt` are placeholdered.** v4's
//! parser and v5's `default_physical` both SYNTHESIZE them at READ time from
//! the wall clock — the `updatedAt` this handler writes into the merged object
//! never survives the vault re-render on either side. The synthesized `id`
//! (`stableUuidFromString("physical:<mountId>")`) is left alone and IS a
//! comparand.
//!
//! **`astral_split_pair` is a RECORDED, both-directions-pinned divergence.**
//! v4's `content.substring(0, 500)` can end on a LONE HIGH SURROGATE, which
//! `JSON.stringify` writes as `\ud83d` and SQLite stores; a Rust `String`
//! cannot hold one, so `utf16_prefix`'s `from_utf16_lossy` yields U+FFFD. The
//! case asserts BOTH sides explicitly (v4 ends `\u{d83d}` unpaired, v5 ends
//! U+FFFD, and both are exactly 500 UTF-16 units), so a change on either side
//! trips. The sibling `astral_even_boundary` case — the cut landing BETWEEN two
//! astral characters — is a plain equality and proves the cap counts UTF-16
//! units rather than chars or bytes.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-headshoulders-backfill.ndjson npx jest -- headshoulders-backfill-tier3
//! Run:
//!   QT_ORACLE_HEADSHOULDERS_BACKFILL=/tmp/oracle-headshoulders-backfill.ndjson \
//!     cargo test -p quilltap-harness --test headshoulders_backfill_tier3_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
};
use quilltap_core::services::headshoulders_backfill_job::{
    handle_headshoulders_backfill, HeadShouldersBackfillPayload,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

mod common;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct ReplySpec {
    text: Option<String>,
    repeat: Option<String>,
    count: Option<usize>,
    suffix: Option<String>,
    throws: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CaseSpec {
    name: String,
    /// A 1-based index into `characters`, or the string `"missing"`.
    character: Value,
    /// A 1-based index into `users`.
    user: usize,
    reply: Option<ReplySpec>,
}

#[derive(Deserialize)]
struct IdRow {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    missing_character_id: String,
    users: Vec<IdRow>,
    characters: Vec<IdRow>,
    handler_cases: Vec<CaseSpec>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/headshoulders.json")
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

fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-hs-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let llm_logs = scratch.join("llm-logs.db");
    std::fs::copy(fixtures_dir().join("headshoulders-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("headshoulders-mount.db"), &mount).unwrap();
    common::materialize_llm_logs(&llm_logs, TEST_PEPPER);
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(llm_logs),
        },
        TEST_PEPPER,
    )
    .expect("open db")
}

/// The canned answer, built from the spec recipe exactly as the oracle's
/// `replyText` builds it.
fn reply_text(r: &Option<ReplySpec>) -> String {
    let Some(r) = r else {
        return String::new();
    };
    if let Some(t) = &r.text {
        return t.clone();
    }
    let mut out = r
        .repeat
        .clone()
        .unwrap_or_default()
        .repeat(r.count.unwrap_or(0));
    out.push_str(r.suffix.as_deref().unwrap_or(""));
    out
}

// ── The model boundary ─────────────────────────────────────────────────────

/// Records every request and answers the case's canned reply (or throws), the
/// same contract as the oracle's `createLLMProvider` mock.
struct CannedProvider {
    reply: Option<ReplySpec>,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl CompletionProvider for CannedProvider {
    async fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        self.calls.lock().unwrap().push(json!({
            "provider": provider,
            "baseUrl": base_url,
            "model": params.model,
            "temperature": params.temperature,
            "maxTokens": params.max_tokens,
            "profileParameters": params.profile_parameters.clone().unwrap_or(Value::Null),
            "messages": params.messages.iter().map(|m| json!({
                "role": m.role.as_str(), "content": m.content,
            })).collect::<Vec<_>>(),
        }));
        if let Some(msg) = self.reply.as_ref().and_then(|r| r.throws.clone()) {
            return Err(CompletionError::new(msg));
        }
        Ok(CompletionResponse {
            content: reply_text(&self.reply),
            usage: None,
            finish_reason: None,
            attachment_results: None,
            cache_usage: None,
        })
    }
}

// ── The diff surface ───────────────────────────────────────────────────────

fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut m = Map::new();
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        other => other.clone(),
    }
}
fn norm(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
}

/// Placeholder the two READ-TIME-synthesized timestamps (see the header).
fn scrub_pd(pd: &Value) -> Value {
    let Value::Object(o) = pd else {
        return pd.clone();
    };
    let mut m = o.clone();
    for k in ["createdAt", "updatedAt"] {
        if m.contains_key(k) {
            m.insert(k.into(), Value::String("<ts>".into()));
        }
    }
    Value::Object(m)
}

/// The character's `physicalDescription` as the overlay reads it, plus the
/// rendered `physical-prompts.json` bytes.
fn dump_after(db: &Db, character_id: &str) -> (Value, Value) {
    let cid = character_id.to_string();
    let character = db
        .read_main(|main| {
            db.read_mount_index(|mount| {
                quilltap_core::db::characters_read::find_by_id(main, mount, &cid)
            })
        })
        .expect("read character");
    let Some(character) = character else {
        return (Value::Null, Value::Null);
    };
    let pd = character
        .get("physicalDescription")
        .map(scrub_pd)
        .unwrap_or(Value::Null);
    let mount_point_id = character
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let file = match mount_point_id {
        None => Value::Null,
        Some(mp) => db
            .read_mount_index(|conn| {
                let mut stmt = conn.prepare(
                    r#"SELECT d."content" FROM "doc_mount_documents" d
                         JOIN "doc_mount_files" f ON f."id" = d."fileId"
                         JOIN "doc_mount_file_links" l ON l."fileId" = f."id"
                        WHERE l."mountPointId" = ?1 AND l."relativePath" = ?2"#,
                )?;
                let out: Option<String> = stmt
                    .query_row(rusqlite::params![mp, "physical-prompts.json"], |r| r.get(0))
                    .ok();
                Ok(out)
            })
            .expect("read vault file")
            .map(Value::String)
            .unwrap_or(Value::Null),
    };
    (pd, file)
}

/// The oracle's four-column `llm_logs` projection.
fn dump_llm_logs(db: &Db) -> Value {
    let mut rows: Vec<Value> = db
        .read_llm_logs(|conn| {
            let mut stmt = conn.prepare(
                r#"SELECT "type", "characterId", "provider", "modelName" FROM "llm_logs""#,
            )?;
            let out = stmt
                .query_map([], |row| {
                    Ok(json!({
                        "type": row.get::<_, Option<String>>(0)?,
                        "characterId": row.get::<_, Option<String>>(1)?,
                        "provider": row.get::<_, Option<String>>(2)?,
                        "modelName": row.get::<_, Option<String>>(3)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(out)
        })
        .expect("read llm_logs");
    rows.sort_by_key(|v| v.to_string());
    Value::Array(rows)
}

/// v4's bag keys, in v5's snake_case tracing-field spelling.
fn snake(key: &str) -> &'static str {
    match key {
        "context" => "context",
        "jobId" => "job_id",
        "characterId" => "character_id",
        "length" => "length",
        other => panic!("unmapped oracle log-bag key {other}"),
    }
}

/// Render one oracle log line the way [`quilltap_core::test_support`]'s
/// `CaptureLayer` renders v5's: `"<LEVEL> <target> <message> <field>=<value>
/// …"` — `tracing` visits the format-args `message` FIRST and renders it with
/// no `=` and no quotes, then each declared field in source order.
///
/// The field names are v5's snake_case spellings of v4's camelCase bag keys
/// ([`snake`]); [`snake`] PANICS on an unmapped key, so a v4-side bag change
/// fails loudly here rather than silently dropping a field from the comparand.
fn expected_log_line(entry: &Value) -> String {
    let level = entry["level"].as_str().unwrap().to_uppercase();
    let message = entry["message"].as_str().unwrap();
    let mut out = format!("{level} quilltap_core::services::headshoulders_backfill_job {message}");
    if let Some(Value::Object(ctx)) = entry.get("context") {
        for (k, v) in ctx {
            let rendered = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.push_str(&format!(" {}={}", snake(k), rendered));
        }
    }
    out
}

#[test]
fn headshoulders_backfill_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_HEADSHOULDERS_BACKFILL") else {
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
    assert_eq!(
        oracle.len(),
        spec.handler_cases.len(),
        "the oracle NDJSON must carry one row per spec case (stale oracle?)"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();

    for case in &spec.handler_cases {
        let character_id = match &case.character {
            Value::String(s) if s == "missing" => spec.missing_character_id.clone(),
            Value::Number(n) => spec.characters[n.as_u64().unwrap() as usize - 1].id.clone(),
            other => panic!("bad character selector {other}"),
        };
        let user_id = spec.users[case.user - 1].id.clone();

        let db = fresh_db(&case.name);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let provider = CannedProvider {
            reply: case.reply.clone(),
            calls: Arc::clone(&calls),
        };
        let payload = HeadShouldersBackfillPayload {
            character_id: character_id.clone(),
        };
        let job_id = format!("oracle-hs-{}", case.name);
        let (outcome, log_lines) = quilltap_core::test_support::captured_with(|| {
            rt.block_on(handle_headshoulders_backfill(
                &db, &provider, &job_id, &user_id, &payload, 0,
            ))
        });

        let want = &oracle[&case.name];
        let name = &case.name;

        let want_threw = want["threw"].as_str();
        let got_threw = outcome.as_ref().err().map(String::as_str);
        if got_threw != want_threw {
            eprintln!("[{name}] THROW MISMATCH: got {got_threw:?} / want {want_threw:?}");
            failed.push(name.clone());
            continue;
        }

        // The request(s) that reached the model boundary — `apiKey` subtracted
        // (see the header).
        let got_calls = Value::Array(calls.lock().unwrap().clone());
        let want_calls = Value::Array(
            want["calls"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|c| {
                    let mut m = c.as_object().unwrap().clone();
                    m.remove("apiKey");
                    Value::Object(m)
                })
                .collect(),
        );
        if norm(&got_calls) != norm(&want_calls) {
            eprintln!(
                "[{name}] CALLS DIVERGE:\n--- got ---\n{}\n--- want ---\n{}",
                norm(&got_calls),
                norm(&want_calls)
            );
            failed.push(name.clone());
            continue;
        }

        let (got_pd, got_file) = dump_after(&db, &character_id);
        let want_pd = want
            .get("physicalDescription")
            .map(scrub_pd)
            .unwrap_or(Value::Null);

        // Every case but one must be surrogate-clean: the oracle's sanitizer
        // exists ONLY for the split-pair row, and if it ever fired anywhere
        // else it would be laundering a real divergence into a green.
        let sanitized = want["sanitizedLoneSurrogates"].as_bool().unwrap_or(false);
        assert_eq!(
            sanitized,
            name == "astral_split_pair",
            "[{name}] the oracle's lone-surrogate sanitizer fired on the wrong case"
        );

        // v5's own written prompt as UTF-16 units — the comparand that survives
        // a value a Rust `String` cannot hold.
        let got_units: Option<Vec<u16>> = got_pd
            .get("headAndShouldersPrompt")
            .and_then(Value::as_str)
            .map(|s| s.encode_utf16().collect());
        let want_units: Option<Vec<u16>> = want["headAndShouldersPromptUtf16"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_u64().unwrap() as u16).collect());

        // The one recorded divergence: v4 keeps the lone high surrogate the
        // 500-unit cut leaves behind; a Rust String cannot hold one, so
        // `utf16_prefix`'s `from_utf16_lossy` substitutes U+FFFD. Pinned in
        // BOTH directions over the LOSSLESS unit arrays.
        if name == "astral_split_pair" {
            let v4 = want_units.clone().expect("v4 wrote a prompt");
            let v5 = got_units.clone().expect("v5 wrote a prompt");
            assert_eq!(v4.len(), 500, "[{name}] v4's cut is 500 UTF-16 units");
            assert_eq!(v5.len(), 500, "[{name}] v5's cut is 500 UTF-16 units too");
            assert_eq!(
                *v4.last().unwrap(),
                0xd83d,
                "[{name}] v4 ends on the UNPAIRED high surrogate"
            );
            assert_eq!(
                *v5.last().unwrap(),
                0xfffd,
                "[{name}] v5 substitutes U+FFFD for the split pair"
            );
            assert_eq!(
                v4[..499],
                v5[..499],
                "[{name}] every unit BEFORE the split is identical"
            );
            continue;
        }
        if got_units != want_units {
            eprintln!("[{name}] written prompt units diverge");
            failed.push(name.clone());
            continue;
        }

        if norm(&got_pd) != norm(&want_pd) {
            eprintln!(
                "[{name}] physicalDescription DIVERGES:\n--- got ---\n{}\n--- want ---\n{}",
                norm(&got_pd),
                norm(&want_pd)
            );
            failed.push(name.clone());
            continue;
        }

        let want_file = want
            .get("physicalPromptsFile")
            .cloned()
            .unwrap_or(Value::Null);
        if got_file != want_file {
            eprintln!(
                "[{name}] physical-prompts.json DIVERGES:\n--- got ---\n{got_file}\n--- want ---\n{want_file}"
            );
            failed.push(name.clone());
            continue;
        }

        let got_logs_rows = dump_llm_logs(&db);
        let want_logs_rows = want.get("llmLogs").cloned().unwrap_or(json!([]));
        if norm(&got_logs_rows) != norm(&want_logs_rows) {
            eprintln!(
                "[{name}] llm_logs DIVERGE:\n--- got ---\n{}\n--- want ---\n{}",
                norm(&got_logs_rows),
                norm(&want_logs_rows)
            );
            failed.push(name.clone());
            continue;
        }

        let want_lines: Vec<String> = want["log"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(expected_log_line)
            .collect();
        let got_lines: Vec<String> = log_lines
            .iter()
            .filter(|l| l.contains("[HeadShouldersBackfill]"))
            .cloned()
            .collect();
        if got_lines != want_lines {
            eprintln!(
                "[{name}] LOG LINES DIVERGE:\n--- got ---\n{got_lines:#?}\n--- want ---\n{want_lines:#?}"
            );
            failed.push(name.clone());
            continue;
        }
    }

    assert!(failed.is_empty(), "cases diverged: {failed:?}");
}
