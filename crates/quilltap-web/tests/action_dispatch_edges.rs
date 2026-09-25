//! P4.D220 — v4 `ad1c4c37f`'s ONE `dispatchAction` primitive, end to end over
//! a live server, on the edges where the old `if (action)` fold was a DATA
//! defect: a bare `?action=` (or an unknown one) fell through to a default that
//! WRITES — a restore that ran, an upload that uploaded, a rule that was
//! created — or that answered for the wrong thing.
//!
//! v4's commit adds unit tests for these (`restore/route.test.ts`,
//! `actions.test.ts`), but every one MOCKS the handler it proves is not called.
//! A mock is not v4 in a route family (memory note `a-v4-mock-is-not-v4-in-a-
//! route-family`), so each vector is mirrored here as a REST arm over a real
//! committed fixture (per-run COPY), with the effect COUNTED where there is
//! one. The envelope BYTES are cross-compared against v4's real route modules
//! by `query_param_semantics_equivalence` (the `__empty` / `__unknown` rows of
//! `system_restore_post`, `files_item_get`, `chat_files_post_*`,
//! `text_replacements_post`, `images_collection_post`, `system_unlock_post`);
//! THIS family pins the order and the effect the tripwire cannot see.
//!
//!   1. `POST /system/restore` — bare and unknown are the envelope
//!      (`["upload","preview"]`), never the restore leg.
//!   2. `GET /files/{id}` — the gate runs BEFORE the file lookup: an unknown
//!      action on a MISSING file is 400, not 404.
//!   3. `POST /chats/{id}/files` — the gate runs AFTER v4's chat-404; a bare
//!      `?action=` with a real multipart file does NOT upload (counted).
//!   4. `POST /settings/text-replacements` — a bare action does NOT create a
//!      rule (counted), and is refused before the body is read.
//!   5. `POST /images` — a bare action is refused, not an import.
//!   6. `POST /terminals/{id}` — v4's `Missing or invalid action parameter` is
//!      the DEFAULT (absent action) only; an unknown action is the envelope
//!      (v5 answered the default's sentence for it — a pre-existing divergence
//!      closed here).
//!   7. `POST /system/unlock?action=change-passphrase` — v4 `runUnlockAction`'s
//!      now-awaited catch: a THROW inside the action is `Error in database key
//!      action` at ERROR + `serverError(<message>)`; a returned refusal writes
//!      no such line.
//!
//! Run:
//!   cargo test -p quilltap-web --test action_dispatch_edges

mod common;

use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

const PLANTED_CHAT: &str = "bb000000-0000-4000-8000-0000000000bb";
const MISSING: &str = "99999999-9999-4999-8999-999999999999";

