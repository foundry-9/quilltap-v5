//! Differential test (W4.1d5): the `send_mail` / `list_email` / `ask_carina` tool
//! handlers vs v4's REAL handlers.
//!
//! - **mail** (tier-2, real-DB): drive `execute_send_mail` (delivery clock pinned
//!   to the fixture's `fixedSentAt`) + `execute_list_email` over a fresh two-DB
//!   fixture copy per scenario; diff the serialized output + `format*` strings, and
//!   READ BACK the delivered letter's content byte-for-byte (proving frontmatter +
//!   body + reply preface landed). No mount-index remap is needed — the minted file
//!   ids never surface in the compared output, and the clock is pinned.
//! - **carina** (tier-3, DB-free): inject a canned `RunCarinaQuery` + a recording
//!   `PostProsperoCarinaError` (mirroring the oracle's jest mocks); diff the
//!   serialized output + `format*` + the recorded Prospero args.
//!
//! Generate the fixture + oracles (Node 24, from the v4 checkout). STAGE the case
//! files outside any `.claude/` path (jest ignores them there):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   STAGE=/tmp/qt-mail-carina-tools-stage
//!   # The jest `--` filters are ANCHORED (`…\.test\.ts$`): they match the whole
//!   # PATH, and an unanchored `carina-tool` also matches the stage DIRECTORY, which
//!   # dragged `mail-tools.test.ts` into the carina run without its fixture env.
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $V5W/harness/oracle/cases/{mail-tools,carina-tool}.test.ts $STAGE/harness/oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/mail-carina-tools.json        $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-mail-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mail-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-mail-carina-tools-fixture.ts
//!   TZ=UTC QT_FIXTURE_TMP_MAIN=/tmp/qt-mail-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mail-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-mail-tools.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "mail-tools\.test\.ts$"
//!   QT_ORACLE_OUT=/tmp/oracle-carina-tool.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "carina-tool\.test\.ts$"
//! Run:
//!   QT_ORACLE_MAIL=/tmp/oracle-mail-tools.ndjson QT_ORACLE_CARINA=/tmp/oracle-carina-tool.ndjson \
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-mail-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mail-mount.db \
//!     cargo test -p quilltap-harness --test mail_carina_tools_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::database_store::read_database_document;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::DbError;
use quilltap_core::services::carina_runner::{
    CarinaError, CarinaErrorKind, CarinaResult, CarinaRunError, PostProsperoCarinaError,
    PostedCarinaMessage, ProsperoCarinaErrorArgs, RunCarinaQuery, RunCarinaQueryOptions,
};
use quilltap_core::tools::ask_carina::{execute_ask_carina, format_ask_carina_results};
use quilltap_core::tools::list_email::{execute_list_email, format_list_email_results};
use quilltap_core::tools::send_mail::{execute_send_mail, format_send_mail_results};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "fixedSentAt")]
    fixed_sent_at: String,
    #[serde(rename = "senderId")]
    sender_id: String,
    #[serde(rename = "recipientId")]
    recipient_id: String,
    #[serde(rename = "emptyId")]
    empty_id: String,
    #[serde(rename = "archivedId")]
    archived_id: String,
    #[serde(rename = "archivedName")]
    archived_name: String,
    #[serde(rename = "seedLetterInSenderMailbox")]
    seed_letter_in_sender_mailbox: SeedLetterRef,
}
#[derive(Deserialize)]
struct SeedLetterRef {
    path: String,
}

#[derive(Deserialize)]
struct Meta {
    #[serde(rename = "recipientVault")]
    recipient_vault: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/mail-carina-tools.json")
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

fn load_oracle(path: &str) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Value = serde_json::from_str(line).expect("oracle line parses");
        let label = row
            .get("label")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        map.insert(label, row);
    }
    map
}

fn clear(p: &Path) {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", p.display()));
    }
}

