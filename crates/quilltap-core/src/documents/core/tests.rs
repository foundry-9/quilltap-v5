//! Unit pins for the document core (P4.D380): every arm of v4's
//! `lib/documents/core/*` the corpus families cannot reach cheaply, and each
//! core log line with its silence leg. The lines run on the test thread here
//! (production runs them inside a `Db::write` closure on the writer thread —
//! hence `global_capture`, never the thread-scoped rig).

use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::db::Writer;
use crate::doc_edit::path_resolver::ResolvedPath;
use crate::doc_edit::DocEditScope;
use crate::documents::DocError;
use crate::inventory::lint_scope::TitleLintScope;
use crate::services::mount_index::blob_transcode::RefusingWebpTranscoder;
use crate::services::mount_index::converters::RefusingTextExtractor;
use crate::services::mount_index::store_file::{
    store_mount_file, AssetStorageMode, CollisionStrategy, StoreFileInput,
};
use crate::test_support::global_capture;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MP: &str = "c3800000-0000-4000-8000-0000000000d1";
const T: &str = "quilltap::documents";

fn instance() -> (Writer, Writer, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
    let main = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    let mount = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
    mount
        .connection()
        .execute(
            "INSERT INTO doc_mount_points (id, name, mountType, createdAt, updatedAt) \
             VALUES (?1, 'Store', 'database', '2026-01-01T00:00:00.000Z', \
             '2026-01-01T00:00:00.000Z')",
            [MP],
        )
        .unwrap();
    (main, mount, dir)
}

fn upload(main: &Writer, mount: &Writer, path: &str, bytes: &[u8]) {
    let mut input = StoreFileInput::new(MP, path, bytes.to_vec());
    input.treat_native_text_as_document = false;
    input.extract_text = false;
    input.enqueue_embedding = false;
    input.collision_strategy = CollisionStrategy::UniqueSuffix;
    input.asset_storage = AssetStorageMode::Database;
    store_mount_file(
        main.connection(),
        mount.connection(),
        &input,
        &RefusingTextExtractor,
        &RefusingWebpTranscoder,
    )
    .unwrap();
}

fn db(rel: &str) -> ResolvedPath {
    ResolvedPath {
        absolute_path: String::new(),
        scope: DocEditScope::DocumentStore,
        mount_point_id: Some(MP.to_string()),
        mount_point_name: Some("Store".to_string()),
        mount_type: Some("database".to_string()),
        base_path: String::new(),
        relative_path: rel.to_string(),
    }
}

/// A filesystem path; `mount_point_id` set for a store, `None` for the legacy
/// general area.
fn disk(base: &std::path::Path, rel: &str, mp: Option<&str>) -> ResolvedPath {
    ResolvedPath {
        absolute_path: base.join(rel).to_string_lossy().into_owned(),
        scope: if mp.is_some() {
            DocEditScope::DocumentStore
        } else {
            DocEditScope::General
        },
        mount_point_id: mp.map(str::to_string),
        mount_point_name: None,
        mount_type: mp.map(|_| "filesystem".to_string()),
        base_path: base.to_string_lossy().into_owned(),
        relative_path: rel.to_string(),
    }
}

fn put(main: &Writer, mount: &Writer, rel: &str, content: &str) -> i64 {
    crate::db::database_store::write_database_document(
        mount.connection(),
        MP,
        rel,
        content,
        TitleLintScope::Unlinted {
            main: main.connection(),
        },
    )
    .unwrap()
}

