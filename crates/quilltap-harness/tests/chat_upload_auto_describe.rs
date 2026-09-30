//! P4.120 — the chat upload's background auto-describe (dogfood #128).
//!
//! v4's `uploadFileToProject` ends a NEW image row with
//! `void autoDescribeChatImageAttachment({ fileEntryId, userId, repos }).catch(warn)`
//! (`lib/chat-files-v2.ts:425-437`). v5 had a named no-op there, so every chat
//! upload stayed `files.description IS NULL` (measured on the Friday copy,
//! 2026-09-29). This pins the port's four load-bearing claims **without a
//! differential arm**, because the oracle cannot see a fire-and-forget spawn:
//!
//! 1. **The fire set equals v4's** — a new image row fires; the sha-dedup
//!    return, `skip`, the duplicate-conflict return and a non-image do not;
//!    `replace` and `keepBoth` (both go through `uploadFileToProject`) do.
//! 2. **The call is v4's** — `chatId` absent (`None`), the uploaded row's id,
//!    and it runs AFTER the response's write committed.
//! 3. **v4's `.catch`** — an `Err` out of the module logs WARN
//!    `Auto-describe failed for chat image upload` with `module`, `fileId`,
//!    `error`, and the upload's response is untouched.
//! 4. **Unarmed skips cleanly** — no spawner (the harness, the CLI's direct
//!    mode) is a DEBUG line, never a call, never a panic.
//!
//! The spawner is the thread-scoped test seam
//! (`quilltap_core::background::arm_background_spawner_for_current_thread`):
//! it QUEUES the futures so a test can count them and then run (or not run)
//! them on its own thread under a log capture.
//!
//! Standalone (no oracle):
//!   cargo test -p quilltap-harness --test chat_upload_auto_describe

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use quilltap_core::api::chat_media::{
    self, ChatFileUploadInput, ImageDescribeDriver, UploadAutoDescribe,
};
use quilltap_core::api::types::Response;
use quilltap_core::background::{
    arm_background_spawner_for_current_thread, disarm_background_spawner_for_current_thread,
};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::photos::save_image_to_album::{
    FileBytesStore, IngestImageRequest, NoSideEffects, SaveImageSideEffects,
};
use quilltap_core::services::file_fallback::{FallbackFile, FallbackResult, FallbackType};
use quilltap_core::test_support::captured_with;

/// The `files-*` fixture pair's test pepper (`harness/oracle/fixtures/files-web.json`).
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER_A: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
/// The non-project chat and the project chat (which already holds `dup.txt`)
/// of the committed `files-*` fixture pair.
const CHAT_G: &str = "c0000000-0000-4000-8000-000000000001";
const CHAT_P: &str = "c0000000-0000-4000-8000-000000000002";
/// The project's existing `dup.txt` row.
const PF_DUP: &str = "f0000000-0000-4000-8000-000000000041";

/// A 1×1 PNG.
const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

type Task = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Arm a thread-scoped spawner that QUEUES what it is handed.
fn arm_queue() -> Arc<Mutex<Vec<Task>>> {
    let q: Arc<Mutex<Vec<Task>>> = Arc::new(Mutex::new(Vec::new()));
    let qq = q.clone();
    arm_background_spawner_for_current_thread(Arc::new(move |fut| {
        qq.lock().unwrap().push(fut);
    }));
    q
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-p4120-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("files-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("files-mount.db"), &mount).unwrap();
    {
        let w = quilltap_core::db::Writer::open_writable(&main, PEPPER).unwrap();
        quilltap_core::test_support::ensure_p4d171_columns(w.connection());
        quilltap_core::test_support::ensure_p4d182_columns(w.connection());
    }
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db")
}

fn codec() -> Arc<dyn quilltap_core::services::file_storage::PixelCodec> {
    Arc::new(quilltap_harness::PrefixingPixelCodec)
}

/// Canned bytes for the module's read.
struct CannedBytes;
impl FileBytesStore for CannedBytes {
    fn read_image_buffer(
        &self,
        _entry: &quilltap_core::db::files::FileEntry,
    ) -> Result<Option<Vec<u8>>, String> {
        Ok(Some(vec![1, 2, 3]))
    }
    fn ingest_image_buffer(
        &self,
        _req: &IngestImageRequest,
    ) -> Result<quilltap_core::db::files::FileEntry, String> {
        Err("unused".into())
    }
}

/// A recording driver: how many calls, and the `chat_id` each carried.
#[derive(Default)]
struct RecordingDescribe {
    calls: AtomicUsize,
    chat_ids: Mutex<Vec<Option<String>>>,
    file_ids: Mutex<Vec<String>>,
}
impl ImageDescribeDriver for RecordingDescribe {
    fn describe<'a>(
        &'a self,
        file: FallbackFile,
        chat_id: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = FallbackResult> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.chat_ids
            .lock()
            .unwrap()
            .push(chat_id.map(str::to_string));
        self.file_ids.lock().unwrap().push(file.id.clone());
        Box::pin(async move {
            FallbackResult {
                type_: FallbackType::ImageDescription,
                text_content: None,
                image_description: Some("a copper kettle".to_string()),
                processing_metadata: None,
                error: None,
            }
        })
    }
}

