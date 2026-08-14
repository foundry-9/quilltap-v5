//! P4.9E2A POST OFFICE / ANNOUNCER route-surface differential:
//! `api::chat_post_office::*` vs v4's REAL chat action handlers
//! (`actions/announcement.ts`, `actions/announcement-preview.ts`,
//! `actions/send-mail.ts`, `actions/mailbox.ts`). Both sides read a FRESH copy of
//! the committed `post-office-{main,mount}.db` fixture per case (baked ids
//! identical → no remap; the announcement message's minted `id`/`createdAt` are
//! blanked, and the delivered letter's `sentAt` is the injected `NOW_MS` the
//! oracle freezes its clock to, so the filename's epoch prefix matches).
//!
//! Coverage is asserted by SHAPE, not by a hand-written count: every case name
//! the oracle emitted must have been driven here, and vice versa
//! (`harness-corpus-shape-constants-rot`).
//!
//! ## Two recorded, direction-asserted differences
//!
//! 1. **[`V4_CREATED_CASES`]** — v4 answers **201** on the two `created`-style
//!    arms. v5's dispatch boundary carries no per-verb success status (the
//!    standing `ChatCreate` precedent: every success is 200 at the HTTP edge), so
//!    those cases assert v4==201 ∧ v5==200 rather than equality. The BODY is
//!    compared byte-for-byte, which is what §1 freezes.
//! 2. **[`VALIDATION_DETAILS_GAP`]** — v4's Zod arm answers `{error: 'Validation
//!    error', details: […]}`; v5's error envelope has no `details` array (the
//!    standing, named P4.6bb deferral). Those cases assert v4 HAS a non-empty
//!    `details` and v5 has none, then compare the rest — the gap is asserted, not
//!    normalized away.
//!
//! ## The `doc_mount_chunks` tripwire (order §2) — CLOSED at unification
//!
//! v4's `writeDatabaseDocument` chunks every write; when this lane was written
//! v5's did not, so the two send-mail cases that deliver a letter differed by
//! exactly one chunk in the recipient vault. Per order §2 the counts were DUMPED
//! and asserted as `v4 == v5 + 1`, so the assertion would FAIL the moment the gap
//! closed.
//!
//! **P4.6BK closed it in the same round** and the tripwire fired as designed. The
//! `chunkCount` in `recipientShape` is now part of the ordinary equality diff —
//! nothing is plucked out, and the two sides agree.
//!
//! ## P4.D37 — the whisper audience (v4 `a163862c`)
//!
//! Nine `announcement_whisper_*` arms: the persisted `targetParticipantIds` on the
//! message dump IS the evidence (named audience, explicit-null, empty-array,
//! duplicate collapse, a live participant whose character row is missing, a custom
//! sender), plus the two 400s the resolver exists to produce (a removed
//! participant, an id from another chat) and the Zod array-of-uuid rejection.
//!
//! **One arm is a COMPOSED pin, not a single-site one.**
//! `announcement_whisper_empty_array_is_null` cannot be turned red by mutating
//! either normalization alone, because v4 and v5 both collapse an empty audience
//! TWICE — once in the resolver (`[]` → PUBLIC) and again in the writer (`[]` →
//! NULL on the row). The writer's own site is pinned directly by
//! `services::announcer::writer`'s `empty_audience_stores_null_…` unit test, which
//! mirrors v4's own jest case (`announcer-audience.test.ts` calls
//! `postAdhocAnnouncement` with `targetParticipantIds: []` directly) and was
//! mutation-proven sensitive.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-post-office.ndjson npx jest -- post-office-routes
//! Run:
//!   QT_ORACLE_POST_OFFICE=/tmp/oracle-post-office.ndjson \
//!     cargo test -p quilltap-harness --test post_office_routes_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::chat_post_office;
use quilltap_core::api::types::{AnnouncerSenderWire, ErrorKind, Response, StaffSenderWire};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const BEA: &str = "a1000000-0000-4000-8000-000000000002";
const CLIO: &str = "a1000000-0000-4000-8000-000000000003";
const DORIAN: &str = "a1000000-0000-4000-8000-000000000004";
const ELISE: &str = "a1000000-0000-4000-8000-000000000005";
const GONE: &str = "a1000000-0000-4000-8000-000000000006";
const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const CONN: &str = "c0000001-0000-4000-8000-000000000001";
const MISSING_ID: &str = "99999999-9999-4999-8999-999999999999";
// P4.D37: CHAT PARTICIPANT ids (not character ids) — the whisper audience's
// currency. P_GONE carries `removedAt`; P_GHOST is live but points at a character
// row that does not exist (so it resolves as a target and names to 'Someone').
const P_ARIA: &str = "e1000000-0000-4000-8000-000000000001";
const P_BEA: &str = "e1000000-0000-4000-8000-000000000002";
const P_GONE: &str = "e1000000-0000-4000-8000-000000000005";
const P_GHOST: &str = "e1000000-0000-4000-8000-000000000006";
const NOW_MS: i64 = 1_777_939_200_000; // 2026-05-05T00:00:00.000Z

