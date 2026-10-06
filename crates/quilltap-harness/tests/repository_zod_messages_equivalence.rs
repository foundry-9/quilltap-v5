//! Tier-1 differential: the `ZodError.message` bytes v4's repositories log and
//! throw when a row fails its schema (P4.130) — v4's REAL `GroupSchema`,
//! `GroupDocMountLinkSchema` and `ChatMetadataBaseSchema` under zod 4.6.5
//! against v5's `quilltap_core::api::zod_issues::{zod_group_issues,
//! zod_group_doc_mount_link_issues}` and `quilltap_core::services::
//! dangerous_content::chat_override::concierge_columns_zod_error`.
//!
//! Exact, per row: the rendered `error.message` (`JSON.stringify(issues, null,
//! 2)` — v4's `Data validation failed` / `Safe validation failed` `error` and
//! the restore/import warning tail) AND the compact issue list, both byte for
//! byte; a row v4 accepts must render no issue on v5. The corpus asks every
//! column absent / `null` / a number / a hydrated BLOB (`{"$float32": n}` — a
//! `Float32Array` on v4's side, as its SQLite collection decodes a BLOB in a
//! non-BLOB column; [`zod_float32_array_cell`] here) / at its boundary ±1,
//! including the two shapes the home could not render before P4.130: the
//! `invalid_union` a `TimestampSchema` reports over a non-string cell, and the
//! datetime `invalid_format` whose `pattern` is zod's JS-form regex. The five
//! chat messages P4.124 pinned by hand (`chat_override.rs`'s unit test) are
//! rows here, proven mechanically.
//!
//! P4.151 A1: the `chatSettings` rows run v4's REAL `ChatSettingsSchema` over
//! a stored `impersonationVoiceMode` and drive v5's REAL
//! `quilltap_core::db::chat_settings::find_by_user_id` over a `chat_settings`
//! table built from the D23 dump (`fresh_schema.json`), the mode planted by a
//! raw UPDATE as the core unit test does. v5's `message` is the `error=` tail
//! of the captured `Data validation failed` ERROR, and BOTH captured lines are
//! compared against v4's OWN two (P4.157 R-C — v4's real
//! `ChatSettingsRepository.findByUserId` over the row, through a
//! `Logger.prototype` spy: `Data validation failed` then the fallback
//! `safeQuery`'s `Error finding entity by filter`, level + message + context
//! in v4's key order); an accepted row must read back with zero lines on
//! both sides. This makes the Zod literal in
//! `chat_settings.rs`'s unit test
//! (`find_by_user_id_voice_mode_null_reads_off_and_an_unknown_value_drops_the_row`)
//! oracle-backed: reorder `ImpersonationVoiceMode::VALUES` or change
//! `ZodIssue::invalid_value`'s message and both go red together. A BLOB in the
//! mode column is NOT a row (R-D: writer-unreachable on both sides; v5's check
//! is `as_str()`-gated, so it would pass v5 and fail v4).
//!
//! Regenerate + run (self-contained; a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-repository-zod-messages.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/repository-zod-messages.ts \
//!     > /tmp/oracle-repository-zod-messages.ndjson
//!   cd $V5W
//!   QT_ORACLE_REPOSITORY_ZOD_MESSAGES=/tmp/oracle-repository-zod-messages.ndjson \
//!     cargo test -p quilltap-harness --test repository_zod_messages_equivalence -- --nocapture

use quilltap_core::api::zod_issues::{
    zod_error_message, zod_float32_array_cell, zod_group_doc_mount_link_issues, zod_group_issues,
    ZodIssue,
};
use quilltap_core::services::dangerous_content::chat_override::concierge_columns_zod_error;
use serde_json::{Map, Value};

/// The `chat_settings` DDL from the D23 dump — never a hand DDL.
fn chat_settings_ddl() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src/services/provisioning/fresh_schema.json");
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    schema["main"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|s| s.starts_with("CREATE TABLE \"chat_settings\" ("))
        .expect("fresh_schema.json carries chat_settings")
        .to_string()
}

