//! Tier-1 differential for the W4.6b Librarian-notifications PURE builders — the
//! byte-exact `build*Content` / `build*OpaqueContent`, `contentHiddenFromCharacters`,
//! and the summary prefixes, diffed against v4's REAL exports from
//! `lib/services/librarian-notifications/writer.ts` (oracle case
//! `harness/oracle/cases/post-office-librarian.ts`).
//!
//! The `post*` chat_messages rows are covered by a central combined tier-3 the
//! parent owns; this proves only the pure builders.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5/harness/oracle/cases/post-office-librarian.ts \
//!       > /tmp/oracle-po-librarian.ndjson
//! Run:
//!   QT_ORACLE_PO_LIBRARIAN=/tmp/oracle-po-librarian.ndjson \
//!     cargo test -p quilltap-harness --test post_office_librarian_equivalence

use serde_json::Value;

use quilltap_core::services::librarian_notifications as lib;
use quilltap_core::services::librarian_notifications::{
    LibrarianActorOrigin as Actor, LibrarianAttachAnnouncement, LibrarianBlobWriteAnnouncement,
    LibrarianCopyAnnouncement, LibrarianDeleteAnnouncement, LibrarianFolderCreatedAnnouncement,
    LibrarianFolderDeletedAnnouncement, LibrarianMoveAnnouncement, LibrarianOpenAnnouncement,
    LibrarianOpenKind, LibrarianRenameAnnouncement, LibrarianScope as Scope, LibrarianUpload,
    LibrarianUploadAnnouncement, LibrarianWriteAnnouncement, LibrarianWriteChange,
    SUMMARY_CONTENT_PREFIX, SUMMARY_OPAQUE_CONTENT_PREFIX,
};

const CHAT: &str = "chat-1";

fn scope(s: &str) -> Scope {
    match s {
        "project" => Scope::Project,
        "document_store" => Scope::DocumentStore,
        "general" => Scope::General,
        other => panic!("unknown scope {other}"),
    }
}

/// `LibrarianOpenKind` from a `(kind, name)` selector.
fn open_kind(k: &str) -> LibrarianOpenKind {
    match k {
        "user" => LibrarianOpenKind::ByUser,
        _ => LibrarianOpenKind::ByCharacter {
            character_name: k.to_string(),
        },
    }
}

fn actor(k: &str) -> Actor {
    match k {
        "user" => Actor::ByUser,
        _ => Actor::ByCharacter {
            character_name: k.to_string(),
        },
    }
}

// (scope, mountPoint-or-empty-for-null, origin-selector)
fn actor_case(id: &str) -> (Scope, Option<String>, Actor) {
    match id {
        "ds-user" => (
            Scope::DocumentStore,
            Some("Household".into()),
            actor("user"),
        ),
        "ds-char" => (Scope::DocumentStore, Some("Household".into()), actor("Ada")),
        "project-char" => (Scope::Project, None, actor("Zed")),
        "general-user" => (Scope::General, None, actor("user")),
        other => panic!("unknown actor case {other}"),
    }
}

fn open_case(id: &str) -> LibrarianOpenAnnouncement {
    let (sc, mp, is_new, origin) = match id {
        "ds-existing-user" => (
            "document_store",
            Some("Household"),
            false,
            open_kind("user"),
        ),
        "ds-new-char" => ("document_store", Some("Household"), true, open_kind("Ada")),
        "project-existing" => ("project", None, false, open_kind("user")),
        "general-new" => ("general", None, true, open_kind("Bea")),
        "ds-self" => ("document_store", Some("self"), false, open_kind("user")),
        "ds-nomount" => ("document_store", None, false, open_kind("user")),
        "ds-reserved-name" => ("document_store", Some("Project"), false, open_kind("user")),
        other => panic!("unknown open case {other}"),
    };
    LibrarianOpenAnnouncement {
        chat_id: CHAT.into(),
        display_title: "The Grand Ledger".into(),
        file_path: "Notes/a b.md".into(),
        scope: scope(sc),
        mount_point: mp.map(String::from),
        is_new,
        origin,
        hidden_from_characters: false,
    }
}