fn fresh_copy(main_fx: &str, mount_fx: &str, tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let main = dir.join(format!("qt-mail-main-rust-{pid}-{tag}.db"));
    let mount = dir.join(format!("qt-mail-mount-rust-{pid}-{tag}.db"));
    for p in [&main, &mount] {
        clear(p);
    }
    std::fs::copy(main_fx, &main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(mount_fx, &mount).unwrap_or_else(|e| panic!("copy mount: {e}"));
    (main, mount)
}

fn open_two_db(main: &Path, mount: &Path, pepper: &str) -> Db {
    Db::open(
        DbPaths {
            main: main.to_path_buf(),
            mount_index: Some(mount.to_path_buf()),
            llm_logs: None,
        },
        pepper,
    )
    .unwrap_or_else(|e| panic!("open two-db: {e}"))
}

#[tokio::test]
async fn mail_carina_tools_matches_oracle() {
    let (Some(mail_oracle), Some(carina_oracle), Some(main_fx), Some(mount_fx)) = (
        env_or_skip("QT_ORACLE_MAIL"),
        env_or_skip("QT_ORACLE_CARINA"),
        env_or_skip("QT_FIXTURE_TMP_MAIN"),
        env_or_skip("QT_FIXTURE_TMP_MOUNT"),
    ) else {
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(format!("{main_fx}.meta.json")).expect("read meta"),
    )
    .expect("parse meta");

    run_mail(
        &spec,
        &meta,
        &load_oracle(&mail_oracle),
        &main_fx,
        &mount_fx,
    )
    .await;
    run_carina(&load_oracle(&carina_oracle)).await;

    eprintln!("OK: mail-carina-tools differential matched the oracles.");
}

/// (send `(json, fmt)`, delivered-letter content, list `(json, fmt)`).
type MailResults = (
    Option<(String, String)>,
    Option<String>,
    Option<(String, String)>,
);

struct MailScenario {
    label: &'static str,
    send: Option<(Value, Option<String>)>, // (args, characterId)
    read_content: bool,
    list_character: Option<String>,
}

async fn run_mail(
    spec: &Spec,
    meta: &Meta,
    oracle: &HashMap<String, Value>,
    main_fx: &str,
    mount_fx: &str,
) {
    let sender = Some(spec.sender_id.clone());
    let scenarios = vec![
        MailScenario {
            label: "send_and_list",
            send: Some((
                json!({ "character": "Aurora", "message": "Hello Aurora, the stars are bright tonight." }),
                sender.clone(),
            )),
            read_content: true,
            list_character: Some(spec.recipient_id.clone()),
        },
        MailScenario {
            label: "reply",
            send: Some((
                json!({ "character": "Aurora", "message": "I will be there.", "in_reply_to": spec.seed_letter_in_sender_mailbox.path }),
                sender.clone(),
            )),
            read_content: true,
            list_character: None,
        },
        MailScenario {
            label: "reply_not_found",
            send: Some((
                json!({ "character": "Aurora", "message": "x", "in_reply_to": "Mail/9999999999999-from-nobody.md" }),
                sender.clone(),
            )),
            read_content: false,
            list_character: None,
        },
        MailScenario {
            label: "missing_message",
            send: Some((json!({ "character": "Aurora" }), sender.clone())),
            read_content: false,
            list_character: None,
        },
        MailScenario {
            label: "recipient_not_found",
            send: Some((
                json!({ "character": "Nobody", "message": "x" }),
                sender.clone(),
            )),
            read_content: false,
            list_character: None,
        },
        MailScenario {
            label: "no_character",
            send: Some((json!({ "character": "Aurora", "message": "x" }), None)),
            read_content: false,
            list_character: None,
        },
        MailScenario {
            label: "list_empty",
            send: None,
            read_content: false,
            list_character: Some(spec.empty_id.clone()),
        },
        MailScenario {
            label: "list_single",
            send: None,
            read_content: false,
            list_character: Some(spec.recipient_id.clone()),
        },
        // ── P4.D65: the three archived-character refusals (the banked P4.D63
        // unit-4 arms). All three fire on the tombstone FLAG, over a character
        // whose vault is perfectly intact.
        MailScenario {
            label: "send_from_archived_sender",
            send: Some((
                json!({ "character": "Aurora", "message": "One last letter." }),
                Some(spec.archived_id.clone()),
            )),
            read_content: false,
            list_character: None,
        },
        // BY ID. The character resolver deliberately keeps resolving an
        // archived character by exact id while skipping it on a NAME match, so
        // this is the only way to reach the RECIPIENT refusal at all.
        MailScenario {
            label: "send_to_archived_recipient_by_id",
            send: Some((
                json!({ "character": spec.archived_id, "message": "Are you still there?" }),
                sender.clone(),
            )),
            read_content: false,
            list_character: None,
        },
        // BY NAME — the other side of that same resolver rule: not found, and
        // the refusal is the ordinary no-such-soul one. Without this arm the id
        // case above could pass on a resolver that had stopped skipping
        // archived name matches.
        MailScenario {
            label: "send_to_archived_recipient_by_name",
            send: Some((
                json!({ "character": spec.archived_name, "message": "Are you still there?" }),
                sender.clone(),
            )),
            read_content: false,
            list_character: None,
        },
        MailScenario {
            label: "list_archived",
            send: None,
            read_content: false,
            list_character: Some(spec.archived_id.clone()),
        },
    ];

    for sc in &scenarios {
        let (main, mount) = fresh_copy(main_fx, mount_fx, sc.label);
        let db = open_two_db(&main, &mount, &spec.test_pepper_base64);

        let send = sc.send.clone();
        let read_content = sc.read_content;
        let list_character = sc.list_character.clone();
        let user_id = spec.user_id.clone();
        let now_iso = spec.fixed_sent_at.clone();
        let recipient_vault = meta.recipient_vault.clone();

        // Run send → read-back → list on one copy inside a single write closure.
        let (send_out, content, list_out): MailResults = db
            .write(move |writers| {
                let mount_c = writers.mount_index().expect("mount present").connection();
                let main_c = writers.main().connection();

                let mut send_out = None;
                let mut content = None;
                if let Some((args, cid)) = &send {
                    let out = execute_send_mail(
                        main_c,
                        mount_c,
                        &user_id,
                        cid.as_deref(),
                        args,
                        &now_iso,
                    );
                    let json = serde_json::to_string(&out).unwrap();
                    let fmt = format_send_mail_results(&out);
                    if read_content && out.success {
                        if let Some(path) = &out.path {
                            let doc = read_database_document(mount_c, &recipient_vault, path)
                                .map_err(|e| DbError::Internal(e.to_string()))?;
                            content = Some(doc.content);
                        }
                    }
                    send_out = Some((json, fmt));
                }

                let mut list_out = None;
                if let Some(cid) = &list_character {
                    let out = execute_list_email(main_c, mount_c, Some(cid), &json!({}));
                    list_out = Some((
                        serde_json::to_string(&out).unwrap(),
                        format_list_email_results(&out),
                    ));
                }
                Ok((send_out, content, list_out))
            })
            .await
            .expect("mail scenario write closure");

        let want = oracle
            .get(sc.label)
            .unwrap_or_else(|| panic!("mail oracle missing {}", sc.label));

        if let Some((json, fmt)) = &send_out {
            let ws = want
                .get("send")
                .unwrap_or_else(|| panic!("no send for {}", sc.label));
            assert_eq!(
                json.as_str(),
                ws.get("json").and_then(Value::as_str).unwrap(),
                "send json {}",
                sc.label
            );
            assert_eq!(
                fmt.as_str(),
                ws.get("fmt").and_then(Value::as_str).unwrap(),
                "send fmt {}",
                sc.label
            );
        }
        if let Some(content) = &content {
            assert_eq!(
                content.as_str(),
                want.get("content").and_then(Value::as_str).unwrap(),
                "letter content {}",
                sc.label
            );
        }
        if let Some((json, fmt)) = &list_out {
            let wl = want
                .get("list")
                .unwrap_or_else(|| panic!("no list for {}", sc.label));
            assert_eq!(
                json.as_str(),
                wl.get("json").and_then(Value::as_str).unwrap(),
                "list json {}",
                sc.label
            );
            assert_eq!(
                fmt.as_str(),
                wl.get("fmt").and_then(Value::as_str).unwrap(),
                "list fmt {}",
                sc.label
            );
        }

        drop(db);
        for p in [&main, &mount] {
            clear(p);
        }
    }
}

// ── carina (DB-free, canned seams) ─────────────────────────────────────────

/// A canned [`RunCarinaQuery`] returning a fixed [`CarinaResult`].
struct CannedCarina(CarinaResult);
impl RunCarinaQuery for CannedCarina {
    fn run(
        &mut self,
        _opts: RunCarinaQueryOptions,
    ) -> impl std::future::Future<Output = Result<CarinaResult, CarinaRunError>> + Send {
        let r = self.0.clone();
        async move { Ok(r) }
    }
}

/// A recording [`PostProsperoCarinaError`].
#[derive(Default)]
struct RecordingProspero(Vec<ProsperoCarinaErrorArgs>);
impl PostProsperoCarinaError for RecordingProspero {
    fn post(&mut self, args: ProsperoCarinaErrorArgs) -> Result<(), CarinaRunError> {
        self.0.push(args);
        Ok(())
    }
}

fn ok_result(answer: &str) -> CarinaResult {
    CarinaResult::Ok {
        answer: answer.to_string(),
        message_id: "msg-1".into(),
        message: PostedCarinaMessage {
            id: "msg-1".into(),
            message: json!({}),
            target_participant_ids: None,
        },
        answerer_id: "ans-1".into(),
        answerer_name: "Sage".into(),
    }
}

fn err_result(kind: CarinaErrorKind, detail: Option<&str>, name: Option<&str>) -> CarinaResult {
    CarinaResult::Err {
        error: CarinaError {
            kind,
            detail: detail.map(str::to_string),
            character_name: name.map(str::to_string),
        },
    }
}

async fn run_carina(oracle: &HashMap<String, Value>) {
    struct Case {
        label: &'static str,
        args: Value,
        result: CarinaResult,
    }
    let cases = vec![
        Case {
            label: "answer_public",
            args: json!({ "character": "Sage", "question": "What is the meaning of life?", "whisper": false }),
            result: ok_result("The answer is 42."),
        },
        Case {
            label: "answer_whisper",
            args: json!({ "character": "Sage", "question": "A secret?", "whisper": true }),
            result: ok_result("Between us: the treasure is under the oak."),
        },
        Case {
            label: "answer_default_whisper",
            args: json!({ "character": "Sage", "question": "No whisper key?" }),
            result: ok_result("Public by default."),
        },
        Case {
            label: "err_not_found",
            args: json!({ "character": "Ghost", "question": "Anyone there?" }),
            result: err_result(CarinaErrorKind::NotFound, None, None),
        },
        Case {
            label: "err_no_profile",
            args: json!({ "character": "Mute", "question": "Speak?" }),
            result: err_result(CarinaErrorKind::NoProfile, None, Some("Mute")),
        },
        Case {
            label: "err_llm_failed_detail",
            args: json!({ "character": "Sage", "question": "Overloaded?" }),
            result: err_result(
                CarinaErrorKind::LlmFailed,
                Some("rate limited"),
                Some("Sage"),
            ),
        },
        Case {
            label: "err_llm_failed_no_detail",
            args: json!({ "character": "Sage", "question": "Silent?" }),
            result: err_result(CarinaErrorKind::LlmFailed, None, Some("Sage")),
        },
        Case {
            label: "invalid_missing_question",
            args: json!({ "character": "Sage" }),
            result: ok_result("unused"),
        },
        Case {
            label: "invalid_nonobject",
            args: json!("nope"),
            result: ok_result("unused"),
        },
    ];

    for c in &cases {
        let mut runner = CannedCarina(c.result.clone());
        let mut prospero = RecordingProspero::default();
        let out = execute_ask_carina(
            &mut runner,
            &mut prospero,
            "user-1",
            "chat-1",
            Some("pp-asker"),
            &c.args,
        )
        .await;
        let got_json = serde_json::to_string(&out).unwrap();
        let got_fmt = format_ask_carina_results(&out);

        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("carina oracle missing {}", c.label));
        assert_eq!(
            got_json.as_str(),
            want.get("resultJson").and_then(Value::as_str).unwrap(),
            "carina json {}",
            c.label
        );
        assert_eq!(
            got_fmt.as_str(),
            want.get("formatted").and_then(Value::as_str).unwrap(),
            "carina fmt {}",
            c.label
        );

        // Prospero args.
        let got_prospero = match prospero.0.first() {
            Some(a) => json!({
                "kind": a.kind.as_str(),
                "characterName": a.character_name,
                "detail": a.detail,
                "whisper": a.whisper,
                "askerParticipantId": a.asker_participant_id,
            }),
            None => Value::Null,
        };
        assert_eq!(
            got_prospero,
            want.get("prospero").cloned().unwrap_or(Value::Null),
            "carina prospero {}",
            c.label
        );
    }
}
