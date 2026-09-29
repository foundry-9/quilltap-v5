//! Differential test (W4.1d5, grown at P4.D234): the Post Office tools
//! (`send_mail` / `list_mail` / `read_mail` / `discard_mail`) + `ask_carina` vs
//! v4's REAL handlers.
//!
//! - **mail** (tier-2, real-DB): each scenario is an ordered list of OPS run on a
//!   fresh two-DB fixture copy inside ONE write closure (delivery clock pinned to
//!   the fixture's `fixedSentAt`): the four handlers (serialized output +
//!   `format*`), a letter's stored content read back byte-for-byte, the reader
//!   vault's link rows + file/document counts (the GC and case-sensitivity
//!   comparand), and `doc_read_file` through the doc-edit handler (the opacity
//!   covenant's contrast). No mount-index remap is needed — no minted id surfaces
//!   in a comparand, and the clock is pinned.
//!   P4.D234 (v4 `39bc98ffc` + `12c336fad`): `list_email` → `list_mail` (NO
//!   alias), `read_mail` + `discard_mail` over Bertie's postbox (every character
//!   in the fixture is OPAQUE, so each read/discard arm is the covenant-bypass
//!   proof), the `in_reply_to` file-name arms, and v5-side capture pins for
//!   v4's handler log lines (fire + silence) — see `assert_mail_logs` and
//!   `assert_catch_lines`.
//! - **carina** (tier-3, DB-free): inject a canned `RunCarinaQuery` + a recording
//!   `PostProsperoCarinaError` (mirroring the oracle's jest mocks); diff the
//!   serialized output + `format*` + the recorded Prospero args.
//!
//! Generate the fixture + oracles (Node 24, from a v4 tree at or after
//! `12c336fad` — the case imports `list-mail-handler` / `read-mail-handler` /
//! `discard-mail-handler`, which do not exist before it). STAGE the case files
//! outside any `.claude/` path (jest ignores them there):
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
use quilltap_core::tools::discard_mail::{execute_discard_mail, format_discard_mail_results};
use quilltap_core::tools::doc_edit::{
    execute_doc_edit_tool, format_doc_edit_results, DocEditToolContext,
};
use quilltap_core::tools::list_mail::{execute_list_mail, format_list_mail_results};
use quilltap_core::tools::read_mail::{execute_read_mail, format_read_mail_results};
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
    #[serde(rename = "readerId")]
    reader_id: String,
}
#[derive(Deserialize)]
struct SeedLetterRef {
    path: String,
}

#[derive(Deserialize)]
struct Meta {
    #[serde(rename = "recipientVault")]
    recipient_vault: String,
    #[serde(rename = "senderVault")]
    sender_vault: String,
    #[serde(rename = "readerVault")]
    reader_vault: String,
    #[serde(rename = "readerPaths")]
    reader_paths: HashMap<String, String>,
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

    let logs = run_mail(
        &spec,
        &meta,
        &load_oracle(&mail_oracle),
        &main_fx,
        &mount_fx,
    )
    .await;
    assert_mail_logs(&logs, &spec, &meta);
    assert_catch_lines(&spec, &meta, &main_fx, &mount_fx).await;
    run_carina(&load_oracle(&carina_oracle)).await;

    eprintln!("OK: mail-carina-tools differential matched the oracles.");
}

/// Which vault a `content` op reads from.
#[derive(Clone, Copy)]
enum VaultRef {
    Recipient,
    Reader,
}

/// One op of a mail scenario — the oracle case's `Op` union, in the same order.
#[derive(Clone)]
enum Op {
    Send(Value, Option<String>),
    List(String),
    Read(Value, Option<String>),
    Discard(Value, Option<String>),
    /// `None` path = the letter the last `send` delivered.
    Content(VaultRef, Option<String>),
    Mount,
    DocRead(String, String),
}

struct MailScenario {
    label: &'static str,
    ops: Vec<Op>,
}

const MISSING_ID: &str = "dead0000-0000-4000-8000-00000000dead";

fn bare(path: &str) -> String {
    path.strip_prefix("Mail/").unwrap_or(path).to_string()
}

fn no_ext(name: &str) -> String {
    name.strip_suffix(".md").unwrap_or(name).to_string()
}