/// v5's REAL `find_by_user_id` over the row, the mode planted raw. Answers
/// `(message, issues, lines)` — the captured lines whole; `(None, None, [])`
/// when the row reads back with no line.
fn v5_chat_settings(
    id: &str,
    row: &Map<String, Value>,
) -> (Option<String>, Option<String>, Vec<String>) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(&chat_settings_ddl()).unwrap();
    let user_id = row["userId"].as_str().unwrap();
    // The core unit test's column set (`chat_settings.rs`), the rest left to
    // the dump's defaults.
    conn.execute(
        "INSERT INTO chat_settings (id, userId, avatarDisplayMode, avatarDisplayStyle, \
         tagStyles, cheapLLMSettings, autoDetectRng, customTools, compositionModeDefault, \
         composerSpellcheck, impersonationVoiceMode, textReplacementsEnabled, \
         autoScrollOnResponseComplete, createdAt, updatedAt) VALUES (?1, ?2, 'ALWAYS', \
         'CIRCULAR', '{}', '{\"strategy\":\"PROVIDER_CHEAPEST\",\"fallbackToLocal\":true,\
         \"embeddingProvider\":\"OPENAI\"}', 1, 1, 0, 0, NULL, 1, 0, ?3, ?4)",
        rusqlite::params![
            row["id"].as_str().unwrap(),
            user_id,
            row["createdAt"].as_str().unwrap(),
            row["updatedAt"].as_str().unwrap(),
        ],
    )
    .unwrap();
    // ABSENT is the NULL cell (v4 reads NULL as `undefined`); a string is
    // planted raw, as no v5 writer would store it.
    let mode = row
        .get("impersonationVoiceMode")
        .cloned()
        .unwrap_or(Value::Null);
    let planted: Option<&str> = match &mode {
        Value::Null => None,
        Value::String(s) => Some(s),
        other => panic!("{id}: the corpus plants only strings / ABSENT, got {other}"),
    };
    conn.execute(
        "UPDATE chat_settings SET impersonationVoiceMode = ?1",
        [planted],
    )
    .unwrap();
    let (got, lines) = quilltap_core::test_support::captured_with(|| {
        quilltap_core::db::chat_settings::find_by_user_id(&conn, user_id).unwrap()
    });
    if let Some(read) = got {
        let want_mode = planted.unwrap_or("off");
        assert_eq!(
            read["impersonationVoiceMode"],
            Value::String(want_mode.into()),
            "{id}: the accepted row's mode"
        );
        return (None, None, lines);
    }
    let first = "ERROR quilltap::db Data validation failed collection=chat_settings error=";
    let message = lines
        .first()
        .and_then(|l| l.strip_prefix(first))
        .unwrap_or_else(|| panic!("{id}: first line {lines:#?}"))
        .to_string();
    let issues = serde_json::from_str::<Value>(&message).unwrap().to_string();
    (Some(message), Some(issues), lines)
}

/// v4's recorded logger call (`{level, message, context}`, P4.157 R-C) as the
/// capture rig renders v5's: `<LEVEL> quilltap::db <message> k=v …`, the
/// context's keys in v4's order. The target is v5's (`db::fallback`'s home);
/// everything after it is v4's.
fn render_v4_line(line: &Value) -> String {
    let mut out = format!(
        "{} quilltap::db {}",
        line["level"].as_str().expect("level").to_uppercase(),
        line["message"].as_str().expect("message")
    );
    if let Some(ctx) = line["context"].as_object() {
        for (k, v) in ctx {
            let v = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.push_str(&format!(" {k}={v}"));
        }
    }
    out
}

/// The corpus's `Float32Array` marker → this crate's.
fn materialize(row: &Map<String, Value>) -> Map<String, Value> {
    row.iter()
        .map(|(k, v)| {
            let cell = match v.get("$float32").and_then(Value::as_u64) {
                Some(n) if v.as_object().is_some_and(|o| o.len() == 1) => {
                    zod_float32_array_cell(n as usize)
                }
                _ => v.clone(),
            };
            (k.clone(), cell)
        })
        .collect()
}

