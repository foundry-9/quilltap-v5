//! Tier-3 differential (P4.170 B.2): the `WARDROBE_OUTFIT_ANNOUNCEMENT` job
//! body — `services::aurora_notifications::handle_wardrobe_outfit_announcement`
//! against v4's REAL `handleWardrobeOutfitAnnouncement`
//! (`lib/background-jobs/handlers/wardrobe-announcement.ts`, `322a5488e`).
//!
//! Both sides open ONE copy of the wardrobe-tools seed pair (a chat whose
//! seeded `equippedOutfit` dresses the caller) and run the corpus
//! (`harness/oracle/fixtures/wardrobe-announcement-job.json`) IN ORDER: each
//! case's `plants` (raw SQL on the main copy — cumulative), then ONE handler
//! call for the job `{ id, payload: { chatId, characterId } }`. Compared per
//! case:
//!   - the `[WardrobeAnnouncement]` lines whole — level, message, every context
//!     field in v4's order (v5's `issuesJson` read under v4's `issues`, the
//!     `…Json` suffix rule), incl. v4's start INFO (with the job id) and the
//!     slot-shape WARN carrying `EquippedSlotsSchema`'s FIRST THREE issues;
//!   - the `chat_messages` rows the call appended (`role`, `content`,
//!     `systemSender`, `systemKind`) — the Aurora whisper, or nothing.
//!
//! RED-FIRST: on unported main the handler logged neither line and skipped a
//! non-object shape silently but accepted a non-UUID element, a `null` slot and
//! a wrongly-typed array as a dressed slot — every warn row, and every INFO,
//! diverged.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout — the pinned
//! worktree while v4 is past the baseline): see the .ts header. Run:
//!   QT_ORACLE_WARDROBE_ANNOUNCEMENT=/tmp/oracle-wardrobe-announcement-job.ndjson \
//!   QT_FIXTURE_WT_MAIN=/tmp/qt-wann-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wann-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_announcement_job_tier3_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::aurora_notifications::handle_wardrobe_outfit_announcement;
use quilltap_core::test_support::global_capture;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    #[serde(rename = "jobId")]
    job_id: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(default)]
    plants: Vec<String>,
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

/// A captured v5 line (`LEVEL target message k=v …`) → `[level, message,
/// [[key, value], …]]`, the `…Json` suffix stripped.
fn v5_line(line: &str) -> Value {
    let (level, rest) = line.split_once(' ').unwrap();
    let (_target, rest) = rest.split_once(' ').unwrap();
    let bytes = rest.as_bytes();
    let mut keys: Vec<(usize, usize)> = Vec::new();
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i - 1] == b' ' {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i && j < bytes.len() && bytes[j] == b'=' && bytes[i].is_ascii_alphabetic() {
                keys.push((i, j));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    let message_end = keys.first().map(|(k, _)| k - 1).unwrap_or(rest.len());
    let fields: Vec<Value> = keys
        .iter()
        .enumerate()
        .map(|(n, (start, eq))| {
            let end = keys
                .get(n + 1)
                .map(|(next, _)| next - 1)
                .unwrap_or(rest.len());
            let key = &rest[*start..*eq];
            let key = key
                .strip_suffix("Json")
                .filter(|k| !k.is_empty())
                .unwrap_or(key);
            json!([key, &rest[eq + 1..end]])
        })
        .collect();
    json!([level.to_lowercase(), &rest[..message_end], fields])
}

fn v4_line(v: &Value) -> Value {
    let fields: Vec<Value> = v["fields"]
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, val)| {
                    let rendered = match val {
                        Value::String(s) => s.clone(),
                        other => serde_json::to_string(other).unwrap(),
                    };
                    json!([k, rendered])
                })
                .collect()
        })
        .unwrap_or_default();
    json!([v["level"], v["message"], fields])
}

#[test]
fn wardrobe_announcement_job_matches_v4() {
    let (Ok(oracle), Ok(main_fx), Ok(mount_fx)) = (
        std::env::var("QT_ORACLE_WARDROBE_ANNOUNCEMENT"),
        std::env::var("QT_FIXTURE_WT_MAIN"),
        std::env::var("QT_FIXTURE_WT_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_WARDROBE_ANNOUNCEMENT + QT_FIXTURE_WT_MAIN + QT_FIXTURE_WT_MOUNT (see header)."
        );
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("wardrobe-tools.json")).unwrap(),
    )
    .unwrap();
    let corpus: Corpus = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("wardrobe-announcement-job.json")).unwrap(),
    )
    .unwrap();
    let rows: Vec<Value> = std::fs::read_to_string(&oracle)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        rows.len(),
        corpus.cases.len(),
        "one oracle row per case — regenerate"
    );

    let scratch = tempfile::Builder::new()
        .prefix("qt-wann-")
        .tempdir()
        .unwrap();
    let (main, mount) = (
        scratch.path().join("main.db"),
        scratch.path().join("mount.db"),
    );
    std::fs::copy(&main_fx, &main).unwrap();
    std::fs::copy(&mount_fx, &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let messages = |chat_id: &str| -> Vec<Value> {
        let chat_id = chat_id.to_string();
        db.read_main(move |c| {
            let mut stmt = c.prepare(
                "SELECT role, content, systemSender, systemKind FROM chat_messages WHERE chatId = ?1 ORDER BY rowid",
            )?;
            let rows = stmt
                .query_map([&chat_id], |r| {
                    Ok(json!({
                        "role": r.get::<_, Option<String>>(0)?,
                        "content": r.get::<_, Option<String>>(1)?,
                        "systemSender": r.get::<_, Option<String>>(2)?,
                        "systemKind": r.get::<_, Option<String>>(3)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
    };

    let mut failed = Vec::new();
    for (case, want) in corpus.cases.iter().zip(&rows) {
        assert_eq!(want["name"], case.name.as_str(), "oracle order");
        for sql in &case.plants {
            let sql = sql.clone();
            db.write_blocking(move |w| {
                w.main().connection().execute_batch(&sql)?;
                Ok(())
            })
            .expect("plant");
        }
        let before = messages(&case.chat_id).len();
        let payload = json!({ "chatId": case.chat_id, "characterId": case.character_id });
        let (result, lines) = rt.block_on(global_capture::capture_async(
            handle_wardrobe_outfit_announcement(&db, &case.job_id, &payload),
        ));
        result.unwrap_or_else(|e| panic!("[{}] handler: {e}", case.name));
        let appended: Vec<Value> = messages(&case.chat_id).split_off(before);
        let got_lines: Vec<Value> = lines
            .iter()
            .filter(|l| l.contains("[WardrobeAnnouncement]"))
            .map(|l| v5_line(l))
            .collect();
        let want_lines: Vec<Value> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(v4_line)
            .collect();
        if got_lines != want_lines {
            eprintln!(
                "[{}] lines\n  v5 {}\n  v4 {}",
                case.name,
                Value::Array(got_lines),
                Value::Array(want_lines)
            );
            failed.push(format!("{}:lines", case.name));
        }
        if Value::Array(appended.clone()) != want["appended"] {
            eprintln!(
                "[{}] appended\n  v5 {}\n  v4 {}",
                case.name,
                Value::Array(appended),
                want["appended"]
            );
            failed.push(format!("{}:appended", case.name));
        }
    }
    assert!(failed.is_empty(), "announcement job diverged: {failed:?}");
}
