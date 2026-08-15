//! P4.D65 — the character-archive tier-2 differential.
//!
//! Drives `quilltap_core::services::character_archive::service::{archive_character,
//! rehydrate_character}` through the SAME twenty case sequences the oracle drives
//! v4's real `lib/characters/archive-service.ts` through, over a FRESH copy of
//! the committed `character-archive-{main,mount}.db` pair per case, and diffs
//! four comparands: the returned result (or the refusal's class + message), the
//! DECRYPTED bundle, `digestProbe`, and a whole-table dump of BOTH partitions.
//!
//! **P4.D80** (v4 `aa464abf`) added three cases and the `digestProbe` section:
//! `character_detail_enrichment` drives `chat_enrichment::get_character_detail`
//! (bug 66 — the chat-GET projection that carries `archivedAt`), and
//! `rehydrate_digest_{clobbered,corrupt}` plant a damaged `files.sha256` on a
//! REAL bundle to reach bug 69's self-heal and the refusal it must still make.
//! `digestProbe` classifies what each ARCHIVE row's recorded digest actually
//! digests (`plaintext` / `stored` / `other`) — the digest itself is minted and
//! blinded, so the CLASS is what proves the self-heal REPAIRED the row rather
//! than merely tolerating the mismatch.
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
//! in place, twice: by `extend-character-archive-twice-linked-blob.ts` (v4 Bug
//! 57 / `de9f70bf` — the sha-deduped blob under two links) and by
//! `extend-character-archive-profile-and-avatars.ts` (the default embedding
//! profile, the avatar-override face, the standalone avatar thumbnail — the §3
//! review's corpus-undriven shapes, order item 6). Each carries its own recipe;
//! extend by mutation, never rebuild, and run them in that order.
//!
//! ⚠ Regenerating this family's oracle at the P4.D65-remainder round ALSO
//! un-mocked a stale jest.setup stub (`getDefaultEmbeddingProfile` → `null`),
//! which had been starving v4's import of a profile the database really held.
//! The un-mock lives in the oracle case beside the seam it neutralizes.
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
    chat_seated: String,
    chat_removed: String,
}

const MISSING_CHARACTER: &str = "00000000-0000-4000-8000-00000000dead";

/// The digest a `rehydrate_digest_corrupt` row is planted with — 64 hex
/// characters that are neither the plaintext digest nor the ciphertext one, so
/// bug 69's self-heal must NOT engage. Byte-identical to the oracle case's
/// constant.
const CORRUPT_DIGEST: &str = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

