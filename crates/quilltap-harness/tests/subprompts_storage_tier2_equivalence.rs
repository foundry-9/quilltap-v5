//! Tier-2 differential: the subprompts STORAGE + FAN-OUT (v4 `2f4254b42`,
//! `lib/subprompts/subprompts.ts` + `chat-fanout.ts`) —
//! `quilltap_core::subprompts::{storage, fanout}`, P4.D163 unit 3.
//!
//! Both sides run each case on a FRESH copy of the committed
//! `crates/quilltap-web/tests/fixtures/subprompts-{main,mount}.db` pair (built
//! by `harness/oracle/fixtures/build-subprompts-fixture.ts` through v4's REAL
//! repositories with a FROZEN clock, so every baked timestamp is the seed
//! sentinel) and compare: the op's RESULT (the returned record / list / bool
//! / fan-out counts, or `{error: {name, message}}`), an id-free semantic
//! CENSUS of the mount tables joined by path (`points` / `folders` / `links`
//! with their `file` + `document` rows / orphan counts) plus the `characters`
//! vault links and the `chats` participants + `compiledIdentityStacks`, and
//! the two RECORDED seams (`compile: [chatId, participantId]…`, `publish:
//! [topic, id]…`). The compiler and the realtime bus are recorders on BOTH
//! sides (v4's own `chat-fanout.test.ts` mocks the same two modules) because
//! P4.D164's block render lands after this unit; the end-to-end compile is
//! that order's `subprompts_prompt_tier2_equivalence`.
//!
//! NORMALIZATION: a timestamp EQUAL to the seed sentinel is diffed exactly (a
//! baked file's `updatedAt` proves `toISOString(mtime)` byte for byte); any
//! other ISO timestamp is `<ts>`. Mount-point ids map to the fixture's vault
//! KEYS via the `.meta.json` sidecar (`charA`…), an unknown one to
//! `<new-point>`. Nothing else is remapped — the census carries no ids.
//!
//! Generate the oracle (Node 24, from the v4 checkout — a pinned worktree
//! while v4 HEAD is past the baseline; cp to a /tmp mirror, jest ignores
//! .claude/):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-subprompts-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/subprompts-storage.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/subprompts.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/subprompts-main.db \
//!   QT_FIXTURE_SP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/subprompts-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-subprompts-storage.ndjson TZ=UTC \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- subprompts-storage
//! Run:
//!   QT_ORACLE_SUBPROMPTS_STORAGE=/tmp/oracle-subprompts-storage.ndjson \
//!     cargo test -p quilltap-harness --test subprompts_storage_tier2_equivalence -- --nocapture

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use quilltap_core::db::{DbError, Writer};
use quilltap_core::subprompts::{
    create_character_subprompt, delete_character_subprompt, fan_out_subprompt_change,
    list_character_subprompts, read_character_subprompt, resolve_selected_subprompts,
    update_character_subprompt, FanoutOptions, FanoutSeams, SubpromptCreateInput, SubpromptError,
    SubpromptPatch,
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    seed_timestamp: String,
    ids: BTreeMap<String, String>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/subprompts.json")
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

/// The recorder seams — `compile` pushes `(chatId, participantId)` and, with
/// `fail_first`, throws on the FIRST call (v4's `mockRejectedValueOnce`
/// arm); `publish` pushes `(topic, id)`.
struct Recorder {
    compile: Mutex<Vec<(String, String)>>,
    publish: Mutex<Vec<(String, Option<String>)>>,
    calls: AtomicUsize,
    fail_first: bool,
}

impl Recorder {
    fn new(fail_first: bool) -> Self {
        Self {
            compile: Mutex::new(vec![]),
            publish: Mutex::new(vec![]),
            calls: AtomicUsize::new(0),
            fail_first,
        }
    }
    fn recorded(&self) -> Value {
        let compile: Vec<Value> = self
            .compile
            .lock()
            .unwrap()
            .iter()
            .map(|(c, p)| json!([c, p]))
            .collect();
        let publish: Vec<Value> = self
            .publish
            .lock()
            .unwrap()
            .iter()
            .map(|(t, id)| json!([t, id]))
            .collect();
        json!({ "compile": compile, "publish": publish })
    }
}

impl FanoutSeams for Recorder {
    fn compile(
        &self,
        _main: &Connection,
        _mount: &Connection,
        chat: &Value,
        participant_id: &str,
    ) -> Result<(), DbError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        self.compile.lock().unwrap().push((
            chat.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            participant_id.to_string(),
        ));
        if self.fail_first && n == 1 {
            return Err(DbError::Internal("boom".to_string()));
        }
        Ok(())
    }
    fn publish_chat(&self, chat_id: &str) {
        self.publish
            .lock()
            .unwrap()
            .push(("chats".to_string(), Some(chat_id.to_string())));
    }
    fn publish_character(&self, character_id: &str) {
        self.publish
            .lock()
            .unwrap()
            .push(("characters".to_string(), Some(character_id.to_string())));
    }
}