fn rename_case(id: &str) -> LibrarianRenameAnnouncement {
    let (sc, mp) = match id {
        "ds" => ("document_store", Some("Household")),
        "project" => ("project", None),
        "ds-self" => ("document_store", Some("self")),
        other => panic!("unknown rename case {other}"),
    };
    LibrarianRenameAnnouncement {
        chat_id: CHAT.into(),
        old_display_title: "Old Name".into(),
        new_display_title: "New Name".into(),
        old_file_path: "Notes/old.md".into(),
        new_file_path: "Notes/new.md".into(),
        scope: scope(sc),
        mount_point: mp.map(String::from),
        hidden_from_characters: false,
    }
}

fn save_diff(id: &str) -> String {
    match id {
        "matching-prefix" => {
            "I've made changes to \"notes.md\":\n```diff\n- old\n+ new\n```".into()
        }
        "no-match-fallback" => "Some diff without the expected prefix.".into(),
        "matching-multiline-title" => "I've made changes to \"a report\":\ndetails here".into(),
        other => panic!("unknown save case {other}"),
    }
}

fn attach_case(id: &str) -> LibrarianAttachAnnouncement {
    let (mp, mime, desc): (Option<&str>, &str, Option<&str>) = match id {
        "image-desc" => (
            Some("Album"),
            "image/png",
            Some("  A sunset over the bay.  "),
        ),
        "image-nodesc" => (Some("Album"), "IMAGE/JPEG", None),
        "doc-nomount" => (None, "text/markdown", Some("A memo.")),
        "image-emptydesc" => (Some("Album"), "image/webp", Some("   ")),
        other => panic!("unknown attach case {other}"),
    };
    LibrarianAttachAnnouncement {
        chat_id: CHAT.into(),
        display_title: "Portrait".into(),
        file_path: "photos/p.png".into(),
        mount_point: mp.map(String::from),
        mount_file_id: "link-1".into(),
        mime_type: mime.into(),
        description: desc.map(String::from),
    }
}

fn write_case(id: &str) -> LibrarianWriteAnnouncement {
    let big_body = (0..200)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let big_chars = "x".repeat(6200);
    let (sc, mp, origin, change): (&str, Option<&str>, Actor, LibrarianWriteChange) = match id {
        "created-body-user" => (
            "document_store",
            Some("Household"),
            actor("user"),
            LibrarianWriteChange::Created {
                body: "Hello world.".into(),
            },
        ),
        "created-empty-char" => (
            "project",
            None,
            actor("Ada"),
            LibrarianWriteChange::Created { body: "   ".into() },
        ),
        "edited-char" => (
            "document_store",
            Some("self"),
            actor("Bea"),
            LibrarianWriteChange::Edited {
                diff: "- a\n+ b".into(),
            },
        ),
        "created-truncate-lines" => (
            "general",
            None,
            actor("user"),
            LibrarianWriteChange::Created { body: big_body },
        ),
        "edited-truncate-chars" => (
            "document_store",
            Some("Household"),
            actor("user"),
            LibrarianWriteChange::Edited { diff: big_chars },
        ),
        other => panic!("unknown write case {other}"),
    };
    LibrarianWriteAnnouncement {
        chat_id: CHAT.into(),
        display_title: "Report".into(),
        uri: "qtap://self/Notes/a.md".into(),
        scope: scope(sc),
        mount_point: mp.map(String::from),
        origin,
        change,
        hidden_from_characters: false,
    }
}

fn move_case(id: &str) -> LibrarianMoveAnnouncement {
    let (sc, mp, origin, is_folder): (&str, Option<&str>, Actor, bool) = match id {
        "file-user" => ("document_store", Some("Household"), actor("user"), false),
        "folder-char" => ("project", None, actor("Ada"), true),
        other => panic!("unknown move case {other}"),
    };
    LibrarianMoveAnnouncement {
        chat_id: CHAT.into(),
        old_display_title: "Before".into(),
        new_display_title: "After".into(),
        old_uri: "qtap://self/a.md".into(),
        new_uri: "qtap://self/b.md".into(),
        scope: scope(sc),
        mount_point: mp.map(String::from),
        origin,
        is_folder,
        hidden_from_characters: false,
    }
}

