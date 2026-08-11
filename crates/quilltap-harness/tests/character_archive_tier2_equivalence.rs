//! P4.D65 — the character-archive tier-2 differential.
//!
//! Drives `quilltap_core::services::character_archive::service::{archive_character,
//! rehydrate_character}` through the SAME eight case sequences the oracle drives
//! v4's real `lib/characters/archive-service.ts` through, over a FRESH copy of
//! the committed `character-archive-{main,mount}.db` pair per case, and diffs
//! three comparands: the returned result (or the refusal's class + message), the
//! DECRYPTED bundle, and a whole-table dump of BOTH partitions.
//!
//! ⚠ **Ciphertext is never compared.** `encryptArchive` draws a fresh salt and
//! IV per bundle, so the persisted bytes differ on every run on both sides. What
//! is comparable — and what actually carries the meaning — is the PLAINTEXT
//! export each side gets back by decrypting its own artifact. The fixture
//! instance has no user passphrase, so both sides resolve `INTERNAL_PASSPHRASE`
//! through their own `resolveArchivePassphrase` analog.
//!
//! Normalization is deliberately narrow (the
//! `uuid-normalizer-blinds-fixture-baked-ids` trap): every id in this corpus is
//! FIXTURE-BAKED and byte-identical on both sides except the archive `files`
//! row's, so the normalizer first collects the baked id set from the untouched
//! copy and only tokenizes UUIDs *outside* it. Timestamps other than the seed
//! stamp collapse to `<ts>`.
//!
//! The committed fixture is built by
//! `harness/oracle/fixtures/build-character-archive-fixture.ts` and then EXTENDED
//! in place by `extend-character-archive-twice-linked-blob.ts` (v4 Bug 57 /
//! `de9f70bf` — the sha-deduped blob under two links). Both carry their own
//! recipes; extend by mutation, never rebuild.
//!
//! Generate the oracle (Node 24, from the v4 checkout — jest ignores `.claude/`
//! venues, so the case + spec are copied to a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-character-archive-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/character-archive-tier2.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/character-archive.json"       "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_ARCHIVE_MAIN=$V5W/crates/quilltap-web/tests/fixtures/character-archive-main.db \
//!   QT_FIXTURE_ARCHIVE_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/character-archive-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-character-archive.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- character-archive-tier2
//!
//! Run:
//!   QT_ORACLE_CHARACTER_ARCHIVE=/tmp/oracle-character-archive.ndjson \
//!     cargo test -p quilltap-harness --test character_archive_tier2_equivalence -- --nocapture
//!
//! Skips (does not fail) when the env var is unset — the standing gated-
//! differential discipline.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::character_archive::crypto::{
    decrypt_archive, is_encrypted_archive, resolve_archive_passphrase, PassphraseSource,
};
use quilltap_core::services::character_archive::service::{
    archive_character, rehydrate_character, ArchiveError, ArchiveSeams,
};
use quilltap_core::services::file_storage::StorageBackend;
use quilltap_host::files_store::LocalStorageBackend;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    seed_timestamp: String,
    sable: String,
    tor: String,
}

const MISSING_CHARACTER: &str = "00000000-0000-4000-8000-00000000dead";

const MAIN_TABLES: [&str; 8] = [
    "characters",
    "chats",
    "memories",
    "files",
    "embedding_status",
    "vector_indices",
    "vector_entries",
    // §3 review (archive-round-2): the re-embed half of rehydrate and the
    // import's one-EMBEDDING_GENERATE-per-restored-memory both write here —
    // without this table a port that never enqueued a job would pass.
    "background_jobs",
];
const MOUNT_TABLES: [&str; 7] = [
    "doc_mount_points",
    "doc_mount_files",
    "doc_mount_documents",
    "doc_mount_folders",
    "doc_mount_file_links",
    "doc_mount_chunks",
    "doc_mount_blobs",
];

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/character-archive.json")
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

// ---------------------------------------------------------------------------
// Dump + canonicalization
// ---------------------------------------------------------------------------

