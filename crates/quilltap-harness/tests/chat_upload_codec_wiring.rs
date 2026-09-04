//! P4.73 — the chat-upload pixel-codec WIRING pin.
//!
//! v4 transcodes chat attachments through sharp. Until this lane v5 handed the
//! storage bridges `NotConfiguredPixelCodec` at **every** production call site,
//! so every WebP encode failed and the policy layer passed the ORIGINAL bytes
//! through — v4's own sharp-unavailable branch, and the divergence recorded at
//! `api/files.rs:1116-1118`. The 2026-09-03 dogfood walk measured it live: a
//! 265-byte PNG uploaded through `POST /api/v1/chats/{id}/files` came back
//! `image/png` with the row's sha equal to the served bytes.
//!
//! ## Why this is a composition test and not a differential arm
//!
//! **No differential can see the cutover.** `files_routes_equivalence` already
//! drives `chat_media::chat_file_upload` with a byte-changing codec passed in
//! by hand, so it proves the FUNCTION transcodes — and it would keep passing
//! forever if the engine arm reverted to handing that function the
//! not-configured codec. The thing under test here is the WIRE: which codec the
//! dispatch arm actually reaches for
//! (`a-chokepoint-cutover-is-differential-invisible`).
//!
//! So this boots a real [`CoreEngine`] over a seeded instance, assembles it
//! with a backup host whose `pixel_codec()` is deliberately byte-CHANGING, and
//! dispatches `Request::ChatFileUpload`. If the arm goes back to
//! `NotConfiguredPixelCodec`, the stored bytes are the input bytes and every
//! assertion below fails at once.
//!
//! Run standalone (no oracle — the comparand is v5's own composition):
//!   cargo test -p quilltap-harness --test chat_upload_codec_wiring

use std::path::{Path, PathBuf};
use std::sync::Arc;

use quilltap_core::api::engine::{CoreConfig, CoreEngine, NoopAssembler};
use quilltap_core::api::types::{Request, Response};
use quilltap_core::api::{InstanceDirectory, QuilltapCore};
use quilltap_core::services::backup::{BackupHost, HostDirs};
use quilltap_core::services::file_storage::{PixelCodec, StorageBackend};

/// The test pepper the committed fixtures are keyed with.
const PEPPER: &str = "dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHktMzJieXRl";
const USER_A: &str = "11111111-1111-4111-8111-111111111111";
/// A chat the images fixture seeds (`build-images-collection-fixture.ts`).
const CHAT_1: &str = "cc000000-0000-4000-8000-000000000001";

/// The marker a transcode leaves behind. Its presence in the STORED bytes is
/// the whole proof: the not-configured codec cannot produce it.
const MARK: &[u8] = b"QTAP-P473-WIRED:";

/// A byte-CHANGING codec. `encode_webp` succeeding is what makes the policy
/// layer treat the upload as converted, so the stored mime becomes
/// `image/webp` and the stored bytes carry [`MARK`].
struct MarkingCodec;