async fn answer(resp: reqwest::Response) -> (u16, Value) {
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

fn envelope(action: &str, available: &[&str]) -> Value {
    json!({ "error": format!("Unknown action: {action}"), "availableActions": available })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bare_or_unknown_action_never_reaches_a_writing_default() {
    // Every test arms the global rig before any callsite is reached (the
    // binary's capture test shares callsites with these).
    global_capture::install();
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = |p: &str| format!("http://{addr}{p}");

    // --- 1. the restore POST (v4 `restore/route.test.ts`: `?action=wipe-
    //        everything` → 400 `['upload','preview']`, neither leg called) ---
    for (query, action) in [
        ("?action=wipe-everything", "wipe-everything"),
        ("?action=", ""),
    ] {
        let (status, body) = answer(
            client
                .post(url(&format!("/api/v1/system/restore{query}")))
                .json(&json!({ "uploadId": "u1", "mode": "replace" }))
                .send()
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            (status, body),
            (400, envelope(action, &["upload", "preview"])),
            "restore {query}: refused, never the restore leg"
        );
    }
    // The discriminator: an ABSENT action DOES reach the restore leg (its own
    // guard answers, not the envelope).
    let (status, body) = answer(
        client
            .post(url("/api/v1/system/restore"))
            .json(&json!({ "uploadId": "u1", "mode": "replace" }))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert!(
        body.get("availableActions").is_none(),
        "the absent action is the restore leg: {status} {body}"
    );

    // --- 1b. the character GET: the lookup precedes the gate (v4
    //         `characters/[id]/handlers/get.ts:35-39` runs `findById` →
    //         `notFound('Character')` BEFORE `dispatchAction`), so a bare or
    //         unknown action on a MISSING character is the 404, not the
    //         envelope. (Fixed at the `b0b6656b5` unification — v5 gated first.)
    for query in ["?action=zzz", "?action="] {
        let (status, body) = answer(
            client
                .get(url(&format!("/api/v1/characters/{MISSING}{query}")))
                .send()
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            status, 404,
            "characters GET {query} on a MISSING character: {body}"
        );
        assert!(
            body.get("availableActions").is_none(),
            "the 404 precedes the gate: {body}"
        );
    }

    // --- 2. the file GET: dispatched BEFORE the lookup ---
    for (query, action) in [("?action=zzz", "zzz"), ("?action=", "")] {
        let (status, body) = answer(
            client
                .get(url(&format!("/api/v1/files/{MISSING}{query}")))
                .send()
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            (status, body),
            (400, envelope(action, &["thumbnail"])),
            "files GET {query} on a MISSING file: the gate precedes the 404"
        );
    }
    // The discriminator: the ABSENT action reaches the download leg's own
    // lookup (this venue carries no `files` table, so that leg answers its own
    // failure — anything but the envelope).
    let (status, body) = answer(
        client
            .get(url(&format!("/api/v1/files/{MISSING}")))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert!(
        body.get("availableActions").is_none(),
        "the absent action is the download leg: {status} {body}"
    );

    // --- 5. the images POST: a bare action is not an import ---
    let (status, body) = answer(
        client
            .post(url("/api/v1/images?action="))
            .json(&json!({ "url": "https://example.invalid/x.png" }))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!((status, body), (400, envelope("", &["generate"])));
}

/// The `system-data` venue (the query-param family's) with one chat planted on
/// the per-run COPY — the chat-send venue carries no `files` table, so an
/// upload could not be counted there.
fn materialize_files_venue() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    for (fixture, name) in [
        ("system-data-main.db", "quilltap.db"),
        ("system-data-mount.db", "quilltap-mount-index.db"),
        ("system-data-llmlogs.db", "quilltap-llm-logs.db"),
    ] {
        std::fs::copy(common::fixtures_dir().join(fixture), data.join(name)).unwrap();
    }
    let w =
        quilltap_core::db::Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER)
            .unwrap();
    w.connection()
        .execute_batch(&format!(
            "CREATE TEMP TABLE \"ade_chat\" AS SELECT * FROM \"chats\" ORDER BY rowid LIMIT 1;
             UPDATE \"ade_chat\" SET \"id\" = '{PLANTED_CHAT}';
             INSERT INTO \"chats\" SELECT * FROM \"ade_chat\";
             DROP TABLE \"ade_chat\";"
        ))
        .unwrap();
    base
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bare_action_does_not_upload_a_chat_file() {
    global_capture::install();
    let base = materialize_files_venue();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = |p: &str| format!("http://{addr}{p}");

    // --- 3. the chat-file POST: after the chat-404, and a bare action does
    //        NOT upload ---
    let count_files = || async {
        // v5 registers no REST GET for the chat's files; the list rides
        // `/api/dispatch` (`chatFilesList`).
        let (status, body) = answer(
            client
                .post(url("/api/dispatch"))
                .json(&json!({ "type": "chatFilesList", "chatId": PLANTED_CHAT }))
                .send()
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, 200, "chat files list: {body}");
        let files = body
            .pointer("/data/files")
            .or_else(|| body.get("files"))
            .unwrap_or_else(|| panic!("no files array in {body}"));
        files.as_array().map(Vec::len).unwrap()
    };
    let file_form = || {
        reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes(b"hello".to_vec())
                .file_name("note.txt")
                .mime_str("text/plain")
                .unwrap(),
        )
    };
    let before = count_files().await;
    for (query, action) in [("?action=", ""), ("?action=zzz", "zzz")] {
        let (status, body) = answer(
            client
                .post(url(&format!("/api/v1/chats/{PLANTED_CHAT}/files{query}")))
                .multipart(file_form())
                .send()
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            (status, body),
            (400, envelope(action, &["link", "attach-mount-file"])),
            "chat files {query}: refused, not uploaded"
        );
    }
    assert_eq!(
        count_files().await,
        before,
        "a refused action must not upload"
    );
    // v4 resolves the chat FIRST: a missing chat is the 404 even for a bare
    // action.
    let (status, body) = answer(
        client
            .post(url(&format!("/api/v1/chats/{MISSING}/files?action=")))
            .multipart(file_form())
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        (status, body),
        (404, json!({ "error": "Chat not found" })),
        "the chat-404 precedes the action gate on this route"
    );
    // The discriminator: an ABSENT action uploads.
    let (status, body) = answer(
        client
            .post(url(&format!("/api/v1/chats/{PLANTED_CHAT}/files")))
            .multipart(file_form())
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, 200, "the absent action is the upload: {body}");
    assert_eq!(count_files().await, before + 1, "and it really uploaded");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bare_action_does_not_create_a_text_replacement_rule() {
    // Every test arms the global rig before any callsite is reached (the
    // binary's capture test shares callsites with these).
    global_capture::install();
    let base = common::materialize_text_replacements_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let list = || async {
        let (status, body) = answer(
            client
                .get(format!("http://{addr}/api/v1/settings/text-replacements"))
                .send()
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        body
    };
    let before = list().await;
    let rule = json!({ "fromText": "zzq", "toText": "zzr" });
    for (query, action) in [("?action=", ""), ("?action=zzz", "zzz")] {
        let (status, body) = answer(
            client
                .post(format!(
                    "http://{addr}/api/v1/settings/text-replacements{query}"
                ))
                .json(&rule)
                .send()
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            (status, body),
            (400, envelope(action, &["bulk-replace"])),
            "{query}"
        );
    }
    // Refused BEFORE the body is read: an unparseable body still gets the
    // envelope (v5 used to parse first and answer `Invalid body`).
    let (status, body) = answer(
        client
            .post(format!(
                "http://{addr}/api/v1/settings/text-replacements?action="
            ))
            .header("content-type", "application/json")
            .body("{not json")
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!((status, body), (400, envelope("", &["bulk-replace"])));
    assert_eq!(
        list().await,
        before,
        "a refused action must not create a rule"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn terminal_post_default_is_the_absent_action_only() {
    // Every test arms the global rig before any callsite is reached (the
    // binary's capture test shares callsites with these).
    global_capture::install();
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |c| c).await;
    let client = reqwest::Client::new();
    let post = |query: &str| {
        client
            .post(format!("http://{addr}/api/v1/terminals/{MISSING}{query}"))
            .json(&json!({}))
            .send()
    };
    // v4 `withActionDispatch({ kill, signal, write }, () => badRequest('Missing
    // or invalid action parameter'))` — the sentence is the DEFAULT.
    let (status, body) = answer(post("").await.unwrap()).await;
    assert_eq!(
        (status, body),
        (
            400,
            json!({ "error": "Missing or invalid action parameter" })
        )
    );
    for (query, action) in [("?action=zzz", "zzz"), ("?action=", "")] {
        let (status, body) = answer(post(query).await.unwrap()).await;
        assert_eq!(
            (status, body),
            (400, envelope(action, &["kill", "signal", "write"])),
            "terminal {query}"
        );
    }
}

/// v4 `runUnlockAction` (`system/unlock/route.ts:90-109` at `ad1c4c37f`): the
/// `try { return await run(body) }` catch. Posed here by corrupting the
/// `.dbkey` AFTER the engine unlocked, so the passphrase change THROWS inside
/// the action (v5's `Internal` kind — a `.dbkey` that no longer parses).
// CURRENT-thread runtime on purpose: `global_capture` is armed per THREAD,
// and the served handler must run on the capturing thread to be seen.
#[tokio::test]
async fn change_passphrase_throw_is_logged_and_answered_500() {
    global_capture::install();

    let base = common::materialize_fixture_instance();
    let data = base.path().join("data");
    quilltap_core::dbkey::save_dbkey(&data, common::TEST_PEPPER, "").expect("write .dbkey");

    let ((wrong_old, corrupt, thrown), lines) = global_capture::capture_async(async {
        let (addr, _state) = common::serve_instance(base.path(), |mut c| {
            c.terminal = false;
            c
        })
        .await;
        let client = reqwest::Client::new();
        let change = |body: Value| {
            client
                .post(format!(
                    "http://{addr}/api/v1/system/unlock?action=change-passphrase"
                ))
                .json(&body)
                .send()
        };
        // A RETURNED refusal first (the handler's own 401) — the silence leg.
        let wrong_old = answer(
            change(json!({ "oldPassphrase": "not-it", "newPassphrase": "x" }))
                .await
                .unwrap(),
        )
        .await;
        // A corrupt `.dbkey` is ALSO a returned refusal in v4: `readDbKeyFile`
        // catches the parse error and returns null, `changePassphrase` answers
        // `{success: false, error: 'No .dbkey file found'}`, the route 401s —
        // no throw, no catch line. (The unification review caught this test
        // posing the corrupt file as "the throw": v5 had answered 500 + the
        // line, a shape v4 never produces.)
        let dbkey = data.join("quilltap.dbkey");
        let good = std::fs::read(&dbkey).unwrap();
        std::fs::write(&dbkey, "{not json").unwrap();
        let corrupt = answer(
            change(json!({ "oldPassphrase": "", "newPassphrase": "x" }))
                .await
                .unwrap(),
        )
        .await;
        std::fs::write(&dbkey, &good).unwrap();
        // Now make the action THROW where v4 throws: the REWRITE fails. A
        // read-only file reads and decrypts fine; `writeFileSync` (v5:
        // `write_dbkey_file`) then refuses.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dbkey, std::fs::Permissions::from_mode(0o444)).unwrap();
        }
        let thrown = answer(
            change(json!({ "oldPassphrase": "", "newPassphrase": "x" }))
                .await
                .unwrap(),
        )
        .await;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dbkey, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        (wrong_old, corrupt, thrown)
    })
    .await;

    assert_eq!(
        wrong_old.0, 401,
        "a wrong old passphrase is the handler's own refusal: {wrong_old:?}"
    );
    assert_eq!(
        corrupt,
        (401, json!({ "error": "No .dbkey file found" })),
        "a corrupt .dbkey is a RETURNED refusal in v4, never the catch"
    );
    let (status, body) = thrown;
    assert_eq!(
        status, 500,
        "a throw inside the action is serverError: {body}"
    );
    let message = body["error"].as_str().unwrap_or_default().to_string();
    assert!(
        !message.is_empty(),
        "serverError carries the thrown message: {body}"
    );

    let errors: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("Error in database key action"))
        .collect();
    assert_eq!(
        errors.len(),
        1,
        "exactly the throw logs; neither refusal (wrong passphrase, corrupt file) does: {lines:#?}"
    );
    let line = errors[0];
    assert!(line.starts_with("ERROR "), "v4 logs at error: {line}");
    for field in [
        "action=change-passphrase".to_string(),
        format!("error={message}"),
    ] {
        assert!(line.contains(&field), "missing {field} in {line}");
    }
}