fn scenarios(spec: &Spec, meta: &Meta) -> Vec<MailScenario> {
    let s = Some(spec.sender_id.clone());
    let r = Some(spec.reader_id.clone());
    let rid = spec.reader_id.clone();
    let p = |k: &str| {
        meta.reader_paths
            .get(k)
            .unwrap_or_else(|| panic!("meta readerPaths missing {k}"))
            .clone()
    };
    let seed = spec.seed_letter_in_sender_mailbox.path.clone();
    let send = |args: Value, cid: Option<String>| Op::Send(args, cid);
    let read = |letter: Value, cid: Option<String>| Op::Read(json!({ "letter": letter }), cid);
    let discard =
        |letter: Value, cid: Option<String>| Op::Discard(json!({ "letter": letter }), cid);
    let sent = Op::Content(VaultRef::Recipient, None);
    let at = |path: String| Op::Content(VaultRef::Reader, Some(path));
    let missing = Some(MISSING_ID.to_string());
    let archived = Some(spec.archived_id.clone());

    vec![
        MailScenario {
            label: "send_and_list",
            ops: vec![
                send(
                    json!({ "character": "Aurora", "message": "Hello Aurora, the stars are bright tonight." }),
                    s.clone(),
                ),
                sent.clone(),
                Op::List(spec.recipient_id.clone()),
            ],
        },
        MailScenario {
            label: "reply",
            ops: vec![
                send(
                    json!({ "character": "Aurora", "message": "I will be there.", "in_reply_to": seed }),
                    s.clone(),
                ),
                sent.clone(),
            ],
        },
        // P4.D234 (v4 `39bc98ffc`): `in_reply_to` takes the FILE NAME and the
        // recipient's letter STORES the resolved `Mail/…` path.
        MailScenario {
            label: "reply_bare_name",
            ops: vec![
                send(
                    json!({ "character": "Aurora", "message": "By its name alone.", "in_reply_to": no_ext(&bare(&seed)) }),
                    s.clone(),
                ),
                sent.clone(),
            ],
        },
        MailScenario {
            label: "reply_leading_slash",
            ops: vec![
                send(
                    json!({ "character": "Aurora", "message": "With a slash.", "in_reply_to": format!("/{seed}") }),
                    s.clone(),
                ),
                sent.clone(),
            ],
        },
        MailScenario {
            label: "reply_self_uri",
            ops: vec![
                send(
                    json!({ "character": "Aurora", "message": "By its URI.", "in_reply_to": format!("qtap://self/{seed}") }),
                    s.clone(),
                ),
                sent.clone(),
            ],
        },
        MailScenario {
            label: "reply_sub_path",
            ops: vec![send(
                json!({ "character": "Aurora", "message": "x", "in_reply_to": "Mail/sub/1700000000000-from-aurora.md" }),
                s.clone(),
            )],
        },
        MailScenario {
            label: "reply_not_found",
            ops: vec![send(
                json!({ "character": "Aurora", "message": "x", "in_reply_to": "Mail/9999999999999-from-nobody.md" }),
                s.clone(),
            )],
        },
        MailScenario {
            label: "missing_message",
            ops: vec![send(json!({ "character": "Aurora" }), s.clone())],
        },
        MailScenario {
            label: "recipient_not_found",
            ops: vec![send(
                json!({ "character": "Nobody", "message": "x" }),
                s.clone(),
            )],
        },
        MailScenario {
            label: "no_character",
            ops: vec![send(json!({ "character": "Aurora", "message": "x" }), None)],
        },
        MailScenario {
            label: "list_empty",
            ops: vec![Op::List(spec.empty_id.clone())],
        },
        MailScenario {
            label: "list_single",
            ops: vec![Op::List(spec.recipient_id.clone())],
        },
        // ── P4.D65: the three archived-character refusals.
        MailScenario {
            label: "send_from_archived_sender",
            ops: vec![send(
                json!({ "character": "Aurora", "message": "One last letter." }),
                archived.clone(),
            )],
        },
        // BY ID — the only way to reach the RECIPIENT refusal at all.
        MailScenario {
            label: "send_to_archived_recipient_by_id",
            ops: vec![send(
                json!({ "character": spec.archived_id, "message": "Are you still there?" }),
                s.clone(),
            )],
        },
        // BY NAME — the other side of that same resolver rule.
        MailScenario {
            label: "send_to_archived_recipient_by_name",
            ops: vec![send(
                json!({ "character": spec.archived_name, "message": "Are you still there?" }),
                s.clone(),
            )],
        },
        MailScenario {
            label: "list_archived",
            ops: vec![Op::List(spec.archived_id.clone())],
        },
        // ── P4.D234: read_mail (v4 `39bc98ffc`). Bertie is OPAQUE.
        MailScenario {
            label: "list_reader",
            ops: vec![Op::List(rid.clone()), Op::Mount],
        },
        MailScenario {
            label: "read_unalerted",
            ops: vec![
                read(json!(bare(&p("unalerted"))), r.clone()),
                at(p("unalerted")),
                Op::List(rid.clone()),
            ],
        },
        MailScenario {
            label: "read_already_alerted_no_ext",
            ops: vec![
                read(json!(no_ext(&bare(&p("alerted")))), r.clone()),
                at(p("alerted")),
            ],
        },
        MailScenario {
            label: "read_self_uri",
            ops: vec![read(
                json!(format!("qtap://self/{}", p("unalerted"))),
                r.clone(),
            )],
        },
        MailScenario {
            label: "read_leading_slash",
            ops: vec![read(json!(format!("/{}", p("unalerted"))), r.clone())],
        },
        MailScenario {
            label: "read_blank",
            ops: vec![read(json!(bare(&p("blank"))), r.clone())],
        },
        MailScenario {
            label: "read_extra_keys",
            ops: vec![Op::Read(
                json!({ "letter": bare(&p("unalerted")), "extra": 1 }),
                r.clone(),
            )],
        },
        MailScenario {
            label: "read_not_found",
            ops: vec![read(json!("no-such-letter"), r.clone())],
        },
        MailScenario {
            label: "read_rummage",
            ops: vec![read(json!("Notes/secret.md"), r.clone())],
        },
        MailScenario {
            label: "read_rummage_spaces",
            ops: vec![read(json!("   "), r.clone())],
        },
        MailScenario {
            label: "read_rummage_uri_after_slash",
            ops: vec![read(
                json!(format!("/qtap://self/{}", p("unalerted"))),
                r.clone(),
            )],
        },
        // The ref is resolved BEFORE the character lookup (M6's target).
        MailScenario {
            label: "read_rummage_before_lookup",
            ops: vec![read(json!("../secret.md"), missing.clone())],
        },
        MailScenario {
            label: "read_missing_character",
            ops: vec![read(json!(bare(&p("unalerted"))), missing.clone())],
        },
        MailScenario {
            label: "read_no_character",
            ops: vec![read(json!(bare(&p("unalerted"))), None)],
        },
        MailScenario {
            label: "read_parse_empty",
            ops: vec![read(json!(""), r.clone())],
        },
        MailScenario {
            label: "read_parse_nonstring",
            ops: vec![read(json!(5), r.clone())],
        },
        MailScenario {
            label: "read_parse_missing",
            ops: vec![Op::Read(json!({}), r.clone())],
        },
        MailScenario {
            label: "read_parse_nonobject",
            ops: vec![Op::Read(json!("nope"), r.clone())],
        },
        MailScenario {
            label: "read_archived",
            ops: vec![read(json!("anything.md"), archived.clone())],
        },
        MailScenario {
            label: "read_mixed_case_by_lower",
            ops: vec![
                read(json!(bare(&p("mixedCase")).to_lowercase()), r.clone()),
                Op::Mount,
                at(p("mixedCase")),
            ],
        },
        MailScenario {
            label: "read_protected",
            ops: vec![read(json!(bare(&p("protected"))), r.clone()), Op::Mount],
        },
        MailScenario {
            label: "read_hard_linked",
            ops: vec![
                read(json!(bare(&p("hardLinked"))), r.clone()),
                Op::Mount,
                at(p("hardLinkedTo")),
            ],
        },
        MailScenario {
            label: "covenant_contrast",
            ops: vec![
                Op::DocRead(format!("qtap://self/{}", p("unalerted")), rid.clone()),
                read(json!(bare(&p("unalerted"))), r.clone()),
            ],
        },
        // ── P4.D234: discard_mail (v4 `12c336fad`).
        MailScenario {
            label: "discard_ok",
            ops: vec![
                discard(json!(bare(&p("unalerted"))), r.clone()),
                at(p("unalerted")),
                Op::List(rid.clone()),
                Op::Mount,
            ],
        },
        MailScenario {
            label: "discard_twice",
            ops: vec![
                discard(json!(bare(&p("blank"))), r.clone()),
                discard(json!(bare(&p("blank"))), r.clone()),
            ],
        },
        MailScenario {
            label: "discard_hard_linked",
            ops: vec![
                discard(json!(bare(&p("hardLinked"))), r.clone()),
                Op::Mount,
                at(p("hardLinkedTo")),
            ],
        },
        MailScenario {
            label: "discard_protected",
            ops: vec![
                discard(json!(format!("qtap://self/{}", p("protected"))), r.clone()),
                Op::Mount,
            ],
        },
        MailScenario {
            label: "discard_mixed_case_by_lower",
            ops: vec![
                discard(json!(bare(&p("mixedCase")).to_lowercase()), r.clone()),
                Op::Mount,
            ],
        },
        MailScenario {
            label: "read_then_discard_then_read",
            ops: vec![
                read(json!(bare(&p("alerted"))), r.clone()),
                discard(json!(bare(&p("alerted"))), r.clone()),
                read(json!(bare(&p("alerted"))), r.clone()),
                Op::Mount,
            ],
        },
        MailScenario {
            label: "discard_not_found",
            ops: vec![discard(json!("no-such-letter.md"), r.clone())],
        },
        MailScenario {
            label: "discard_rummage",
            ops: vec![discard(json!("Mail/Mail/x"), r.clone())],
        },
        MailScenario {
            label: "discard_rummage_before_lookup",
            ops: vec![discard(json!("a\\b"), missing.clone())],
        },
        MailScenario {
            label: "discard_missing_character",
            ops: vec![discard(json!(bare(&p("unalerted"))), missing.clone())],
        },
        MailScenario {
            label: "discard_no_character",
            ops: vec![discard(json!(bare(&p("unalerted"))), None)],
        },
        MailScenario {
            label: "discard_parse_empty",
            ops: vec![discard(json!(""), r.clone())],
        },
        MailScenario {
            label: "discard_parse_nonobject",
            ops: vec![Op::Discard(Value::Null, r.clone())],
        },
        MailScenario {
            label: "discard_archived",
            ops: vec![discard(json!("anything.md"), archived.clone())],
        },
        MailScenario {
            label: "discard_other_characters_letter",
            ops: vec![
                discard(
                    json!(bare(&p("unalerted"))),
                    Some(spec.recipient_id.clone()),
                ),
                Op::Mount,
            ],
        },
    ]
}

