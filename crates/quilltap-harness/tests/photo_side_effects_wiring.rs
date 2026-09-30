//! P4.120 — the production `SaveImageSideEffects` WIRING pins.
//!
//! Until this order v5 handed every photo path `NoSideEffects`: v4's
//! `enqueueEmbeddingJobsForMountPoint` after a photo write was a no-op on the
//! save-to-album doors, on the `describe_image` tool's vision tier, and (once
//! wired) on the chat upload's auto-describe — so a described or kept image's
//! fresh chunks never got an `EMBEDDING_GENERATE` job until the next full mount
//! scan.
//!
//! **No differential can see this** (the oracle jest-mocks the seam — that is
//! why `photo_tools_equivalence` runs `NoSideEffects` on purpose), so these are
//! composition pins, in the `chat_upload_codec_wiring` mould: boot a real
//! [`CoreEngine`] over a seeded instance, dispatch the real `Request`, and count
//! what the post-write hook handed the background spawner. Each pin is
//! red-first against the literal `NoSideEffects` the engine used to pass.
//!
//!  1. `MountEmbeddingSideEffects` itself: armed, it enqueues a PENDING
//!     `EMBEDDING_GENERATE` job for a chunk of the touched mount; unarmed it is
//!     a DEBUG-logged no-op.
//!  2. **Upload path** — `Request::ChatFileUpload` on the engine: the spawned
//!     auto-describe persists a description, updates the blank link, and the
//!     module's enqueue lands a job for the mount.
//!  3. **Save-to-album path** — `Request::ChatSaveGalleryImage` on the engine
//!     hands the spawner an enqueue.
//!  4. **`describe_image` path** — a default `BuiltInToolRunner`'s side effects
//!     enqueue too (the executor's wiring, reached from the Salon turn, the
//!     enclave and the Run Tool modal alike).
//!
//! Standalone: cargo test -p quilltap-harness --test photo_side_effects_wiring

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use quilltap_core::api::engine::{
    CoreConfig, CoreEngine, EngineAssembler, EngineAssembly, EngineShutdown,
};
use quilltap_core::api::types::{Request, Response};
use quilltap_core::api::{InstanceDirectory, QuilltapCore};
use quilltap_core::background::{
    arm_background_spawner, arm_background_spawner_for_current_thread, disarm_background_spawner,
    disarm_background_spawner_for_current_thread,
};
use quilltap_core::db::runtime::Db;
use quilltap_core::photos::save_image_to_album::{FileBytesStore, IngestImageRequest};
use quilltap_core::services::backup::{BackupHost, HostDirs};
use quilltap_core::services::file_fallback::{FallbackFile, FallbackResult, FallbackType};
use quilltap_core::services::file_storage::{PixelCodec, StorageBackend};

/// The `system-data` fixture pair's pepper: a provisioned-shape instance that
/// carries a user, a DEFAULT embedding profile, the Quilltap Uploads store and a
/// non-project chat — everything the upload + enqueue chain touches.
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const CHAT_1: &str = "c1000000-0000-4000-8000-000000000002";
const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

type Task = Pin<Box<dyn Future<Output = ()> + Send>>;

/// The two pins below arm different spawner scopes (thread vs process); one at
/// a time keeps the process-global one from being seen by the other.
static SERIAL: Mutex<()> = Mutex::new(());

fn arm_queue() -> Arc<Mutex<Vec<Task>>> {
    let q: Arc<Mutex<Vec<Task>>> = Arc::new(Mutex::new(Vec::new()));
    let qq = q.clone();
    arm_background_spawner_for_current_thread(Arc::new(move |fut| {
        qq.lock().unwrap().push(fut);
    }));
    q
}

/// Run queued tasks until the queue is empty — a task may itself spawn (the
/// auto-describe's enqueue is spawned from inside the describe task).
async fn drain(q: &Arc<Mutex<Vec<Task>>>) -> usize {
    let mut ran = 0;
    loop {
        let next = { q.lock().unwrap().pop() };
        match next {
            Some(t) => {
                t.await;
                ran += 1;
            }
            None => return ran,
        }
    }
}

