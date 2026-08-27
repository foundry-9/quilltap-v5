//! P4.62 — the files-family edges' BODY GUARDS vs v4's REAL route handlers, over
//! real HTTP.
//!
//! The differential behind the wrong-type-collapse census's `files_routes.rs`
//! row. Of that file's seven closure-form (`.and_then(|v| v.as_str())`) sites,
//! FIVE read caller input and TWO read the server's OWN response entity:
//!
//! - **`content` / `encoding` on the mount-file PUT** — DIVERGENT, fixed here.
//!   v4 runs the whole body through `writeBodySchema` and answers
//!   `Invalid body: <joined zod issues>`; v5 invented its own `content is
//!   required` sentence and then silently ACCEPTED an unknown `encoding`, a
//!   negative or fractional `expected_mtime`, and a string `force`. (The last
//!   two are `as_i64`/`as_bool`, not the census needle — same class, found by
//!   reading the schema rather than the needle.)
//! - **the multipart `tags` part's `tagId`** — SPLIT. v4's `.map` on a truthy
//!   non-array throws to the outer catch as a **500**, where v5 answered 400;
//!   fixed. The wrong-typed *element* half is DIVERGENT-RECORDED and escalated —
//!   see `RECORDED_DIVERGENCES`.
//! - **`fileId` on the `?action=link` leg** — DIVERGENT, fixed here. v4's guard
//!   is `!fileId || typeof fileId !== 'string'`, so an EMPTY STRING is refused
//!   too; and v4 resolves the CHAT first, so on a missing chat the 404 precedes
//!   the 400 (the `a6870c5a` guard-order class).
//! - **the `attach-mount-file` `str_field`** — FAITHFUL. v4's two hand-rolled
//!   `!v || typeof v !== 'string'` guards refuse absent, null, wrong-typed and
//!   empty alike, and the core reproduces them AFTER the chat-404. The
//!   `attach_*` arms are what turn that reading into a measurement.
//! - **the 201-vs-200 `createdAt`/`updatedAt` pick** — FAITHFUL, entity reads:
//!   they parse the server's own `Files` response, never a request body. Not
//!   drivable as a guard arm; recorded in the census.
//!
//! ## Venue
//!
//! The committed `system-data-*` family, not the shared `chat-send` instance:
//! chat-send carries no `files` table at all, so every `?action=link` arm died
//! as a 500 `no such table: files` instead of reaching the guard under test.
//! (Measured, not assumed — that 500 is what the first run of this family
//! answered.) The materializer is duplicated from
//! `system_body_guards_equivalence` rather than added to `tests/common`, which
//! this lane does not own.
//!
//! Generate the oracle (Node 24, from the v4 checkout or a pinned worktree —
//! see the .test.ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-files-guards-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/files-body-guards.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-files-body-guards.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- files-body-guards
//! Run:
//!   QT_ORACLE_FILES_BODY_GUARDS=/tmp/oracle-files-body-guards.ndjson \
//!     cargo test -p quilltap-web --test files_body_guards_equivalence -- --nocapture

mod common;

use std::collections::HashMap;

use serde_json::{json, Value};

/// A chat the committed `system-data-main.db` fixture really carries.
const CHAT_OK: &str = "c1000000-0000-4000-8000-000000000001";
const CHAT_MISSING: &str = "c1000000-0000-4000-8000-0000000000ff";
const MOUNT: &str = "d1000000-0000-4000-8000-000000000001";