/// Sentinel-or-`<ts>`.
fn ts(v: Option<String>, sentinel: &str) -> Value {
    match v {
        Some(s)
            if s.len() >= 11
                && s.as_bytes()[10] == b'T'
                && s[..4].chars().all(|c| c.is_ascii_digit()) =>
        {
            if s == sentinel {
                Value::String(s)
            } else {
                Value::String("<ts>".to_string())
            }
        }
        Some(s) => Value::String(s),
        None => Value::Null,
    }
}

fn norm_result(v: Value, sentinel: &str) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.into_iter().map(|x| norm_result(x, sentinel)).collect()),
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, val) in o {
                if k == "updatedAt" {
                    out.insert(k, ts(val.as_str().map(str::to_string), sentinel));
                } else {
                    out.insert(k, norm_result(val, sentinel));
                }
            }
            Value::Object(out)
        }
        other => other,
    }
}

fn error_json(e: &SubpromptError) -> Value {
    let name = match e {
        SubpromptError::NotFound(_) => "SubpromptNotFoundError",
        SubpromptError::Validation(_) => "SubpromptValidationError",
        SubpromptError::Archived { .. } => "CharacterArchivedError",
        SubpromptError::Store(_) | SubpromptError::Db(_) => "Error",
    };
    json!({ "error": { "name": name, "message": e.to_string() } })
}

fn opt_str(row: &rusqlite::Row<'_>, idx: usize) -> Option<String> {
    row.get::<_, Option<String>>(idx).unwrap_or(None)
}
fn opt_val(row: &rusqlite::Row<'_>, idx: usize) -> Value {
    match row.get_ref(idx) {
        Ok(rusqlite::types::ValueRef::Null) | Err(_) => Value::Null,
        Ok(rusqlite::types::ValueRef::Integer(i)) => json!(i),
        Ok(rusqlite::types::ValueRef::Real(f)) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                json!(f as i64)
            } else {
                json!(f)
            }
        }
        Ok(rusqlite::types::ValueRef::Text(t)) => {
            Value::String(String::from_utf8_lossy(t).into_owned())
        }
        Ok(rusqlite::types::ValueRef::Blob(b)) => Value::String(hex::encode(b)),
    }
}