/// Every column of `table`, marshalled generically. BLOBs collapse to
/// `blob:<len>` exactly as the oracle's `canonicalizeRows` does (the bytes
/// themselves are proven by the sha256 columns beside them).
fn dump_table(conn: &rusqlite::Connection, table: &str) -> Vec<Value> {
    let sql = format!("SELECT * FROM \"{table}\"");
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt
        .query_map([], |row| {
            let mut m = Map::new();
            for (i, name) in names.iter().enumerate() {
                let v = match row.get_ref(i) {
                    Ok(rusqlite::types::ValueRef::Null) => Value::Null,
                    Ok(rusqlite::types::ValueRef::Integer(n)) => Value::from(n),
                    Ok(rusqlite::types::ValueRef::Real(f)) => {
                        // SQLite REAL columns holding integral values come back
                        // as `1.0` here and as `1` from better-sqlite3; collapse.
                        if f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                            Value::from(f as i64)
                        } else {
                            Value::from(f)
                        }
                    }
                    Ok(rusqlite::types::ValueRef::Text(t)) => {
                        Value::String(String::from_utf8_lossy(t).to_string())
                    }
                    Ok(rusqlite::types::ValueRef::Blob(b)) => {
                        Value::String(format!("blob:{}", b.len()))
                    }
                    Err(_) => Value::Null,
                };
                m.insert(name.clone(), v);
            }
            Ok(Value::Object(m))
        })
        .and_then(|it| it.collect::<Result<Vec<_>, _>>());
    let mut rows = rows.unwrap_or_default();
    // The oracle sorts each table's rows by their canonical JSON; the key sort
    // is implicit on both sides (serde_json's `preserve_order` is ON, so the
    // Rust map keeps SELECT order — hence the explicit key sort below).
    for r in &mut rows {
        *r = sort_keys(r);
    }
    rows.sort_by_key(|r| r.to_string());
    rows
}