const MAIN_TABLES: [&str; 9] = [
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
    // P4.D65-remainder: the fixture now carries a DEFAULT embedding profile (it
    // is what makes `background_jobs` a positive comparand rather than a
    // tripwire), and a bundle import is one of the few paths that can mint
    // profiles — so the table is dumped to pin that this one never does.
    "embedding_profiles",
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

/// A UUID neither baked into the fixture nor tokenized YET, as it appears in a
/// probe (see [`normalize_state`]).
const UNKNOWN_UUID: &str = "<unknown>";

impl Normalizer {
    fn token(&mut self, uuid: &str) -> String {
        let next = self.minted.len();
        self.minted
            .entry(uuid.to_string())
            .or_insert_with(|| format!("<minted-{next}>"))
            .clone()
    }

    /// Normalize WITHOUT minting: a UUID already tokenized resolves to its
    /// token, one that is not collapses to [`UNKNOWN_UUID`]. Used only as a
    /// sort key, never as a comparand.
    fn probe(&self, v: &Value) -> Value {
        match v {
            Value::String(s) => {
                let mut out = String::with_capacity(s.len());
                let mut i = 0usize;
                while i < s.len() {
                    if let Some(cand) = s.get(i..i + 36.min(s.len() - i)) {
                        if cand.len() == 36 && is_uuid(cand) {
                            if self.baked.contains(cand) {
                                out.push_str(cand);
                            } else {
                                out.push_str(self.minted.get(cand).map_or(UNKNOWN_UUID, |t| t));
                            }
                            i += 36;
                            continue;
                        }
                    }
                    if let Some(cand) = s.get(i..i + 24.min(s.len() - i)) {
                        if cand.len() == 24 && (looks_iso(cand) || looks_scrubbed_iso(cand)) {
                            if cand == self.seed_ts {
                                out.push_str(cand);
                            } else {
                                out.push_str("<ts>");
                            }
                            i += 24;
                            continue;
                        }
                    }
                    let ch = s[i..].chars().next().expect("char boundary");
                    out.push(ch);
                    i += ch.len_utf8();
                }
                Value::String(out)
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| self.probe(x)).collect()),
            Value::Object(m) => {
                let mut out = Map::new();
                for (k, val) in m {
                    out.insert(k.clone(), self.probe(val));
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
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

/// Normalize a whole state dump under ONE canonical traversal.
///
/// The `<minted-N>` labels are assigned in visit order, so the visit order has
/// to be a function of the CONTENT rather than of the minted bytes — otherwise
/// two rows that differ only by a minted id can swap labels between the sides
/// and a set-identical table reports as a divergence. Two rules make it one:
///
///  1. **Rows are visited in the order of their PROBE** — the row normalized
///     against what is already known, with still-unseen ids collapsed. A row is
///     therefore ordered by what it says, not by the id it happens to carry.
///  2. **The mount partition is walked FIRST.** Its rows are
///     content-distinguishable (a chunk carries its text), while the main
///     partition's `background_jobs` rows are not: two `MOUNT_CHUNK` embedding
///     jobs differ ONLY in the chunk id they point at. Walking the chunks first
///     means those ids are already tokenized when the jobs are read, which is
///     exactly what tells the two job rows apart.
///
/// Ties that survive both rules would be rows indistinguishable in every
/// comparand, where the label assignment cannot change the verdict.
fn normalize_state(norm: &mut Normalizer, state: &Value) -> Value {
    let mut out = Map::new();
    for table in MOUNT_TABLES.iter().chain(MAIN_TABLES.iter()) {
        let rows = state
            .get(*table)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut ordered: Vec<(String, Value)> = rows
            .into_iter()
            .map(|r| (norm.probe(&r).to_string(), r))
            .collect();
        ordered.sort_by(|a, b| a.0.cmp(&b.0));
        let normalized: Vec<Value> = ordered.into_iter().map(|(_, r)| norm.value(&r)).collect();
        out.insert((*table).to_string(), Value::Array(normalized));
    }
    Value::Object(out)
}

/// One side's three comparands under one shared token map.
///
/// STATE comes first, deliberately: it is the only section with a canonical
/// visit order (see [`normalize_state`]), so letting it seed the map means the
/// `result`'s and the bundle's minted ids resolve to the labels the rows they
/// refer to already carry — the archive `files` row's id and the
/// `archiveFileId` in the result become the same token, which is itself part of
/// what the case proves.
fn normalize_sides(
    baked: &BTreeSet<String>,
    seed_ts: &str,
    state: &Value,
    result: &Value,
    bundles: &Value,
    digest_probe: &Value,
) -> Value {
    let mut norm = Normalizer {
        baked: baked.clone(),
        seed_ts: seed_ts.to_string(),
        minted: HashMap::new(),
    };
    let state = normalize_state(&mut norm, state);
    let result = norm.value(result);
    let bundles = norm.value(bundles);
    let digest_probe = norm.value(digest_probe);
    json!({
        "result": result,
        "bundles": bundles,
        "digestProbe": digest_probe,
        "state": state,
    })
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

/// The fixture instance's real passphrase state: no user passphrase, nothing
/// cached, so `resolve_archive_passphrase` lands on `INTERNAL_PASSPHRASE` — the
/// same value v4's own resolver reaches there. Every case but the two
/// passphrase arms runs on it.
fn ambient_passphrase<'a>() -> PassphraseSource<'a> {
    PassphraseSource {
        cached: None,
        has_user_passphrase: false,
    }
}

/// The two passphrases the mismatch arm archives and rehydrates under —
/// byte-identical to the oracle case's constants, because the bundle sealed by
/// one side is only ever read back by the same side.
const OLD_PASSPHRASE: &str = "the-passphrase-that-was-in-effect";
const NEW_PASSPHRASE: &str = "the-passphrase-in-effect-now";

fn seams<'a>(work: &Work, app_version: &str, passphrase: PassphraseSource<'a>) -> ArchiveSeams<'a> {
    ArchiveSeams {
        backend: Some(Arc::clone(&work.backend)),
        passphrase,
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
///
/// `seal` is `Some` only where the sealing passphrase differs from the current
/// one (the mismatch arm) — the comparand is what the bundle SAYS, and reading
/// it back under a passphrase that cannot open it would only re-prove the
/// refusal the `result` comparand already carries.
///
/// Returns the bundles, the per-row digest CLASS (`digestProbe` — see the
/// oracle header; it is what proves bug 69's self-heal repaired the row rather
/// than merely tolerating the mismatch), and every bundle's plaintext digest so
/// the caller can blind it out of `result`.
fn main_bundles(
    db: &Db,
    backend: &dyn StorageBackend,
    user_id: &str,
    seal: Option<&str>,
) -> (Vec<Value>, Vec<Value>, Vec<String>) {
    use quilltap_core::db::files::FilesRepository;
    let uid = user_id.to_string();
    let rows = db
        .read_main(move |c| FilesRepository::new(c).find_by_category(&uid, "ARCHIVE"))
        .expect("read archive files");
    let mut out = Vec::new();
    let mut probes = Vec::new();
    let mut plaintext_digests = Vec::new();
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
            let pass = match seal {
                Some(p) => p.to_string(),
                None => {
                    resolve_archive_passphrase(ambient_passphrase()).expect("resolve passphrase")
                }
            };
            std::borrow::Cow::Owned(decrypt_archive(&raw, &pass).expect("decrypt bundle"))
        } else {
            std::borrow::Cow::Borrowed(&raw[..])
        };
        let records =
            quilltap_core::services::quilltap_import::ndjson::read_ndjson_lines(&plaintext)
                .expect("read bundle ndjson");
        let parsed =
            quilltap_core::services::quilltap_import::ndjson::assemble_export_from_stream(&records)
                .expect("assemble bundle");
        out.push(json!({"manifest": parsed.manifest, "data": parsed.data}));

        let plaintext_digest = sha256_hex(&plaintext);
        let stored_digest = sha256_hex(&raw);
        probes.push(json!({
            "fileId": file.id,
            "digests": if file.sha256 == plaintext_digest {
                "plaintext"
            } else if file.sha256 == stored_digest {
                "stored"
            } else {
                "other"
            },
        }));
        plaintext_digests.push(plaintext_digest);
    }
    (out, probes, plaintext_digests)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
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
    /// P4.D80. Absent in a STALE oracle, which is exactly what makes a stale
    /// NDJSON loud here rather than silent (`oracle-regen-silent-stale-pass`):
    /// every case with an ARCHIVE row would report `[]` against a populated v5
    /// side.
    #[serde(default, rename = "digestProbe")]
    digest_probe: Value,
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
    assert_eq!(oracle.len(), 20, "the corpus is twenty cases");

    // The extender-minted ids, read from the committed sidecar rather than
    // transcribed — the oracle case reads the same file, so a re-pin in
    // `extend-character-archive-profile-and-avatars.ts` cannot leave the two
    // sides driving different rows.
    let meta: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("character-archive-main.db.meta.json"))
            .expect("fixture sidecar"),
    )
    .expect("fixture sidecar json");
    let avatar_thumbnail_file_id = meta["avatarThumbnailFileId"]
        .as_str()
        .expect("sidecar carries avatarThumbnailFileId — re-run the profile/avatars extender")
        .to_string();

    // The baked id set: every UUID present in the untouched fixture. Anything
    // outside it in a post-op dump was MINTED by the operation.
    let baked = {
        let work = open_work(&spec);
        let mut set = BTreeSet::new();
        collect_uuids(&dump_state(&work.db), &mut set);
        assert_fixture_carries_twice_linked_blobs(&work.db);
        assert_fixture_carries_profile_and_avatars(&work.db, &spec, &avatar_thumbnail_file_id);
        set
    };

    let mut mismatches: Vec<String> = Vec::new();

    for case in &oracle {
        let work = open_work(&spec);
        let s = seams(&work, &case.app_version, ambient_passphrase());
        let db = &work.db;
        // Non-null only where the sealing passphrase differs from the current
        // one; see `main_bundles`.
        let mut seal: Option<&str> = None;

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
            // ── The passphrase-unavailable refusal (§4.2c) ──
            // A USER passphrase protects the instance and this process has
            // never seen it. The resolve happens BEFORE anything is written, so
            // the state dump doubles as the proof that a refused archive leaves
            // no bundle, no tombstone and no flipped seat behind.
            "archive_key_unavailable" => {
                let s = seams(
                    &work,
                    &case.app_version,
                    PassphraseSource {
                        cached: None,
                        has_user_passphrase: true,
                    },
                );
                match archive_character(db, &spec.user_id, &spec.sable, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            // ── The passphrase-CHANGE diagnosis ──
            // Seal under the old passphrase, then rehydrate with the new one
            // cached — precisely what a passphrase change leaves behind for a
            // bundle written before it. The header's `keyHash` is what turns
            // this into a named refusal rather than a bare GCM authentication
            // failure, and the character must stay archived.
            "archive_rehydrate_passphrase_mismatch" => {
                seal = Some(OLD_PASSPHRASE);
                let sealing = seams(
                    &work,
                    &case.app_version,
                    PassphraseSource {
                        cached: Some(OLD_PASSPHRASE),
                        has_user_passphrase: true,
                    },
                );
                archive_character(db, &spec.user_id, &spec.sable, &sealing)
                    .await
                    .expect("archive under the old passphrase");
                let current = seams(
                    &work,
                    &case.app_version,
                    PassphraseSource {
                        cached: Some(NEW_PASSPHRASE),
                        has_user_passphrase: true,
                    },
                );
                match rehydrate_character(db, &spec.user_id, &spec.sable, &current).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            // ── `pruneComplete: false` ──
            // The prune is the one phase that may fail without failing the
            // archive. Reaching it needs a link delete that fails, so the case
            // plants a BEFORE DELETE trigger that aborts every link delete in
            // the vault: nothing is deleted on either side, v4 falls through to
            // its undead-links honesty check and v5 to its propagated error, and
            // BOTH report `pruneComplete: false` over an untouched vault.
            "archive_prune_incomplete" => {
                db.write(|ws| {
                    ws.mount_index()
                        .expect("mount index")
                        .connection()
                        .execute_batch(
                            "CREATE TRIGGER qt_block_link_delete \
                             BEFORE DELETE ON doc_mount_file_links \
                             BEGIN SELECT RAISE(ABORT, 'planted: vault link deletes are blocked'); END;",
                        )
                        .map_err(Into::into)
                })
                .await
                .expect("plant the delete-blocking trigger");
                match archive_character(db, &spec.user_id, &spec.sable, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            // ── The pre-revision tombstone's avatar thumbnail (§6 step 4) ──
            // `archivedAvatarFileId` is vestigial: the shipped service never
            // writes it, so only a tombstone left by the pre-§4.2a revision
            // carries one. The raw UPDATE is deliberate — the §4.4 guard
            // sanctions exactly one patch on an archived row. Rehydrate must
            // clear the key in its FOLLOW-UP patch and delete the standalone
            // thumbnail row the fixture's extender planted.
            "rehydrate_pre_revision_avatar" => {
                archive_character(db, &spec.user_id, &spec.sable, &s)
                    .await
                    .expect("archive");
                let sable = spec.sable.clone();
                let thumb = avatar_thumbnail_file_id.clone();
                db.write(move |ws| {
                    ws.main().connection().execute(
                        "UPDATE characters SET archivedAvatarFileId = ?1 WHERE id = ?2",
                        rusqlite::params![thumb, sable],
                    )?;
                    Ok(())
                })
                .await
                .expect("plant the pre-revision avatar pointer");
                match rehydrate_character(db, &spec.user_id, &spec.sable, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            // ── Bug 66: `get_character_detail` carries the tombstone ────────
            // The Salon sidebar renders from the chat GET, which enriches every
            // participant through this function. BOTH returns are probed — the
            // avatar-override early return (Sable's `avatarOverrides[0]` points
            // at the quay chat) and the main one — before and after a tombstone.
            //
            // The tombstone is planted by raw UPDATE rather than by archiving:
            // archiving PRUNES the vault, and a pruned override face would
            // resolve to nothing and fall through to the main return, so the
            // avatar-override + archived probe would stop being that shape.
            "character_detail_enrichment" => {
                use quilltap_core::services::chat_enrichment::get_character_detail;
                let probe = |db: &Db, character_id: &str, chat_id: Option<&str>| -> Value {
                    let detail = db
                        .read_main(|main| {
                            db.read_mount_index(|mount| {
                                get_character_detail(main, mount, character_id, chat_id)
                            })
                        })
                        .expect("read main");
                    // `None` (the missing-character probe) serializes to
                    // `null`, which is v4's `?? null` on the same probe.
                    serde_json::to_value(detail).unwrap_or(Value::Null)
                };
                let mut probes: Vec<Value> = Vec::new();
                let mut push = |label: &str, detail: Value| {
                    probes.push(json!({"label": label, "detail": detail}));
                };
                push("sable_live_main", probe(db, &spec.sable, None));
                push(
                    "sable_live_override",
                    probe(db, &spec.sable, Some(&spec.chat_seated)),
                );
                push(
                    "sable_live_other_chat",
                    probe(db, &spec.sable, Some(&spec.chat_removed)),
                );
                push("tor_live_main", probe(db, &spec.tor, None));
                push("missing_character", probe(db, MISSING_CHARACTER, None));
                let sable = spec.sable.clone();
                db.write(move |ws| {
                    ws.main().connection().execute(
                        "UPDATE characters SET archivedAt = ?1 WHERE id = ?2",
                        rusqlite::params!["2026-03-02T00:00:00.000Z", sable],
                    )?;
                    Ok(())
                })
                .await
                .expect("plant tombstone");
                push("sable_archived_main", probe(db, &spec.sable, None));
                push(
                    "sable_archived_override",
                    probe(db, &spec.sable, Some(&spec.chat_seated)),
                );
                json!({ "probes": probes })
            }
            // ── Bug 69: the clobbered-row self-heal ─────────────────────────
            // v4's file watcher re-derived `sha256` from the ENCRYPTED bytes
            // moments after an archive was written, so the row held the
            // ciphertext digest and every later rehydrate refused the bundle as
            // corrupt — archiving was one-way. Plant exactly that damage and
            // the rehydrate must succeed, warn, and repair the row (the
            // `digestProbe` section is where the repair is visible; the
            // ARCHIVE row's sha256 itself is minted and blinded).
            //
            // v5 cannot CAUSE the damage — it ports no watcher and no boot
            // reconciliation — but it opens the same instances a pre-4.9 v4
            // damaged, which is why the arm ports at all.
            "rehydrate_digest_clobbered" | "rehydrate_digest_corrupt" => {
                let archived = archive_character(db, &spec.user_id, &spec.sable, &s)
                    .await
                    .expect("archive");
                let file_id = archived.archive_file_id.clone().unwrap_or_default();
                let planted = if case.name == "rehydrate_digest_corrupt" {
                    // Neither the plaintext digest nor the file-as-stored one,
                    // so the self-heal must NOT engage. Deterministic, which
                    // keeps the refusal sentence's `expected …` half a real
                    // comparand.
                    CORRUPT_DIGEST.to_string()
                } else {
                    let entry = {
                        let fid = file_id.clone();
                        db.read_main(move |c| {
                            quilltap_core::db::files::FilesRepository::new(c).find_full_by_id(&fid)
                        })
                        .expect("read archive row")
                        .expect("archive row")
                    };
                    let raw = quilltap_core::services::file_storage::download_file(
                        db,
                        work.backend.as_ref(),
                        &quilltap_core::db::files::FileEntry {
                            id: entry.id.clone(),
                            sha256: entry.sha256.clone(),
                            original_filename: entry.original_filename.clone(),
                            mime_type: entry.mime_type.clone(),
                            size: entry.size,
                            width: entry.width,
                            height: entry.height,
                            category: entry.category.clone(),
                            generation_prompt: None,
                            generation_model: None,
                            generation_revised_prompt: None,
                            description: entry.description.clone(),
                            storage_key: entry.storage_key.clone(),
                        },
                    )
                    .expect("download the bundle as stored");
                    sha256_hex(&raw)
                };
                let fid = file_id.clone();
                // The clobbered arm plants `size` WRONG too, and it must come
                // back out wrong: the repair writes `sha256` ALONE (an archive
                // row's `size` is the real on-disk encrypted byte count). That
                // turns "sha256 only" into a measured comparand on the `files`
                // state dump instead of a claim about the patch struct.
                let plant_size = case.name == "rehydrate_digest_clobbered";
                db.write(move |ws| {
                    let c = ws.main().connection();
                    if plant_size {
                        c.execute(
                            "UPDATE files SET sha256 = ?1, size = 1 WHERE id = ?2",
                            rusqlite::params![planted, fid],
                        )?;
                    } else {
                        c.execute(
                            "UPDATE files SET sha256 = ?1 WHERE id = ?2",
                            rusqlite::params![planted, fid],
                        )?;
                    }
                    Ok(())
                })
                .await
                .expect("plant the damaged digest");
                match rehydrate_character(db, &spec.user_id, &spec.sable, &s).await {
                    Ok(r) => r.to_value(),
                    Err(e) => error_value(&e),
                }
            }
            other => panic!("unknown oracle case: {other}"),
        };

        let (bundle_rows, probe_rows, plaintext_digests) =
            main_bundles(db, work.backend.as_ref(), &spec.user_id, seal);
        let bundles = Value::Array(bundle_rows);
        let digest_probe = Value::Array(probe_rows);
        // A refusal sentence quotes the bundle's plaintext digest, which is
        // minted per run (the manifest carries a bundle-time stamp) and can
        // never agree across two runs, let alone two languages. Blind it — the
        // PLANTED half of the sentence stays literal, so the two digests remain
        // distinguishable. The oracle blinds its own side identically.
        let result = blind_plaintext_sha(&result, &plaintext_digests);
        let state = dump_state(db);

        // The ARCHIVE row's sha256 digests a plaintext carrying a bundle-time
        // stamp, so it can never agree; blind it BEFORE normalizing, or it
        // would perturb the canonical row order that decides the token labels.
        let mut got_state = state;
        let mut want_state = case.state.clone();
        blind_archive_sha(&mut got_state);
        blind_archive_sha(&mut want_state);

        // v4 nests an error's rider keys under `details`; v5 carries them flat
        // (the P4.6ah precedent), so the two shapes are reconciled on v4's side.
        let mut got = normalize_sides(
            &baked,
            &spec.seed_timestamp,
            &got_state,
            &result,
            &bundles,
            &digest_probe,
        );
        let mut want = normalize_sides(
            &baked,
            &spec.seed_timestamp,
            &want_state,
            &flatten_details(&case.result),
            &case.bundles,
            &case.digest_probe,
        );
        for side in [&mut got, &mut want] {
            resort_state(&mut side["state"]);
        }

        for section in ["result", "bundles", "digestProbe"] {
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

/// The three shapes `extend-character-archive-profile-and-avatars.ts` adds are
/// each load-bearing for exactly one arm, and each is the kind of coverage a
/// later fixture edit can vacate in silence — the cases would keep passing
/// while proving nothing (`harness-corpus-shape-constants-rot`).
///
///  - the DEFAULT embedding profile is what makes `background_jobs` a positive
///    comparand: without one, both sides enqueue zero and a port that never
///    enqueued anything passes;
///  - the `avatarOverrides[].imageId` link is the override half of
///    `pruneVault`'s keep-set union, which the seeded `defaultImageId` alone
///    left dead;
///  - the standalone thumbnail `files` row is what rehydrate's cosmetic
///    thumbnail delete actually removes — a missing row deletes as a silent
///    no-op on both sides.
fn assert_fixture_carries_profile_and_avatars(db: &Db, spec: &Spec, thumbnail_file_id: &str) {
    let defaults: i64 = db
        .read_main(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM embedding_profiles WHERE isDefault = 1",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .expect("read embedding_profiles");
    assert_eq!(
        defaults, 1,
        "the fixture must carry exactly one DEFAULT embedding profile — re-run \
         harness/oracle/fixtures/extend-character-archive-profile-and-avatars.ts"
    );

    let sable = spec.sable.clone();
    let overrides: String = db
        .read_main(move |c| {
            Ok(c.query_row(
                "SELECT avatarOverrides FROM characters WHERE id = ?1",
                [&sable],
                |r| r.get::<_, String>(0),
            )?)
        })
        .expect("read Sable's avatarOverrides");
    let overrides: Value = serde_json::from_str(&overrides).expect("avatarOverrides json");
    let override_ids: Vec<&str> = overrides
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|o| o.get("imageId").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        override_ids.len(),
        1,
        "Sable must carry exactly one avatarOverride (the keep-by-id arm the \
         ten managed paths cannot reach); found {overrides}"
    );
    let override_link = override_ids[0].to_string();
    let linked: i64 = db
        .read_mount_index(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM doc_mount_file_links WHERE id = ?1",
                [&override_link],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .expect("read the override link");
    assert_eq!(
        linked, 1,
        "the avatarOverride must point at a REAL vault link, or the keep-set \
         union proves nothing"
    );

    let thumb = thumbnail_file_id.to_string();
    let thumbs: i64 = db
        .read_main(move |c| {
            Ok(
                c.query_row("SELECT COUNT(*) FROM files WHERE id = ?1", [&thumb], |r| {
                    r.get::<_, i64>(0)
                })?,
            )
        })
        .expect("read the thumbnail row");
    assert_eq!(
        thumbs, 1,
        "the standalone avatar-thumbnail `files` row must exist, or rehydrate's \
         thumbnail delete is a no-op on both sides"
    );
}

/// The manifest's `createdAt` is minted at bundle time, so it is normalized
/// inside the bundle comparand (the generic ISO rule already does that). This
/// helper only exists for the ARCHIVE `files` row's **sha256**, which digests a
/// plaintext carrying that same minted stamp and therefore can never agree
/// across two runs — let alone two languages. Its LENGTH still can, which is
/// why `size` is left as a real comparand and `appVersion` is injected rather
/// than stripped.
/// Replace every bundle's PLAINTEXT digest wherever it appears in a comparand
/// (a `ArchiveVerificationError`'s `got …` half is the only place it does).
/// See the call site for why it can never agree across two runs.
fn blind_plaintext_sha(v: &Value, digests: &[String]) -> Value {
    if digests.is_empty() {
        return v.clone();
    }
    let mut text = v.to_string();
    for d in digests {
        text = text.replace(d.as_str(), "<plaintext-sha>");
    }
    serde_json::from_str(&text).expect("re-parse the blinded result")
}

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