/// The reader vault's link rows + the file/document counts — the oracle's
/// `mount` step, the same SQL both sides.
fn mount_rows(mount: &rusqlite::Connection, reader_vault: &str) -> Value {
    let mut stmt = mount
        .prepare(
            "SELECT relativePath, linkGroupId IS NOT NULL AS grouped, allowCharacterRead, allowCharacterWrite \
             FROM doc_mount_file_links WHERE mountPointId = ? ORDER BY relativePath",
        )
        .expect("prepare links");
    let links: Vec<Value> = stmt
        .query_map([reader_vault], |row| {
            Ok(json!({
                "relativePath": row.get::<_, String>(0)?,
                "grouped": row.get::<_, i64>(1)?,
                "allowCharacterRead": row.get::<_, i64>(2)?,
                "allowCharacterWrite": row.get::<_, i64>(3)?,
            }))
        })
        .expect("query links")
        .collect::<Result<_, _>>()
        .expect("collect links");
    let count = |t: &str| -> i64 {
        mount
            .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .expect("count")
    };
    json!({ "links": links, "files": count("doc_mount_files"), "documents": count("doc_mount_documents") })
}

/// Every Post Office / mail-handler line one op logged, for the pins.
fn mail_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .filter(|l| {
            l.contains("quilltap_core::tools::read_mail")
                || l.contains("quilltap_core::tools::discard_mail")
                || l.contains("quilltap_core::tools::list_mail")
                || l.contains("quilltap::post_office::")
        })
        .collect()
}

