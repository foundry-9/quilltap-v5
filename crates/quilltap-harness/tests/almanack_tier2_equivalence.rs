//! P4.37 unit 12 — the Almanack tier-2 differential: report DATA, rendered
//! MARKDOWN, route envelopes, progress frames and persisted rows, diffed
//! against v4's REAL `lib/tools/almanack` + `system/tools` route handlers at
//! `f4955e0e` over the committed `almanack-*` fixture family.
//!
//! ## The comparison contract
//!
//! - **Renderer proof (per data case, byte-exact, ZERO normalization):**
//!   `render_almanack_markdown(v4_data)` must equal v4's own markdown — the
//!   tier-1 renderer proof re-run over REAL collector output instead of the
//!   hand-written fixture.
//! - **Data proof (per data case):** v4's data and v5's data compare as JSON
//!   after ONE normalization list (below). The runtime-environment block,
//!   `version` and `nodeEnv` are SPLICED — the Rust context takes the oracle's
//!   recorded values as its host inputs, which is exactly what they are
//!   (v4 reads `process`/`os`; v5's host supplies facts). Nothing else about
//!   them is asserted; the recorded phase-1 divergence.
//! - **The normalization list** (the recorded phase-2 divergences + disk
//!   sizes; see `almanack/phase2_machinery.rs`'s header):
//!     * `databaseSecurity.databases[*].sizeBytes` — the oracle's working
//!       copies are mutated by v4's own init before phase 1 stats them.
//!     * `plugins.{enabled,disabled,byCapability,npmInstalled,bundled}` — v5
//!       has no plugin loader (the DB-derived counts stay EXACT).
//!     * `themeInfo.{stats,themes,themesWithIcons,totalIconOverrides,
//!       totalUnknownIconOverrides}` — v5 has no server-side theme registry
//!       (`activeThemeId`/`colorMode` stay EXACT).
//!     * `imageProviders[*].models` / `embeddingProviders[*].models` — v4
//!       falls back to a plugin's static model list on a cold cache; v5 has no
//!       static lists (the provider/displayName SETS stay EXACT, and
//!       `modelsByProvider` — warm-cache — stays EXACT).
//! - **Route flow:** envelopes compare field-by-field with run-minted UUIDs
//!   remapped `<minted-N>` (first-seen order; the pinned fixture ids keep
//!   themselves) and content-derived fields (`size`, sha256, blob bytes)
//!   sentineled. The CONTENT itself is proven by composition: v5's route
//!   content must equal v5's data-case markdown BYTE-EXACT, and the oracle's
//!   route content must equal the oracle's data-case markdown after
//!   normalizing only the volatile phase-1 lines (uptime/free-memory/db-size
//!   rows — the runtime block moves between two v4 calls).
//! - **Progress frames:** the generate run's replay must match v4's
//!   operation-progress replay — seven 1-based `phase` frames then `done`,
//!   keys/labels byte-equal (`ts` stripped).
//! - **Fallback coverage:** NO section of the v5 report may equal its
//!   `collect()` fallback shape in the happy-path case — a collector that
//!   always throws must not pass by coinciding with the oracle's fallback
//!   (the `49769ec4` fail-soft-gate lesson).
//!
//! ## Regenerate
//!
//! Fixture family (v4's real create paths; see
//! `harness/oracle/fixtures/build-almanack-fixture.ts`), then the oracle
//! (`harness/oracle/cases/almanack-routes.test.ts` — /tmp jest mirror, pinned
//! v4 worktree, TZ=UTC), then:
//!   QT_ORACLE_ALMANACK_TIER2=/tmp/oracle-almanack.ndjson \
//!     cargo test -p quilltap-harness --test almanack_tier2_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use quilltap_core::almanack::{
    generate_almanack_data, render_almanack_markdown, AlmanackContext, AlmanackPaths,
    AlmanackReportData, RuntimeFacts,
};
use quilltap_core::api::almanack::{
    almanack_delete, almanack_generate, almanack_get, almanack_list,
};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::content_disposition::{build_content_disposition, Disposition};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::provider_manifest::Registry;
use quilltap_core::services::creation_progress::{CreationProgressBus, CreationProgressEmitter};
use quilltap_core::services::file_storage::StorageBackend;
use serde_json::{json, Value};

const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const UPLOADS_MOUNT: &str = "be000000-0000-4000-8000-000000000001";
const PROGRESS_ID: &str = "fd000000-0000-4000-8000-000000000001";

/// UUIDs the remap keeps verbatim: everything else that walks past the
/// canonical serializer is treated as run-minted and labelled `<minted-N>`.
/// Baked-but-unpinned fixture ids (repo-minted at build time) are identical in
/// both databases, so a positional label still compares equal — only ids minted
/// DURING the run genuinely need the remap.
const KEEP_UUIDS: [&str; 3] = [USER, UPLOADS_MOUNT, PROGRESS_ID];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn spec() -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/almanack.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("read almanack.json")).unwrap()
}

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