fn sort_keys(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut out = Map::new();
            for k in m.keys().collect::<BTreeSet<_>>() {
                out.insert(k.clone(), sort_keys(&m[k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(sort_keys).collect()),
        other => other.clone(),
    }
}

fn dump_state(db: &Db) -> Value {
    let mut state = Map::new();
    for t in MAIN_TABLES {
        let rows = db.read_main(|c| Ok(dump_table(c, t))).expect("read main");
        state.insert(t.to_string(), Value::Array(rows));
    }
    for t in MOUNT_TABLES {
        let rows = db
            .read_mount_index(|c| Ok(dump_table(c, t)))
            .expect("read mount");
        state.insert(t.to_string(), Value::Array(rows));
    }
    Value::Object(state)
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b[8] == b'-'
        && b[13] == b'-'
        && b[18] == b'-'
        && b[23] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| matches!(i, 8 | 13 | 18 | 23) || c.is_ascii_hexdigit())
}

/// Collect every UUID that appears anywhere in a value — the "baked" set when
/// run over the untouched fixture copy.
fn collect_uuids(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::String(s) => {
            for tok in s.split(|c: char| !(c.is_ascii_hexdigit() || c == '-')) {
                if is_uuid(tok) {
                    out.insert(tok.to_string());
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_uuids(x, out)),
        Value::Object(m) => m.values().for_each(|x| collect_uuids(x, out)),
        _ => {}
    }
}

struct Normalizer {
    baked: BTreeSet<String>,
    seed_ts: String,
    minted: HashMap<String, String>,
}

impl Normalizer {
    fn token(&mut self, uuid: &str) -> String {
        let next = self.minted.len();
        self.minted
            .entry(uuid.to_string())
            .or_insert_with(|| format!("<minted-{next}>"))
            .clone()
    }

    /// Replace minted UUIDs (substring-wise, so `"<uuid>/character-archive.qtap"`
    /// storage keys and filenames normalize too) and non-seed ISO timestamps.
    fn string(&mut self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let bytes = s.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 36 <= bytes.len() {
                if let Some(cand) = s.get(i..i + 36) {
                    if is_uuid(cand) {
                        if self.baked.contains(cand) {
                            out.push_str(cand);
                        } else {
                            let t = self.token(cand);
                            out.push_str(&t);
                        }
                        i += 36;
                        continue;
                    }
                }
            }
            // ISO-8601 `YYYY-MM-DDTHH:MM:SS.mmmZ` (24 chars). The seed stamp is
            // fixture-baked and stays literal; anything else was minted at op
            // time and can only ever differ.
            if i + 24 <= bytes.len() {
                if let Some(cand) = s.get(i..i + 24) {
                    if looks_iso(cand) {
                        if cand == self.seed_ts {
                            out.push_str(cand);
                        } else {
                            out.push_str("<ts>");
                        }
                        i += 24;
                        continue;
                    }
                }
            }
            // The archive filename's `:`/`.`-scrubbed stamp
            // (`2026-03-01T00-00-00-000Z-character-archive.qtap`).
            if i + 24 <= bytes.len() {
                if let Some(cand) = s.get(i..i + 24) {
                    if looks_scrubbed_iso(cand) {
                        out.push_str("<ts>");
                        i += 24;
                        continue;
                    }
                }
            }
            let ch = s[i..].chars().next().expect("char boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }

    fn value(&mut self, v: &Value) -> Value {
        match v {
            Value::String(s) => Value::String(self.string(s)),
            Value::Array(a) => Value::Array(a.iter().map(|x| self.value(x)).collect()),
            Value::Object(m) => {
                let mut out = Map::new();
                for (k, val) in m {
                    out.insert(k.clone(), self.value(val));
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
    }
}

fn looks_iso(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 24
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'.'
        && b[23] == b'Z'
}

/// The `:`/`.`-scrubbed variant `createArchiveFileRecord` bakes into the
/// bundle's `originalFilename`.
fn looks_scrubbed_iso(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 24
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b'-'
        && b[16] == b'-'
        && b[19] == b'-'
        && b[23] == b'Z'
}

// ---------------------------------------------------------------------------
// The case runner
// ---------------------------------------------------------------------------

struct Work {
    _dir: tempfile::TempDir,
    db: Db,
    backend: Arc<dyn StorageBackend>,
}

fn open_work(spec: &Spec) -> Work {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.db");
    let mount = dir.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("character-archive-main.db"), &main).expect("copy main");
    std::fs::copy(fixtures_dir().join("character-archive-mount.db"), &mount).expect("copy mount");
    std::fs::create_dir_all(dir.path().join("files")).expect("files dir");
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    let backend: Arc<dyn StorageBackend> =
        Arc::new(LocalStorageBackend::new(dir.path().join("files")));
    Work {
        _dir: dir,
        db,
        backend,
    }
}

fn seams<'a>(work: &Work, app_version: &str) -> ArchiveSeams<'a> {
    ArchiveSeams {
        backend: Some(Arc::clone(&work.backend)),
        // The fixture instance has no user passphrase, so this resolves to the
        // internal sentinel — the same value v4's `resolveArchivePassphrase`
        // lands on there.
        passphrase: PassphraseSource {
            cached: None,
            has_user_passphrase: false,
        },
        // v4 bakes `packageJson.version` into the manifest, so it is part of the
        // bundle's BYTE LENGTH. Inject the oracle's own value and `files.size`
        // becomes a real comparand instead of a version artifact.
        app_version: app_version.to_string(),
        codec: None,
        extractor: quilltap_core::services::mount_index::converters::default_text_extractor(),
    }
}

/// v4's thrown-error shape: `{name, message}` off the Error subclass.
fn error_value(e: &ArchiveError) -> Value {
    use quilltap_core::services::character_archive::crypto::ArchiveCryptoError as C;
    let name = match e {
        ArchiveError::CharacterNotFound(_) | ArchiveError::NotArchived(_) => "Error",
        ArchiveError::Verification(_) => "ArchiveVerificationError",
        ArchiveError::Rehydration { .. } => "CharacterRehydrationError",
        ArchiveError::Crypto(C::PassphraseMismatch { .. }) => "ArchivePassphraseMismatchError",
        ArchiveError::Crypto(C::Integrity { .. }) => "ArchiveIntegrityError",
        ArchiveError::Crypto(C::Format { .. }) => "ArchiveFormatError",
        ArchiveError::Crypto(C::KeyUnavailable) => "ArchiveKeyUnavailableError",
        ArchiveError::Db(_) | ArchiveError::Storage(_) => "Error",
    };
    json!({"error": {"name": name, "message": e.to_string()}})
}

/// Read back every ARCHIVE bundle, DECRYPT it, and parse — the oracle's
/// `bundles` comparand.
fn main_bundles(db: &Db, backend: &dyn StorageBackend, user_id: &str) -> Vec<Value> {
    use quilltap_core::db::files::FilesRepository;
    let uid = user_id.to_string();
    let rows = db
        .read_main(move |c| FilesRepository::new(c).find_by_category(&uid, "ARCHIVE"))
        .expect("read archive files");
    let mut out = Vec::new();
    let mut sorted: Vec<_> = rows;
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    for file in sorted {
        let entry = quilltap_core::db::files::FileEntry {
            id: file.id.clone(),
            sha256: file.sha256.clone(),
            original_filename: file.original_filename.clone(),
            mime_type: file.mime_type.clone(),
            size: file.size,
            width: file.width,
            height: file.height,
            category: file.category.clone(),
            generation_prompt: None,
            generation_model: None,
            generation_revised_prompt: None,
            description: file.description.clone(),
            storage_key: file.storage_key.clone(),
        };
        let Ok(raw) = quilltap_core::services::file_storage::download_file(db, backend, &entry)
        else {
            continue;
        };
        let plaintext = if is_encrypted_archive(&raw) {
            let pass = resolve_archive_passphrase(PassphraseSource {
                cached: None,
                has_user_passphrase: false,
            })
            .expect("resolve passphrase");
            decrypt_archive(&raw, &pass).expect("decrypt bundle")
        } else {
            raw
        };
        let records =
            quilltap_core::services::quilltap_import::ndjson::read_ndjson_lines(&plaintext)
                .expect("read bundle ndjson");
        let parsed =
            quilltap_core::services::quilltap_import::ndjson::assemble_export_from_stream(&records)
                .expect("assemble bundle");
        out.push(json!({"manifest": parsed.manifest, "data": parsed.data}));
    }
    out
}

/// A dispatch `Response` in the `{status, body}` shape v4's route answers with.
///
/// The rider keys (`code` / `characterId`) live flat on v5's `CoreError` where
/// v4 nests them under `details` — the P4.6ah `FILE_HAS_ASSOCIATIONS`
/// precedent — so the comparison flattens v4's side rather than nesting v5's
/// (see `flatten_details`).
fn route_value(resp: &quilltap_core::api::Response) -> Value {
    use quilltap_core::api::{ErrorKind, Response as R};
    match resp {
        R::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked | ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            let mut body = Map::new();
            body.insert("error".into(), json!(e.message));
            if let Some(code) = &e.code {
                body.insert("code".into(), json!(code));
            }
            if let Some(cid) = &e.character_id {
                body.insert("characterId".into(), json!(cid));
            }
            json!({"status": status, "body": Value::Object(body)})
        }
        R::Files(v) => json!({"status": 200, "body": v}),
        other => json!({
            "status": 200,
            "body": serde_json::to_value(other).unwrap_or(Value::Null),
        }),
    }
}

/// Lift v4's `details` bag onto the error body, so the two shapes line up.
fn flatten_details(v: &Value) -> Value {
    let mut out = v.clone();
    let Some(body) = out.get_mut("body").and_then(Value::as_object_mut) else {
        return out;
    };
    let Some(details) = body.remove("details") else {
        return out;
    };
    if let Some(map) = details.as_object() {
        for (k, val) in map {
            body.insert(k.clone(), val.clone());
        }
    }
    out
}

/// Every `character` record the `.qtap` writer emits for a `scope: 'all'`
/// characters export — the comparand for the three-key archive carry.
fn export_character_records(db: &Db, user_id: &str, app_version: &str) -> Value {
    use quilltap_core::services::qtap_export::{self, ExportOptions};
    let opts = ExportOptions {
        entity_type: "characters".to_string(),
        scope: "all".to_string(),
        selected_ids: Vec::new(),
        include_memories: false,
    };
    let records = db
        .read_main(|main| {
            db.read_mount_index(|mount| {
                Ok(qtap_export::stream_export_records(
                    main,
                    mount,
                    None,
                    user_id,
                    &opts,
                    false,
                    // The manifest's clock never reaches a `character` record;
                    // the envelope is byte-proven by the export family.
                    "2026-03-01T00:00:00.000Z",
                    app_version,
                ))
            })
        })
        .expect("read")
        .expect("stream export records");
    Value::Array(
        records
            .into_iter()
            .filter(|r| r.get("kind").and_then(Value::as_str) == Some("character"))
            .filter_map(|r| r.get("data").cloned())
            .collect(),
    )
}

#[derive(Deserialize)]
struct OracleCase {
    name: String,
    #[serde(default, rename = "appVersion")]
    app_version: String,
    #[serde(default)]
    result: Value,
    #[serde(default)]
    bundles: Value,
    #[serde(default)]
    state: Value,
}

#[tokio::test(flavor = "multi_thread")]
async fn character_archive_tier2_equivalence() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHARACTER_ARCHIVE") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec")).expect("spec");
    let oracle: Vec<OracleCase> = std::fs::read_to_string(&oracle_path)
        .expect("oracle ndjson")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert_eq!(oracle.len(), 13, "the corpus is thirteen cases");

    // The baked id set: every UUID present in the untouched fixture. Anything
    // outside it in a post-op dump was MINTED by the operation.
    let baked = {
        let work = open_work(&spec);
        let mut set = BTreeSet::new();
        collect_uuids(&dump_state(&work.db), &mut set);
        assert_fixture_carries_twice_linked_blobs(&work.db);
        set
    };

    let mut mismatches: Vec<String> = Vec::new();

    for case in &oracle {
        let work = open_work(&spec);
        let s = seams(&work, &case.app_version);
        let db = &work.db;

        let result: Value = match case.name.as_str() {
            "archive_sable" => match archive_character(db, &spec.user_id, &spec.sable, &s).await {
                Ok(r) => r.to_value(),
                Err(e) => error_value(&e),
            },
            "archive_sable_twice" => {
                archive_character(db, &spec.user_id, &spec.sable, &s)
                    .await
                    .expect("first archive");
                match archive_character(db, &spec.user_id, &spec.sable, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            "archive_then_rehydrate" => {
                archive_character(db, &spec.user_id, &spec.sable, &s)
                    .await
                    .expect("archive");
                match rehydrate_character(db, &spec.user_id, &spec.sable, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            "archive_tor_no_vault" => {
                match archive_character(db, &spec.user_id, &spec.tor, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            "rehydrate_not_archived" => {
                match rehydrate_character(db, &spec.user_id, &spec.sable, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            "archive_missing_character" => {
                match archive_character(db, &spec.user_id, MISSING_CHARACTER, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            "rehydrate_no_bundle" => {
                let sable = spec.sable.clone();
                db.write(move |ws| {
                    ws.main().connection().execute(
                        "UPDATE characters SET archivedAt = ?1 WHERE id = ?2",
                        rusqlite::params!["2026-03-02T00:00:00.000Z", sable],
                    )?;
                    Ok(())
                })
                .await
                .expect("flag tombstone");
                match rehydrate_character(db, &spec.user_id, &spec.sable, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            // ── The files-delete ARCHIVE_BUNDLE_HELD guard ──
            // v4 nests the rider under `details`; v5 carries `code` +
            // `characterId` flat on the `CoreError` (the P4.6ah
            // FILE_HAS_ASSOCIATIONS precedent) and the transport renders them
            // into the body, so the CONTENT is what is compared.
            name @ ("files_delete_bundle_held"
            | "files_delete_bundle_force"
            | "files_delete_bundle_unheld") => {
                let archived = archive_character(db, &spec.user_id, &spec.sable, &s)
                    .await
                    .expect("archive");
                let file_id = archived.archive_file_id.clone().unwrap_or_default();
                if name == "files_delete_bundle_unheld" {
                    rehydrate_character(db, &spec.user_id, &spec.sable, &s)
                        .await
                        .expect("rehydrate");
                }
                let force = name == "files_delete_bundle_force";
                let resp = quilltap_core::api::files::file_delete(
                    db,
                    &spec.user_id,
                    &file_id,
                    force,
                    false,
                    Some(work.backend.as_ref()),
                )
                .await;
                route_value(&resp)
            }
            // ── The export picker's archived filter ──
            "export_entities_after_archive" => {
                archive_character(db, &spec.user_id, &spec.sable, &s)
                    .await
                    .expect("archive");
                let body = db
                    .read_main(|main| {
                        db.read_mount_index(|mount| {
                            Ok(quilltap_core::services::qtap_export::export_entities(
                                main,
                                mount,
                                &spec.user_id,
                                "characters",
                            ))
                        })
                    })
                    .expect("read")
                    .expect("export entities");
                json!({"status": 200, "body": body})
            }
            // ── The three-key export carry, NON-NULL leg ──
            "export_all_after_archive" => {
                archive_character(db, &spec.user_id, &spec.sable, &s)
                    .await
                    .expect("archive");
                let characters = export_character_records(db, &spec.user_id, &case.app_version);
                json!({ "characters": characters })
            }
            "rehydrate_missing_bundle_file" => {
                let archived = archive_character(db, &spec.user_id, &spec.sable, &s)
                    .await
                    .expect("archive");
                if let Some(fid) = archived.archive_file_id {
                    db.write(move |ws| {
                        quilltap_core::db::files::FilesRepository::new(ws.main().connection())
                            .delete(&fid)
                    })
                    .await
                    .expect("delete bundle row");
                }
                match rehydrate_character(db, &spec.user_id, &spec.sable, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            other => panic!("unknown oracle case: {other}"),
        };

        let bundles = Value::Array(main_bundles(db, work.backend.as_ref(), &spec.user_id));
        let state = dump_state(db);

        let mut norm = Normalizer {
            baked: baked.clone(),
            seed_ts: spec.seed_timestamp.clone(),
            minted: HashMap::new(),
        };
        let got = norm.value(&json!({"result": result, "bundles": bundles, "state": state}));

        let mut norm_want = Normalizer {
            baked: baked.clone(),
            seed_ts: spec.seed_timestamp.clone(),
            minted: HashMap::new(),
        };
        let mut want = norm_want.value(&json!({
            // v4 nests an error's rider keys under `details`; v5 carries them
            // flat (the P4.6ah precedent), so the two shapes are reconciled on
            // v4's side before the comparison.
            "result": flatten_details(&case.result),
            "bundles": case.bundles,
            "state": case.state,
        }));
        let mut got = got;
        for side in [&mut got, &mut want] {
            blind_archive_sha(&mut side["state"]);
            resort_state(&mut side["state"]);
        }

        for section in ["result", "bundles"] {
            if got[section] != want[section] {
                mismatches.push(format!(
                    "case {} section {section}:\n  got  {}\n  want {}",
                    case.name,
                    truncate(&got[section]),
                    truncate(&want[section]),
                ));
            }
        }
        // The state diff is reported PER TABLE, and per differing row inside it:
        // a whole-partition dump printed side by side is unreadable and hides
        // which of the fifteen tables actually moved.
        for table in MAIN_TABLES.iter().chain(MOUNT_TABLES.iter()) {
            let g = got["state"].get(*table).cloned().unwrap_or(Value::Null);
            let w = want["state"].get(*table).cloned().unwrap_or(Value::Null);
            if g == w {
                continue;
            }
            let ga = g.as_array().cloned().unwrap_or_default();
            let wa = w.as_array().cloned().unwrap_or_default();
            let only_got: Vec<&Value> = ga.iter().filter(|r| !wa.contains(r)).collect();
            let only_want: Vec<&Value> = wa.iter().filter(|r| !ga.contains(r)).collect();
            mismatches.push(format!(
                "case {} table {table} ({} rows got / {} want)\n  only in v5:  {}\n  only in v4:  {}",
                case.name,
                ga.len(),
                wa.len(),
                truncate(&Value::Array(only_got.into_iter().cloned().collect())),
                truncate(&Value::Array(only_want.into_iter().cloned().collect())),
            ));
        }
        println!(
            "case {:<30} result={} bundles={}",
            case.name,
            if got["result"] == want["result"] {
                "ok"
            } else {
                "DIFF"
            },
            if got["bundles"] == want["bundles"] {
                "ok"
            } else {
                "DIFF"
            },
        );
    }

    assert!(
        mismatches.is_empty(),
        "character-archive divergences:\n{}",
        mismatches.join("\n\n")
    );
}

/// The twice-linked blob shape (v4 Bug 57 / `de9f70bf`) lives in the FIXTURE,
/// not in a case gesture: `archive_then_rehydrate` is the arm that exercises it,
/// and it does so only for as long as Sable's vault actually holds one blob
/// under two links. That makes it exactly the kind of coverage a later fixture
/// edit can vacate in silence — the case would keep passing while proving
/// nothing — so the shape is asserted rather than assumed
/// (`harness-corpus-shape-constants-rot`: assert the shape, not a hand count).
///
/// TWO shapes are required, because what happens after the preflight's dedupe
/// differs: portrait's content row survives the prune (one of its links is
/// Sable's `defaultImageId`), so rehydrate meets an ALREADY-PRESENT blob id and
/// takes the skip-if-present leg; landscape's links are both doomed, the orphan
/// GC takes its content row, and rehydrate restores it fresh.
fn assert_fixture_carries_twice_linked_blobs(db: &Db) {
    let multi: Vec<i64> = db
        .read_mount_index(|c| {
            let mut stmt = c.prepare(
                "SELECT COUNT(*) AS links FROM doc_mount_file_links l \
                 JOIN doc_mount_blobs b ON b.fileId = l.fileId \
                 GROUP BY l.fileId HAVING links > 1",
            )?;
            let rows = stmt
                .query_map([], |r| r.get::<_, i64>("links"))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read mount fixture");
    assert_eq!(
        multi.len(),
        2,
        "the fixture must carry exactly two sha-deduped blobs linked twice each \
         (the Bug-57 shape); found {multi:?} — re-run \
         harness/oracle/fixtures/extend-character-archive-twice-linked-blob.ts"
    );
    assert!(
        multi.iter().all(|&n| n == 2),
        "each twice-linked blob carries exactly two links; found {multi:?}"
    );
}

/// The manifest's `createdAt` is minted at bundle time, so it is normalized
/// inside the bundle comparand (the generic ISO rule already does that). This
/// helper only exists for the ARCHIVE `files` row's **sha256**, which digests a
/// plaintext carrying that same minted stamp and therefore can never agree
/// across two runs — let alone two languages. Its LENGTH still can, which is
/// why `size` is left as a real comparand and `appVersion` is injected rather
/// than stripped.
fn blind_archive_sha(state: &mut Value) {
    let Some(rows) = state.get_mut("files").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        if row.get("category").and_then(Value::as_str) == Some("ARCHIVE") {
            if let Some(m) = row.as_object_mut() {
                m.insert(
                    "sha256".into(),
                    Value::String("<minted-plaintext-sha>".into()),
                );
            }
        }
    }
}

/// Re-sort every table's rows by the RUST serialization, on both sides: the
/// oracle sorts by `JSON.stringify`, which orders escapes and number forms
/// differently, so a set-equal table can arrive in a different order.
fn resort_state(state: &mut Value) {
    let Some(map) = state.as_object_mut() else {
        return;
    };
    for (_, v) in map.iter_mut() {
        if let Some(rows) = v.as_array_mut() {
            rows.sort_by_key(|r| r.to_string());
        }
    }
}

fn truncate(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 1800 {
        format!("{}… ({} bytes)", &s[..1800], s.len())
    } else {
        s
    }
}