fn seams(driver: &Arc<RecordingDescribe>) -> UploadAutoDescribe {
    UploadAutoDescribe {
        bytes: Arc::new(CannedBytes),
        describe: Some(driver.clone() as Arc<dyn ImageDescribeDriver>),
        side_effects: Arc::new(NoSideEffects) as Arc<dyn SaveImageSideEffects + Send + Sync>,
    }
}

struct Upload<'a> {
    chat: &'a str,
    filename: &'a str,
    content_type: &'a str,
    body: Vec<u8>,
    resolution: Option<&'a str>,
    conflicting: Option<&'a str>,
}

fn png() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(PNG_B64)
        .unwrap()
}

fn run(
    rt: &tokio::runtime::Runtime,
    db: &Db,
    u: Upload<'_>,
    seams: Option<UploadAutoDescribe>,
) -> Response {
    rt.block_on(chat_media::chat_file_upload_with_auto_describe(
        db,
        codec(),
        USER_A,
        u.chat,
        ChatFileUploadInput {
            filename: u.filename.to_string(),
            content_type: u.content_type.to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(&u.body),
            resolution: u.resolution.map(str::to_string),
            conflicting_file_id: u.conflicting.map(str::to_string),
        },
        seams,
    ))
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn uploaded_id(r: &Response) -> String {
    let v = serde_json::to_value(r).unwrap();
    v["data"]["file"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no file id in {v}"))
        .to_string()
}

fn is_duplicate(r: &Response) -> bool {
    serde_json::to_value(r).unwrap()["data"]["duplicate"] == true
}

/// Item 1 — the fire set, case by case, counted at the spawner.
#[test]
fn the_fire_set_equals_v4s() {
    let rt = rt();
    let driver = Arc::new(RecordingDescribe::default());

    // (name, chat, filename, content-type, body, resolution, conflicting, expect)
    #[allow(clippy::type_complexity)]
    let cases: Vec<(
        &str,
        &str,
        &str,
        &str,
        Vec<u8>,
        Option<&str>,
        Option<&str>,
        usize,
    )> = vec![
        (
            "new_png_nonproject",
            CHAT_G,
            "wire.png",
            "image/png",
            png(),
            None,
            None,
            1,
        ),
        (
            "text_is_not_an_image",
            CHAT_G,
            "note.txt",
            "text/plain",
            b"a note".to_vec(),
            None,
            None,
            0,
        ),
        // A project chat's filename conflict, unresolved: the duplicate return.
        (
            "duplicate_conflict",
            CHAT_P,
            "dup.txt",
            "image/png",
            png(),
            None,
            None,
            0,
        ),
        (
            "skip",
            CHAT_P,
            "dup.txt",
            "image/png",
            png(),
            Some("skip"),
            None,
            0,
        ),
        (
            "replace",
            CHAT_P,
            "dup.txt",
            "image/png",
            png(),
            Some("replace"),
            Some(PF_DUP),
            1,
        ),
        (
            "keep_both",
            CHAT_P,
            "dup.txt",
            "image/png",
            png(),
            Some("keepBoth"),
            None,
            1,
        ),
    ];
    for (name, chat, filename, ct, body, res, cfid, expect) in cases {
        let db = fresh_db(name);
        let q = arm_queue();
        let resp = run(
            &rt,
            &db,
            Upload {
                chat,
                filename,
                content_type: ct,
                body,
                resolution: res,
                conflicting: cfid,
            },
            Some(seams(&driver)),
        );
        disarm_background_spawner_for_current_thread();
        if name == "duplicate_conflict" {
            assert!(
                is_duplicate(&resp),
                "{name}: expected the duplicate envelope"
            );
        }
        assert_eq!(q.lock().unwrap().len(), expect, "{name}: spawned describes");
    }
    // Nothing above ran a queued task, so the driver was never asked.
    assert_eq!(driver.calls.load(Ordering::SeqCst), 0);
}

/// Item 1 (the dedup arm) — the SAME bytes twice into a non-project chat: the
/// first is a new row, the second the sha-dedup return that only re-links.
#[test]
fn a_sha_dedup_upload_does_not_fire() {
    let rt = rt();
    let driver = Arc::new(RecordingDescribe::default());
    let db = fresh_db("dedup");
    let q = arm_queue();
    let up = || Upload {
        chat: CHAT_G,
        filename: "same.png",
        content_type: "image/png",
        body: png(),
        resolution: None,
        conflicting: None,
    };
    let first = run(&rt, &db, up(), Some(seams(&driver)));
    assert_eq!(q.lock().unwrap().len(), 1, "the new row fires");
    let second = run(&rt, &db, up(), Some(seams(&driver)));
    disarm_background_spawner_for_current_thread();
    assert_eq!(
        uploaded_id(&first),
        uploaded_id(&second),
        "the dedup returns the same row"
    );
    assert_eq!(q.lock().unwrap().len(), 1, "the dedup return does NOT fire");
}

/// Item 2 — the call itself: v4's `chatId`-less call, on the uploaded row, run
/// AFTER the response, persisting the description onto the `files` row.
#[test]
fn the_spawned_call_is_v4s_and_persists_the_description() {
    let rt = rt();
    let driver = Arc::new(RecordingDescribe::default());
    let db = fresh_db("call");
    let q = arm_queue();
    let resp = run(
        &rt,
        &db,
        Upload {
            chat: CHAT_G,
            filename: "kettle.png",
            content_type: "image/png",
            body: png(),
            resolution: None,
            conflicting: None,
        },
        Some(seams(&driver)),
    );
    disarm_background_spawner_for_current_thread();
    let id = uploaded_id(&resp);

    // After the response, before the task runs: nothing described yet.
    assert_eq!(driver.calls.load(Ordering::SeqCst), 0);
    let desc_before = read_description(&db, &id);
    assert_eq!(desc_before, None, "the upload's own write leaves it blank");

    let tasks: Vec<Task> = std::mem::take(&mut *q.lock().unwrap());
    assert_eq!(tasks.len(), 1);
    for t in tasks {
        rt.block_on(t);
    }
    assert_eq!(driver.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *driver.chat_ids.lock().unwrap(),
        vec![None],
        "v4's call passes NO chatId (the uncensored fallback sees chat = null)"
    );
    assert_eq!(*driver.file_ids.lock().unwrap(), vec![id.clone()]);
    assert_eq!(
        read_description(&db, &id).as_deref(),
        Some("a copper kettle")
    );
}

fn read_description(db: &Db, id: &str) -> Option<String> {
    let id = id.to_string();
    db.read_main(move |c| {
        Ok(
            c.query_row("SELECT description FROM files WHERE id = ?1", [&id], |r| {
                r.get::<_, Option<String>>(0)
            })?,
        )
    })
    .unwrap()
}

/// Item 3 — v4's `.catch`: an `Err` out of the module is a WARN, never a
/// failure of the (already sent) upload. Forced by dropping `files` between the
/// upload and the task, so the module's precheck read throws (v4's uncaught
/// `repos.files.findById`).
#[test]
fn an_err_from_the_module_logs_v4s_warn() {
    let rt = rt();
    let driver = Arc::new(RecordingDescribe::default());
    let db = fresh_db("warn");
    let q = arm_queue();
    let resp = run(
        &rt,
        &db,
        Upload {
            chat: CHAT_G,
            filename: "doomed.png",
            content_type: "image/png",
            body: png(),
            resolution: None,
            conflicting: None,
        },
        Some(seams(&driver)),
    );
    disarm_background_spawner_for_current_thread();
    let id = uploaded_id(&resp);
    // The response is already the success envelope.
    assert!(serde_json::to_value(&resp).unwrap()["data"]["file"]["id"].is_string());

    rt.block_on(async {
        db.write(|ws| {
            ws.main().connection().execute_batch("DROP TABLE files")?;
            Ok(())
        })
        .await
        .unwrap();
    });
    let tasks: Vec<Task> = std::mem::take(&mut *q.lock().unwrap());
    let (_, logs) = captured_with(|| {
        for t in tasks {
            rt.block_on(t);
        }
    });
    let warns: Vec<&String> = logs
        .iter()
        .filter(|l| l.contains("Auto-describe failed for chat image upload"))
        .collect();
    assert_eq!(warns.len(), 1, "exactly one v4 warn: {logs:?}");
    let w = warns[0];
    assert!(w.starts_with("WARN "), "{w}");
    assert!(w.contains("module=chat-files-v2"), "{w}");
    assert!(w.contains(&format!("fileId={id}")), "{w}");
    assert!(w.contains("error="), "{w}");
    assert_eq!(
        driver.calls.load(Ordering::SeqCst),
        0,
        "no vision call on the Err path"
    );

    // Silence leg: the same task over an INTACT db logs no such warn.
    let db2 = fresh_db("warn-silent");
    let q2 = arm_queue();
    let _ = run(
        &rt,
        &db2,
        Upload {
            chat: CHAT_G,
            filename: "fine.png",
            content_type: "image/png",
            body: png(),
            resolution: None,
            conflicting: None,
        },
        Some(seams(&driver)),
    );
    disarm_background_spawner_for_current_thread();
    let tasks: Vec<Task> = std::mem::take(&mut *q2.lock().unwrap());
    let (_, logs) = captured_with(|| {
        for t in tasks {
            rt.block_on(t);
        }
    });
    assert!(
        !logs
            .iter()
            .any(|l| l.contains("Auto-describe failed for chat image upload")),
        "{logs:?}"
    );
}

/// Item 4 — nothing armed: the upload succeeds, no call is made, and the skip
/// is a DEBUG line (with seams; and, separately, with no seams at all).
#[test]
fn an_unarmed_engine_skips_the_fire_with_a_debug_line() {
    let rt = rt();
    let driver = Arc::new(RecordingDescribe::default());

    disarm_background_spawner_for_current_thread();
    let db = fresh_db("unarmed");
    let (resp, logs) = captured_with(|| {
        run(
            &rt,
            &db,
            Upload {
                chat: CHAT_G,
                filename: "quiet.png",
                content_type: "image/png",
                body: png(),
                resolution: None,
                conflicting: None,
            },
            Some(seams(&driver)),
        )
    });
    assert!(serde_json::to_value(&resp).unwrap()["data"]["file"]["id"].is_string());
    assert_eq!(driver.calls.load(Ordering::SeqCst), 0);
    let skip: Vec<&String> = logs
        .iter()
        .filter(|l| l.contains("no background spawner armed"))
        .collect();
    assert_eq!(skip.len(), 1, "{logs:?}");
    assert!(skip[0].starts_with("DEBUG "), "{}", skip[0]);

    // No seams at all (the differential families' posture): also a clean skip.
    let db = fresh_db("noseams");
    let (resp, logs) = captured_with(|| {
        run(
            &rt,
            &db,
            Upload {
                chat: CHAT_G,
                filename: "quiet2.png",
                content_type: "image/png",
                body: png(),
                resolution: None,
                conflicting: None,
            },
            None,
        )
    });
    assert!(serde_json::to_value(&resp).unwrap()["data"]["file"]["id"].is_string());
    assert!(
        logs.iter()
            .any(|l| l.starts_with("DEBUG ") && l.contains("no auto-describe seams wired")),
        "{logs:?}"
    );
}