/// The id-free census — the SAME queries and shapes as the oracle's `census`.
fn census(
    main: &Connection,
    mount: &Connection,
    sentinel: &str,
    vaults: &HashMap<String, String>,
) -> Value {
    let point = |id: Option<String>| -> Value {
        match id {
            Some(id) => Value::String(
                vaults
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| "<new-point>".to_string()),
            ),
            None => Value::Null,
        }
    };
    let s_of = |v: &Value| v.as_str().unwrap_or_default().to_string();

    let mut points: Vec<Value> = mount
        .prepare("SELECT id, name, storeType, createdAt, updatedAt FROM doc_mount_points")
        .unwrap()
        .query_map([], |r| {
            Ok(json!({
                "point": point(opt_str(r, 0)), "name": opt_val(r, 1), "storeType": opt_val(r, 2),
                "createdAt": ts(opt_str(r, 3), sentinel), "updatedAt": ts(opt_str(r, 4), sentinel),
            }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    points.sort_by(|a, b| {
        s_of(&a["point"])
            .cmp(&s_of(&b["point"]))
            .then(s_of(&a["name"]).cmp(&s_of(&b["name"])))
    });

    let mut folders: Vec<Value> = mount
        .prepare(
            "SELECT f.mountPointId, f.name, f.path, p.path AS parentPath, f.createdAt, f.updatedAt \
             FROM doc_mount_folders f LEFT JOIN doc_mount_folders p ON p.id = f.parentId",
        )
        .unwrap()
        .query_map([], |r| {
            Ok(json!({
                "point": point(opt_str(r, 0)), "name": opt_val(r, 1), "path": opt_val(r, 2),
                "parentPath": opt_val(r, 3),
                "createdAt": ts(opt_str(r, 4), sentinel), "updatedAt": ts(opt_str(r, 5), sentinel),
            }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    folders.sort_by(|a, b| {
        s_of(&a["point"])
            .cmp(&s_of(&b["point"]))
            .then(s_of(&a["path"]).cmp(&s_of(&b["path"])))
    });

    let cols = [
        "mountPointId",
        "relativePath",
        "fileName",
        "folderPath",
        "linkGroupId",
        "originalFileName",
        "originalMimeType",
        "description",
        "descriptionUpdatedAt",
        "conversionStatus",
        "conversionError",
        "plainTextLength",
        "extractedText",
        "extractedTextSha256",
        "extractionStatus",
        "extractionError",
        "chunkCount",
        "allowEmbed",
        "allowCharacterRead",
        "allowCharacterWrite",
        "lastModified",
        "createdAt",
        "updatedAt",
        "f_sha256",
        "f_size",
        "f_type",
        "f_source",
        "f_createdAt",
        "f_updatedAt",
        "d_content",
        "d_sha",
        "d_len",
        "d_createdAt",
        "d_updatedAt",
        "d_id",
    ];
    let sql =
        "SELECT l.mountPointId, l.relativePath, l.fileName, fo.path AS folderPath, l.linkGroupId, \
               l.originalFileName, l.originalMimeType, l.description, l.descriptionUpdatedAt, \
               l.conversionStatus, l.conversionError, l.plainTextLength, l.extractedText, \
               l.extractedTextSha256, l.extractionStatus, l.extractionError, l.chunkCount, \
               l.allowEmbed, l.allowCharacterRead, l.allowCharacterWrite, l.lastModified, \
               l.createdAt, l.updatedAt, \
               fi.sha256, fi.fileSizeBytes, fi.fileType, fi.source, fi.createdAt, fi.updatedAt, \
               d.content, d.contentSha256, d.plainTextLength, d.createdAt, d.updatedAt, d.id \
               FROM doc_mount_file_links l \
               LEFT JOIN doc_mount_folders fo ON fo.id = l.folderId \
               LEFT JOIN doc_mount_files fi ON fi.id = l.fileId \
               LEFT JOIN doc_mount_documents d ON d.fileId = l.fileId";
    let idx = |name: &str| cols.iter().position(|c| *c == name).unwrap();
    let mut links: Vec<Value> = mount
        .prepare(sql)
        .unwrap()
        .query_map([], |r| {
            let g = |name: &str| opt_val(r, idx(name));
            let gs = |name: &str| opt_str(r, idx(name));
            let file = if gs("f_sha256").is_none() {
                Value::Null
            } else {
                json!({
                    "sha256": g("f_sha256"), "fileSizeBytes": g("f_size"), "fileType": g("f_type"),
                    "source": g("f_source"),
                    "createdAt": ts(gs("f_createdAt"), sentinel), "updatedAt": ts(gs("f_updatedAt"), sentinel),
                })
            };
            let document = if gs("d_id").is_none() {
                Value::Null
            } else {
                json!({
                    "content": g("d_content"), "contentSha256": g("d_sha"), "plainTextLength": g("d_len"),
                    "createdAt": ts(gs("d_createdAt"), sentinel), "updatedAt": ts(gs("d_updatedAt"), sentinel),
                })
            };
            Ok(json!({
                "point": point(gs("mountPointId")),
                "relativePath": g("relativePath"),
                "fileName": g("fileName"),
                "folderPath": g("folderPath"),
                "linkGroup": gs("linkGroupId").is_some(),
                "originalFileName": g("originalFileName"),
                "originalMimeType": g("originalMimeType"),
                "description": g("description"),
                "descriptionUpdatedAt": ts(gs("descriptionUpdatedAt"), sentinel),
                "conversionStatus": g("conversionStatus"),
                "conversionError": g("conversionError"),
                "plainTextLength": g("plainTextLength"),
                "extractedText": g("extractedText"),
                "extractedTextSha256": g("extractedTextSha256"),
                "extractionStatus": g("extractionStatus"),
                "extractionError": g("extractionError"),
                "chunkCount": g("chunkCount"),
                "allowEmbed": g("allowEmbed"),
                "allowCharacterRead": g("allowCharacterRead"),
                "allowCharacterWrite": g("allowCharacterWrite"),
                "lastModified": ts(gs("lastModified"), sentinel),
                "createdAt": ts(gs("createdAt"), sentinel),
                "updatedAt": ts(gs("updatedAt"), sentinel),
                "file": file,
                "document": document,
            }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    links.sort_by(|a, b| {
        s_of(&a["point"])
            .cmp(&s_of(&b["point"]))
            .then(s_of(&a["relativePath"]).cmp(&s_of(&b["relativePath"])))
    });

    let count = |sql: &str| -> i64 { mount.query_row(sql, [], |r| r.get(0)).unwrap() };
    let orphan_files = count(
        "SELECT COUNT(*) FROM doc_mount_files fi WHERE NOT EXISTS (SELECT 1 FROM doc_mount_file_links l WHERE l.fileId = fi.id)",
    );
    let orphan_documents = count(
        "SELECT COUNT(*) FROM doc_mount_documents d WHERE NOT EXISTS (SELECT 1 FROM doc_mount_file_links l WHERE l.fileId = d.fileId)",
    );

    let characters: Vec<Value> = main
        .prepare("SELECT id, characterDocumentMountPointId, archivedAt FROM characters ORDER BY id")
        .unwrap()
        .query_map([], |r| {
            Ok(json!({
                "id": opt_val(r, 0),
                "vault": point(opt_str(r, 1)),
                "archivedAt": ts(opt_str(r, 2), sentinel),
            }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    let chats: Vec<Value> = main
        .prepare(
            "SELECT id, participants, compiledIdentityStacks, updatedAt FROM chats ORDER BY id",
        )
        .unwrap()
        .query_map([], |r| {
            let parts: Value = opt_str(r, 1)
                .filter(|s| !s.is_empty())
                .map(|s| serde_json::from_str(&s).unwrap())
                .unwrap_or(Value::Null);
            let parts = match parts {
                Value::Array(a) => Value::Array(
                    a.into_iter()
                        .map(|p| {
                            let mut o = p.as_object().cloned().unwrap_or_default();
                            for k in ["createdAt", "updatedAt"] {
                                let v = o.get(k).and_then(Value::as_str).map(str::to_string);
                                if v.is_some() {
                                    o.insert(k.to_string(), ts(v, sentinel));
                                }
                            }
                            Value::Object(o)
                        })
                        .collect(),
                ),
                other => other,
            };
            let stacks: Value = opt_str(r, 2)
                .filter(|s| !s.is_empty())
                .map(|s| serde_json::from_str(&s).unwrap())
                .unwrap_or(Value::Null);
            Ok(json!({
                "id": opt_val(r, 0),
                "participants": parts,
                "compiledIdentityStacks": stacks,
                "updatedAt": ts(opt_str(r, 3), sentinel),
            }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    json!({
        "points": points, "folders": folders, "links": links,
        "orphanFiles": orphan_files, "orphanDocuments": orphan_documents,
        "characters": characters, "chats": chats,
    })
}

/// Canonical (sorted-key) JSON for the diff.
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
        _ => v.clone(),
    }
}

fn first_diff(a: &Value, b: &Value) -> String {
    let sa = serde_json::to_string_pretty(a).unwrap();
    let sb = serde_json::to_string_pretty(b).unwrap();
    let la: Vec<&str> = sa.lines().collect();
    let lb: Vec<&str> = sb.lines().collect();
    for i in 0..la.len().max(lb.len()) {
        let x = la.get(i).copied().unwrap_or("<eof>");
        let y = lb.get(i).copied().unwrap_or("<eof>");
        if x != y {
            let lo = i.saturating_sub(3);
            let ctx: Vec<String> = (lo..i)
                .map(|j| format!("   = {}", la.get(j).copied().unwrap_or("")))
                .collect();
            return format!("{}\n  GOT : {x}\n  WANT: {y}", ctx.join("\n"));
        }
    }
    String::new()
}

struct Case {
    name: &'static str,
    census: bool,
    recorded: bool,
    fail_first: bool,
}
const fn c(name: &'static str) -> Case {
    Case {
        name,
        census: false,
        recorded: false,
        fail_first: false,
    }
}
const fn cc(name: &'static str) -> Case {
    Case {
        name,
        census: true,
        recorded: false,
        fail_first: false,
    }
}
const fn fo(name: &'static str, fail_first: bool) -> Case {
    Case {
        name,
        census: true,
        recorded: true,
        fail_first,
    }
}

const CASES: &[Case] = &[
    c("list_a"),
    c("list_b_no_folder"),
    c("list_c_no_vault"),
    c("list_d_archived_reads"),
    c("list_missing_character"),
    c("read_a_terse"),
    c("read_a_Verse_exact_case"),
    c("read_a_verse_lower_case"),
    c("read_a_no_title"),
    c("read_a_gone"),
    c("read_a_nested_invalid_id"),
    c("read_a_broken_document"),
    c("read_a_notes_txt_not_a_subprompt"),
    c("read_d_keep_archived_reads"),
    c("read_c_no_vault"),
    c("read_missing_character"),
    c("resolve_a_terse_VERSE_gone"),
    c("resolve_a_listing_order_not_tick_order"),
    c("resolve_a_empty"),
    c("resolve_a_dupes_one_wanted"),
    c("resolve_a_broken_dropped"),
    c("resolve_c_no_vault"),
    c("resolve_d_archived_reads"),
    c("resolve_missing_character"),
    cc("create_a_collision_suffixes"),
    cc("create_a_trims_title_and_content"),
    cc("create_b_ensures_folder"),
    cc("create_b_twice_ensures_folder_each_time"),
    cc("create_c_provisions_vault"),
    cc("create_d_archived_409"),
    cc("create_a_blank_title"),
    cc("create_a_blank_content"),
    c("create_a_title_101_units"),
    c("create_a_title_100_astral_is_200_units"),
    c("create_a_title_validated_before_content"),
    c("create_missing_character"),
    cc("update_a_title_only"),
    cc("update_a_content_only"),
    cc("update_a_both"),
    cc("update_a_empty_patch_rewrites"),
    cc("update_a_no_title_file_keeps_id_title"),
    cc("update_a_Verse_exact_case"),
    c("update_a_verse_lower_case"),
    c("update_a_invalid_id"),
    c("update_a_missing_id"),
    c("update_a_broken_document"),
    cc("update_a_blank_title"),
    c("update_a_blank_content"),
    c("update_d_archived_409"),
    cc("update_c_no_vault_provisions_then_404"),
    c("update_missing_character"),
    cc("delete_a_terse"),
    cc("delete_a_broken_link_only"),
    cc("delete_a_gone"),
    c("delete_a_invalid_id"),
    cc("delete_d_archived_409"),
    c("delete_d_invalid_id_before_archive_check"),
    cc("delete_c_no_vault_provisions_then_false"),
    c("delete_missing_character"),
    fo("fanout_terse", false),
    fo("fanout_verse_case_insensitive", false),
    fo("fanout_terse_remove_selection", false),
    fo("fanout_TERSE_remove_selection_case_insensitive", false),
    fo("fanout_verse_remove_selection_strips_VERSE", false),
    fo("fanout_gone", false),
    fo("fanout_b_terse", false),
    fo("fanout_missing_character", false),
    fo("fanout_terse_compile_fails_once", true),
    fo("fanout_terse_remove_compile_fails_once", true),
];

fn v(s: &[&str]) -> Vec<String> {
    s.iter().map(|x| x.to_string()).collect()
}

fn sp_json(r: Result<quilltap_core::subprompts::Subprompt, SubpromptError>) -> Value {
    match r {
        Ok(s) => serde_json::to_value(s).unwrap(),
        Err(e) => error_json(&e),
    }
}

/// Run one case's op; returns the JSON result exactly as the oracle shapes it.
fn run_op(
    name: &str,
    main: &Connection,
    mount: &Connection,
    ids: &BTreeMap<String, String>,
    rec: &Recorder,
) -> Value {
    let id = |k: &str| ids.get(k).unwrap().as_str();
    let (a, b, cch, d, x) = (
        id("charA"),
        id("charB"),
        id("charC"),
        id("charD"),
        id("missing"),
    );
    let list = |cid: &str| -> Value {
        match list_character_subprompts(main, mount, cid) {
            Ok(l) => serde_json::to_value(l).unwrap(),
            Err(e) => error_json(&SubpromptError::Db(e)),
        }
    };
    let read = |cid: &str, sid: &str| -> Value {
        match read_character_subprompt(main, mount, cid, sid) {
            Ok(Some(s)) => serde_json::to_value(s).unwrap(),
            Ok(None) => Value::Null,
            Err(e) => error_json(&e),
        }
    };
    let resolve = |cid: &str, sel: &[&str]| -> Value {
        serde_json::to_value(resolve_selected_subprompts(main, mount, cid, &v(sel))).unwrap()
    };
    let create = |cid: &str, t: &str, ct: &str| -> Value {
        sp_json(create_character_subprompt(
            main,
            mount,
            cid,
            &SubpromptCreateInput {
                title: t.to_string(),
                content: ct.to_string(),
            },
        ))
    };
    let update = |cid: &str, sid: &str, t: Option<&str>, ct: Option<&str>| -> Value {
        sp_json(update_character_subprompt(
            main,
            mount,
            cid,
            sid,
            &SubpromptPatch {
                title: t.map(str::to_string),
                content: ct.map(str::to_string),
            },
        ))
    };
    let delete = |cid: &str, sid: &str| -> Value {
        match delete_character_subprompt(main, mount, cid, sid) {
            Ok(b) => json!(b),
            Err(e) => error_json(&e),
        }
    };
    let fanout = |cid: &str, sid: &str, remove: bool| -> Value {
        let r = fan_out_subprompt_change(
            main,
            mount,
            cid,
            sid,
            FanoutOptions {
                remove_selection: remove,
            },
            rec,
        );
        json!({ "chatsTouched": r.chats_touched, "seatsRecompiled": r.seats_recompiled })
    };
    let many = |xs: Vec<Value>| -> Value {
        // v4's `[await a, await b, …]` — a throw mid-way surfaces as the error
        // object for the WHOLE result; the `sp_json` arms already carry it.
        if let Some(err) = xs.iter().find(|x| x.get("error").is_some()) {
            return err.clone();
        }
        Value::Array(xs)
    };

    match name {
        "list_a" => list(a),
        "list_b_no_folder" => list(b),
        "list_c_no_vault" => list(cch),
        "list_d_archived_reads" => list(d),
        "list_missing_character" => list(x),
        "read_a_terse" => read(a, "terse"),
        "read_a_Verse_exact_case" => read(a, "Verse"),
        "read_a_verse_lower_case" => read(a, "verse"),
        "read_a_no_title" => read(a, "no-title"),
        "read_a_gone" => read(a, "gone"),
        "read_a_nested_invalid_id" => read(a, "drafts/x"),
        "read_a_broken_document" => read(a, "broken"),
        "read_a_notes_txt_not_a_subprompt" => read(a, "notes.txt"),
        "read_d_keep_archived_reads" => read(d, "keep"),
        "read_c_no_vault" => read(cch, "terse"),
        "read_missing_character" => read(x, "terse"),
        "resolve_a_terse_VERSE_gone" => resolve(a, &["terse", "VERSE", "gone"]),
        "resolve_a_listing_order_not_tick_order" => resolve(a, &["alpha", "zulu", "no-title"]),
        "resolve_a_empty" => resolve(a, &[]),
        "resolve_a_dupes_one_wanted" => resolve(a, &["terse", "TERSE"]),
        "resolve_a_broken_dropped" => resolve(a, &["broken"]),
        "resolve_c_no_vault" => resolve(cch, &["terse"]),
        "resolve_d_archived_reads" => resolve(d, &["keep"]),
        "resolve_missing_character" => resolve(x, &["terse"]),
        "create_a_collision_suffixes" => many(vec![
            create(a, "Be terse", "one"),
            create(a, "Be Terse!", "two"),
            create(a, "  be   terse ", "three"),
        ]),
        "create_a_trims_title_and_content" => {
            create(a, "  Padded  ", "\n\n  body with edges  \n\n")
        }
        "create_b_ensures_folder" => create(b, "First", "x"),
        "create_b_twice_ensures_folder_each_time" => {
            many(vec![create(b, "First", "x"), create(b, "Second", "y")])
        }
        "create_c_provisions_vault" => create(cch, "Fresh", "x"),
        "create_d_archived_409" => create(d, "Nope", "x"),
        "create_a_blank_title" => create(a, "   ", "x"),
        "create_a_blank_content" => create(a, "T", " \n "),
        "create_a_title_101_units" => create(a, &"x".repeat(101), "x"),
        "create_a_title_100_astral_is_200_units" => create(a, &"😀".repeat(100), "x"),
        "create_a_title_validated_before_content" => create(a, " ", " "),
        "create_missing_character" => create(x, "T", "x"),
        "update_a_title_only" => update(a, "terse", Some("Be brief"), None),
        "update_a_content_only" => update(a, "terse", None, Some("Two lines at most.")),
        "update_a_both" => update(a, "terse", Some("Brief"), Some("Short.")),
        "update_a_empty_patch_rewrites" => update(a, "terse", None, None),
        "update_a_no_title_file_keeps_id_title" => {
            update(a, "no-title", None, Some("Now with a body."))
        }
        "update_a_Verse_exact_case" => update(a, "Verse", Some("In verse"), None),
        "update_a_verse_lower_case" => update(a, "verse", Some("In verse"), None),
        "update_a_invalid_id" => update(a, "../x", Some("x"), None),
        "update_a_missing_id" => update(a, "ghost", Some("x"), None),
        "update_a_broken_document" => update(a, "broken", Some("x"), None),
        "update_a_blank_title" => update(a, "terse", Some("  "), None),
        "update_a_blank_content" => update(a, "terse", None, Some("")),
        "update_d_archived_409" => update(d, "keep", Some("x"), None),
        "update_c_no_vault_provisions_then_404" => update(cch, "terse", Some("x"), None),
        "update_missing_character" => update(x, "terse", Some("x"), None),
        "delete_a_terse" => delete(a, "terse"),
        "delete_a_broken_link_only" => delete(a, "broken"),
        "delete_a_gone" => delete(a, "gone"),
        "delete_a_invalid_id" => delete(a, "a/b"),
        "delete_d_archived_409" => delete(d, "keep"),
        "delete_d_invalid_id_before_archive_check" => delete(d, "a/b"),
        "delete_c_no_vault_provisions_then_false" => delete(cch, "terse"),
        "delete_missing_character" => delete(x, "terse"),
        "fanout_terse" => fanout(a, "terse", false),
        "fanout_verse_case_insensitive" => fanout(a, "verse", false),
        "fanout_terse_remove_selection" => fanout(a, "terse", true),
        "fanout_TERSE_remove_selection_case_insensitive" => fanout(a, "TERSE", true),
        "fanout_verse_remove_selection_strips_VERSE" => fanout(a, "verse", true),
        "fanout_gone" => fanout(a, "gone", false),
        "fanout_b_terse" => fanout(b, "terse", false),
        "fanout_missing_character" => fanout(x, "terse", false),
        "fanout_terse_compile_fails_once" => fanout(a, "terse", false),
        "fanout_terse_remove_compile_fails_once" => fanout(a, "terse", true),
        other => panic!("unknown case {other}"),
    }
}

#[test]
fn subprompts_storage_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SUBPROMPTS_STORAGE") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let main_fixture = fixtures_dir().join("subprompts-main.db");
    let mount_fixture = fixtures_dir().join("subprompts-mount.db");
    let meta: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("subprompts-main.db.meta.json")).unwrap(),
    )
    .unwrap();
    // vault id → key
    let vaults: HashMap<String, String> = meta["vaults"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (v.as_str().unwrap().to_string(), k.clone()))
        .collect();

    let oracle: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("oracle row");
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    assert!(!oracle.is_empty(), "empty oracle (the empty-file trap)");
    assert_eq!(
        oracle.len(),
        CASES.len(),
        "oracle case count != corpus (shape)"
    );

    let mut failed: Vec<String> = Vec::new();
    for case in CASES {
        let name = case.name;
        let want = oracle
            .get(name)
            .unwrap_or_else(|| panic!("oracle missing {name}"));
        let scratch =
            std::env::temp_dir().join(format!("qt-sp-rust-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let main_work = scratch.join("main.db");
        let mount_work = scratch.join("mount.db");
        std::fs::copy(&main_fixture, &main_work).unwrap();
        std::fs::copy(&mount_fixture, &mount_work).unwrap();
        // The op + the census run on WRITABLE connections to both partitions
        // (the `Writer::open_writable` idiom — the storage ops borrow the two
        // connections, exactly what the API handlers hand them inside one
        // `db.write` closure).
        let main_w =
            Writer::open_writable(&main_work, &spec.test_pepper_base64).expect("open main");
        let mount_w =
            Writer::open_writable(&mount_work, &spec.test_pepper_base64).expect("open mount");
        let main = main_w.connection();
        let mount = mount_w.connection();
        let rec = Recorder::new(case.fail_first);
        let result = run_op(name, main, mount, &spec.ids, &rec);
        let census_v = if case.census {
            census(main, mount, &spec.seed_timestamp, &vaults)
        } else {
            Value::Null
        };

        let got_result = sorted(&norm_result(result, &spec.seed_timestamp));
        let want_result = sorted(&want["result"]);
        if got_result != want_result {
            failed.push(format!(
                "{name} RESULT:\n{}",
                first_diff(&got_result, &want_result)
            ));
        }
        if case.census {
            let got_c = sorted(&census_v);
            let want_c = sorted(&want["census"]);
            if got_c != want_c {
                failed.push(format!("{name} CENSUS:\n{}", first_diff(&got_c, &want_c)));
            }
        }
        if case.recorded {
            let got_r = sorted(&rec.recorded());
            let want_r = sorted(&want["recorded"]);
            if got_r != want_r {
                failed.push(format!("{name} RECORDED:\n{}", first_diff(&got_r, &want_r)));
            }
        }
        eprintln!(
            "[{name}] {}",
            if failed.last().is_some_and(|f| f.starts_with(name)) {
                "MISMATCH"
            } else {
                "OK"
            }
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }
    assert!(
        failed.is_empty(),
        "{} case(s) differ:\n{}",
        failed.len(),
        failed.join("\n\n")
    );
    eprintln!(
        "OK: subprompts storage tier-2 matched oracle ({} cases).",
        CASES.len()
    );
}