#[test]
fn read_document_falls_back_to_the_uploaded_blob_with_v4s_debug() {
    global_capture::install();
    let (main, mount, _d) = instance();
    put(&main, &mount, "docs/a.md", "# A");
    upload(&main, &mount, "up/notes.md", b"uploaded \xff bytes");
    let (doc, lines) =
        global_capture::capture(|| read_document(mount.connection(), &db("docs/a.md")));
    let doc = doc.unwrap();
    assert_eq!(
        (doc.content.as_str(), doc.source),
        ("# A", DocumentSource::Document)
    );
    assert!(lines.is_empty(), "a document read logs nothing: {lines:?}");
    let (doc, lines) =
        global_capture::capture(|| read_document(mount.connection(), &db("up/notes.md")));
    let doc = doc.unwrap();
    assert_eq!(doc.content, "uploaded \u{fffd} bytes");
    assert_eq!(doc.source, DocumentSource::BlobBytes);
    assert_eq!(doc.size, 16);
    assert_eq!(doc.stored_mime_type, None);
    assert_eq!(
        lines.last().unwrap(),
        &format!("DEBUG {T} readDocument: no document at path; reading the uploaded blob mountPointId={MP} relativePath=up/notes.md source=blob-bytes size=16")
    );
    // Absence keeps the ORIGINAL ENOENT message.
    let err = read_document(mount.connection(), &db("up/none.md")).unwrap_err();
    assert!(err.is_enoent(), "{err:?}");
    assert_eq!(
        err.message(),
        "ENOENT: database document not found at up/none.md"
    );
}

#[test]
fn read_document_outside_a_database_store_never_reads_a_blob() {
    let dir = tempfile::tempdir().unwrap();
    let err = read_document(
        &rusqlite::Connection::open_in_memory().unwrap(),
        &disk(dir.path(), "x.md", Some(MP)),
    )
    .unwrap_err();
    assert!(err.is_enoent(), "{err:?}");
}