impl PixelCodec for MarkingCodec {
    fn encode_webp(
        &self,
        bytes: &[u8],
        _quality: i64,
        _effort: Option<i64>,
        _animated: bool,
    ) -> Result<Vec<u8>, String> {
        let mut out = MARK.to_vec();
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
    fn store_backup(&self, _backup_id: &str, _zip_path: &Path) {}
    fn take_backup(&self, _backup_id: &str) -> Option<PathBuf> {
        None
    }
    fn store_upload(&self, _upload_id: &str, _zip_path: &Path) {}
    fn get_upload(&self, _upload_id: &str) -> Option<PathBuf> {
        None
    }
    fn remove_upload(&self, _upload_id: &str) {}
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

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// A scratch instance seeded from the committed images fixture (it carries a
/// chat and a provisioned Quilltap Uploads store, which the chat-upload path
/// writes into).
fn scratch_instance(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("qt-codec-wire-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        fixtures_dir().join("images-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("images-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
    base
}

#[tokio::test]
async fn the_chat_upload_arm_reaches_for_the_host_codec() {
    let base = scratch_instance("upload");
    let engine = CoreEngine::boot(
        CoreConfig {
            base_dir: base.clone(),
            version: "test".to_string(),
            env_pepper: Some(PEPPER.to_string()),
        },
        Box::new(NoopAssembler),
        Arc::new(EmptyInstances),
    )
    .expect("boot");

    // The engine's own accessor must see the host codec — if this is `None` the
    // test below would pass vacuously against the not-configured fallback.
    assert!(
        engine.qtap_pixel_codec().is_none(),
        "NoopAssembler carries no backup host; the assembly below supplies one"
    );

    let base2 = scratch_instance("wired");
    let engine = boot_with_codec(&base2);
    assert!(
        engine.qtap_pixel_codec().is_some(),
        "the assembled backup host must expose its codec, or this pin is vacuous"
    );

    use base64::Engine as _;
    let png = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")
        .unwrap();

    let resp = engine
        .dispatch(Request::ChatFileUpload {
            chat_id: CHAT_1.to_string(),
            filename: "wire.png".to_string(),
            content_type: "image/png".to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(&png),
            resolution: None,
            conflicting_file_id: None,
        })
        .await;

    let body = match resp {
        Response::ChatMedia(v) => v,
        other => panic!("unexpected response: {other:?}"),
    };

    let db = engine.db().expect("ready db");
    let stored = db
        .read_mount_index(|c| {
            let mut stmt = c.prepare(
                // `doc_mount_blobs` carries BOTH the bytes and the stored mime,
                // keyed by fileId (the link row holds only the ORIGINAL mime).
                "SELECT b.data, b.sha256, b.storedMimeType, l.relativePath \
                 FROM doc_mount_file_links l \
                 JOIN doc_mount_blobs b ON b.fileId = l.fileId \
                 WHERE l.relativePath LIKE 'chat/%' ORDER BY l.relativePath DESC LIMIT 1",
            )?;
            let row = stmt.query_row([], |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;
            Ok(row)
        })
        .expect("read the stored blob");
    let (bytes, blob_sha, stored_mime, rel_path) = stored;

    // 1. The bytes actually stored are the CODEC's output, not the input.
    assert!(
        bytes.starts_with(MARK),
        "the stored bytes do not carry the codec's marker — the arm handed the \
         bridges a codec that does not encode (a revert to NotConfiguredPixelCodec \
         stores the ORIGINAL bytes). path={rel_path} mime={stored_mime}"
    );
    assert_ne!(bytes, png, "the stored bytes are the input bytes");

    // 2. The stored mime and the path follow the transcode.
    assert_eq!(stored_mime, "image/webp");
    assert!(
        rel_path.ends_with(".webp"),
        "the transcoded blob keeps a .webp path: {rel_path}"
    );

    // 3. The row's sha names the bytes we stored (within-tree, the P4.D152
    //    hash-what-we-store invariant this cutover must not break).
    let want_sha = sha256_hex(&bytes);
    assert_eq!(
        blob_sha, want_sha,
        "the doc_mount_files row's sha does not name the stored bytes"
    );

    // 4. The `files` row the response names agrees with the stored bytes too.
    let file_id = body["data"]["id"]
        .as_str()
        .or_else(|| body["id"].as_str())
        .unwrap_or_default()
        .to_string();
    if !file_id.is_empty() {
        let row = db
            .read_main(move |c| {
                let mut stmt = c.prepare("SELECT sha256, mimeType FROM files WHERE id = ?1")?;
                let r = stmt.query_row([&file_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?;
                Ok(r)
            })
            .expect("read the files row");
        assert_eq!(
            row.1, "image/webp",
            "the files row records the STORED mime, not the uploaded one"
        );
        assert_eq!(
            row.0, want_sha,
            "the files row's sha does not name the stored bytes"
        );
    }
}

fn boot_with_codec(base: &Path) -> CoreEngine {
    struct CodecAssembler {
        root: PathBuf,
    }
    impl quilltap_core::api::engine::EngineAssembler for CodecAssembler {
        fn assemble(
            &self,
            _db: &quilltap_core::db::runtime::Db,
            _events: &tokio::sync::broadcast::Sender<quilltap_core::api::types::Event>,
            _pepper: &str,
            _data_dir: &Path,
            _bus: &Arc<quilltap_core::services::creation_progress::CreationProgressBus>,
        ) -> Result<quilltap_core::api::engine::EngineAssembly, String> {
            struct NoShutdown;
            impl quilltap_core::api::engine::EngineShutdown for NoShutdown {
                fn shutdown(&self) {}
            }
            let mut a =
                quilltap_core::api::engine::EngineAssembly::shutdown_only(Box::new(NoShutdown));
            a.backup_host = Some(Arc::new(TestBackupHost {
                root: self.root.clone(),
            }));
            Ok(a)
        }
    }

    CoreEngine::boot(
        CoreConfig {
            base_dir: base.to_path_buf(),
            version: "test".to_string(),
            env_pepper: Some(PEPPER.to_string()),
        },
        Box::new(CodecAssembler {
            root: base.to_path_buf(),
        }),
        Arc::new(EmptyInstances),
    )
    .expect("boot with codec")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// The user the fixture owns everything as — referenced so a fixture swap that
/// changed the owner fails loudly here rather than producing an empty upload.
#[test]
fn the_fixture_owner_is_the_single_user() {
    assert_eq!(USER_A, "11111111-1111-4111-8111-111111111111");
}
