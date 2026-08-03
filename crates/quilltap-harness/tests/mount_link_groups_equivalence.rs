//! Differential: `linkFile` binds a hard-link group, `copyFile` does not
//! (v4 `40319484`, `lib/mount-index/file-ops.ts`).
//!
//! **This surface had no oracle coverage at all before this family** — nothing
//! drove v4's real `linkFile` — so the `link`-vs-`copy` distinction, the single
//! user-visible thing the drift is about, was unverified.
//!
//! Both sides run the same op sequence over a fresh copy of the shared
//! doc-mount-file-links fixture, in both storage shapes:
//!
//!   - **DB → DB.** A copy and a link both end up sharing the source's content
//!     row (the store is content-addressed) and are indistinguishable until
//!     something is written. Then a write through any GROUP member repoints
//!     every member, while the copy forks onto its own content row. Linking a
//!     third location EXTENDS the group rather than minting a second.
//!   - **FS → FS.** The OS shares the bytes through the inode, so no content
//!     fan-out is needed — but the index rows are per-path, so `linkFile`
//!     records the group anyway. Two filesystem mounts are minted on a scratch
//!     directory and the source is indexed first (an unindexed source has no
//!     link id to bind — the arm v4 only warns about).
//!
//! Group ids and content-row ids are minted internally and are never compared:
//! both dumps replace them with first-seen labels in dump order, so what is
//! asserted is the RELATIONSHIP — who shares a group, who shares content — not
//! the values. Mount ids and the scratch paths are pinned/never dumped.
//!
//! Generate the oracle (Node 24, from the v4 checkout), after building the
//! shared fixture:
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   MIRROR=~/source/quilltap-server/.qt-oracle-mirror   # cp -R harness/oracle → here
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-dmfl-fixture.db \
//!     $N/node --import tsx $MIRROR/oracle/fixtures/build-doc-mount-file-links-fixture.ts
//!   QT_FIXTURE_MOUNT_LINK_GROUPS=/tmp/qt-dmfl-fixture.db \
//!     $N/node --import tsx $MIRROR/oracle/cases/mount-link-groups.ts \
//!     > /tmp/oracle-mount-link-groups.ndjson
//! Run:
//!   QT_ORACLE_MOUNT_LINK_GROUPS=/tmp/oracle-mount-link-groups.ndjson \
//!   QT_FIXTURE_MOUNT_LINK_GROUPS=/tmp/qt-dmfl-fixture.db \
//!     cargo test -p quilltap-harness --test mount_link_groups_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::db::doc_mount_points::{CreateOptions, DmpCreate, DocMountPointsRepository};
use quilltap_core::db::Writer;
use quilltap_core::services::mount_index::converters::RefusingTextExtractor;
use quilltap_core::services::mount_index::file_ops::{copy_file, link_file, CopyOpts};
use quilltap_core::services::mount_index::scanner::process_mount_file;
use serde_json::{json, Value};

const TEST_PEPPER: &str = "ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=";
const STORE_ID: &str = "5700000e-0000-4000-8000-0000000000d5";
const FS_SOURCE_MOUNT_ID: &str = "5700000e-0000-4000-8000-0000000000f1";
const FS_DEST_MOUNT_ID: &str = "5700000e-0000-4000-8000-0000000000f2";

fn fs_mount(base_path: &str, name: &str) -> DmpCreate {
    DmpCreate {
        name: name.to_string(),
        base_path: base_path.to_string(),
        mount_type: "filesystem".to_string(),
        store_type: "documents".to_string(),
        include_patterns: Vec::new(),
        exclude_patterns: Vec::new(),
        enabled: true,
        last_scanned_at: None,
        scan_status: "idle".to_string(),
        last_scan_error: None,
        conversion_status: "idle".to_string(),
        conversion_error: None,
        file_count: 0.0,
        chunk_count: 0.0,
        total_size_bytes: 0.0,
    }
}

/// `(mountPointId, relativePath, linkGroupId, fileId, description)`.
type LinkDumpRow = (String, String, Option<String>, String, Option<String>);