struct Scratch {
    root: PathBuf,
    db: Db,
    paths: AlmanackPaths,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Copy the committed family (+ seed the pinned backups directory) and open it.
fn scratch(tag: &str, legacy_llm: bool, sp: &Value) -> Scratch {
    let root = std::env::temp_dir().join(format!("qt-almanack-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let data = root.join("data");
    let backups = data.join("backups");
    std::fs::create_dir_all(&backups).unwrap();
    let main = data.join("quilltap.db");
    let mount = data.join("quilltap-mount-index.db");
    let llm = data.join("quilltap-llm-logs.db");
    std::fs::copy(fixtures_dir().join("almanack-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("almanack-mount.db"), &mount).unwrap();
    std::fs::copy(
        fixtures_dir().join(if legacy_llm {
            "almanack-llmlogs-legacy.db"
        } else {
            "almanack-llmlogs.db"
        }),
        &llm,
    )
    .unwrap();
    for b in sp["backups"].as_array().unwrap() {
        let name = b["filename"].as_str().unwrap();
        let size = b["sizeBytes"].as_u64().unwrap() as usize;
        std::fs::write(backups.join(name), vec![0x51u8; size]).unwrap();
    }
    let db = Db::open(
        DbPaths {
            main: main.clone(),
            mount_index: Some(mount.clone()),
            llm_logs: Some(llm.clone()),
        },
        TEST_PEPPER,
    )
    .expect("open fixture copies");
    let paths = AlmanackPaths {
        main_db: main,
        llm_logs_db: Some(llm),
        mount_index_db: Some(mount),
        data_dir: data.clone(),
        backups_dir: backups,
    };
    Scratch { root, db, paths }
}

/// The host facts, SPLICED from the oracle's recorded runtime block — v5's
/// host supplies these as inputs, and the differential supplies what v4's
/// process observed so every downstream byte is comparable.
fn spliced_facts(oracle_data: &Value) -> (RuntimeFacts, String, String, String) {
    let re = &oracle_data["runtimeEnvironment"];
    let facts = RuntimeFacts {
        node_version: re["nodeVersion"].as_str().unwrap().to_string(),
        platform: re["platform"].as_str().unwrap().to_string(),
        arch: re["arch"].as_str().unwrap().to_string(),
        os_type: re["osType"].as_str().unwrap().to_string(),
        os_release: re["osRelease"].as_str().unwrap().to_string(),
        total_memory_bytes: re["totalMemoryBytes"].as_f64().unwrap(),
        free_memory_bytes: re["freeMemoryBytes"].as_f64().unwrap(),
        runtime_type: re["runtimeType"].as_str().unwrap().to_string(),
        uptime_seconds: re["uptimeSeconds"].as_f64().unwrap(),
        timezone: re["timezone"].as_str().unwrap().to_string(),
    };
    let data_dir = re["dataDirectory"].as_str().unwrap().to_string();
    let version = oracle_data["version"].as_str().unwrap().to_string();
    let node_env = oracle_data["nodeEnv"].as_str().unwrap().to_string();
    (facts, data_dir, version, node_env)
}

fn pinned_now_ms(sp: &Value) -> i64 {
    quilltap_core::clock::iso_to_ms(sp["pinnedNowIso"].as_str().unwrap()).unwrap()
}

/// EXPECTED DIVERGENCE — the BUILTIN provider (pinned in BOTH directions).
///
/// v4's provider registry lists `BUILTIN` ("Built-in (TF-IDF)"): the
/// `qtap-plugin-builtin-embeddings` bundled plugin. v5's compiled-in manifest
/// registry deliberately carries no manifest for it (the TF-IDF builtin is
/// compiled into the embedding layer, not a declarative provider), so the
/// Almanack's provider table and embedding-provider list OMIT the BUILTIN row
/// in v5. Asserted both ways below: if v4 stops listing it, or a v5 manifest
/// appears, this pin trips and the strip must be retired.
fn strip_builtin_provider(v4_data: &mut Value, v5_data: &Value) -> Result<(), String> {
    let has_builtin = |v: &Value, list: &str, key: &str| -> bool {
        v[list]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|r| r[key].as_str() == Some("BUILTIN")))
    };
    if !has_builtin(v4_data, "providers", "name") {
        return Err(
            "v4 no longer lists the BUILTIN provider — retire the pinned divergence".into(),
        );
    }
    if !has_builtin(v4_data, "embeddingProviders", "provider") {
        return Err("v4 no longer lists BUILTIN under embeddingProviders — retire the pin".into());
    }
    if has_builtin(v5_data, "providers", "name")
        || has_builtin(v5_data, "embeddingProviders", "provider")
    {
        return Err("v5 now carries a BUILTIN manifest — retire the pinned divergence".into());
    }
    for (list, key) in [("providers", "name"), ("embeddingProviders", "provider")] {
        if let Some(rows) = v4_data.get_mut(list).and_then(Value::as_array_mut) {
            rows.retain(|r| r[key].as_str() != Some("BUILTIN"));
        }
    }
    Ok(())
}

/// The normalization list from the header, applied identically to both sides.
fn normalize_data(data: &mut Value) {
    if let Some(dbs) = data
        .pointer_mut("/databaseSecurity/databases")
        .and_then(Value::as_array_mut)
    {
        for row in dbs {
            row["sizeBytes"] = json!("<size>");
        }
    }
    for key in [
        "enabled",
        "disabled",
        "byCapability",
        "npmInstalled",
        "bundled",
    ] {
        if let Some(p) = data.pointer_mut("/plugins") {
            p[key] = json!("<registry>");
        }
    }
    for key in [
        "stats",
        "themes",
        "themesWithIcons",
        "totalIconOverrides",
        "totalUnknownIconOverrides",
    ] {
        if let Some(t) = data.pointer_mut("/themeInfo") {
            t[key] = json!("<registry>");
        }
    }
    for list in ["imageProviders", "embeddingProviders"] {
        if let Some(rows) = data.get_mut(list).and_then(Value::as_array_mut) {
            for row in rows {
                row["models"] = json!("<models>");
            }
        }
    }
    // The standing `is not valid JSON:` wording seam (deferred loud since
    // P4.6bb): the parse-failure reason carries the JSON engine's OWN message
    // (V8's vs serde_json's). The prefix, store and path stay exact; only the
    // engine's tail is sentineled.
    if let Some(rows) = data
        .pointer_mut("/scriptorium/customTools/parseFailureDetail")
        .and_then(Value::as_array_mut)
    {
        for row in rows {
            if let Some(reason) = row.get("reason").and_then(Value::as_str) {
                const SEAM: &str = "is not valid JSON:";
                if let Some(at) = reason.find(SEAM) {
                    let pinned = format!("{}{} <engine-wording>", &reason[..at], SEAM);
                    row["reason"] = json!(pinned);
                }
            }
        }
    }
}

/// Line prefixes whose VALUES are volatile between two calls in the same v4
/// process (the runtime block + on-disk sizes) — used only for the
/// oracle-vs-oracle route/data markdown consistency check.
const VOLATILE_LINE_PREFIXES: [&str; 13] = [
    "- **Version**: ",
    "- **Node Environment**: ",
    "- **Node Version**: ",
    "- **Platform**: ",
    "- **OS**: ",
    "- **Runtime Type**: ",
    "- **Electron Shell**: ",
    "- **Shell Capabilities**: ",
    "- **Total Memory**: ",
    "- **Free Memory**: ",
    "- **Uptime**: ",
    "- **Timezone**: ",
    "- **Data Directory**: ",
];

fn normalize_volatile_lines(markdown: &str) -> String {
    markdown
        .lines()
        .map(|line| {
            for p in VOLATILE_LINE_PREFIXES {
                if line.starts_with(p) {
                    return format!("{p}<NORM>");
                }
            }
            // The three DB-size table rows (3 cells; the 5-cell backup rows
            // with the same labels are deterministic and stay exact).
            for label in ["| Main | ", "| LLM Logs | ", "| Mount Index | "] {
                if line.starts_with(label) && line.matches('|').count() == 4 {
                    return format!("{label}<NORM> |");
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Canonicalize + remap a JSON value: BLOB-ish/content-derived columns
/// sentineled, then every non-pinned UUID replaced `<minted-N>` in first-seen
/// order over the serialized form.
fn remap_minted(value: &Value) -> String {
    let serialized = serde_json::to_string(value).unwrap();
    let mut out = String::with_capacity(serialized.len());
    let bytes = serialized.as_bytes();
    let mut map: HashMap<String, String> = HashMap::new();
    let mut next = 1usize;
    let mut i = 0usize;
    let is_uuid_at = |s: &[u8], i: usize| -> bool {
        if i + 36 > s.len() {
            return false;
        }
        for (off, ch) in s[i..i + 36].iter().enumerate() {
            match off {
                8 | 13 | 18 | 23 => {
                    if *ch != b'-' {
                        return false;
                    }
                }
                _ => {
                    if !ch.is_ascii_hexdigit() {
                        return false;
                    }
                }
            }
        }
        true
    };
    while i < bytes.len() {
        if is_uuid_at(bytes, i) {
            let candidate = &serialized[i..i + 36];
            if KEEP_UUIDS.contains(&candidate) {
                out.push_str(candidate);
            } else {
                let label = map.entry(candidate.to_string()).or_insert_with(|| {
                    let l = format!("<minted-{next}>");
                    next += 1;
                    l
                });
                out.push_str(label);
            }
            i += 36;
        } else {
            out.push(serialized.as_bytes()[i] as char);
            i += 1;
        }
    }
    out
}

/// Sentinel the content-derived columns of a persisted-row dump: sizes, hashes
/// and the report bytes themselves (content equality is proven at the envelope
/// level; the dumps prove row STRUCTURE and placement).
fn sentinel_content_columns(rows: &mut Value) {
    const CONTENT_KEYS: [&str; 7] = [
        "sha256",
        "size",
        "sizeBytes",
        "fileSizeBytes",
        "contentLength",
        "data",
        "content",
    ];
    const TS_KEYS: [&str; 6] = [
        "createdAt",
        "updatedAt",
        "lastModified",
        "storedAt",
        "lastScannedAt",
        "lastUsed",
    ];
    match rows {
        Value::Array(items) => {
            for item in items {
                sentinel_content_columns(item);
            }
        }
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if CONTENT_KEYS.contains(&k.as_str()) && !v.is_null() {
                    *v = json!("<content>");
                } else if TS_KEYS.contains(&k.as_str()) && !v.is_null() {
                    *v = json!("<ts>");
                } else {
                    sentinel_content_columns(v);
                }
            }
        }
        _ => {}
    }
}

/// Dump the persisted report rows exactly as the oracle does.
fn dump_persisted(db: &Db) -> Value {
    fn rows(conn: &rusqlite::Connection, sql: &str) -> Vec<Value> {
        let mut stmt = conn.prepare(sql).unwrap();
        let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        let mut q = stmt.query([]).unwrap();
        while let Some(row) = q.next().unwrap() {
            let mut obj = serde_json::Map::new();
            for (idx, name) in cols.iter().enumerate() {
                let v: Value = match row.get_ref(idx).unwrap() {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => json!(n),
                    rusqlite::types::ValueRef::Real(f) => json!(f),
                    rusqlite::types::ValueRef::Text(t) => {
                        json!(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(b) => json!(format!("<blob:{}>", b.len())),
                };
                obj.insert(name.clone(), v);
            }
            out.push(Value::Object(obj));
        }
        out
    }
    let files_rows = db
        .read_main(|c| {
            Ok(rows(
                c,
                "SELECT * FROM \"files\" WHERE \"folderPath\" = '/reports' ORDER BY \"originalFilename\"",
            ))
        })
        .unwrap();
    let (links, files, documents, blobs) = db
        .read_mount_index(|c| {
            Ok((
                rows(c, "SELECT * FROM doc_mount_file_links WHERE relativePath LIKE 'diagnostics/%' ORDER BY relativePath"),
                rows(c, "SELECT f.* FROM doc_mount_files f WHERE f.id IN (SELECT fileId FROM doc_mount_file_links WHERE relativePath LIKE 'diagnostics/%') ORDER BY f.id"),
                rows(c, "SELECT d.* FROM doc_mount_documents d WHERE d.fileId IN (SELECT fileId FROM doc_mount_file_links WHERE relativePath LIKE 'diagnostics/%') ORDER BY d.id"),
                rows(c, "SELECT b.* FROM doc_mount_blobs b WHERE b.fileId IN (SELECT fileId FROM doc_mount_file_links WHERE relativePath LIKE 'diagnostics/%') ORDER BY b.id"),
            ))
        })
        .unwrap();
    json!({
        "files_rows": files_rows,
        "mount_links": links,
        "mount_files": files,
        "mount_documents": documents,
        "mount_blobs": blobs,
    })
}

struct ScratchBackend;
impl StorageBackend for ScratchBackend {
    fn upload(&self, _k: &str, _c: &[u8], _t: &str) -> Result<(), String> {
        Err("unused: report bytes live in the Uploads mount".into())
    }
    fn download(&self, key: &str) -> Result<Vec<u8>, String> {
        Err(format!("unused disk key: {key}"))
    }
    fn delete(&self, _k: &str) -> Result<(), String> {
        // v4's disk delete is ENOENT-tolerant; a mount-blob key never lands
        // here, but the files-row delete path calls delete() regardless.
        Ok(())
    }
    fn exists(&self, _k: &str) -> Result<bool, String> {
        Ok(false)
    }
}

fn outcome(resp: &Response) -> (u16, Value) {
    match resp {
        Response::System(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::NotFound => 404,
                ErrorKind::BadRequest | ErrorKind::Unprocessable => 400,
                ErrorKind::Internal => 500,
                _ => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => panic!("unexpected response variant: {other:?}"),
    }
}

struct Failures(Vec<String>);
impl Failures {
    fn check(&mut self, name: &str, cond: bool, detail: impl FnOnce() -> String) {
        if !cond {
            self.0.push(format!("{name}: {}", detail()));
        } else {
            eprintln!("OK {name}");
        }
    }
}

#[test]
fn almanack_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_ALMANACK_TIER2") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_ALMANACK_TIER2 to the oracle NDJSON (see the test header)."
            );
            return;
        }
    };
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        assert_eq!(
            v["baseline"].as_str(),
            Some("f4955e0e"),
            "oracle regenerated at a different baseline — regenerate at f4955e0e"
        );
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    // The case SET, not merely each-present: a truncated oracle fails loudly.
    let expected_cases = [
        "data_exact",
        "data_approximate",
        "generate_route",
        "progress_frames",
        "persisted_after_generate",
        "list_route",
        "get_route",
        "get_download_route",
        "get_missing_route",
        "delete_route",
        "delete_missing_route",
        "list_after_delete",
        "persisted_after_delete",
        "generate_untracked",
    ];
    for c in expected_cases {
        assert!(oracle.contains_key(c), "oracle is missing case {c}");
    }
    assert_eq!(
        oracle.len(),
        expected_cases.len(),
        "unexpected extra oracle cases"
    );

    let sp = spec();
    let now_ms = pinned_now_ms(&sp);
    let mut failed = Failures(Vec::new());

    let (facts, data_dir, version, node_env) = spliced_facts(&oracle["data_exact"]["data"]);

    // ── The two data cases ────────────────────────────────────────────────────
    let mut v5_markdown_exact = String::new();
    for (case, legacy) in [("data_exact", false), ("data_approximate", true)] {
        let exp = &oracle[case];
        let sc = scratch(case, legacy, &sp);
        let (facts_c, data_dir_c, version_c, node_env_c) = spliced_facts(&exp["data"]);
        let mut paths = sc.paths.clone();
        paths.data_dir = PathBuf::from(&data_dir_c);
        let ctx = AlmanackContext {
            db: &sc.db,
            registry: Registry::built_in(),
            user_id: USER,
            paths,
            facts: facts_c,
            passphrase_protected: true,
            version: version_c,
            node_env: node_env_c,
            now_ms,
        };
        let v5_data = generate_almanack_data(&ctx, &CreationProgressEmitter::inert());
        let v5_markdown = render_almanack_markdown(&v5_data);
        if case == "data_exact" {
            v5_markdown_exact = v5_markdown.clone();
        }

        // Renderer proof: v5's renderer over v4's REAL data, byte-exact.
        let v4_data_parsed: AlmanackReportData =
            serde_json::from_value(exp["data"].clone()).expect("v4 data round-trips the model");
        let rendered_v4 = render_almanack_markdown(&v4_data_parsed);
        let v4_markdown = exp["markdown"].as_str().unwrap();
        failed.check(
            &format!("{case}:renderer"),
            rendered_v4 == v4_markdown,
            || {
                let (line, ours, theirs) = first_line_diff(&rendered_v4, v4_markdown);
                format!("render(v4 data) != v4 markdown at line {line}:\n  rust: {ours}\n  v4:   {theirs}")
            },
        );

        // Model round-trip: a field the Rust model silently dropped would be
        // invisible to a render that never mentions it.
        let reserialized = serde_json::to_value(&v4_data_parsed).unwrap();
        let mut orig = exp["data"].clone();
        let mut rt = reserialized;
        canonicalize_numbers(&mut orig);
        canonicalize_numbers(&mut rt);
        failed.check(&format!("{case}:round_trip"), orig == rt, || {
            first_json_diff(&orig, &rt)
        });

        // Data proof: one normalization list, both sides — after the pinned
        // BUILTIN-provider divergence is asserted and stripped.
        let mut v4_norm = exp["data"].clone();
        let mut v5_norm = serde_json::to_value(&v5_data).unwrap();
        match strip_builtin_provider(&mut v4_norm, &v5_norm) {
            Ok(()) => eprintln!("OK {case}:builtin_divergence_pin"),
            Err(e) => failed.0.push(format!("{case}:builtin_divergence_pin: {e}")),
        }
        // dogfood #67 (cast-size histogram) + #68 (wardrobe-permission counts):
        // v4 `e6554b6e` ADOPTED both fixes this port made first, so v4's data now
        // agrees with v5's — the reconcile shim is retired and these are plain
        // comparisons in the `data` check below. The fixture still seeds duplicate
        // cast sizes and a NULL-flag character, so the comparison stays meaningful.
        normalize_data(&mut v4_norm);
        normalize_data(&mut v5_norm);
        canonicalize_numbers(&mut v4_norm);
        canonicalize_numbers(&mut v5_norm);
        failed.check(&format!("{case}:data"), v4_norm == v5_norm, || {
            first_json_diff(&v4_norm, &v5_norm)
        });

        // The provider/displayName SETS under the normalized model lists.
        for list in ["imageProviders", "embeddingProviders"] {
            let names = |v: &Value| -> Vec<String> {
                v[list]
                    .as_array()
                    .map(|rows| {
                        rows.iter()
                            .map(|r| {
                                format!(
                                    "{}/{}",
                                    r["provider"].as_str().unwrap_or("?"),
                                    r["displayName"].as_str().unwrap_or("?")
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };
            // The pinned BUILTIN divergence applies to the set comparison too.
            let v4_names: Vec<String> = names(&exp["data"])
                .into_iter()
                .filter(|n| !n.starts_with("BUILTIN/"))
                .collect();
            let v5_names = names(&serde_json::to_value(&v5_data).unwrap());
            failed.check(&format!("{case}:{list}_set"), v4_names == v5_names, || {
                format!("v4 {v4_names:?} != v5 {v5_names:?}")
            });
        }

        if case == "data_exact" {
            assert_fallback_coverage(&mut failed, &v5_data);
            failed.check(
                "data_exact:exact_attribution",
                v5_data.wire_records.exact_profile_attribution,
                || "expected the migrated fixture to take the exact arm".into(),
            );
        } else {
            failed.check(
                "data_approximate:approximate_attribution",
                !v5_data.wire_records.exact_profile_attribution,
                || "expected the legacy fixture to take the approximate arm".into(),
            );
        }
    }

    // ── The route flow ────────────────────────────────────────────────────────
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    {
        let sc = scratch("flow", false, &sp);
        let mut paths = sc.paths.clone();
        paths.data_dir = PathBuf::from(&data_dir);
        let ctx = AlmanackContext {
            db: &sc.db,
            registry: Registry::built_in(),
            user_id: USER,
            paths,
            facts: facts.clone(),
            passphrase_protected: true,
            version: version.clone(),
            node_env: node_env.clone(),
            now_ms,
        };
        let bus = Arc::new(CreationProgressBus::new());
        let (events_tx, _events_rx) = tokio::sync::broadcast::channel(256);
        let emitter = CreationProgressEmitter::active(PROGRESS_ID, bus.clone(), events_tx);
        let backend = ScratchBackend;

        let gen_resp = rt.block_on(almanack_generate(&ctx, &emitter));
        let (status, mut body) = outcome(&gen_resp);
        let exp = &oracle["generate_route"];
        let v5_content = body["content"].as_str().unwrap_or_default().to_string();
        let v5_size = body["size"].as_f64().unwrap_or(-1.0);
        let report_id = body["reportId"].as_str().unwrap_or_default().to_string();
        let v4_content = exp["body"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let v4_size = exp["body"]["size"].as_f64().unwrap_or(-2.0);
        check_envelope(&mut failed, "generate_route", exp, status, &mut body);
        failed.check(
            "generate_route:content_is_the_report",
            v5_content == v5_markdown_exact,
            || "v5 route content != v5 data-case markdown".into(),
        );
        failed.check(
            "generate_route:v4_content_consistency",
            normalize_volatile_lines(&v4_content)
                == normalize_volatile_lines(oracle["data_exact"]["markdown"].as_str().unwrap()),
            || "v4 route content != v4 data-case markdown modulo volatile lines".into(),
        );
        failed.check(
            "generate_route:size_is_content_len",
            v5_size == v5_content.len() as f64 && v4_size == v4_content.len() as f64,
            || {
                format!(
                    "v5 {v5_size} vs {} / v4 {v4_size} vs {}",
                    v5_content.len(),
                    v4_content.len()
                )
            },
        );

        // Progress frames (ts stripped, kinds + payloads byte-equal).
        let frames: Vec<Value> = bus
            .replay(PROGRESS_ID)
            .iter()
            .map(|f| {
                let mut v = serde_json::to_value(f).unwrap();
                v.as_object_mut().unwrap().remove("ts");
                v
            })
            .collect();
        let exp_frames = oracle["progress_frames"]["frames"].clone();
        failed.check(
            "progress_frames",
            Value::Array(frames.clone()) == exp_frames,
            || format!("rust: {frames:?}\noracle: {exp_frames}"),
        );

        // Persisted rows after generate.
        check_dump(
            &mut failed,
            "persisted_after_generate",
            &oracle["persisted_after_generate"]["tables"],
            &dump_persisted(&sc.db),
        );

        // list
        let (status, mut body) = outcome(&almanack_list(&sc.db, USER));
        check_envelope(
            &mut failed,
            "list_route",
            &oracle["list_route"],
            status,
            &mut body,
        );

        // get
        let (status, mut body) = outcome(&almanack_get(&sc.db, &backend, USER, &report_id));
        let get_content = body["content"].as_str().unwrap_or_default().to_string();
        failed.check(
            "get_route:content_is_the_report",
            get_content == v5_content,
            || "get content != generate content".into(),
        );
        check_envelope(
            &mut failed,
            "get_route",
            &oracle["get_route"],
            status,
            &mut body,
        );

        // The download leg is web-edge-only: the edge serves the get verb's
        // content with attachment headers built by the shared helper. The
        // header STRING is cross-checked against v4's real one (the filenames
        // are pinned-equal), the body against the get content on each side.
        let dl = &oracle["get_download_route"];
        failed.check(
            "get_download_route:status_and_type",
            dl["status"].as_u64() == Some(200)
                && dl["contentType"].as_str() == Some("text/markdown"),
            || format!("oracle download leg: {dl}"),
        );
        let v5_disposition = build_content_disposition(
            body["filename"].as_str().unwrap_or_default(),
            Disposition::Attachment,
        );
        failed.check(
            "get_download_route:content_disposition",
            dl["contentDisposition"].as_str() == Some(v5_disposition.as_str()),
            || format!("v4 {:?} != v5 {v5_disposition:?}", dl["contentDisposition"]),
        );
        failed.check(
            "get_download_route:body_is_the_report",
            dl["bodyText"].as_str() == Some(v4_content.as_str()),
            || "v4 download bytes != v4 generate content".into(),
        );

        // get missing
        let (status, mut body) = outcome(&almanack_get(
            &sc.db,
            &backend,
            USER,
            "fe000000-0000-4000-8000-000000000001",
        ));
        check_envelope(
            &mut failed,
            "get_missing_route",
            &oracle["get_missing_route"],
            status,
            &mut body,
        );

        // delete
        let (status, mut body) =
            outcome(&rt.block_on(almanack_delete(&sc.db, &backend, USER, &report_id)));
        check_envelope(
            &mut failed,
            "delete_route",
            &oracle["delete_route"],
            status,
            &mut body,
        );

        // delete missing (same id again)
        let (status, mut body) =
            outcome(&rt.block_on(almanack_delete(&sc.db, &backend, USER, &report_id)));
        check_envelope(
            &mut failed,
            "delete_missing_route",
            &oracle["delete_missing_route"],
            status,
            &mut body,
        );

        // list after delete
        let (status, mut body) = outcome(&almanack_list(&sc.db, USER));
        check_envelope(
            &mut failed,
            "list_after_delete",
            &oracle["list_after_delete"],
            status,
            &mut body,
        );

        check_dump(
            &mut failed,
            "persisted_after_delete",
            &oracle["persisted_after_delete"]["tables"],
            &dump_persisted(&sc.db),
        );
    }

    // ── The untracked-generate arm ────────────────────────────────────────────
    {
        let sc = scratch("untracked", false, &sp);
        let mut paths = sc.paths.clone();
        paths.data_dir = PathBuf::from(&data_dir);
        let ctx = AlmanackContext {
            db: &sc.db,
            registry: Registry::built_in(),
            user_id: USER,
            paths,
            facts: facts.clone(),
            passphrase_protected: true,
            version: version.clone(),
            node_env: node_env.clone(),
            now_ms,
        };
        let gen_resp = rt.block_on(almanack_generate(&ctx, &CreationProgressEmitter::inert()));
        let (status, mut body) = outcome(&gen_resp);
        check_envelope(
            &mut failed,
            "generate_untracked",
            &oracle["generate_untracked"],
            status,
            &mut body,
        );
    }

    assert!(
        failed.0.is_empty(),
        "{} check(s) failed:\n{}",
        failed.0.len(),
        failed.0.join("\n")
    );
}

/// Compare an envelope against the oracle's `{status, body}` with content
/// sentineled and minted UUIDs remapped.
fn check_envelope(failed: &mut Failures, name: &str, exp: &Value, status: u16, body: &mut Value) {
    let exp_status = exp["status"].as_u64().unwrap() as u16;
    if status != exp_status {
        failed
            .0
            .push(format!("{name}: status {status} != oracle {exp_status}"));
        return;
    }
    let mut exp_body = exp["body"].clone();
    sentinel_envelope(body);
    sentinel_envelope(&mut exp_body);
    let ours = remap_minted(body);
    let theirs = remap_minted(&exp_body);
    if ours != theirs {
        failed.0.push(format!(
            "{name}: body mismatch\n  rust:   {ours}\n  oracle: {theirs}"
        ));
    } else {
        eprintln!("OK {name}");
    }
}

/// Content-derived envelope fields: the report bytes and their length (their
/// equality is proven by the composition checks in the flow).
fn sentinel_envelope(body: &mut Value) {
    if let Some(obj) = body.as_object_mut() {
        if obj.contains_key("content") {
            obj.insert("content".into(), json!("<content>"));
        }
        if obj.contains_key("size") {
            obj.insert("size".into(), json!("<len>"));
        }
        if let Some(reports) = obj.get_mut("reports").and_then(Value::as_array_mut) {
            for r in reports {
                if let Some(ro) = r.as_object_mut() {
                    if ro.contains_key("size") {
                        ro.insert("size".into(), json!("<len>"));
                    }
                }
            }
        }
    }
}

fn check_dump(failed: &mut Failures, name: &str, exp: &Value, got: &Value) {
    let mut exp = exp.clone();
    let mut got = got.clone();
    sentinel_content_columns(&mut exp);
    sentinel_content_columns(&mut got);
    canonicalize_numbers(&mut exp);
    canonicalize_numbers(&mut got);
    let theirs = remap_minted(&exp);
    let ours = remap_minted(&got);
    if ours != theirs {
        failed.0.push(format!(
            "{name}: dump mismatch\n  rust:   {ours}\n  oracle: {theirs}"
        ));
    } else {
        eprintln!("OK {name}");
    }
}

/// `3` and `3.0` are the same number wearing different serde clothes.
fn canonicalize_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f.abs() < 9.0e15 {
                    *v = json!(f as i64);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(canonicalize_numbers),
        Value::Object(map) => map.values_mut().for_each(canonicalize_numbers),
        _ => {}
    }
}

fn first_line_diff(a: &str, b: &str) -> (usize, String, String) {
    for (idx, (la, lb)) in a.lines().zip(b.lines()).enumerate() {
        if la != lb {
            return (idx + 1, la.to_string(), lb.to_string());
        }
    }
    (
        a.lines().count().min(b.lines().count()) + 1,
        format!("<{} lines>", a.lines().count()),
        format!("<{} lines>", b.lines().count()),
    )
}

fn first_json_diff(a: &Value, b: &Value) -> String {
    fn walk(a: &Value, b: &Value, path: &str) -> Option<String> {
        match (a, b) {
            (Value::Object(ma), Value::Object(mb)) => {
                for k in ma.keys().chain(mb.keys()) {
                    match (ma.get(k), mb.get(k)) {
                        (Some(va), Some(vb)) => {
                            if let Some(d) = walk(va, vb, &format!("{path}/{k}")) {
                                return Some(d);
                            }
                        }
                        (a_side, b_side) => {
                            return Some(format!(
                                "{path}/{k}: presence differs (v4 {} / v5 {})",
                                a_side.is_some(),
                                b_side.is_some()
                            ))
                        }
                    }
                }
                None
            }
            (Value::Array(aa), Value::Array(ab)) => {
                if aa.len() != ab.len() {
                    return Some(format!("{path}: array length {} != {}", aa.len(), ab.len()));
                }
                for (i, (va, vb)) in aa.iter().zip(ab.iter()).enumerate() {
                    if let Some(d) = walk(va, vb, &format!("{path}[{i}]")) {
                        return Some(d);
                    }
                }
                None
            }
            _ => {
                if a != b {
                    Some(format!("{path}: v4 {a} != v5 {b}"))
                } else {
                    None
                }
            }
        }
    }
    walk(a, b, "").unwrap_or_else(|| "values equal after all?".into())
}

/// NO section may render its `collect()` fallback in the happy path — each
/// assertion targets a field the fixture deliberately makes non-default.
fn assert_fallback_coverage(failed: &mut Failures, d: &AlmanackReportData) {
    let mut check = |name: &str, ok: bool| {
        failed.check(&format!("fallback_coverage:{name}"), ok, || {
            "section equals its collect() fallback — the fixture (or a v5 collector) is blind"
                .into()
        });
    };
    check(
        "databaseSecurity",
        d.database_security.passphrase_protected && !d.database_security.databases.is_empty(),
    );
    check(
        "backupStatus",
        d.backup_status.iter().all(|b| b.count > 0.0),
    );
    check("migrationState", d.migration_state.applied_count > 0.0);
    check(
        "plugins_db_counts",
        d.plugins.plugin_config_rows > 0.0 && d.plugins.character_plugin_data_rows > 0.0,
    );
    check("apiKeyTypes", !d.api_key_types.is_empty());
    check("providers", !d.providers.is_empty());
    check("modelsByProvider", !d.models_by_provider.is_empty());
    check("providerModelCache", !d.provider_model_cache.is_empty());
    check("apiKeyUsage", d.api_key_usage.len() >= 2);
    check("cheapLLM", d.cheap_llm.provider.is_some());
    check("imagePromptLLM", d.image_prompt_llm.provider.is_some());
    check("embeddingProvider", d.embedding_provider.provider.is_some());
    check("imageProviders", !d.image_providers.is_empty());
    check("embeddingProviders", !d.embedding_providers.is_empty());
    check(
        "mcpServers",
        d.mcp_servers.configured > 0.0 && d.mcp_servers.enabled > 0.0,
    );
    check(
        "themeInfo",
        d.theme_info.active_theme_id.is_some() && d.theme_info.color_mode == "dark",
    );
    check(
        "databaseStats",
        d.database_stats.characters > 0.0
            && d.database_stats.connection_profiles.web_search_enabled > 0.0,
    );
    check(
        "chatStats",
        d.chat_stats.total_messages > 0.0
            && d.chat_stats.agent_mode_chats > 0.0
            && d.chat_stats.dangerous_chats > 0.0,
    );
    check(
        "chatBreakdown",
        !d.chat_breakdown.by_type.is_empty()
            && d.chat_breakdown.multi_document_chats > 0.0
            && d.chat_breakdown.paused_chats > 0.0
            && d.chat_breakdown.narrative_timeline_chats > 0.0
            && d.chat_breakdown.chats_with_non_empty_state > 0.0,
    );
    check(
        "autonomousRooms",
        d.autonomous_rooms.total > 0.0
            && d.autonomous_rooms.overdue > 0.0
            && d.autonomous_rooms.scheduled > 0.0
            && d.autonomous_rooms.destructive_tools_allowed > 0.0,
    );
    check(
        "memoryBreakdown",
        d.memory_breakdown.total > 0.0
            && d.memory_breakdown.with_occurred_at > 0.0
            && d.memory_breakdown.with_entities > 0.0
            && d.memory_breakdown.with_embedding > 0.0
            && !d.memory_breakdown.by_kind.is_empty(),
    );
    check(
        "characterBreakdown",
        d.character_breakdown.total > 0.0
            && d.character_breakdown.vaultless > 0.0
            && d.character_breakdown.npcs > 0.0
            && d.character_breakdown.carina_answerers > 0.0
            && d.character_breakdown.core_whisper_overrides > 0.0,
    );
    check(
        "featureConfig",
        d.feature_config.dangerous_content.mode == "DETECT_ONLY"
            && d.feature_config.text_replacements.rules > 0.0,
    );
    check(
        "instanceSettings",
        d.instance_settings.stale_chat_days == 45.0
            && d.instance_settings.chats_eligible_for_next_sweep > 0.0
            && d.instance_settings.last_maintenance_sweep_at.is_some()
            && d.instance_settings.max_concurrent_jobs == 6.0,
    );
    check(
        "backgroundJobs",
        !d.background_jobs.by_status.is_empty()
            && !d.background_jobs.failed.is_empty()
            && d.background_jobs.attempts_exhausted > 0.0
            && d.background_jobs.oldest_pending_scheduled_at.is_some(),
    );
    check(
        "embeddingPipeline",
        !d.embedding_pipeline.status_by_entity_type.is_empty()
            && d.embedding_pipeline.conversation_chunks.total > 0.0
            && d.embedding_pipeline.conversation_chunks.unembedded > 0.0
            && d.embedding_pipeline.help_docs.total > 0.0
            && !d.embedding_pipeline.stored_dimensions.is_empty()
            && d.embedding_pipeline.dimension_mismatch,
    );
    check(
        "terminal",
        d.terminal.total_sessions == 3.0
            && d.terminal.live_sessions > 0.0
            && d.terminal.non_zero_exits > 0.0
            && d.terminal.distinct_shells.len() == 2,
    );
    check(
        "storageStats",
        d.storage_stats.total_files > 0.0
            && d.storage_stats.not_ok_files > 0.0
            && !d.storage_stats.generated_images_by_model.is_empty()
            && d.storage_stats.folders.len() > 1,
    );
    let s = &d.scriptorium;
    check(
        "scriptorium",
        s.available
            && s.content.file_rows > 0.0
            && s.content.blob_rows > 0.0
            && s.content.chunk_rows > 0.0
            && s.mount_points.total > 0.0
            && s.mount_points.scan_errors > 0.0
            && !s.mount_points.well_known.is_empty(),
    );
    check(
        "scriptorium_links",
        s.links.hard_link_groups > 0.0
            && s.links.policy_embed_denied > 0.0
            && s.links.policy_character_read_denied > 0.0
            && s.links.policy_character_write_denied > 0.0
            && s.links.extraction_errors > 0.0
            && s.links.conversion_errors > 0.0,
    );
    check(
        "scriptorium_vaults",
        s.character_vaults.total > 0.0
            && s.character_vaults.with_keystone > 0.0
            && s.character_vaults.with_metadata > 0.0,
    );
    check(
        "scriptorium_wardrobe",
        s.wardrobe.iter().all(|t| t.items > 0.0) && s.wardrobe.iter().any(|t| t.archived > 0.0),
    );
    check(
        "scriptorium_custom_tools",
        s.custom_tools.total > 0.0
            && s.custom_tools.parse_failures > 0.0
            && s.custom_tools.with_llm_consult > 0.0
            && s.custom_tools.with_effects > 0.0
            && s.custom_tools.metadata_gated > 0.0
            && s.custom_tools.with_presets > 0.0,
    );
    check(
        "scriptorium_post_office",
        s.post_office.letters > 0.0
            && s.post_office.unannounced > 0.0
            && s.post_office.mailboxes > 0.0,
    );
    check(
        "scriptorium_photos",
        s.photos.character_vault_photos > 0.0
            && s.photos.character_vault_bytes > 0.0
            && s.photos.user_gallery_photos > 0.0,
    );
    check(
        "scriptorium_scenarios",
        s.scenarios.iter().all(|t| t.count > 0.0),
    );
    check(
        "scriptorium_state",
        s.state_cascade.chats_with_state > 0.0
            && s.state_cascade.projects_with_state > 0.0
            && s.state_cascade.groups_with_state > 0.0
            && s.state_cascade.general_state_present,
    );
    check(
        "personae",
        !d.personae.top_characters.is_empty()
            && !d.personae.projects.is_empty()
            && d.personae
                .groups
                .iter()
                .any(|g| g.members.iter().any(|m| m == "(missing character)")),
    );
    let w = &d.wire_records;
    check(
        "wireRecords",
        w.total_entries > 0.0
            && w.token_usage.total_tokens > 0.0
            && !w.by_type.is_empty()
            && !w.connection_profile_lifetime.is_empty()
            && !w.connection_profile_window.is_empty()
            && !w.image_profile_window.is_empty()
            && !w.cache_by_provider.is_empty()
            && !w.cache_by_profile.is_empty()
            && w.retention_days == 45.0
            && w.verbose_mode,
    );
    check(
        "wireRecords_deleted_profile_label",
        w.connection_profile_window
            .iter()
            .any(|r| r.label.starts_with("(deleted profile ")),
    );
}

// Silence the unused-path warning when the env var is unset (SKIP path).
#[allow(dead_code)]
fn _keep(_: &Path) {}