fn copy_case(id: &str) -> LibrarianCopyAnnouncement {
    let origin = match id {
        "user" => actor("user"),
        "char" => actor("Ada"),
        other => panic!("unknown copy case {other}"),
    };
    LibrarianCopyAnnouncement {
        chat_id: CHAT.into(),
        source_display_title: "Original".into(),
        dest_display_title: "Duplicate".into(),
        source_mount_point: "Household".into(),
        dest_mount_point: "Archive".into(),
        source_uri: "qtap://Household/a.md".into(),
        dest_uri: "qtap://Archive/a.md".into(),
        origin,
        hidden_from_characters: false,
    }
}

fn blob_case(id: &str) -> LibrarianBlobWriteAnnouncement {
    let (mp, mime, size, desc, origin): (Option<&str>, &str, i64, Option<&str>, Actor) = match id {
        "image-desc" => (
            Some("Album"),
            "image/png",
            12345,
            Some("  A red door.  "),
            actor("user"),
        ),
        "asset-nodesc" => (None, "application/pdf", 999, None, actor("Ada")),
        other => panic!("unknown blob case {other}"),
    };
    LibrarianBlobWriteAnnouncement {
        chat_id: CHAT.into(),
        display_title: "Asset".into(),
        uri: "qtap://Album/img.png".into(),
        mount_point: mp.map(String::from),
        mime_type: mime.into(),
        size_bytes: size,
        description: desc.map(String::from),
        origin,
    }
}

fn upload_case(id: &str) -> LibrarianUploadAnnouncement {
    let up = |file_id: &str, filename: &str| LibrarianUpload {
        file_id: file_id.into(),
        filename: filename.into(),
    };
    let uploads = match id {
        "empty" => vec![],
        "single" => vec![up("f-1", "sunset.png")],
        "plural" => vec![up("f-1", "a.png"), up("f-2", "b.jpg")],
        other => panic!("unknown upload case {other}"),
    };
    LibrarianUploadAnnouncement {
        chat_id: CHAT.into(),
        uploads,
    }
}

fn content_hidden_case(id: &str) -> &'static str {
    match id {
        "no-frontmatter" => "Just a body, no frontmatter.",
        "hidden-true" => "---\ncharacter_read: false\n---\nsecret",
        "hidden-false" => "---\ncharacter_read: true\n---\nvisible",
        "default-visible" => "---\ntitle: Note\n---\nvisible",
        other => panic!("unknown content_hidden case {other}"),
    }
}

fn summary_body(id: &str) -> &'static str {
    match id {
        "plain" => "They agreed to meet at dawn.",
        "whitespace" => "   Trimmed body.   \n",
        other => panic!("unknown summary case {other}"),
    }
}

fn s(v: String) -> Value {
    Value::String(v)
}