#[test]
fn stat_document_counts_documents_and_folders_but_never_an_upload() {
    let (main, mount, _d) = instance();
    put(&main, &mount, "docs/a.md", "# A");
    upload(&main, &mount, "up/b.md", b"b");
    let m = mount.connection();
    assert_eq!(
        stat_document(m, &db("docs/a.md")).unwrap(),
        Some(DocKind::File)
    );
    assert_eq!(
        stat_document(m, &db("docs")).unwrap(),
        Some(DocKind::Folder)
    );
    assert_eq!(stat_document(m, &db("up/b.md")).unwrap(), None);
    assert_eq!(stat_document(m, &db("nope.md")).unwrap(), None);
    let mut no_mp = db("docs/a.md");
    no_mp.mount_point_id = None;
    assert_eq!(
        stat_document(m, &no_mp).unwrap_err().message(),
        "Database-backed ResolvedPath is missing mountPointId"
    );
    // documentExists: a database folder is not a document; on disk it is taken.
    assert!(!document_exists(m, &db("docs")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    assert!(document_exists(m, &disk(dir.path(), "sub", None)).unwrap());
}

#[test]
fn an_eacces_stat_bubbles_with_nodes_message() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let r = disk(dir.path(), "locked/a.md", Some(MP));
    let got = stat_document(&rusqlite::Connection::open_in_memory().unwrap(), &r);
    let del = delete_document(
        &rusqlite::Connection::open_in_memory().unwrap(),
        &rusqlite::Connection::open_in_memory().unwrap(),
        &r,
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let want = format!("EACCES: permission denied, stat '{}'", r.absolute_path);
    assert_eq!(got.unwrap_err().message(), want);
    assert_eq!(del.unwrap_err().message(), want);
}

#[test]
fn write_document_logs_the_write_then_the_chokepoint_and_refuses_a_stale_mtime() {
    global_capture::install();
    let (main, mount, _d) = instance();
    let first = put(&main, &mount, "docs/a.md", "old");
    let refresh = Refresh::Now {
        main: main.connection(),
        mount: mount.connection(),
    };
    let (r, lines) = global_capture::capture(|| {
        write_document(
            main.connection(),
            mount.connection(),
            &db("docs/a.md"),
            "nëw",
            Some(first - 1),
            refresh,
        )
    });
    match r {
        Err(DocError::MtimeMismatch(m)) => assert_eq!(
            m,
            "Document was modified by another process (mtime mismatch). Please reload and try again."
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        lines,
        vec![format!("DEBUG {T} writeDocument: mtime conflict relativePath=docs/a.md mountPointId={MP} error=Document was modified by another process (mtime mismatch). Please reload and try again.")]
    );
    let (r, lines) = global_capture::capture(|| {
        write_document(
            main.connection(),
            mount.connection(),
            &db("docs/a.md"),
            "nëw 😀",
            Some(first),
            refresh,
        )
    });
    r.unwrap();
    let ours: Vec<&String> = lines.iter().filter(|l| l.contains(T)).collect();
    assert_eq!(
        ours,
        vec![
            &format!("DEBUG {T} writeDocument: wrote document relativePath=docs/a.md mountPointId={MP} bytes=6"),
            &format!("DEBUG {T} afterDocumentWrite: scheduling store refresh mountPointId={MP} relativePath=docs/a.md"),
        ]
    );
    // The refresh ran: the store's cached stats count the document's chunk.
    let chunks: f64 = mount
        .connection()
        .query_row(
            "SELECT chunkCount FROM doc_mount_points WHERE id = ?1",
            [MP],
            |r| r.get(0),
        )
        .unwrap();
    assert!(chunks > 0.0, "refreshStats ran");
}

#[test]
fn a_write_without_a_mount_point_id_never_reaches_the_chokepoint() {
    global_capture::install();
    let dir = tempfile::tempdir().unwrap();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let r = disk(dir.path(), "notes/a.md", None);
    let (got, lines) = global_capture::capture(|| {
        write_document(&conn, &conn, &r, "x", None, Refresh::Scheduled(None))
    });
    got.unwrap();
    assert_eq!(
        lines,
        vec![format!(
            "DEBUG {T} writeDocument: wrote document relativePath=notes/a.md bytes=1"
        )]
    );
}

#[test]
fn a_failing_chain_logs_v4s_one_warn_and_never_fails_the_write() {
    global_capture::install();
    let (main, mount, _d) = instance();
    // A mount partition with no tables under the chain (the write itself lands
    // in the real one): every step reads nothing, and `refreshStats` fails.
    let empty = rusqlite::Connection::open_in_memory().unwrap();
    let (r, lines) = global_capture::capture(|| {
        write_document(
            main.connection(),
            mount.connection(),
            &db("docs/a.md"),
            "x",
            None,
            Refresh::Now {
                main: main.connection(),
                mount: &empty,
            },
        )
    });
    r.unwrap();
    let warns: Vec<&String> = lines
        .iter()
        .filter(|l| l.starts_with("WARN quilltap::documents"))
        .collect();
    assert_eq!(warns.len(), 1, "{lines:?}");
    assert!(
        warns[0].starts_with(&format!("WARN {T} Background re-index, embedding, or stats refresh failed after a document write mountPointId={MP} path=docs/a.md error=")),
        "{}",
        warns[0]
    );
}

#[test]
fn ensure_document_creates_once_then_reports_existing() {
    global_capture::install();
    let (main, mount, _d) = instance();
    let refresh = Refresh::Now {
        main: main.connection(),
        mount: mount.connection(),
    };
    let (r, lines) = global_capture::capture(|| {
        ensure_document(
            main.connection(),
            mount.connection(),
            &db("Untitled Document.md"),
            refresh,
        )
    });
    assert!(matches!(r.unwrap(), Ensured::Created(_)));
    assert_eq!(
        lines.iter().rfind(|l| l.contains(T)).unwrap(),
        &format!("DEBUG {T} ensureDocument: created a blank document relativePath=Untitled Document.md mountPointId={MP}")
    );
    assert_eq!(
        ensure_document(
            main.connection(),
            mount.connection(),
            &db("Untitled Document.md"),
            refresh
        )
        .unwrap(),
        Ensured::Existing
    );
}

#[test]
fn rename_document_refusals_and_the_database_move() {
    global_capture::install();
    let (main, mount, _d) = instance();
    put(&main, &mount, "a.md", "a");
    put(&main, &mount, "folder/x.md", "x");
    let (m, mn) = (mount.connection(), main.connection());
    let refresh = Refresh::Now { main: mn, mount: m };
    assert_eq!(
        rename_document(mn, m, &db("none.md"), &db("b.md"), refresh).unwrap(),
        RenameOutcome::SourceMissing
    );
    assert_eq!(
        rename_document(mn, m, &db("folder"), &db("b.md"), refresh).unwrap(),
        RenameOutcome::SourceNotAFile
    );
    // A FOLDER at the destination is taken (v4 measures any statDocument hit).
    assert_eq!(
        rename_document(mn, m, &db("a.md"), &db("folder"), refresh).unwrap(),
        RenameOutcome::DestinationTaken
    );
    let (r, lines) =
        global_capture::capture(|| rename_document(mn, m, &db("a.md"), &db("A.md"), refresh));
    assert_eq!(
        r.unwrap(),
        RenameOutcome::Moved,
        "a case-only rename is legal"
    );
    let ours: Vec<&String> = lines.iter().filter(|l| l.contains(T)).collect();
    assert_eq!(
        ours,
        vec![&format!(
            "DEBUG {T} renameDocument: moved database document mountPointId={MP} from=a.md to=A.md"
        )],
        "no chokepoint on the database branch"
    );
    // UNSUPPORTED bubbles.
    match rename_document(mn, m, &db("A.md"), &db("A.png"), refresh) {
        Err(DocError::Unsupported(_)) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn rename_on_disk_refreshes_the_destination() {
    global_capture::install();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "a").unwrap();
    std::fs::write(dir.path().join("taken.md"), "t").unwrap();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let from = disk(dir.path(), "a.md", Some(MP));
    assert_eq!(
        rename_document(
            &conn,
            &conn,
            &from,
            &disk(dir.path(), "taken.md", Some(MP)),
            Refresh::Scheduled(None)
        )
        .unwrap(),
        RenameOutcome::DestinationTaken
    );
    let (r, lines) = global_capture::capture(|| {
        rename_document(
            &conn,
            &conn,
            &from,
            &disk(dir.path(), "sub/b.md", Some(MP)),
            Refresh::Scheduled(None),
        )
    });
    assert_eq!(r.unwrap(), RenameOutcome::Moved);
    let ours: Vec<&String> = lines.iter().filter(|l| l.contains(T)).collect();
    assert_eq!(
        ours[0],
        &format!("DEBUG {T} renameDocument: moved file from=a.md to=sub/b.md")
    );
    assert_eq!(
        ours[1],
        &format!("DEBUG {T} afterDocumentWrite: scheduling store refresh mountPointId={MP} relativePath=sub/b.md")
    );
    assert!(dir.path().join("sub/b.md").is_file());
}

#[test]
fn delete_document_outcomes() {
    global_capture::install();
    let (main, mount, _d) = instance();
    put(&main, &mount, "folder/x.md", "x");
    upload(&main, &mount, "up/u.md", b"u");
    let (m, mn) = (mount.connection(), main.connection());
    assert_eq!(
        delete_document(mn, m, &db("folder")).unwrap(),
        DeleteOutcome::NotAFile
    );
    assert_eq!(
        delete_document(mn, m, &db("none.md")).unwrap(),
        DeleteOutcome::NotFound
    );
    let (r, lines) = global_capture::capture(|| delete_document(mn, m, &db("folder/x.md")));
    assert_eq!(r.unwrap(), DeleteOutcome::Deleted);
    assert_eq!(
        lines.iter().filter(|l| l.contains(T)).collect::<Vec<_>>(),
        vec![&format!("DEBUG {T} deleteDocument: deleted database document mountPointId={MP} relativePath=folder/x.md")]
    );
    // An upload is deletable (v4's comment: "an uploaded blob included").
    assert_eq!(
        delete_document(mn, m, &db("up/u.md")).unwrap(),
        DeleteOutcome::Deleted
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.md"), "f").unwrap();
    let (r, lines) =
        global_capture::capture(|| delete_document(mn, m, &disk(dir.path(), "f.md", None)));
    assert_eq!(r.unwrap(), DeleteOutcome::Deleted);
    assert!(
        lines.contains(&format!(
            "DEBUG {T} deleteDocument: deleted file relativePath=f.md"
        )),
        "{lines:?}"
    );
}

#[test]
fn view_target_for_classifies_non_text_store_files() {
    let (main, mount, _d) = instance();
    put(&main, &mount, "a.md", "a");
    upload(&main, &mount, "up/pic.png", b"\x89PNG\r\n\x1a\nxx");
    upload(&main, &mount, "up/conf.yaml", b"a: 1\n");
    let m = mount.connection();
    assert_eq!(
        view_target_for(m, &db("a.md")).unwrap(),
        None,
        "native text is edited"
    );
    let png = view_target_for(m, &db("UP/PIC.png"))
        .unwrap()
        .expect("a viewer");
    assert_eq!(png.relative_path, "up/pic.png", "the link's own spelling");
    assert_eq!(png.mount_point_id, MP);
    let yaml = view_target_for(m, &db("up/conf.yaml"))
        .unwrap()
        .expect("yaml is shown in a database store");
    let json = serde_json::to_string(&yaml).unwrap();
    assert!(
        json.starts_with(&format!("{{\"mountPointId\":\"{MP}\",\"relativePath\":\"up/conf.yaml\",\"fileName\":\"conf.yaml\",\"fileType\":")),
        "{json}"
    );
    assert_eq!(
        view_target_for(m, &db("up/none.png")).unwrap(),
        None,
        "no link, database → none"
    );
    let mut no_mp = db("up/pic.png");
    no_mp.mount_point_id = None;
    assert_eq!(view_target_for(m, &no_mp).unwrap(), None);
    // An unindexed disk file is still there to look at.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("raw.bin"), [0u8; 7]).unwrap();
    let t = view_target_for(m, &disk(dir.path(), "raw.bin", Some(MP)))
        .unwrap()
        .expect("viewer");
    let v = serde_json::to_value(&t).unwrap();
    assert_eq!(v["fileType"], "blob");
    assert_eq!(v["fileSizeBytes"], 7);
    assert_eq!(v["originalMimeType"], serde_json::Value::Null);
    assert!(v["updatedAt"].as_str().unwrap().ends_with('Z'));
    assert_eq!(
        view_target_for(m, &disk(dir.path(), "raw.md", Some(MP))).unwrap(),
        None,
        "disk text is edited"
    );
    assert_eq!(
        view_target_for(m, &disk(dir.path(), "gone.bin", Some(MP))).unwrap(),
        None
    );
}

// --- the sweeps (item 10) + the matcher's caches (item 20) -----------------

mod sweeps {
    use super::super::moves::{
        sweep_chat_documents_after_folder_move_with, sweep_chat_documents_after_move_with,
        SweepLookups, SweepStore,
    };
    use crate::test_support::global_capture;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    const STORE: &str = "5a000000-0000-4000-8000-000000000001";
    const OTHER: &str = "5a000000-0000-4000-8000-000000000002";

    #[derive(Clone, Default)]
    struct Counted {
        names: Rc<RefCell<Vec<String>>>,
        chats: Rc<RefCell<Vec<String>>>,
        by_name: HashMap<String, Vec<String>>,
        by_chat: HashMap<String, String>,
    }
    impl SweepLookups for Counted {
        fn store_ids_for_name(&mut self, name: &str) -> Vec<String> {
            self.names.borrow_mut().push(name.to_string());
            self.by_name
                .get(&name.trim().to_lowercase())
                .cloned()
                .unwrap_or_default()
        }
        fn official_store_for_chat(&mut self, chat_id: &str) -> Option<String> {
            self.chats.borrow_mut().push(chat_id.to_string());
            self.by_chat.get(chat_id).cloned()
        }
    }

    fn main_with(rows: &[(&str, &str, &str, &str, Option<&str>)]) -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE chat_documents (id TEXT PRIMARY KEY, chatId TEXT NOT NULL, \
             filePath TEXT NOT NULL, scope TEXT, mountPoint TEXT, displayTitle TEXT, \
             isActive INTEGER, createdAt TEXT, updatedAt TEXT)",
        )
        .unwrap();
        for (id, chat, path, scope, mp) in rows {
            c.execute(
                "INSERT INTO chat_documents VALUES (?1, ?2, ?3, ?4, ?5, 'old', 1, \
                 '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
                rusqlite::params![id, chat, path, scope, mp],
            )
            .unwrap();
        }
        c
    }

    fn path_of(c: &rusqlite::Connection, id: &str) -> (String, String) {
        c.query_row(
            "SELECT filePath, displayTitle FROM chat_documents WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }

    fn lookups() -> Counted {
        let mut l = Counted::default();
        l.by_name.insert("lore".into(), vec![STORE.into()]);
        l.by_name
            .insert("twins".into(), vec![STORE.into(), OTHER.into()]);
        l.by_chat.insert("chat-b".into(), STORE.into());
        l.by_chat.insert("chat-c".into(), OTHER.into());
        l
    }

    #[test]
    fn a_move_follows_every_row_that_reaches_the_same_store() {
        global_capture::install();
        let main = main_with(&[
            (
                "r-a",
                "chat-a",
                "notes/a.md",
                "document_store",
                Some("Lore"),
            ),
            ("r-b", "chat-b", "notes/a.md", "project", None),
            ("r-c", "chat-c", "notes/a.md", "project", None),
            ("r-g", "chat-a", "notes/a.md", "general", None),
            (
                "r-id",
                "chat-a",
                "notes/a.md",
                "document_store",
                Some(STORE),
            ),
            (
                "r-twin",
                "chat-a",
                "notes/a.md",
                "document_store",
                Some("Twins"),
            ),
            ("r-bare", "chat-a", "notes/a.md", "document_store", None),
        ]);
        let l = lookups();
        let store = SweepStore {
            scope: "document_store",
            mount_point: Some(Some("lore")),
            mount_point_id: Some(STORE),
        };
        let (n, lines) = global_capture::capture(|| {
            sweep_chat_documents_after_move_with(
                &main,
                l.clone(),
                store,
                "notes/a.md",
                "archive/b.md",
                None,
                Some("chat-x"),
            )
        });
        // By name (case-insensitive), as `project` through chat B's project,
        // and by the store's own id; not chat C's other project, not the
        // `general` row, not an ambiguous name, not a name-less store row.
        assert_eq!(n, 3);
        for id in ["r-a", "r-b", "r-id"] {
            assert_eq!(
                path_of(&main, id),
                ("archive/b.md".into(), "b.md".into()),
                "{id}"
            );
        }
        for id in ["r-c", "r-g", "r-twin", "r-bare"] {
            assert_eq!(path_of(&main, id).0, "notes/a.md", "{id}");
        }
        assert_eq!(
            lines,
            vec![format!("INFO quilltap::documents Swept chat_documents after a document move chatId=chat-x scope=document_store mountPoint=lore mountPointId={STORE} from=notes/a.md to=archive/b.md rowsUpdated=3")]
        );
        // Item 20: one lookup per cache key — "Lore" and "Twins" once each,
        // each chat once.
        assert_eq!(l.names.borrow().len(), 2, "{:?}", l.names.borrow());
        assert_eq!(l.chats.borrow().len(), 2, "{:?}", l.chats.borrow());
    }

    #[test]
    fn the_name_cache_keys_on_the_trimmed_lower_cased_name_and_the_chat_cache_on_the_chat() {
        let main = main_with(&[
            ("r-1", "chat-b", "a.md", "document_store", Some("Lore")),
            ("r-2", "chat-b", "a.md", "document_store", Some("  LORE ")),
            ("r-3", "chat-b", "a.md", "project", None),
            ("r-4", "chat-b", "a.md", "project", None),
        ]);
        let l = lookups();
        let store = SweepStore {
            scope: "project",
            mount_point: None,
            mount_point_id: Some(STORE),
        };
        let n = sweep_chat_documents_after_move_with(
            &main,
            l.clone(),
            store,
            "a.md",
            "b.md",
            Some("Bee"),
            None,
        );
        assert_eq!(n, 4);
        assert_eq!(
            l.names.borrow().as_slice(),
            ["Lore"],
            "one findByName for both spellings"
        );
        assert_eq!(
            l.chats.borrow().as_slice(),
            ["chat-b"],
            "one project hop for one chat"
        );
        assert_eq!(path_of(&main, "r-4").1, "Bee", "an explicit title wins");
    }

    #[test]
    fn without_a_store_id_only_the_literal_address_moves_and_a_miss_logs_debug() {
        global_capture::install();
        let main = main_with(&[
            ("r-g", "chat-a", "a.md", "general", None),
            ("r-p", "chat-a", "a.md", "project", None),
        ]);
        let store = SweepStore {
            scope: "general",
            mount_point: Some(None),
            mount_point_id: None,
        };
        assert_eq!(
            sweep_chat_documents_after_move_with(
                &main,
                lookups(),
                store,
                "a.md",
                "b.md",
                None,
                None
            ),
            1
        );
        let (n, lines) = global_capture::capture(|| {
            sweep_chat_documents_after_move_with(
                &main,
                lookups(),
                store,
                "none.md",
                "b.md",
                None,
                Some("c1"),
            )
        });
        assert_eq!(n, 0);
        assert_eq!(
            lines,
            vec!["DEBUG quilltap::documents No chat_documents rows to sweep after a document move chatId=c1 scope=general mountPoint=null from=none.md".to_string()]
        );
    }

    #[test]
    fn a_failed_update_warns_and_answers_zero() {
        global_capture::install();
        let main = main_with(&[("r-g", "chat-a", "a.md", "general", None)]);
        // A trigger makes the UPDATE itself fail; the read still finds the row.
        main.execute_batch(
            "CREATE TRIGGER no_update BEFORE UPDATE ON chat_documents \
             BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
        let store = SweepStore {
            scope: "general",
            mount_point: None,
            mount_point_id: None,
        };
        let (n, lines) = global_capture::capture(|| {
            sweep_chat_documents_after_move_with(
                &main,
                lookups(),
                store,
                "a.md",
                "b.md",
                None,
                None,
            )
        });
        assert_eq!(n, 0);
        assert_eq!(
            lines,
            vec!["WARN quilltap::documents Failed to sweep chat_documents after a document move scope=general from=a.md to=b.md error=refused".to_string()]
        );
        let (n, lines) = global_capture::capture(|| {
            sweep_chat_documents_after_folder_move_with(&main, lookups(), store, "", "x", None)
        });
        assert_eq!(n, 0);
        assert!(lines.is_empty(), "no row under '/': {lines:?}");
    }

    #[test]
    fn a_folder_move_rewrites_the_prefix_case_sensitively_and_logs_only_a_positive_sweep() {
        global_capture::install();
        let main = main_with(&[
            (
                "r-1",
                "chat-a",
                "Notes/a.md",
                "document_store",
                Some("Lore"),
            ),
            (
                "r-2",
                "chat-a",
                "Notes/sub/b.md",
                "document_store",
                Some("Lore"),
            ),
            (
                "r-3",
                "chat-a",
                "notes/c.md",
                "document_store",
                Some("Lore"),
            ),
            (
                "r-4",
                "chat-a",
                "Notesy/d.md",
                "document_store",
                Some("Lore"),
            ),
        ]);
        let store = SweepStore {
            scope: "document_store",
            mount_point: Some(Some("Lore")),
            mount_point_id: Some(STORE),
        };
        let (n, lines) = global_capture::capture(|| {
            sweep_chat_documents_after_folder_move_with(
                &main,
                lookups(),
                store,
                "Notes",
                "Archive/",
                Some("c1"),
            )
        });
        assert_eq!(n, 2);
        assert_eq!(
            path_of(&main, "r-1"),
            ("Archive/a.md".into(), "a.md".into())
        );
        assert_eq!(
            path_of(&main, "r-2"),
            ("Archive/sub/b.md".into(), "b.md".into())
        );
        assert_eq!(path_of(&main, "r-3").0, "notes/c.md", "R-6: case-sensitive");
        assert_eq!(path_of(&main, "r-4").0, "Notesy/d.md");
        assert_eq!(
            lines,
            vec![format!("INFO quilltap::documents Swept chat_documents after a folder move chatId=c1 scope=document_store mountPoint=Lore mountPointId={STORE} from=Notes to=Archive/ rowsUpdated=2")]
        );
        let (_, lines) = global_capture::capture(|| {
            sweep_chat_documents_after_folder_move_with(
                &main,
                lookups(),
                store,
                "Nowhere",
                "X",
                None,
            )
        });
        assert!(
            lines.is_empty(),
            "no DEBUG arm in the folder twin: {lines:?}"
        );
    }
}