/// `(label, per-op steps, per-op mail log lines)`.
type ScenarioRun = (Vec<Value>, Vec<Vec<String>>);

async fn run_mail(
    spec: &Spec,
    meta: &Meta,
    oracle: &HashMap<String, Value>,
    main_fx: &str,
    mount_fx: &str,
) -> HashMap<&'static str, Vec<Vec<String>>> {
    let mut logs = HashMap::new();
    let scs = scenarios(spec, meta);
    assert_eq!(
        scs.len(),
        oracle.len(),
        "scenario count v5 {} vs oracle {}",
        scs.len(),
        oracle.len()
    );
    for sc in scs {
        let (main, mount) = fresh_copy(main_fx, mount_fx, sc.label);
        let db = open_two_db(&main, &mount, &spec.test_pepper_base64);

        let ops = sc.ops.clone();
        let user_id = spec.user_id.clone();
        let now_iso = spec.fixed_sent_at.clone();
        let vaults = (meta.recipient_vault.clone(), meta.reader_vault.clone());

        let (steps, op_logs): ScenarioRun = db
            .write(move |writers| {
                let mount_c = writers.mount_index().expect("mount present").connection();
                let main_c = writers.main().connection();
                let mut sent_path: Option<String> = None;
                let mut steps = Vec::new();
                let mut op_logs = Vec::new();
                for op in &ops {
                    let (step, lines) = quilltap_core::test_support::captured_with(|| -> Result<Value, DbError> {
                        Ok(match op {
                            Op::Send(args, cid) => {
                                let out = execute_send_mail(
                                    main_c,
                                    mount_c,
                                    &user_id,
                                    cid.as_deref(),
                                    args,
                                    &now_iso,
                                );
                                sent_path = if out.success { out.path.clone() } else { None };
                                json!({ "op": "send", "json": serde_json::to_string(&out).unwrap(), "fmt": format_send_mail_results(&out) })
                            }
                            Op::List(cid) => {
                                let out =
                                    execute_list_mail(main_c, mount_c, "chat-x", Some(cid), &json!({}));
                                json!({ "op": "list", "json": serde_json::to_string(&out).unwrap(), "fmt": format_list_mail_results(&out) })
                            }
                            Op::Read(args, cid) => {
                                let out =
                                    execute_read_mail(main_c, mount_c, "chat-x", cid.as_deref(), args);
                                json!({ "op": "read", "json": serde_json::to_string(&out).unwrap(), "fmt": format_read_mail_results(&out) })
                            }
                            Op::Discard(args, cid) => {
                                let out = execute_discard_mail(
                                    main_c,
                                    mount_c,
                                    "chat-x",
                                    cid.as_deref(),
                                    args,
                                );
                                json!({ "op": "discard", "json": serde_json::to_string(&out).unwrap(), "fmt": format_discard_mail_results(&out) })
                            }
                            Op::Content(vault, path) => {
                                let vault_id = match vault {
                                    VaultRef::Recipient => &vaults.0,
                                    VaultRef::Reader => &vaults.1,
                                };
                                let path = path.clone().or_else(|| sent_path.clone());
                                let content = match path {
                                    Some(p) => match read_database_document(mount_c, vault_id, &p) {
                                        Ok(doc) => Value::String(doc.content),
                                        Err(quilltap_core::db::database_store::StoreError::Store(e))
                                            if e.code
                                                == quilltap_core::db::database_store::DbStoreErrorCode::NotFound =>
                                        {
                                            Value::Null
                                        }
                                        Err(e) => return Err(DbError::Internal(e.to_string())),
                                    },
                                    None => Value::Null,
                                };
                                json!({ "op": "content", "content": content })
                            }
                            Op::Mount => json!({ "op": "mount", "rows": mount_rows(mount_c, &vaults.1) }),
                            Op::DocRead(uri, cid) => {
                                let ctx = DocEditToolContext {
                                    chat_id: "chat-x".to_string(),
                                    user_id: user_id.clone(),
                                    project_id: None,
                                    character_id: Some(cid.clone()),
                                    operator_override: false,
                                    files_dir: None,
                                    blob_webp: Default::default(),
                                    mount_pool: None,
                                };
                                let out = execute_doc_edit_tool(
                                    main_c,
                                    mount_c,
                                    "doc_read_file",
                                    &json!({ "uri": uri }),
                                    &ctx,
                                );
                                json!({ "op": "docRead", "success": out.success, "fmt": format_doc_edit_results(&out) })
                            }
                        })
                    });
                    steps.push(step?);
                    op_logs.push(mail_lines(lines));
                }
                Ok((steps, op_logs))
            })
            .await
            .expect("mail scenario write closure");

        let want = oracle
            .get(sc.label)
            .unwrap_or_else(|| panic!("mail oracle missing {}", sc.label));
        let want_steps = want
            .get("steps")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("no steps for {}", sc.label));
        assert_eq!(steps.len(), want_steps.len(), "step count {}", sc.label);
        for (i, (got, want)) in steps.iter().zip(want_steps).enumerate() {
            assert_eq!(got, want, "{} step {i}", sc.label);
        }
        logs.insert(sc.label, op_logs);

        drop(db);
        for p in [&main, &mount] {
            clear(p);
        }
    }
    logs
}