enum Kind {
    /// PUT the mount-file item route with this JSON body (`None` = not JSON).
    MountWrite(Option<Value>),
    /// POST `files?action=upload` with a `file` part and this raw `tags` field.
    Upload(&'static str),
    /// POST `chats/{chat}/files?action={action}` with this JSON body.
    ChatFiles {
        chat: &'static str,
        action: &'static str,
        body: Value,
    },
}

struct Case {
    name: &'static str,
    kind: Kind,
}

fn write(name: &'static str, body: Value) -> Case {
    Case {
        name,
        kind: Kind::MountWrite(Some(body)),
    }
}

fn link(name: &'static str, chat: &'static str, body: Value) -> Case {
    Case {
        name,
        kind: Kind::ChatFiles {
            chat,
            action: "link",
            body,
        },
    }
}

fn attach(name: &'static str, chat: &'static str, body: Value) -> Case {
    Case {
        name,
        kind: Kind::ChatFiles {
            chat,
            action: "attach-mount-file",
            body,
        },
    }
}

fn cases() -> Vec<Case> {
    vec![
        // ── the mount-file PUT's JSON leg: v4's `writeBodySchema` ──
        write("write_content_absent", json!({})),
        write("write_content_null", json!({"content": null})),
        write("write_content_number", json!({"content": 7})),
        write("write_content_object", json!({"content": {}})),
        write(
            "write_encoding_bad_enum",
            json!({"content": "hi", "encoding": "hex"}),
        ),
        write(
            "write_encoding_number",
            json!({"content": "hi", "encoding": 7}),
        ),
        write(
            "write_encoding_null",
            json!({"content": "hi", "encoding": null}),
        ),
        write(
            "write_mtime_negative",
            json!({"content": "hi", "expected_mtime": -5}),
        ),
        write(
            "write_mtime_float",
            json!({"content": "hi", "expected_mtime": 1.5}),
        ),
        write(
            "write_mtime_string",
            json!({"content": "hi", "expected_mtime": "5"}),
        ),
        write(
            "write_force_string",
            json!({"content": "hi", "force": "true"}),
        ),
        write("write_force_number", json!({"content": "hi", "force": 1})),
        // Three failing fields at once — joined in SCHEMA key order.
        write(
            "write_two_issues",
            json!({"encoding": "hex", "force": "true"}),
        ),
        Case {
            name: "write_body_not_json",
            kind: Kind::MountWrite(None),
        },
        write("write_body_null", Value::Null),
        write("write_body_number", json!(42)),
        write("write_body_array", json!([])),
        // ── the upload leg's `tags` part ──
        Case {
            name: "upload_tags_not_json",
            kind: Kind::Upload("{oops"),
        },
        Case {
            name: "upload_tags_object",
            kind: Kind::Upload("{\"tagId\":\"x\"}"),
        },
        Case {
            name: "upload_tags_number",
            kind: Kind::Upload("7"),
        },
        Case {
            name: "upload_tags_string",
            kind: Kind::Upload("\"x\""),
        },
        // ── chats/{id}/files: the CHAT is resolved before either guard ──
        link("link_missing_chat", CHAT_MISSING, json!({})),
        link("link_absent", CHAT_OK, json!({})),
        link("link_null", CHAT_OK, json!({"fileId": null})),
        link("link_empty", CHAT_OK, json!({"fileId": ""})),
        link("link_number", CHAT_OK, json!({"fileId": 7})),
        link("link_object", CHAT_OK, json!({"fileId": {}})),
        link(
            "link_unknown_string",
            CHAT_OK,
            json!({"fileId": "f0000000-0000-4000-8000-0000000000ff"}),
        ),
        attach("attach_missing_chat", CHAT_MISSING, json!({})),
        attach("attach_mount_absent", CHAT_OK, json!({})),
        attach(
            "attach_mount_null",
            CHAT_OK,
            json!({"mountPointId": null, "relativePath": "a.md"}),
        ),
        attach(
            "attach_mount_number",
            CHAT_OK,
            json!({"mountPointId": 7, "relativePath": "a.md"}),
        ),
        attach(
            "attach_mount_empty",
            CHAT_OK,
            json!({"mountPointId": "", "relativePath": "a.md"}),
        ),
        attach(
            "attach_path_absent",
            CHAT_OK,
            json!({"mountPointId": MOUNT}),
        ),
        attach(
            "attach_path_number",
            CHAT_OK,
            json!({"mountPointId": MOUNT, "relativePath": 7}),
        ),
        attach(
            "attach_unknown_file",
            CHAT_OK,
            json!({"mountPointId": MOUNT, "relativePath": "nope.md"}),
        ),
    ]
}

/// The shapes v4 ACCEPTS and v5 cannot yet reproduce, recorded rather than
/// fixed. Not drivable in a DB-free oracle (they reach `saveFileEntry`), so they
/// are asserted against v4's SOURCE, quoted, not against an oracle row:
///
/// - `tags: '[{"tagId": 5}]'` / `'[{}]'` — v4's `tags.map(t => t.tagId)` carries
///   the raw value into `linkedTo` and `tags`, so a number (or `undefined`)
///   lands in the row. v5 drops every non-string. Closing it needs
///   `Request::FileUpload.tags` widened past `Vec<String>` in
///   `quilltap-core/src/api/types.rs` — outside this lane's ownership, ORDERED
///   as the follow-up.
/// - `tags: 'null'` — parses to a FALSY value, so v4 skips the `.map` and
///   uploads with no tags. v5 now agrees (the `js_truthy` arm), but the oracle
///   cannot show it without a database, so it is not an arm.
const ESCALATED: &str = "files_routes.rs tags element types → quilltap-core/src/api/types.rs";

#[tokio::test(flavor = "multi_thread")]
async fn files_body_guards_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_FILES_BODY_GUARDS") else {
        eprintln!("SKIP: set QT_ORACLE_FILES_BODY_GUARDS (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let base = materialize_system_data_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    let all = cases();
    let mut failed: Vec<String> = Vec::new();
    for case in &all {
        let (status, got) = match &case.kind {
            Kind::MountWrite(body) => {
                let url = format!("http://{addr}/api/v1/mount-points/{MOUNT}/files/notes/a.md");
                let req = client.put(&url).header("content-type", "application/json");
                let req = match body {
                    None => req.body("this is not json at all"),
                    Some(v) => req.body(serde_json::to_string(v).unwrap()),
                };
                send(req).await
            }
            Kind::Upload(tags) => {
                let form = reqwest::multipart::Form::new()
                    .part(
                        "file",
                        reqwest::multipart::Part::bytes(vec![1u8, 2, 3])
                            .file_name("note.txt")
                            .mime_str("text/plain")
                            .unwrap(),
                    )
                    .text("tags", *tags);
                let resp = client
                    .post(format!("http://{addr}/api/v1/files?action=upload"))
                    .multipart(form)
                    .send()
                    .await
                    .unwrap();
                let s = resp.status().as_u16();
                let raw = resp.text().await.unwrap();
                (s, serde_json::from_str(&raw).unwrap_or(Value::String(raw)))
            }
            Kind::ChatFiles { chat, action, body } => {
                let url = format!("http://{addr}/api/v1/chats/{chat}/files?action={action}");
                send(
                    client
                        .post(&url)
                        .header("content-type", "application/json")
                        .body(serde_json::to_string(body).unwrap()),
                )
                .await
            }
        };

        let want = oracle
            .get(case.name)
            .unwrap_or_else(|| panic!("oracle missing case '{}'", case.name));
        let want_status = want["status"].as_u64().unwrap() as u16;
        if status != want_status {
            eprintln!("[{}] STATUS {status} != {want_status} ({got})", case.name);
            failed.push(format!("{}_status", case.name));
        } else if got != want["body"] {
            eprintln!("[{}] BODY {got} != {}", case.name, want["body"]);
            failed.push(case.name.to_string());
        } else {
            eprintln!("[{}] OK ({status}).", case.name);
        }
    }

    assert_eq!(
        all.len(),
        oracle.len(),
        "the Rust case list and the oracle disagree: {} vs {}",
        all.len(),
        oracle.len()
    );
    assert!(failed.is_empty(), "files body guards FAILED: {failed:?}");
    eprintln!(
        "OK: files body guards matched oracle ({} arms). Escalated, not fixed: {ESCALATED}.",
        all.len()
    );
}

/// See the module header's ## Venue note. Duplicated from
/// `system_body_guards_equivalence` — `tests/common` is outside this lane.
fn materialize_system_data_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    for (fixture, name) in [
        ("system-data-main.db", "quilltap.db"),
        ("system-data-mount.db", "quilltap-mount-index.db"),
        ("system-data-llmlogs.db", "quilltap-llm-logs.db"),
    ] {
        std::fs::copy(common::fixtures_dir().join(fixture), data.join(name))
            .unwrap_or_else(|e| panic!("copy {fixture}: {e}"));
    }
    base
}

async fn send(req: reqwest::RequestBuilder) -> (u16, Value) {
    let resp = req.send().await.unwrap();
    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap();
    (
        status,
        serde_json::from_str(&raw).unwrap_or(Value::String(raw)),
    )
}