fn rust_value(kind: &str, id: &str) -> Value {
    match kind {
        "content_hidden" => {
            Value::Bool(lib::content_hidden_from_characters(content_hidden_case(id)))
        }
        "open_content" => s(lib::build_open_content(&open_case(id))),
        "open_opaque" => s(lib::build_open_opaque_content(&open_case(id))),
        "rename_content" => s(lib::build_rename_content(&rename_case(id))),
        "rename_opaque" => s(lib::build_rename_opaque_content(&rename_case(id))),
        "save_content" => s(lib::build_save_content(&save_diff(id))),
        "save_opaque" => s(lib::build_save_opaque_content(&save_diff(id))),
        "delete_content" => {
            let (sc, mp, or) = actor_case(id);
            s(lib::build_delete_content(&LibrarianDeleteAnnouncement {
                chat_id: CHAT.into(),
                display_title: "The Ledger".into(),
                file_path: "Notes/x.md".into(),
                scope: sc,
                mount_point: mp,
                origin: or,
                hidden_from_characters: false,
            }))
        }
        "delete_opaque" => {
            let (sc, mp, or) = actor_case(id);
            s(lib::build_delete_opaque_content(
                &LibrarianDeleteAnnouncement {
                    chat_id: CHAT.into(),
                    display_title: "The Ledger".into(),
                    file_path: "Notes/x.md".into(),
                    scope: sc,
                    mount_point: mp,
                    origin: or,
                    hidden_from_characters: false,
                },
            ))
        }
        "folder_created_content" => {
            let (sc, mp, or) = actor_case(id);
            s(lib::build_folder_created_content(
                &LibrarianFolderCreatedAnnouncement {
                    chat_id: CHAT.into(),
                    folder_path: "Archive/2026".into(),
                    scope: sc,
                    mount_point: mp,
                    origin: or,
                },
            ))
        }
        "folder_created_opaque" => {
            let (sc, mp, or) = actor_case(id);
            s(lib::build_folder_created_opaque_content(
                &LibrarianFolderCreatedAnnouncement {
                    chat_id: CHAT.into(),
                    folder_path: "Archive/2026".into(),
                    scope: sc,
                    mount_point: mp,
                    origin: or,
                },
            ))
        }
        "folder_deleted_content" => {
            let (sc, mp, or) = actor_case(id);
            s(lib::build_folder_deleted_content(
                &LibrarianFolderDeletedAnnouncement {
                    chat_id: CHAT.into(),
                    folder_path: "Archive/2026".into(),
                    scope: sc,
                    mount_point: mp,
                    origin: or,
                },
            ))
        }
        "folder_deleted_opaque" => {
            let (sc, mp, or) = actor_case(id);
            s(lib::build_folder_deleted_opaque_content(
                &LibrarianFolderDeletedAnnouncement {
                    chat_id: CHAT.into(),
                    folder_path: "Archive/2026".into(),
                    scope: sc,
                    mount_point: mp,
                    origin: or,
                },
            ))
        }
        "attach_content" => s(lib::build_attach_content(&attach_case(id))),
        "attach_opaque" => s(lib::build_attach_opaque_content(&attach_case(id))),
        "write_content" => s(lib::build_write_content(&write_case(id))),
        "write_opaque" => s(lib::build_write_opaque_content(&write_case(id))),
        "move_content" => s(lib::build_move_content(&move_case(id))),
        "move_opaque" => s(lib::build_move_opaque_content(&move_case(id))),
        "copy_content" => s(lib::build_copy_content(&copy_case(id))),
        "copy_opaque" => s(lib::build_copy_opaque_content(&copy_case(id))),
        "blob_content" => s(lib::build_blob_write_content(&blob_case(id))),
        "blob_opaque" => s(lib::build_blob_write_opaque_content(&blob_case(id))),
        "upload_content" => s(lib::build_upload_content(&upload_case(id))),
        "upload_opaque" => s(lib::build_upload_opaque_content(&upload_case(id))),
        "summary_content" => s(lib::build_summary_content(summary_body(id))),
        "summary_opaque" => s(lib::build_summary_opaque_content(summary_body(id))),
        "const" => match id {
            "summary_content_prefix" => s(SUMMARY_CONTENT_PREFIX.to_string()),
            "summary_opaque_content_prefix" => s(SUMMARY_OPAQUE_CONTENT_PREFIX.to_string()),
            other => panic!("unknown const {other}"),
        },
        other => panic!("unknown kind {other}"),
    }
}

#[test]
fn post_office_librarian_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PO_LIBRARIAN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("QT_ORACLE_PO_LIBRARIAN unset; skipping");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let kind = row["kind"].as_str().unwrap();
        let id = row["id"].as_str().unwrap();
        let want = &row["value"];
        let got = rust_value(kind, id);
        assert_eq!(&got, want, "librarian builder {kind}/{id} diverges");
        count += 1;
    }
    assert!(count > 0, "oracle had no rows");
    eprintln!("post-office-librarian: {count} rows matched");
}