/// v5-side capture pins for v4's handler log lines (P4.D234 — every one was
/// absent from v5 before this lane: the four new read/discard lines, the
/// `discardLetter` debug, and the pre-existing `Reply target not in sender
/// mailbox` debug). Each fires on its own branch with v4's level, message and
/// fields, and stays silent on the siblings. The texts are v4's
/// (`read-mail-handler.ts`, `discard-mail-handler.ts`, `mailbox.ts`,
/// `deliver.ts` at `12c336fad`).
fn assert_mail_logs(logs: &HashMap<&'static str, Vec<Vec<String>>>, spec: &Spec, meta: &Meta) {
    let op = |label: &str, i: usize| -> &Vec<String> {
        &logs
            .get(label)
            .unwrap_or_else(|| panic!("no logs for {label}"))[i]
    };
    let r = &spec.reader_id;
    let rv = &meta.reader_vault;
    let p = |k: &str| meta.reader_paths[k].clone();

    // read_mail: letter read — markedAlerted true, then false.
    assert_eq!(
        op("read_unalerted", 0),
        &vec![format!(
            "DEBUG quilltap_core::tools::read_mail read_mail: letter read module=read-mail-handler chatId=chat-x characterId={r} path={} markedAlerted=true",
            p("unalerted")
        )],
        "read_unalerted logs"
    );
    assert_eq!(
        op("read_already_alerted_no_ext", 0),
        &vec![format!(
            "DEBUG quilltap_core::tools::read_mail read_mail: letter read module=read-mail-handler chatId=chat-x characterId={r} path={} markedAlerted=false",
            p("alerted")
        )],
    );
    // read_mail: no such letter.
    assert_eq!(
        op("read_not_found", 0),
        &vec![format!(
            "DEBUG quilltap_core::tools::read_mail read_mail: no such letter module=read-mail-handler chatId=chat-x characterId={r} path=Mail/no-such-letter.md"
        )],
    );
    // discardLetter + discard_mail: letter discarded (INFO), in that order.
    assert_eq!(
        op("discard_ok", 0),
        &vec![
            format!(
                "DEBUG quilltap::post_office::mailbox discardLetter vaultId={rv} path={} deleted=true",
                p("unalerted")
            ),
            format!(
                "INFO quilltap_core::tools::discard_mail discard_mail: letter discarded module=discard-mail-handler chatId=chat-x characterId={r} path={}",
                p("unalerted")
            ),
        ],
    );
    // The second discard of the same letter: deleted=false + no such letter.
    assert_eq!(
        op("discard_twice", 1),
        &vec![
            format!(
                "DEBUG quilltap::post_office::mailbox discardLetter vaultId={rv} path={} deleted=false",
                p("blank")
            ),
            format!(
                "DEBUG quilltap_core::tools::discard_mail discard_mail: no such letter module=discard-mail-handler chatId=chat-x characterId={r} path={}",
                p("blank")
            ),
        ],
    );
    // Reply target not in sender mailbox — logs the RESOLVED reference (an
    // unresolvable one stays raw).
    let sv = &meta.sender_vault;
    assert_eq!(
        op("reply_not_found", 0),
        &vec![format!(
            "DEBUG quilltap::post_office::deliver Reply target not in sender mailbox senderVaultId={sv} inReplyTo=Mail/9999999999999-from-nobody.md"
        )],
    );
    assert_eq!(
        op("reply_sub_path", 0),
        &vec![format!(
            "DEBUG quilltap::post_office::deliver Reply target not in sender mailbox senderVaultId={sv} inReplyTo=Mail/sub/1700000000000-from-aurora.md"
        )],
    );
    // Silence legs: every refusal before the vault, every successful reply,
    // every list — no mail line at all.
    for label in [
        "read_rummage",
        "read_rummage_spaces",
        "read_rummage_before_lookup",
        "read_missing_character",
        "read_no_character",
        "read_parse_empty",
        "read_parse_nonobject",
        "read_archived",
        "discard_rummage",
        "discard_parse_empty",
        "discard_archived",
        "reply",
        "reply_bare_name",
        "send_and_list",
        "list_reader",
        "list_single",
    ] {
        for (i, lines) in logs[label].iter().enumerate() {
            assert!(lines.is_empty(), "{label} op {i} logged {lines:?}");
        }
    }
}

