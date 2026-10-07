//! P4.163 (contract C1 item 3) — v4's per-repository CREATE wrap lines, diffed
//! against the `db::fallback` homes the import and the restore call when they
//! refuse a row.
//!
//! The oracle (`harness/oracle/cases/create-wrap-lines.ts`) drives v4's REAL
//! repositories' `create` (and `chats.addMessage`) with a payload that fails
//! its schema, plain and inside v4's REAL `withStrictRepositoryFailures`, and
//! records every ERROR/WARN line through a `Logger.prototype` spy — context in
//! v4's own key order, `undefined` fields dropped as winston drops them. This
//! side calls the v5 home for the same kind with the same context and the
//! oracle's OWN `error` text (the ZodError message — its bytes are
//! `repository_zod_messages`' business, not this family's) and compares the
//! rendered lines whole: sentence, level, key order, the bare `error`, and
//! `strictFailures=true` exactly where v4 appends it.
//!
//! Which lines are compared: every base-repository refusal logs `Data
//! validation failed {collection, error}` (`validate`) then `_create`'s line,
//! then the repository's wrap. The characters repository's pair is ONE home
//! (`log_character_create_failure` — it overrides `createErrorMessage()`), so
//! all three of its lines are compared; for every other kind the caller logs
//! the validation line itself (the restore's established shape), and this
//! family compares `_create`'s line (`log_create_failure`) and the wrap, and
//! asserts the validation line's shape on the v4 side. `chats.addMessage` is a
//! standalone `safeQuery`: ONE line, no `collection`.
//!
//! Regenerate (from a v4 checkout pinned at the baseline, Node 24):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-create-wrap-lines.ndjson
//!   cd ~/source/quilltap-server
//!   PATH=$N:$PATH npx tsx $V5W/harness/oracle/cases/create-wrap-lines.ts \
//!     > /tmp/oracle-create-wrap-lines.ndjson
//!   cd $V5W
//!   QT_ORACLE_CREATE_WRAP_LINES=/tmp/oracle-create-wrap-lines.ndjson \
//!     cargo test -p quilltap-harness --test create_wrap_lines_equivalence -- --nocapture

use quilltap_core::db::{fallback, DbError};
use serde_json::Value;

/// One v4 line rendered the way v5's capture rig renders a `quilltap::db`
/// event (`"<LEVEL> quilltap::db <message> k=v …"`).
fn v4_line(line: &Value) -> String {
    let level = line["level"].as_str().expect("level").to_ascii_uppercase();
    let mut out = format!(
        "{level} quilltap::db {}",
        line["message"].as_str().expect("message")
    );
    for pair in line["fields"].as_array().expect("fields") {
        let key = pair[0].as_str().expect("field key");
        let value = match &pair[1] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.push_str(&format!(" {key}={value}"));
    }
    out
}

/// The named context field's string value in a v4 line, if v4 logged it.
fn field<'a>(line: &'a Value, key: &str) -> Option<&'a str> {
    line["fields"]
        .as_array()?
        .iter()
        .find(|p| p[0] == key)
        .and_then(|p| p[1].as_str())
}

/// v5's lines for one oracle row: the home(s) for `label`, called with the
/// context the wrap line carries and the oracle's own error text, inside the
/// strict scope when the row was strict.
fn v5_lines(label: &str, strict: bool, wrap: &Value) -> Vec<String> {
    let error = DbError::Internal(field(wrap, "error").expect("error").to_string());
    let user_id = field(wrap, "userId");
    let name = field(wrap, "name");
    let provider = field(wrap, "provider");
    let run = || match label {
        "character" => {
            fallback::log_character_create_failure(&error);
            fallback::log_character_create_wrap_failure(user_id.unwrap(), name, &error);
        }
        "connection_profile" | "connection_profile_nameless" => {
            fallback::log_create_failure("connection_profiles", &error);
            fallback::log_connection_profile_create_wrap_failure(
                user_id.unwrap(),
                name,
                provider,
                &error,
            );
        }
        "image_profile" => {
            fallback::log_create_failure("image_profiles", &error);
            fallback::log_image_profile_create_wrap_failure(
                user_id.unwrap(),
                name,
                provider,
                &error,
            );
        }
        "embedding_profile" => {
            fallback::log_create_failure("embedding_profiles", &error);
            fallback::log_embedding_profile_create_wrap_failure(
                user_id.unwrap(),
                name,
                provider,
                &error,
            );
        }
        "file" => {
            fallback::log_create_failure("files", &error);
            fallback::log_file_create_wrap_failure(
                user_id.unwrap(),
                field(wrap, "filename"),
                &error,
            );
        }
        "folder" => {
            fallback::log_create_failure("folders", &error);
            fallback::log_folder_create_wrap_failure(user_id.unwrap(), field(wrap, "path"), &error);
        }
        "tag" => {
            fallback::log_create_failure("tags", &error);
            fallback::log_tag_create_wrap_failure(user_id.unwrap(), name, &error);
        }
        "roleplay_template" => {
            fallback::log_create_failure("roleplay_templates", &error);
            fallback::log_roleplay_template_create_wrap_failure(user_id.unwrap(), name, &error);
        }
        "prompt_template" => {
            fallback::log_create_failure("prompt_templates", &error);
            fallback::log_prompt_template_create_wrap_failure(user_id.unwrap(), name, &error);
        }
        "chat_message_add" => {
            fallback::log_chat_message_add_create_wrap_failure(
                field(wrap, "chatId").expect("chatId"),
                &error,
            );
        }
        other => panic!("no v5 home mapped for oracle label {other:?}"),
    };
    let ((), lines) = quilltap_core::test_support::captured_with(|| {
        if strict {
            fallback::with_strict_repository_failures(run)
        } else {
            run()
        }
    });
    lines
}

#[test]
fn create_wrap_lines_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_CREATE_WRAP_LINES") else {
        eprintln!("SKIP: set QT_ORACLE_CREATE_WRAP_LINES to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read the oracle NDJSON");
    let mut rows = 0usize;
    let mut seen = std::collections::BTreeSet::new();
    for raw in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(raw).expect("oracle row");
        let label = row["label"].as_str().expect("label");
        let strict = row["strict"].as_bool().expect("strict");
        assert_eq!(row["threw"], true, "{label}: v4's create must refuse");
        let logs = row["logs"].as_array().expect("logs");
        let wrap = logs.last().expect("v4 logged nothing");
        let compared = match label {
            "character" => logs.len(),
            "chat_message_add" => 1,
            _ => 2,
        };
        if label != "chat_message_add" {
            assert_eq!(
                logs.len(),
                3,
                "{label}: validate + _create + wrap: {logs:?}"
            );
            assert_eq!(logs[0]["message"], "Data validation failed", "{label}");
            let keys: Vec<&str> = logs[0]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p[0].as_str().unwrap())
                .collect();
            assert_eq!(keys, ["collection", "error"], "{label}: v4's validate line");
        } else {
            assert_eq!(logs.len(), 1, "{label}: a standalone safeQuery: {logs:?}");
        }
        let want: Vec<String> = logs[logs.len() - compared..].iter().map(v4_line).collect();
        let got = v5_lines(label, strict, wrap);
        assert_eq!(got, want, "{label} (strict={strict})");
        seen.insert(label.to_string());
        rows += 1;
    }
    assert_eq!(rows, 22, "eleven ops, each plain and strict");
    assert_eq!(seen.len(), 11, "{seen:?}");
    eprintln!("create_wrap_lines: {rows} rows matched");
}