struct EmptyInstances;
impl InstanceDirectory for EmptyInstances {
    fn list(&self) -> Result<quilltap_core::api::types::InstancesDto, String> {
        Ok(quilltap_core::api::types::InstancesDto {
            instances: vec![],
            default_instance: None,
        })
    }
}

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

struct CannedDescribe;
impl quilltap_core::api::chat_media::ImageDescribeDriver for CannedDescribe {
    fn describe<'a>(
        &'a self,
        _file: FallbackFile,
        _chat_id: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = FallbackResult> + Send + 'a>> {
        Box::pin(async {
            FallbackResult {
                type_: FallbackType::ImageDescription,
                text_content: None,
                image_description: Some("a copper kettle on a windowsill".to_string()),
                processing_metadata: None,
                error: None,
            }
        })
    }
}

struct MarkingCodec;
impl PixelCodec for MarkingCodec {
    fn encode_webp(
        &self,
        bytes: &[u8],
        _q: i64,
        _e: Option<i64>,
        _a: bool,
    ) -> Result<Vec<u8>, String> {
        let mut out = b"QTAP-P4120:".to_vec();
        out.extend_from_slice(bytes);
        Ok(out)
    }
    fn measure(&self, _bytes: &[u8]) -> (Option<i64>, Option<i64>) {
        (Some(1), Some(1))
    }
}

struct TestBackupHost {
    root: PathBuf,
}
impl BackupHost for TestBackupHost {
    fn storage(&self) -> Arc<dyn StorageBackend> {
        Arc::new(quilltap_core::services::file_storage::NotConfiguredStorageBackend)
    }
    fn pixel_codec(&self) -> Arc<dyn PixelCodec> {
        Arc::new(MarkingCodec)
    }
    fn temp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }
    fn host_dirs(&self) -> HostDirs {
        HostDirs {
            npm_plugins: None,
            themes: None,
        }
    }
    fn app_version(&self) -> String {
        "test".to_string()
    }
    fn now_ms(&self) -> i64 {
        0
    }
    fn store_backup(&self, _b: &str, _z: &Path) {}
    fn take_backup(&self, _b: &str) -> Option<PathBuf> {
        None
    }
    fn store_upload(&self, _u: &str, _z: &Path) {}
    fn get_upload(&self, _u: &str) -> Option<PathBuf> {
        None
    }
    fn remove_upload(&self, _u: &str) {}
}