/// Cases where v4 answers 201 and the dispatch boundary answers 200 (see the
/// module header).
const V4_CREATED_CASES: &[&str] = &[
    "announcement_staff",
    "announcement_staff_pascal",
    "announcement_character",
    "announcement_custom",
    "announcement_whisper_named",
    "announcement_whisper_null_is_public",
    "announcement_whisper_empty_array_is_null",
    "announcement_whisper_duplicate_ids",
    "announcement_whisper_ghost_participant",
    "announcement_whisper_custom_sender",
    "send_mail_ok",
    "send_mail_reply",
];

/// Cases whose 400 body carries v4's Zod `details` array, which v5's envelope
/// does not model (see the module header).
const VALIDATION_DETAILS_GAP: &[&str] = &[
    "announcement_empty_content",
    "announcement_display_name_too_long",
    "announcement_unknown_staff",
    "announcement_whisper_invalid_uuid",
    "preview_empty_seed",
    "send_mail_empty_body",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    aria_vault: String,
    bea_vault: String,
    elise_vault: String,
    seeded_letter_path: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/post-office-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
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

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-po-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("post-office-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("post-office-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// Plant a tombstone on this case's FRESH COPY (P4.D65).
///
/// Two things happen here, and both are deliberate.
///
/// **The three archive columns are ADDED first.** This family's committed
/// fixture predates the D23 re-dump that introduced them, and it stays that
/// way: only three cases want an archived seat, while widening the committed
/// bytes would move every other case's dump AND the sibling `announcer_tier3`
/// family that shares the file. Adding them per case, on the throwaway copy, is
/// the same shape the boot ensure performs on a real instance carried forward
/// from 4.7 — and it leaves the fixture's vintage (and the read tolerance it
/// covers) exactly as it was.
///
/// **The flag goes on raw, not through the repository:**
/// `validate_character_archive_patch` sanctions exactly one patch on an
/// archived row, and would refuse the one that puts the row into that state to
/// begin with. The stamp is a fixed literal so nothing minted reaches a dump.
fn archive_character(db: &Db, rt: &tokio::runtime::Runtime, character_id: &str) {
    let id = character_id.to_string();
    rt.block_on(db.write(move |ws| {
        let conn = ws.main().connection();
        let existing: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(characters)")?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>("name"))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for column in ["archivedAt", "archiveFileId", "archivedAvatarFileId"] {
            if !existing.iter().any(|c| c == column) {
                conn.execute_batch(&format!(
                    "ALTER TABLE \"characters\" ADD COLUMN \"{column}\" TEXT"
                ))?;
            }
        }
        conn.execute(
            "UPDATE characters SET archivedAt = ?1 WHERE id = ?2",
            rusqlite::params!["2026-05-02T00:00:00.000Z", id],
        )?;
        Ok(())
    }))
    .expect("plant the tombstone");
}

// ---------------------------------------------------------------------------
// Normalization (the courier-images mold)
// ---------------------------------------------------------------------------

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
/// Blank the minted announcement keys (`id` / `createdAt` — the writer mints both
/// from the process clock on the Rust side and from v4's frozen clock there).
fn blank_minted(v: &mut Value) {
    if let Value::Object(o) = v {
        for k in ["id", "createdAt"] {
            if o.contains_key(k) {
                o.insert(k.to_string(), Value::String(format!("<{k}>")));
            }
        }
        o.iter_mut().for_each(|(_, x)| blank_minted(x));
    } else if let Value::Array(a) = v {
        a.iter_mut().for_each(blank_minted);
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_minted(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

/// The web-edge (status, body) for a Response (the HTTP transport mapping).
fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatPostOffice(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 423,
                // The store-unavailable refusal (P4.23) — v4's deliberate
                // contextful 503 (context.ts:176-205).
                ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

// ---------------------------------------------------------------------------
// Table dumps (mirror the oracle's readers exactly)
// ---------------------------------------------------------------------------

fn dump_messages(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    Value::Array(
        db.read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
            .unwrap(),
    )
}

fn dump_character_vault_fk(db: &Db, character_id: &str) -> Value {
    let cid = character_id.to_string();
    let row = db
        .read_main(move |c| quilltap_core::db::characters_read::find_by_id_raw(c, &cid))
        .unwrap();
    row.map(|r| {
        json!({
            "id": r.get("id").cloned().unwrap_or(Value::Null),
            "characterDocumentMountPointId":
                r.get("characterDocumentMountPointId").cloned().unwrap_or(Value::Null),
        })
    })
    .unwrap_or(Value::Null)
}

fn dump_vault_mail(db: &Db, vault_id: &str) -> Value {
    let vid = vault_id.to_string();
    db.read_mount_index(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT l.relativePath AS path, d.content AS content \
             FROM doc_mount_file_links l \
             JOIN doc_mount_files f ON f.id = l.fileId \
             JOIN doc_mount_documents d ON d.fileId = f.id \
             WHERE l.mountPointId = ?1 AND l.relativePath LIKE 'Mail/%' \
             ORDER BY l.relativePath",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![vid], |r| {
                Ok(json!({
                    "path": r.get::<_, String>(0)?,
                    "content": r.get::<_, String>(1)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, quilltap_core::db::DbError>(Value::Array(rows))
    })
    .unwrap()
}

fn dump_vault_shape(db: &Db, vault_id: &str) -> Value {
    let vid = vault_id.to_string();
    db.read_mount_index(move |conn| {
        let count = |sql: &str| -> rusqlite::Result<i64> {
            conn.query_row(sql, rusqlite::params![vid.clone()], |r| r.get(0))
        };
        let link_count =
            count("SELECT COUNT(*) FROM doc_mount_file_links WHERE mountPointId = ?1")?;
        let folder_count = count("SELECT COUNT(*) FROM doc_mount_folders WHERE mountPointId = ?1")?;
        let chunk_count = count("SELECT COUNT(*) FROM doc_mount_chunks WHERE mountPointId = ?1")?;
        let mp = conn
            .query_row(
                "SELECT id, name, storeType, mountType FROM doc_mount_points WHERE id = ?1",
                rusqlite::params![vid.clone()],
                |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "name": r.get::<_, String>(1)?,
                        "storeType": r.get::<_, Option<String>>(2)?,
                        "mountType": r.get::<_, Option<String>>(3)?,
                    }))
                },
            )
            .ok()
            .unwrap_or(Value::Null);
        let character_mount_point_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM doc_mount_points WHERE storeType = 'character'",
            [],
            |r| r.get(0),
        )?;
        Ok::<_, quilltap_core::db::DbError>(json!({
            "mountPoint": mp,
            "linkCount": link_count,
            "folderCount": folder_count,
            "chunkCount": chunk_count,
            "characterMountPointCount": character_mount_point_count,
        }))
    })
    .unwrap()
}

fn staff(id: StaffSenderWire) -> AnnouncerSenderWire {
    AnnouncerSenderWire::Staff { staff_id: id }
}

#[test]
fn post_office_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_POST_OFFICE") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("post-office-main.db.meta.json")).unwrap(),
    )
    .unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert!(
        !oracle.is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();
    let now_iso = quilltap_core::clock::iso_from_unix_ms(NOW_MS);

    let mut check = |name: &str, resp: &Response, tables: Option<Value>| {
        driven.insert(name.to_string());
        let Some(want) = oracle.get(name) else {
            failed.push(format!("{name}_MISSING_FROM_ORACLE"));
            return;
        };
        let (status, body) = status_body(resp);
        let want_status = want["status"].as_u64().unwrap() as u16;
        let mut want_body = want["body"].clone();

        // The v4-201 vs dispatch-200 difference, asserted in both directions.
        if V4_CREATED_CASES.contains(&name) {
            if want_status != 201 || status != 200 {
                eprintln!(
                    "[{name}] the recorded 201-vs-200 difference no longer holds \
                     (v4={want_status}, v5={status}) — re-rule it or drop the entry"
                );
                failed.push(format!("{name}_status"));
            }
        } else if status != want_status {
            eprintln!("[{name}] STATUS {status} != {want_status}");
            failed.push(format!("{name}_status"));
        }

        // The Zod `details` gap, asserted in both directions.
        if VALIDATION_DETAILS_GAP.contains(&name) {
            let v4_details_ok = want_body
                .get("details")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty());
            let v5_has_details = body.get("details").is_some();
            if !v4_details_ok || v5_has_details {
                eprintln!(
                    "[{name}] the recorded validation-details gap no longer holds \
                     (v4 details present={v4_details_ok}, v5 details present={v5_has_details}) \
                     — re-rule it or drop the entry"
                );
                failed.push(format!("{name}_details_gap"));
            }
            if let Some(o) = want_body.as_object_mut() {
                o.remove("details");
            }
        }

        if norm(&body) != norm(&want_body) {
            eprintln!(
                "[{name}] BODY MISMATCH:\n{}",
                first_diff(&norm(&body), &norm(&want_body))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] body OK.");
        }

        if let Some(got_tables) = tables {
            let want_tables = want["tables"].clone();
            if norm(&got_tables) != norm(&want_tables) {
                eprintln!(
                    "[{name} tables] MISMATCH:\n{}",
                    first_diff(&norm(&got_tables), &norm(&want_tables))
                );
                failed.push(format!("{name}_tables"));
            } else {
                eprintln!("[{name} tables] OK.");
            }
        }
    };

    // ── ?action=announcement ────────────────────────────────────────────────
    let mut announcement = |tag: &str,
                            name: &str,
                            chat: &str,
                            content: &str,
                            sender: AnnouncerSenderWire,
                            targets: Option<&[&str]>,
                            dump: bool| {
        let targets: Option<Vec<String>> =
            targets.map(|t| t.iter().map(|s| s.to_string()).collect());
        let db = fresh_db(&spec, tag);
        let r = rt.block_on(chat_post_office::chat_announcement_post(
            &db,
            chat,
            content,
            &sender,
            targets.as_deref(),
        ));
        let tables = dump.then(|| json!({ "messages": dump_messages(&db, CHAT) }));
        check(name, &r, tables);
    };
    announcement(
        "ann1",
        "announcement_staff",
        CHAT,
        "  The Lantern is lit; the evening post departs at eight.  ",
        staff(StaffSenderWire::Lantern),
        None,
        true,
    );
    announcement(
        "ann2",
        "announcement_staff_pascal",
        CHAT,
        "The table is open.",
        staff(StaffSenderWire::Pascal),
        None,
        true,
    );
    announcement(
        "ann3",
        "announcement_character",
        CHAT,
        "Dorian sends word from the far bank.",
        AnnouncerSenderWire::Character {
            character_id: DORIAN.to_string(),
        },
        None,
        true,
    );
    announcement(
        "ann4",
        "announcement_custom",
        CHAT,
        "A stranger leaves a card at the door.",
        AnnouncerSenderWire::Custom {
            display_name: "The Night Porter".to_string(),
        },
        None,
        true,
    );
    announcement(
        "ann5",
        "announcement_character_missing",
        CHAT,
        "Nobody sends this.",
        AnnouncerSenderWire::Character {
            character_id: MISSING_ID.to_string(),
        },
        None,
        false,
    );
    announcement(
        "ann6",
        "announcement_chat_missing",
        MISSING_ID,
        "Into the void.",
        staff(StaffSenderWire::Host),
        None,
        false,
    );
    announcement(
        "ann7",
        "announcement_empty_content",
        CHAT,
        "",
        staff(StaffSenderWire::Host),
        None,
        false,
    );
    announcement(
        "ann8",
        "announcement_whitespace_content",
        CHAT,
        "   \n\t  ",
        staff(StaffSenderWire::Host),
        None,
        true,
    );
    announcement(
        "ann9",
        "announcement_display_name_too_long",
        CHAT,
        "Too long a name.",
        AnnouncerSenderWire::Custom {
            display_name: "x".repeat(121),
        },
        None,
        false,
    );
    // ── P4.D37 (a163862c): the whisper audience ─────────────────────────────
    announcement(
        "annw1",
        "announcement_whisper_named",
        CHAT,
        "Only the two of you need hear this.",
        staff(StaffSenderWire::Host),
        Some(&[P_ARIA, P_BEA]),
        true,
    );
    announcement(
        "annw2",
        "announcement_whisper_null_is_public",
        CHAT,
        "Everyone hears this one.",
        staff(StaffSenderWire::Host),
        None,
        true,
    );
    announcement(
        "annw3",
        "announcement_whisper_empty_array_is_null",
        CHAT,
        "Nobody named.",
        staff(StaffSenderWire::Host),
        Some(&[]),
        true,
    );
    announcement(
        "annw4",
        "announcement_whisper_duplicate_ids",
        CHAT,
        "Said once.",
        staff(StaffSenderWire::Host),
        Some(&[P_BEA, P_ARIA, P_BEA]),
        true,
    );
    announcement(
        "annw5",
        "announcement_whisper_removed_participant",
        CHAT,
        "For someone who left.",
        staff(StaffSenderWire::Host),
        Some(&[P_GONE]),
        false,
    );
    announcement(
        "annw6",
        "announcement_whisper_foreign_id",
        CHAT,
        "For a stranger.",
        staff(StaffSenderWire::Host),
        Some(&[P_ARIA, MISSING_ID]),
        false,
    );
    announcement(
        "annw7",
        "announcement_whisper_ghost_participant",
        CHAT,
        "For the unnameable.",
        staff(StaffSenderWire::Host),
        Some(&[P_GHOST]),
        true,
    );
    announcement(
        "annw8",
        "announcement_whisper_custom_sender",
        CHAT,
        "Psst.",
        AnnouncerSenderWire::Custom {
            display_name: "A Distant Bell".to_string(),
        },
        Some(&[P_ARIA]),
        true,
    );
    announcement(
        "annw9",
        "announcement_whisper_invalid_uuid",
        CHAT,
        "Malformed audience.",
        staff(StaffSenderWire::Host),
        Some(&["not-a-uuid"]),
        false,
    );

    // `announcement_unknown_staff` — v4 rejects `staffId: 'quartermaster'` at the
    // enum. The typed boundary makes an unknown staff id unrepresentable, so the
    // rejection happens one layer earlier (dispatch deserialization): assert THAT,
    // then answer the same envelope v5's dispatch edge would.
    {
        let raw = json!({
            "type": "chatAnnouncementPost",
            "chatId": CHAT,
            "contentMarkdown": "Who?",
            "sender": { "kind": "staff", "staffId": "quartermaster" },
        });
        let parsed: Result<quilltap_core::api::types::Request, _> = serde_json::from_value(raw);
        assert!(
            parsed.is_err(),
            "an unknown staffId must not deserialize — the §1 enum is the Zod enum"
        );
        check(
            "announcement_unknown_staff",
            &Response::error(ErrorKind::BadRequest, "Validation error"),
            None,
        );
    }

    // ── ?action=announcement-preview (the pre-LLM arms) ─────────────────────
    let mut preview =
        |tag: &str, name: &str, chat: &str, seed: &str, character: &str, profile: &str| {
            let db = fresh_db(&spec, tag);
            let r = rt.block_on(chat_post_office::chat_announcement_preview(
                &db,
                None, // the pre-LLM arms all return before the driver is reached
                "e18e05bc-63e8-4539-8a85-719b7a508850",
                chat,
                seed,
                character,
                profile,
                None,
                None,
            ));
            check(name, &r, None);
        };
    preview(
        "pv1",
        "preview_character_missing",
        CHAT,
        "Tell them the lamps are out.",
        MISSING_ID,
        CONN,
    );
    preview(
        "pv2",
        "preview_profile_missing",
        CHAT,
        "Tell them the lamps are out.",
        DORIAN,
        MISSING_ID,
    );
    preview("pv3", "preview_empty_seed", CHAT, "", DORIAN, CONN);
    preview(
        "pv4",
        "preview_chat_missing",
        MISSING_ID,
        "Tell them.",
        DORIAN,
        CONN,
    );

    // ── ?action=send-mail ──────────────────────────────────────────────────
    {
        let db = fresh_db(&spec, "sm1");
        let r = rt.block_on(chat_post_office::chat_send_mail(
            &db,
            CHAT,
            ARIA,
            BEA,
            "Bea — I asked Dorian. He says the oil was short.",
            None,
            &now_iso,
        ));
        let tables = json!({
            "recipientMail": dump_vault_mail(&db, &meta.bea_vault),
            "senderMail": dump_vault_mail(&db, &meta.aria_vault),
            "recipientShape": dump_vault_shape(&db, &meta.bea_vault),
        });
        check("send_mail_ok", &r, Some(tables));
    }
    {
        let db = fresh_db(&spec, "sm2");
        let r = rt.block_on(chat_post_office::chat_send_mail(
            &db,
            CHAT,
            ARIA,
            BEA,
            "Bea — answering yours below.",
            Some(&meta.seeded_letter_path),
            &now_iso,
        ));
        let tables = json!({
            "recipientMail": dump_vault_mail(&db, &meta.bea_vault),
            "recipientShape": dump_vault_shape(&db, &meta.bea_vault),
        });
        check("send_mail_reply", &r, Some(tables));
    }
    {
        let db = fresh_db(&spec, "sm3");
        let r = rt.block_on(chat_post_office::chat_send_mail(
            &db,
            CHAT,
            ARIA,
            BEA,
            "Answering a letter I never had.",
            Some("Mail/1700000000000-from-nobody.md"),
            &now_iso,
        ));
        let tables = json!({ "recipientMail": dump_vault_mail(&db, &meta.bea_vault) });
        check("send_mail_reply_not_found", &r, Some(tables));
    }
    let mut send_mail_err =
        |tag: &str, name: &str, chat: &str, from: &str, to: &str, body: &str| {
            let db = fresh_db(&spec, tag);
            let r = rt.block_on(chat_post_office::chat_send_mail(
                &db, chat, from, to, body, None, &now_iso,
            ));
            check(name, &r, None);
        };
    send_mail_err(
        "sm4",
        "send_mail_from_llm_character",
        CHAT,
        CLIO,
        BEA,
        "Signed by an LLM character.",
    );
    send_mail_err(
        "sm5",
        "send_mail_from_removed_participant",
        CHAT,
        GONE,
        BEA,
        "Signed by someone who left.",
    );
    send_mail_err(
        "sm6",
        "send_mail_from_stranger",
        CHAT,
        DORIAN,
        BEA,
        "Signed by a stranger to this scene.",
    );
    send_mail_err(
        "sm7",
        "send_mail_recipient_missing",
        CHAT,
        ARIA,
        MISSING_ID,
        "Into the void.",
    );
    send_mail_err(
        "sm8",
        "send_mail_sender_row_missing",
        CHAT,
        MISSING_ID,
        BEA,
        "From a participant with no character row.",
    );
    send_mail_err("sm9", "send_mail_empty_body", CHAT, ARIA, BEA, "");
    send_mail_err(
        "sm10",
        "send_mail_chat_missing",
        MISSING_ID,
        ARIA,
        BEA,
        "Into the void.",
    );
    // ── P4.D65: the archived-character 400s (the banked P4.D63 unit-4 arms) ──
    // The tombstone is planted per case on the FRESH COPY rather than baked
    // into the committed fixture: nothing else in this family wants an archived
    // seat, and a baked one would drag every other case's dumps with it. Both
    // send-mail arms read the sender and the recipient by RAW ID, so no name
    // resolution stands between the flag and the refusal.
    {
        let db = fresh_db(&spec, "sm11");
        archive_character(&db, &rt, ARIA);
        let r = rt.block_on(chat_post_office::chat_send_mail(
            &db,
            CHAT,
            ARIA,
            BEA,
            "One last letter.",
            None,
            &now_iso,
        ));
        check("send_mail_from_archived_sender", &r, None);
    }
    {
        let db = fresh_db(&spec, "sm12");
        archive_character(&db, &rt, BEA);
        let r = rt.block_on(chat_post_office::chat_send_mail(
            &db,
            CHAT,
            ARIA,
            BEA,
            "Are you still there?",
            None,
            &now_iso,
        ));
        check("send_mail_to_archived_recipient", &r, None);
    }

    // ── GET ?action=mailbox ────────────────────────────────────────────────
    {
        let db = fresh_db(&spec, "mb1");
        let r = rt.block_on(chat_post_office::chat_mailbox_list(&db, CHAT, ARIA));
        let tables = json!({
            "vault": dump_character_vault_fk(&db, ARIA),
            "shape": dump_vault_shape(&db, &meta.aria_vault),
        });
        check("mailbox_aria", &r, Some(tables));
    }
    {
        let db = fresh_db(&spec, "mb2");
        let r = rt.block_on(chat_post_office::chat_mailbox_list(&db, CHAT, BEA));
        check("mailbox_bea_empty", &r, None);
    }
    {
        let db = fresh_db(&spec, "mb3");
        let r = rt.block_on(chat_post_office::chat_mailbox_list(&db, CHAT, ELISE));
        let tables = json!({
            "vault": dump_character_vault_fk(&db, ELISE),
            "shape": dump_vault_shape(&db, &meta.elise_vault),
        });
        check("mailbox_elise_adopts_vault", &r, Some(tables));
    }
    let mut mailbox_err = |tag: &str, name: &str, chat: &str, character: &str| {
        let db = fresh_db(&spec, tag);
        let r = rt.block_on(chat_post_office::chat_mailbox_list(&db, chat, character));
        check(name, &r, None);
    };
    mailbox_err("mb4", "mailbox_character_row_missing", CHAT, MISSING_ID);
    mailbox_err("mb5", "mailbox_forbidden_llm_character", CHAT, CLIO);
    mailbox_err("mb6", "mailbox_forbidden_stranger", CHAT, DORIAN);
    mailbox_err("mb7", "mailbox_missing_character_id", CHAT, "");
    mailbox_err("mb8", "mailbox_chat_missing", MISSING_ID, ARIA);
    {
        // Authorized (Aria is a seat the operator plays), then refused on the
        // FLAG — before the vault ensure, so an archived character's mailbox
        // read never touches her vault.
        let db = fresh_db(&spec, "mb9");
        archive_character(&db, &rt, ARIA);
        let r = rt.block_on(chat_post_office::chat_mailbox_list(&db, CHAT, ARIA));
        check("mailbox_archived_character", &r, None);
    }

    // Shape assertion: the driven set and the oracle's set must be identical.
    let recorded: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = recorded.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&recorded).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case coverage drifted — oracle-only: {missing:?}, rust-only: {extra:?}"
    );
    eprintln!("post-office-routes: {} cases driven.", driven.len());

    assert!(failed.is_empty(), "post-office-routes FAILED: {failed:?}");
}
