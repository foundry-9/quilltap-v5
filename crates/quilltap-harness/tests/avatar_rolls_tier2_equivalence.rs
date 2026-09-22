//! P4.D185 AVATAR-ROLLS differential: `photos::avatar_rolls_service` vs v4's
//! REAL `lib/photos/avatar-rolls-service.ts` (`4dcbe0d21`). Both sides read a
//! FRESH copy of the committed `avatar-rolls-{main,mount}.db` pair per case, so
//! the mutating save / set-avatar / delete cases cannot contaminate each other.
//!
//! Each case is judged on TWO comparands: what the call answered (or threw), and
//! — for every mutating case — a whole-table census of `files`, the characters'
//! two avatar pointers, `chats.characterAvatars`, `doc_mount_file_links`,
//! `doc_mount_blobs` and `doc_mount_files` afterwards. The answer alone cannot
//! tell "dropped the roll's link" from "dropped every link to the blob", which
//! is the single most expensive way to get this module wrong.
//!
//! **The minted ids are normalized, and nothing else is.** A save mints a link,
//! a `doc_mount_files` row and a blob; both engines mint different uuids, so
//! every id that is not baked into the fixture is replaced by `<minted-N>` in
//! FIRST-SIGHT order, which still distinguishes "the answer names the row the
//! census shows" from "the answer names something else".
//!
//! The bytes behind an album save come from [`FixtureBlobBytes`], which resolves
//! a `mount-blob:` storage key out of the fixture's own blob table — the same
//! read v4's real `fileStorageManager.downloadFile` makes (`manager.ts:386`) and
//! the same one the host's `ProductionFileBytes` makes in production. The oracle
//! un-mocks jest.setup's storage stub for exactly this reason (see its header).
//!
//! The committed fixture is built by `harness/oracle/fixtures/
//! build-avatar-rolls-fixture.ts` — its header carries the runnable command.
//!
//! Generate the oracle (Node 24, from the lane's pinned v4 worktree — see the
//! .ts header for the full recipe):
//!   … QT_ORACLE_OUT=/tmp/oracle-avatar-rolls.ndjson npx jest -- "cases/avatar-rolls-tier2\.test\.ts$"
//! Run:
//!   QT_ORACLE_AVATAR_ROLLS=/tmp/oracle-avatar-rolls.ndjson \
//!     cargo test -p quilltap-harness --test avatar_rolls_tier2_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::photos::avatar_rolls_service as svc;
use serde::Deserialize;
use serde_json::{json, Map, Value};

const ROLF: &str = "a1000000-0000-4000-8000-000000000001";
const SAGE: &str = "a1000000-0000-4000-8000-000000000002";
const TALL: &str = "a1000000-0000-4000-8000-000000000003";
const NOBODY: &str = "a9000000-0000-4000-8000-0000000000ff";

const F_ROLL_NEW: &str = "f1000000-0000-4000-8000-000000000001";
const F_ROLL_KEPT: &str = "f1000000-0000-4000-8000-000000000002";
const F_ROLL_ALBUM: &str = "f1000000-0000-4000-8000-000000000003";
const F_ROLL_NOLINK: &str = "f1000000-0000-4000-8000-000000000004";
const F_ROLL_SAGE: &str = "f1000000-0000-4000-8000-000000000006";
const F_NOT_ROLL: &str = "f1000000-0000-4000-8000-000000000007";
const F_ROLL_TALL: &str = "f1000000-0000-4000-8000-000000000008";
const F_MISSING: &str = "99999999-9999-4999-8999-999999999999";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    kept_at: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/avatar-rolls.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}
/// `api::characters`'s read helper, which is private to that module: one main
/// read with a mount read nested inside it.
fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(
        &rusqlite::Connection,
        &rusqlite::Connection,
    ) -> Result<T, quilltap_core::db::DbError>,
) -> Result<T, quilltap_core::db::DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
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