/// First-seen labelling, shared by both dumps: the VALUES are minted, the
/// relationships are the contract.
struct Labeller {
    map: HashMap<String, String>,
}

impl Labeller {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
    fn label(&mut self, prefix: &str, raw: Option<&str>) -> Value {
        match raw {
            None => Value::Null,
            Some(raw) => {
                let next = format!("{prefix}{}", self.map.len());
                Value::String(self.map.entry(raw.to_string()).or_insert(next).clone())
            }
        }
    }
}

#[test]
fn link_binds_a_group_and_copy_does_not() {
    let oracle_path = match std::env::var("QT_ORACLE_MOUNT_LINK_GROUPS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MOUNT_LINK_GROUPS to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_MOUNT_LINK_GROUPS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_MOUNT_LINK_GROUPS to the seed fixture .db (see header)."
            );
            return;
        }
    };
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let want: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    let scratch = std::env::temp_dir().join(format!("qt-mlg-rust-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let fs_source_base = scratch.join("fs-source");
    let fs_dest_base = scratch.join("fs-dest");
    std::fs::create_dir_all(&fs_source_base).expect("mkdir fs-source");
    std::fs::create_dir_all(&fs_dest_base).expect("mkdir fs-dest");
    std::fs::write(
        fs_source_base.join("shared.md"),
        "# Shared\n\nOne inode, two paths.\n",
    )
    .expect("seed fs source");

    let work = scratch.join("mount-index-work.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer =
        Writer::open_writable(&work, TEST_PEPPER).unwrap_or_else(|e| panic!("open fixture: {e}"));
    let conn = writer.connection();
    let extractor = RefusingTextExtractor;

    // ---- DB → DB -------------------------------------------------------
    let links = writer.doc_mount_file_links();
    links
        .write_database_document(STORE_ID, "origin.md", "# Origin\n\nThe first revision.")
        .expect("seed origin.md");

    copy_file(
        conn,
        &CopyOpts {
            source_mount_point_id: STORE_ID.to_string(),
            source_path: "origin.md".to_string(),
            dest_mount_point_id: STORE_ID.to_string(),
            dest_path: "copy-of-origin.md".to_string(),
            force: false,
        },
        &extractor,
    )
    .expect("copy_file");
    for dest in ["linked-origin.md", "third-origin.md"] {
        link_file(conn, STORE_ID, "origin.md", STORE_ID, dest, &extractor)
            .unwrap_or_else(|e| panic!("link_file {dest}: {e:?}"));
    }
    links
        .write_database_document(
            STORE_ID,
            "linked-origin.md",
            "# Origin\n\nThe second revision.",
        )
        .expect("write through a group member");

    // ---- FS → FS -------------------------------------------------------
    let points = DocMountPointsRepository::new(conn);
    for (id, name, base) in [
        (FS_SOURCE_MOUNT_ID, "FS Source", &fs_source_base),
        (FS_DEST_MOUNT_ID, "FS Dest", &fs_dest_base),
    ] {
        points
            .create(
                &fs_mount(base.to_str().expect("utf-8 base path"), name),
                &CreateOptions {
                    id: id.to_string(),
                    created_at: "2026-02-01T00:00:00.000Z".to_string(),
                    updated_at: "2026-02-01T00:00:00.000Z".to_string(),
                },
            )
            .expect("create fs mount");
    }
    // Index the source: an UNINDEXED filesystem source has no link id to bind.
    let source_mount = points
        .find_service_info_by_id(FS_SOURCE_MOUNT_ID)
        .expect("read fs source mount")
        .expect("fs source mount exists");
    process_mount_file(
        conn,
        &source_mount,
        fs_source_base
            .join("shared.md")
            .to_str()
            .expect("utf-8 path"),
        "shared.md",
        &extractor,
    )
    .expect("index the fs source");

    link_file(
        conn,
        FS_SOURCE_MOUNT_ID,
        "shared.md",
        FS_DEST_MOUNT_ID,
        "mirror.md",
        &extractor,
    )
    .expect("fs link_file");

    // ---- Dump ----------------------------------------------------------
    let mut labeller = Labeller::new();
    let mut got_links = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT l.mountPointId, l.relativePath, l.linkGroupId, l.fileId, l.description \
                   FROM doc_mount_file_links l \
                  ORDER BY l.mountPointId, l.relativePath",
            )
            .expect("prepare link dump");
        let rows: Vec<LinkDumpRow> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .expect("query link dump")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect link dump");
        for (mount_point_id, relative_path, link_group_id, file_id, description) in rows {
            got_links.push(json!({
                "mountPointId": mount_point_id,
                "relativePath": relative_path,
                "linkGroupId": labeller.label("G", link_group_id.as_deref()),
                "fileId": labeller.label("F", Some(&file_id)),
                "description": description.unwrap_or_default(),
            }));
        }
    }

    let mut got_contents = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT f.id AS fileId, COUNT(l.id) AS linkCount, d.content AS content \
                   FROM doc_mount_files f \
                   LEFT JOIN doc_mount_file_links l ON l.fileId = f.id \
                   LEFT JOIN doc_mount_documents d ON d.fileId = f.id \
                  GROUP BY f.id \
                  ORDER BY d.content, f.id",
            )
            .expect("prepare content census");
        let rows: Vec<(String, i64, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("query content census")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect content census");
        for (file_id, link_count, content) in rows {
            if link_count == 0 {
                continue;
            }
            got_contents.push(json!({
                "fileId": labeller.label("F", Some(&file_id)),
                "linkCount": link_count,
                "content": content,
            }));
        }
    }

    drop(writer);
    let _ = std::fs::remove_dir_all(&scratch);

    assert_eq!(
        Value::Array(got_links.clone()),
        want["links"],
        "link rows diverged"
    );
    assert_eq!(
        Value::Array(got_contents.clone()),
        want["contents"],
        "content census diverged"
    );

    // Shape assertions, so a corpus that silently stopped exercising the
    // distinction cannot pass. The labels are dump-order, so name them by path.
    let group_of = |path: &str| -> Value {
        got_links
            .iter()
            .find(|r| r["relativePath"] == Value::String(path.into()))
            .unwrap_or_else(|| panic!("{path} link"))["linkGroupId"]
            .clone()
    };
    let content_of = |path: &str| -> Value {
        got_links
            .iter()
            .find(|r| r["relativePath"] == Value::String(path.into()))
            .unwrap_or_else(|| panic!("{path} link"))["fileId"]
            .clone()
    };

    // `copy` does NOT bind, and so does NOT inherit the edit…
    assert!(
        group_of("copy-of-origin.md").is_null(),
        "a copy must not join a hard-link group"
    );
    assert_ne!(
        content_of("copy-of-origin.md"),
        content_of("origin.md"),
        "the copy must have forked on the first write through the group"
    );
    // …while `link` binds all three into ONE group, sharing one content row.
    assert!(
        !group_of("origin.md").is_null(),
        "link must bind a hard-link group"
    );
    assert_eq!(
        group_of("origin.md"),
        group_of("linked-origin.md"),
        "linked pair shares one group"
    );
    assert_eq!(
        group_of("origin.md"),
        group_of("third-origin.md"),
        "linking a third location EXTENDS the group"
    );
    assert_eq!(
        content_of("origin.md"),
        content_of("linked-origin.md"),
        "a write through a member repoints the anchor"
    );
    assert_eq!(
        content_of("origin.md"),
        content_of("third-origin.md"),
        "a write through a member repoints every member"
    );
    // The fs pair is bound too, across two different mounts.
    assert!(
        !group_of("shared.md").is_null(),
        "an fs link must be bound into a group"
    );
    assert_eq!(
        group_of("shared.md"),
        group_of("mirror.md"),
        "the fs pair shares one group"
    );

    eprintln!(
        "OK: mount-link-groups matched oracle ({} links, {} content rows).",
        got_links.len(),
        got_contents.len()
    );
}