struct Assembler {
    root: PathBuf,
}
impl EngineAssembler for Assembler {
    fn assemble(
        &self,
        _db: &Db,
        _events: &tokio::sync::broadcast::Sender<quilltap_core::api::types::Event>,
        _pepper: &str,
        _data_dir: &Path,
        _bus: &Arc<quilltap_core::services::creation_progress::CreationProgressBus>,
    ) -> Result<EngineAssembly, String> {
        struct NoShutdown;
        impl EngineShutdown for NoShutdown {
            fn shutdown(&self) {}
        }
        let mut a = EngineAssembly::shutdown_only(Box::new(NoShutdown));
        a.backup_host = Some(Arc::new(TestBackupHost {
            root: self.root.clone(),
        }));
        a.save_image_bytes = Some(Arc::new(CannedBytes));
        a.image_describe = Some(Arc::new(CannedDescribe));
        Ok(a)
    }
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// A scratch instance from a committed fixture pair, healed for the columns
/// the write paths bind.
fn scratch_instance(tag: &str, stem: &str, pepper: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("qt-p4120w-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        fixtures_dir().join(format!("{stem}-main.db")),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join(format!("{stem}-mount.db")),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
    let w = quilltap_core::db::Writer::open_writable(&data.join("quilltap.db"), pepper).unwrap();
    quilltap_core::test_support::ensure_p4d171_columns(w.connection());
    quilltap_core::test_support::ensure_p4d182_columns(w.connection());
    base
}

fn boot(base: &Path, pepper: &str, assembler: Box<dyn EngineAssembler>) -> CoreEngine {
    CoreEngine::boot(
        CoreConfig {
            base_dir: base.to_path_buf(),
            version: "test".to_string(),
            env_pepper: Some(pepper.to_string()),
            display_zone: quilltap_core::host_zone::TimeZone::UTC,
        },
        assembler,
        Arc::new(EmptyInstances),
    )
    .expect("boot")
}

fn count_embedding_jobs(db: &Db) -> i64 {
    db.read_main(|c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM background_jobs WHERE type = 'EMBEDDING_GENERATE' AND status = 'PENDING'",
            [],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .unwrap()
}

/// Pin 2 — the upload path, end to end through the engine: dispatch
/// `Request::ChatFileUpload`; the spawned auto-describe persists a description
/// onto the row and the blank link, and its enqueue lands a job.
// The guard IS the serialisation; a current-thread test holds it across its awaits on purpose.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn an_uploaded_image_is_described_and_its_mount_enqueued() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let base = scratch_instance("upload", "system-data", PEPPER);
    let engine = boot(&base, PEPPER, Box::new(Assembler { root: base.clone() }));
    let db = engine.db().expect("ready db");
    let q = arm_queue();

    let resp = engine
        .dispatch(Request::ChatFileUpload {
            chat_id: CHAT_1.to_string(),
            filename: "kettle.png".to_string(),
            content_type: "image/png".to_string(),
            data: PNG_B64.to_string(),
            resolution: None,
            conflicting_file_id: None,
        })
        .await;
    let body = match resp {
        Response::ChatMedia(v) => v,
        other => panic!("unexpected response: {other:?}"),
    };
    let file_id = body["file"]["id"].as_str().expect("file id").to_string();

    // The response is out; the describe is queued, not run.
    assert_eq!(q.lock().unwrap().len(), 1, "one auto-describe spawned");
    assert_eq!(read_description(&db, &file_id), None);
    assert_eq!(count_embedding_jobs(&db), 0);

    // Run it (and the enqueue it spawns) to completion.
    let ran = drain(&q).await;
    disarm_background_spawner_for_current_thread();
    assert_eq!(ran, 2, "the describe, then its embedding enqueue");
    assert_eq!(
        read_description(&db, &file_id).as_deref(),
        Some("a copper kettle on a windowsill")
    );
    assert!(
        count_embedding_jobs(&db) >= 1,
        "no EMBEDDING_GENERATE job for the described mount"
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

/// Pin 3 — the save-to-album door. `Request::ChatSaveGalleryImage` on the
/// engine hands the spawner the post-write enqueue (it passed `NoSideEffects`,
/// so this counted ZERO before P4.120). The fixture is the chat-gallery pair
/// the differential family uses; its ids are its own meta file's.
// The guard IS the serialisation; a current-thread test holds it across its awaits on purpose.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn the_engines_save_to_album_door_enqueues() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let base = scratch_instance("save", "chat-gallery", PEPPER);
    let meta: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("chat-gallery-main.db.meta.json")).unwrap(),
    )
    .unwrap();
    let general_mp = meta["generalMp"].as_str().unwrap().to_string();
    let engine = boot(&base, PEPPER, Box::new(Assembler { root: base.clone() }));
    // `save_image_to_album` calls its side effects from INSIDE the writer closure,
    // i.e. on the WRITER thread — a thread-scoped spawner armed here would not be
    // seen there, so this pin arms the process-global one (the production shape).
    let q: Arc<Mutex<Vec<Task>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let qq = q.clone();
        arm_background_spawner(Arc::new(move |fut| qq.lock().unwrap().push(fut)));
    }

    let resp = engine
        .dispatch(Request::ChatSaveGalleryImage {
            chat_id: "c1000000-0000-4000-8000-000000000001".to_string(),
            body: serde_json::json!({
                "fileId": "f1000000-0000-4000-8000-000000000007",
                "mountPointId": general_mp,
            }),
        })
        .await;
    disarm_background_spawner();
    match &resp {
        Response::ChatMedia(_) => {}
        other => panic!("the save did not succeed: {other:?}"),
    }
    assert_eq!(
        q.lock().unwrap().len(),
        1,
        "a successful save hands the spawner exactly one embedding enqueue"
    );
}