/// Resolve a `mount-blob:` storage key out of the fixture's own blob table —
/// what v4's real `downloadFile` does for these keys, and what the host's
/// `ProductionFileBytes` does in production.
fn read_mount_blob(db: &Db, storage_key: &str) -> Option<Vec<u8>> {
    let (_mp, blob_id) =
        quilltap_core::services::file_storage::parse_mount_blob_storage_key(storage_key)?;
    db.read_mount_index(move |conn| {
        conn.query_row(
            "SELECT data FROM doc_mount_blobs WHERE id = ?1",
            rusqlite::params![blob_id],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .map_err(Into::into)
    })
    .ok()
}

/// The `files` identity + storage key an album save needs, read off the row.
fn file_bytes_for(db: &Db, file_id: &str) -> Option<Vec<u8>> {
    let fid = file_id.to_string();
    let key: Option<String> = db
        .read_main(move |conn| {
            conn.query_row(
                "SELECT storageKey FROM files WHERE id = ?1",
                rusqlite::params![fid],
                |r| r.get::<_, Option<String>>(0),
            )
            .map_err(Into::into)
        })
        .ok()?;
    read_mount_blob(db, key.as_deref()?)
}

/// The whole-table census the write cases are judged on — the same tables and
/// the same order as the oracle's.
fn census(db: &Db) -> Value {
    let j = |s: Option<String>| -> Value {
        match s.as_deref() {
            None | Some("") => Value::Null,
            Some(t) => serde_json::from_str(t).unwrap_or(Value::Null),
        }
    };
    let files = db
        .read_main(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, originalFilename, generationKey IS NOT NULL, tags FROM files ORDER BY id",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("files census");
    let characters = db
        .read_main(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, defaultImageId, avatarOverrides FROM characters ORDER BY id",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("characters census");
    let chats = db
        .read_main(|conn| {
            let mut stmt = conn.prepare("SELECT id, characterAvatars FROM chats ORDER BY id")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("chats census");
    let links = db
        .read_mount_index(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, mountPointId, relativePath, fileId FROM doc_mount_file_links \
                 ORDER BY mountPointId, relativePath",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("links census");
    let blobs = db
        .read_mount_index(|conn| {
            let mut stmt = conn.prepare("SELECT fileId FROM doc_mount_blobs ORDER BY fileId")?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("blobs census");
    let mount_files = db
        .read_mount_index(|conn| {
            let mut stmt = conn.prepare("SELECT id, sha256 FROM doc_mount_files ORDER BY id")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("mount files census");

    json!({
        "files": files.into_iter().map(|(id, name, keyed, tags)| json!({
            "id": id, "originalFilename": name, "keyed": keyed, "tags": j(tags),
        })).collect::<Vec<_>>(),
        "characters": characters.into_iter().map(|(id, dii, ov)| json!({
            "id": id, "defaultImageId": dii, "avatarOverrides": j(ov),
        })).collect::<Vec<_>>(),
        "chats": chats.into_iter().map(|(id, ca)| json!({
            "id": id, "characterAvatars": j(ca),
        })).collect::<Vec<_>>(),
        "links": links.into_iter().map(|(id, mp, rel, fid)| json!({
            "id": id, "mountPointId": mp, "relativePath": rel, "fileId": fid,
        })).collect::<Vec<_>>(),
        "blobs": blobs.into_iter().map(|f| json!({ "fileId": f })).collect::<Vec<_>>(),
        "mountFiles": mount_files.into_iter().map(|(id, sha)| json!({
            "id": id, "sha256": sha,
        })).collect::<Vec<_>>(),
    })
}

/// Replace every uuid-shaped string that is NOT baked into the fixture with
/// `<minted-N>` in first-sight order, so a freshly minted link / mount-file /
/// blob id compares across engines while still linking the answer to the census.
struct Minted {
    baked: std::collections::HashSet<String>,
    seen: HashMap<String, String>,
}

impl Minted {
    fn new(baked: std::collections::HashSet<String>) -> Self {
        Minted {
            baked,
            seen: HashMap::new(),
        }
    }
    fn walk(&mut self, v: &mut Value) {
        match v {
            Value::String(s) => {
                if is_uuid(s) && !self.baked.contains(s.as_str()) {
                    let next = self.seen.len();
                    let label = self
                        .seen
                        .entry(s.clone())
                        .or_insert_with(|| format!("<minted-{next}>"))
                        .clone();
                    *s = label;
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| self.walk(x)),
            Value::Object(o) => o.iter_mut().for_each(|(_, x)| self.walk(x)),
            _ => {}
        }
    }
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.as_bytes()[8] == b'-'
        && s.as_bytes()[13] == b'-'
        && s.as_bytes()[18] == b'-'
        && s.as_bytes()[23] == b'-'
        && s.bytes().all(|b| b == b'-' || b.is_ascii_hexdigit())
}

/// Sort object keys so a mismatch reports values rather than ordering. The
/// entry DTO's raw key ORDER is pinned separately by [`key_order_pin`].
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        other => other.clone(),
    }
}

fn first_diff(a: &Value, b: &Value) -> String {
    let (x, y) = (
        serde_json::to_string_pretty(a).unwrap(),
        serde_json::to_string_pretty(b).unwrap(),
    );
    let mut out = String::new();
    for (la, lb) in x.lines().zip(y.lines()) {
        if la == lb {
            out.push_str(&format!("   = {la}\n"));
        } else {
            out.push_str(&format!("  GOT : {la}\n  WANT: {lb}\n"));
            return out;
        }
    }
    if x.lines().count() != y.lines().count() {
        out.push_str(&format!(
            "  (line count {} vs {})\n",
            x.lines().count(),
            y.lines().count()
        ));
    }
    out
}

fn open_fresh(spec: &Spec, tag: &str) -> (Db, PathBuf) {
    let scratch =
        std::env::temp_dir().join(format!("qt-avatar-rolls-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("avatar-rolls-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("avatar-rolls-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    (db, scratch)
}

/// v4's `saveAvatarRollToAlbum`, assembled from this port's two halves (the
/// bytes seam cannot run on the writer thread — see the service's module doc).
fn save_to_album(db: &Db, spec: &Spec, character_id: &str, file_id: &str) -> Result<Value, String> {
    let plan = {
        let cid = character_id.to_string();
        let fid = file_id.to_string();
        read_main_mount(db, move |main, mount| {
            Ok(svc::plan_album_save(main, mount, &cid, &fid))
        })
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
    };
    match plan {
        svc::AlbumSavePlan::AlreadyInAlbum { link_id } => {
            Ok(json!({ "linkId": link_id, "alreadyInAlbum": true }))
        }
        svc::AlbumSavePlan::NeedsBytes(bytes) => {
            let data = file_bytes_for(db, &bytes.file_id).unwrap_or_default();
            let cid = character_id.to_string();
            let fid = bytes.file_id.clone();
            let name = bytes.original_filename.clone();
            let mime = bytes.mime_type.clone();
            let kept_at = spec.kept_at.clone();
            let link_id = db
                .write_blocking(move |w| {
                    let mount = w
                        .mount_index()
                        .expect("the fixture carries a mount index")
                        .connection();
                    let main = w.main().connection();
                    Ok(svc::commit_album_save(
                        main, mount, &cid, &fid, &data, &name, &mime, &kept_at, &quilltap_core::services::mount_index::blob_transcode::RefusingWebpTranscoder,
                    ))
                })
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            Ok(json!({ "linkId": link_id, "alreadyInAlbum": false }))
        }
    }
}

#[test]
fn avatar_rolls_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_AVATAR_ROLLS") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        let name = v["name"].as_str().unwrap().to_string();
        order.push(name.clone());
        oracle.insert(name, v);
    }
    assert!(
        order.len() >= 23,
        "the oracle is stale: {} cases, expected at least 23",
        order.len()
    );

    // Every id the fixture bakes; anything else a case names was minted.
    let baked: std::collections::HashSet<String> = {
        let meta: Value = serde_json::from_str(
            &std::fs::read_to_string(fixtures_dir().join("avatar-rolls-main.db.meta.json"))
                .expect("read meta sidecar"),
        )
        .unwrap();
        let mut s: std::collections::HashSet<String> = std::collections::HashSet::new();
        fn collect(v: &Value, out: &mut std::collections::HashSet<String>) {
            match v {
                Value::String(x) if is_uuid(x) => {
                    out.insert(x.clone());
                }
                Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
                Value::Object(o) => o.iter().for_each(|(_, x)| collect(x, out)),
                _ => {}
            }
        }
        collect(&meta, &mut s);
        for id in [
            ROLF,
            SAGE,
            TALL,
            NOBODY,
            F_ROLL_NEW,
            F_ROLL_KEPT,
            F_ROLL_ALBUM,
            F_ROLL_NOLINK,
            F_ROLL_SAGE,
            F_NOT_ROLL,
            F_ROLL_TALL,
            F_MISSING,
        ] {
            s.insert(id.to_string());
        }
        // The chats and the project mount point are baked too.
        for id in [
            "c1000000-0000-4000-8000-000000000001",
            "c1000000-0000-4000-8000-000000000002",
            "c1000000-0000-4000-8000-000000000003",
            "f0000000-0000-4000-8000-00000000000f",
            "f1000000-0000-4000-8000-000000000005",
            "83000000-0000-4000-8000-000000000001",
        ] {
            s.insert(id.to_string());
        }
        s
    };

    let mut got_rows: Vec<(String, Value)> = Vec::new();
    let mut scratches: Vec<PathBuf> = Vec::new();

    // ---- the read cases (no census; one shared fresh copy is enough) --------
    {
        let (db, scratch) = open_fresh(&spec, "reads");
        let list = |db: &Db, cid: &str, limit: Option<i64>, offset: Option<i64>| -> Value {
            let cid = cid.to_string();
            let out = read_main_mount(db, move |main, mount| {
                Ok(svc::list_avatar_rolls(main, mount, &cid, limit, offset))
            })
            .expect("read");
            match out {
                Ok(v) => json!({ "result": v, "error": Value::Null }),
                Err(e) => json!({ "result": Value::Null, "error": e.to_string() }),
            }
        };
        for (name, cid, limit, offset) in [
            ("list_page", ROLF, None, None),
            ("list_limit_offset", ROLF, Some(2), Some(1)),
            ("list_limit_clamped_low", ROLF, Some(0), Some(-5)),
            ("list_limit_clamped_high", ROLF, Some(5000), None),
            ("list_offset_past_end", ROLF, None, Some(99)),
            ("list_sage", SAGE, None, None),
            ("list_no_vault", TALL, None, None),
            ("list_missing_character", NOBODY, None, None),
        ] {
            got_rows.push((name.to_string(), list(&db, cid, limit, offset)));
        }
        drop(db);
        scratches.push(scratch);
    }

    // ---- the mutating cases (a fresh copy each, then the census) ------------
    enum Op {
        Save(&'static str, &'static str),
        SetAvatar(&'static str, &'static str),
        Delete(&'static str, &'static str),
    }
    for (name, op, census_after) in [
        ("save_new", Op::Save(ROLF, F_ROLL_NEW), true),
        ("save_idempotent", Op::Save(ROLF, F_ROLL_KEPT), true),
        ("save_not_a_roll", Op::Save(ROLF, F_NOT_ROLL), true),
        (
            "save_other_characters_roll",
            Op::Save(ROLF, F_ROLL_SAGE),
            true,
        ),
        ("save_missing_file", Op::Save(ROLF, F_MISSING), false),
        ("save_no_vault", Op::Save(TALL, F_ROLL_TALL), true),
        (
            "set_avatar_saves_first",
            Op::SetAvatar(ROLF, F_ROLL_NEW),
            true,
        ),
        (
            "set_avatar_already_kept",
            Op::SetAvatar(ROLF, F_ROLL_KEPT),
            true,
        ),
        (
            "delete_scrubs_everything",
            Op::Delete(ROLF, F_ROLL_NEW),
            true,
        ),
        (
            "delete_keeps_the_album_copy",
            Op::Delete(ROLF, F_ROLL_KEPT),
            true,
        ),
        (
            "delete_album_only_roll",
            Op::Delete(ROLF, F_ROLL_ALBUM),
            true,
        ),
        (
            "delete_unlinked_roll",
            Op::Delete(ROLF, F_ROLL_NOLINK),
            true,
        ),
        (
            "delete_clears_legacy_portrait",
            Op::Delete(SAGE, F_ROLL_SAGE),
            true,
        ),
        (
            "delete_miss_is_not_a_throw",
            Op::Delete(ROLF, F_NOT_ROLL),
            true,
        ),
        (
            "delete_other_characters_roll",
            Op::Delete(ROLF, F_ROLL_SAGE),
            true,
        ),
    ] {
        let (db, scratch) = open_fresh(&spec, name);
        let mut row = match op {
            Op::Save(cid, fid) => match save_to_album(&db, &spec, cid, fid) {
                Ok(v) => json!({ "result": v, "error": Value::Null }),
                Err(e) => json!({ "result": Value::Null, "error": e }),
            },
            Op::SetAvatar(cid, fid) => {
                // v4 `setAvatarRollAsPortrait`: save first (idempotent), then
                // point `defaultImageId` at the ALBUM link.
                match save_to_album(&db, &spec, cid, fid) {
                    Ok(saved) => {
                        let link_id = saved["linkId"].as_str().unwrap_or_default().to_string();
                        let already = saved["alreadyInAlbum"].as_bool().unwrap_or(false);
                        let c = cid.to_string();
                        let f = fid.to_string();
                        let l = link_id.clone();
                        let out = db
                            .write_blocking(move |w| {
                                let mount = w
                                    .mount_index()
                                    .expect("the fixture carries a mount index")
                                    .connection();
                                let main = w.main().connection();
                                Ok(svc::point_portrait_at_link(
                                    main, mount, &c, &f, &l, !already,
                                ))
                            })
                            .expect("write");
                        match out {
                            Ok(()) => json!({
                                "result": { "linkId": link_id, "addedToAlbum": !already },
                                "error": Value::Null,
                            }),
                            Err(e) => json!({ "result": Value::Null, "error": e.to_string() }),
                        }
                    }
                    Err(e) => json!({ "result": Value::Null, "error": e }),
                }
            }
            Op::Delete(cid, fid) => {
                let c = cid.to_string();
                let f = fid.to_string();
                let out = db
                    .write_blocking(move |w| {
                        let mount = w
                            .mount_index()
                            .expect("the fixture carries a mount index")
                            .connection();
                        let main = w.main().connection();
                        Ok(svc::delete_avatar_roll(main, mount, &c, &f))
                    })
                    .expect("write");
                match out {
                    Ok(v) => json!({ "result": v.to_json(), "error": Value::Null }),
                    Err(e) => json!({ "result": Value::Null, "error": e.to_string() }),
                }
            }
        };
        if census_after {
            row.as_object_mut()
                .unwrap()
                .insert("state".into(), census(&db));
        }
        got_rows.push((name.to_string(), row));
        drop(db);
        scratches.push(scratch);
    }

    for s in &scratches {
        let _ = std::fs::remove_dir_all(s);
    }

    let mut failed = Vec::new();
    for (name, got) in &got_rows {
        let want_row = oracle
            .get(name)
            .unwrap_or_else(|| panic!("the oracle has no case {name}"));
        let mut want = json!({
            "result": want_row.get("result").cloned().unwrap_or(Value::Null),
            "error": want_row.get("error").cloned().unwrap_or(Value::Null),
        });
        if let Some(state) = want_row.get("state") {
            want.as_object_mut()
                .unwrap()
                .insert("state".into(), state.clone());
        }
        let mut g = got.clone();
        Minted::new(baked.clone()).walk(&mut g);
        let mut w = want;
        Minted::new(baked.clone()).walk(&mut w);
        // `chats.characterAvatars` reaches DISK re-serialized from the map the
        // scrub rewrote, and `sorted()` below is blind to key ORDER. v4's
        // `{...existing}; delete next[id]` keeps the survivors in insertion
        // order; a `Map::remove` under `preserve_order` swap-moves the last key
        // into the hole (the round's §3 catch). Chat A wears FOUR seats with
        // the scrubbed one SECOND, so the raw key sequence discriminates — with
        // three, the scrubbed key is second-to-last and the two deletes agree.
        if name == "delete_scrubs_everything" {
            let keys_of = |v: &Value| -> Vec<String> {
                v["state"]["chats"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|c| c["id"] == json!("c1000000-0000-4000-8000-000000000001"))
                    .and_then(|c| c["characterAvatars"].as_object())
                    .map(|o| o.keys().cloned().collect())
                    .unwrap_or_default()
            };
            let (gk, wk) = (keys_of(&g), keys_of(&w));
            assert!(
                gk.len() >= 3,
                "{name}: chat A must still wear at least THREE seats after the scrub, \
                 or a swap-remove of the second-to-last key is invisible: {gk:?}"
            );
            assert_eq!(
                gk, wk,
                "{name}: surviving `characterAvatars` keys must keep v4's order (swap-remove?)"
            );
        }
        let (g, w) = (sorted(&g), sorted(&w));
        if g != w {
            eprintln!("[{name}] MISMATCH:\n{}", first_diff(&g, &w));
            failed.push(name.clone());
        } else {
            eprintln!("[{name}] OK.");
        }
    }
    assert!(failed.is_empty(), "avatar-rolls FAILED: {failed:?}");
}

/// §C.6's key order is v4's declaration order, and `sorted()` above cannot see
/// it. Pin the raw sequence of the richest entry against v4's own bytes.
#[test]
fn key_order_pin() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_AVATAR_ROLLS") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let want: Vec<String> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["name"] == "list_page")
        .and_then(|v| {
            v["result"]["entries"][0]
                .as_object()
                .map(|o| o.keys().cloned().collect())
        })
        .expect("the oracle's list_page entry");

    let (db, scratch) = open_fresh(&spec, "keyorder");
    let cid = ROLF.to_string();
    let got_body = read_main_mount(&db, move |main, mount| {
        Ok(svc::list_avatar_rolls(main, mount, &cid, None, None))
    })
    .expect("read")
    .expect("list");
    let got: Vec<String> = got_body["entries"][0]
        .as_object()
        .expect("an entry")
        .keys()
        .cloned()
        .collect();
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);

    assert_eq!(got, want, "the AvatarRollEntry key order moved");
    eprintln!(
        "OK: AvatarRollEntry key order matches v4 ({} keys).",
        got.len()
    );
}

/// The three `[AvatarRolls]` info lines (v4 `avatar-rolls-service.ts:180,
/// 211, 301`) — a log-only surface no differential can see, pinned under a
/// thread-scoped capture over raw connections onto a scratch copy of the
/// committed pair (the service takes connections, so nothing here crosses the
/// writer thread). The delete arm is the unlinked roll on purpose: v4 logs
/// `rollLinkId: rollLink?.linkId ?? null`, and the port once rendered `""`.
#[test]
fn the_three_avatar_rolls_log_lines_fire_with_v4s_fields() {
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let scratch = std::env::temp_dir().join(format!("qt-avatar-rolls-logs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main_p = scratch.join("main.db");
    let mount_p = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("avatar-rolls-main.db"), &main_p).unwrap();
    std::fs::copy(fixtures_dir().join("avatar-rolls-mount.db"), &mount_p).unwrap();
    let main_w =
        quilltap_core::db::Writer::open_writable(&main_p, &spec.test_pepper_base64).unwrap();
    let mount_w =
        quilltap_core::db::Writer::open_writable(&mount_p, &spec.test_pepper_base64).unwrap();
    let (main, mount) = (main_w.connection(), mount_w.connection());

    // 1. copied into the album — the bytes come off the roll's own blob.
    let plan = svc::plan_album_save(main, mount, ROLF, F_ROLL_NEW).expect("plan");
    let svc::AlbumSavePlan::NeedsBytes(needs) = plan else {
        panic!("F_ROLL_NEW is not in the album yet");
    };
    let storage_key: String = main
        .query_row(
            "SELECT storageKey FROM files WHERE id = ?1",
            rusqlite::params![F_ROLL_NEW],
            |r| r.get(0),
        )
        .unwrap();
    let (_mp, blob_id) =
        quilltap_core::services::file_storage::parse_mount_blob_storage_key(&storage_key).unwrap();
    let bytes: Vec<u8> = mount
        .query_row(
            "SELECT data FROM doc_mount_blobs WHERE id = ?1",
            rusqlite::params![blob_id],
            |r| r.get(0),
        )
        .unwrap();
    let (link_id, lines) = quilltap_core::test_support::captured_with(|| {
        svc::commit_album_save(
            main,
            mount,
            ROLF,
            F_ROLL_NEW,
            &bytes,
            &needs.original_filename,
            &needs.mime_type,
            &spec.kept_at,
            &quilltap_core::services::mount_index::blob_transcode::RefusingWebpTranscoder,
        )
        .expect("commit")
    });
    assert!(
        lines.iter().any(
            |l| l.contains("[AvatarRolls] Roll copied into the photo album")
                && l.contains(F_ROLL_NEW)
                && l.contains(&link_id)
        ),
        "{lines:?}"
    );

    // 2. promoted to the portrait — `addedToAlbum` rides the line.
    let (_, lines) = quilltap_core::test_support::captured_with(|| {
        svc::point_portrait_at_link(main, mount, ROLF, F_ROLL_NEW, &link_id, true).expect("promote")
    });
    assert!(
        lines.iter().any(
            |l| l.contains("[AvatarRolls] Roll promoted to the character portrait")
                && l.contains("added_to_album=true")
        ),
        "{lines:?}"
    );

    // 3. deleted — the UNLINKED roll, whose `rollLinkId` v4 logs as `null`.
    let (out, lines) = quilltap_core::test_support::captured_with(|| {
        svc::delete_avatar_roll(main, mount, ROLF, F_ROLL_NOLINK).expect("delete")
    });
    assert!(out.deleted);
    let deleted = lines
        .iter()
        .find(|l| l.contains("[AvatarRolls] Roll deleted"))
        .unwrap_or_else(|| panic!("no delete line: {lines:?}"));
    assert!(
        deleted.contains("roll_link_id=\"null\"") || deleted.contains("roll_link_id=null"),
        "v4 logs `rollLinkId: null` for an unlinked roll, never the empty string: {deleted}"
    );
    assert!(
        deleted.contains("kept_in_album=false") && deleted.contains("chats_scrubbed=0"),
        "{deleted}"
    );

    drop(main_w);
    drop(mount_w);
    let _ = std::fs::remove_dir_all(&scratch);
}