#[test]
fn repository_zod_messages_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_REPOSITORY_ZOD_MESSAGES") else {
        eprintln!(
            "SKIP: set QT_ORACLE_REPOSITORY_ZOD_MESSAGES to the oracle NDJSON (see test header)."
        );
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    assert!(
        !text.trim().is_empty(),
        "{path} is EMPTY — a failed regen truncates its redirect"
    );

    let mut failures: Vec<String> = Vec::new();
    let (mut rows, mut ok_rows) = (0usize, 0usize);
    // The shapes the corpus must still ask, so a trimmed regen cannot go green.
    let (mut saw_union, mut saw_datetime, mut saw_float32, mut saw_astral_ok) =
        (false, false, false, false);
    let mut chat_messages = 0usize;
    let (mut settings_messages, mut settings_ok) = (0usize, 0usize);
    let mut settings_two_line_rows = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        rows += 1;
        let want: Value = serde_json::from_str(line).expect("oracle row is JSON");
        let id = want["id"].as_str().expect("id");
        let schema = want["schema"].as_str().expect("schema");
        let row = materialize(want["row"].as_object().expect("row"));
        let want_message = want.get("message").and_then(Value::as_str);
        if want["ok"] == Value::Bool(true) {
            ok_rows += 1;
            if id == "group-name-99x-astral" {
                saw_astral_ok = true;
            }
        }
        if let Some(m) = want_message {
            saw_union |= m.contains("\"code\": \"invalid_union\"");
            saw_datetime |= m.contains("\"format\": \"datetime\"");
            saw_float32 |=
                m.contains("received Float32Array") && m.contains("expected unknown to be <=100");
        }

        let (got_message, got_issues): (Option<String>, Option<String>) = match schema {
            "group" | "groupDocMountLink" => {
                let issues: Vec<ZodIssue> = if schema == "group" {
                    zod_group_issues(&row)
                } else {
                    zod_group_doc_mount_link_issues(&row)
                };
                if issues.is_empty() {
                    (None, None)
                } else {
                    (
                        Some(zod_error_message(&issues)),
                        Some(serde_json::to_string(&issues).unwrap()),
                    )
                }
            }
            "chatMetadataBase" => {
                if want_message.is_some() {
                    chat_messages += 1;
                }
                // The concierge twin renders only the message; the issue list
                // is that message re-parsed (key order kept — preserve_order).
                let m = concierge_columns_zod_error(&Value::Object(row));
                let issues = m
                    .as_deref()
                    .map(|m| serde_json::from_str::<Value>(m).unwrap().to_string());
                (m, issues)
            }
            "chatSettings" => {
                if want_message.is_some() {
                    settings_messages += 1;
                } else {
                    settings_ok += 1;
                }
                let (m, i, lines) = v5_chat_settings(id, &row);
                // P4.157 R-C: BOTH lines against v4's own (recorded through
                // its real `ChatSettingsRepository.findByUserId`) — the second
                // used to be checked only against v5's first.
                let want_lines: Vec<String> = want["lines"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{id}: the oracle row carries no `lines`"))
                    .iter()
                    .map(render_v4_line)
                    .collect();
                if want_message.is_some() {
                    settings_two_line_rows += usize::from(want_lines.len() == 2);
                }
                if lines != want_lines {
                    failures.push(format!(
                        "{id}: LINES differ\n  v4: {want_lines:#?}\n  v5: {lines:#?}"
                    ));
                    continue;
                }
                (m, i)
            }
            other => panic!("{id}: unknown schema {other}"),
        };
        if got_message.as_deref() != want_message {
            failures.push(format!(
                "{id}: MESSAGE differs\n  v4: {want_message:?}\n  v5: {got_message:?}"
            ));
            continue;
        }
        let want_issues = want.get("issues").map(Value::to_string);
        if got_issues != want_issues {
            failures.push(format!(
                "{id}: ISSUES differ\n  v4: {want_issues:?}\n  v5: {got_issues:?}"
            ));
        }
    }
    eprintln!("repository_zod_messages: {rows} rows ({ok_rows} accepted)");
    assert!(rows >= 58, "the corpus shrank ({rows} rows)");
    assert!(
        saw_union && saw_datetime && saw_float32 && saw_astral_ok,
        "the corpus must still ask the union ({saw_union}), datetime ({saw_datetime}), \
         Float32Array ({saw_float32}) and astral ({saw_astral_ok}) shapes"
    );
    assert_eq!(
        chat_messages, 6,
        "the six refused Concierge chat rows (P4.124's five + a numeric reason)"
    );
    assert_eq!(
        (settings_messages, settings_ok),
        (4, 3),
        "the chatSettings rows: four refused modes ('maybe', '', 'Off', '1') + three \
         accepted (absent → 'off', 'ask', 'always') — P4.151 A1"
    );
    assert_eq!(
        settings_two_line_rows, 4,
        "v4 logged its two ERROR lines on each refused chatSettings row (P4.157 R-C)"
    );
    assert!(
        failures.is_empty(),
        "{} row(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