/// The catch arms' ERROR lines (v4's `<tool> handler threw unexpectedly
/// {chatId}`) and `markAlerted`'s NOT_FOUND warn, pinned on a v5-ONLY plant.
///
/// ⚠ Not a differential: the plant (a mount index whose `doc_mount_file_links`
/// table is gone) reaches v4's catch only where a repository THROWS, and v4's
/// mount-index reads are fallback `withRawDb` / `safeQuery` calls that answer
/// null instead — so v4 would read the same broken store as an empty postbox or
/// an absent letter. v5's `read_database_document` / `list_database_files`
/// propagate the error (pre-existing, not this lane's surface), so here the
/// catch IS reachable, and it must log v4's line.
async fn assert_catch_lines(spec: &Spec, meta: &Meta, main_fx: &str, mount_fx: &str) {
    let (main, mount) = fresh_copy(main_fx, mount_fx, "catch-plant");
    let db = open_two_db(&main, &mount, &spec.test_pepper_base64);
    let reader = spec.reader_id.clone();
    let reader_vault = meta.reader_vault.clone();
    let letter = meta.reader_paths["unalerted"].clone();
    let lines: Vec<(String, String, Vec<String>)> = db
        .write(move |writers| {
            let mount_c = writers.mount_index().expect("mount present").connection();
            let main_c = writers.main().connection();

            // markAlerted on a letter that is not there: warn, no-op.
            let (res, warn) = quilltap_core::test_support::captured_with(|| {
                quilltap_core::post_office::mailbox::mark_alerted(
                    mount_c,
                    &reader_vault,
                    "Mail/gone.md",
                )
            });
            assert!(res.is_ok());
            let mut out = vec![("mark_alerted".to_string(), String::new(), mail_lines(warn))];

            mount_c
                .execute_batch("DROP TABLE doc_mount_file_links")
                .map_err(|e| DbError::Internal(e.to_string()))?;
            let name = letter.strip_prefix("Mail/").unwrap().to_string();
            let (o, l) = quilltap_core::test_support::captured_with(|| {
                execute_list_mail(main_c, mount_c, "chat-plant", Some(&reader), &json!({}))
            });
            out.push(("list".into(), o.listing, mail_lines(l)));
            let (o, l) = quilltap_core::test_support::captured_with(|| {
                execute_read_mail(
                    main_c,
                    mount_c,
                    "chat-plant",
                    Some(&reader),
                    &json!({ "letter": name }),
                )
            });
            out.push(("read".into(), o.text, mail_lines(l)));
            let (o, l) = quilltap_core::test_support::captured_with(|| {
                execute_discard_mail(
                    main_c,
                    mount_c,
                    "chat-plant",
                    Some(&reader),
                    &json!({ "letter": name }),
                )
            });
            out.push(("discard".into(), o.message, mail_lines(l)));
            Ok(out)
        })
        .await
        .expect("catch plant");

    assert_eq!(
        lines[0].2,
        vec![format!(
            "WARN quilltap::post_office::mailbox markAlerted: letter no longer present vaultId={} path=Mail/gone.md",
            meta.reader_vault
        )]
    );
    for (tool, prefix, module) in [
        (
            "list",
            "The Post Office stumbled and couldn't sort your post — ",
            "list-mail-handler",
        ),
        (
            "read",
            "The Post Office stumbled and couldn't fetch your letter — ",
            "read-mail-handler",
        ),
        (
            "discard",
            "The Post Office stumbled and the letter stays where it was — ",
            "discard-mail-handler",
        ),
    ] {
        let (_, text, l) = lines.iter().find(|(t, _, _)| t == tool).unwrap();
        assert!(text.starts_with(prefix), "{tool}: {text}");
        let errors: Vec<&String> = l.iter().filter(|x| x.starts_with("ERROR ")).collect();
        assert_eq!(errors.len(), 1, "{tool}: {l:?}");
        let want = format!(
            "ERROR quilltap_core::tools::{tool}_mail {tool}_mail handler threw unexpectedly module={module} chatId=chat-plant error="
        );
        assert!(errors[0].starts_with(&want), "{tool}: {}", errors[0]);
    }
    drop(db);
    for p in [&main, &mount] {
        clear(p);
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
