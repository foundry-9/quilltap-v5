//! Differential for the doc-edit OPACITY COVENANT (P4.D200 — v4 bugs 152
//! `1065a1f53` and 153 `89fcc3c0d`).
//!
//! Drives the ported `build_{read,write}_resolution_context` /
//! `resolve_doc_edit_path` / `get_accessible_mount_points` /
//! `execute_doc_edit_tool` / `flatten_tier_pool` over the SAME real two-partition
//! fixture the oracle drives v4's real functions over
//! (`harness/oracle/cases/doc-opacity.test.ts`).
//!
//! **Why a new family rather than rows on `doc_edit_path_resolver`:** the bug lived
//! in the COMPOSITION — the resolution-context builders, the tiered pool's group
//! tier, and the enumerator — and v4's own two regression suites reach it only with
//! the repositories mocked. This family uses real repositories over a real DB, so
//! "an opaque character keeps her group stores" (152) and "and is not shown a vault
//! she cannot open" (153) are proven against the same state a real instance has.
//!
//! **The comparison is EXACT on names, codes and messages**, which is where every
//! discriminator lives: the fixture is built ONCE and copied to both sides, so ids
//! are shared, and the UUID/timestamp normalizer (the doc-blob one) only collapses
//! the ids the blob WRITE ops mint. A store NAME, an error CODE and a refusal
//! MESSAGE all survive normalization untouched.
//!
//! **P4.100 closed the four gaps the `89fcc3c0d` §3 review left "structurally
//! covered but unexercised":** the fixture's `groupLinkedMountPointId` (a store
//! LINKED, not official, to the group), and three ops —
//! `blob_read_own_vault_by_name`, `group_linked_store_opaque_read`,
//! `write_peer_vault_transparent` — plus a fourth check with no oracle
//! counterpart (the ACCESS_DENIED warn's two-character `characters:` field,
//! pinned via the capture layer right after the main loop). Landing
//! `write_peer_vault_transparent` required WRAPPING the oracle's
//! `buildWriteResolutionContext` calls (`resolve_write`/`context_write`) — until
//! this row, nothing made that builder throw, so the throw was left unwrapped
//! and would have ABORTED the whole oracle case.
//!
//! **P4.D216 (v4 `d1c06cd9d`) — the pre-built mount pool.** A `pool` actor is a
//! character-less, project-less context carrying a hand-built pool of the
//! fixture's REAL ids (the Scenario Builder's shape: both cast vaults in the
//! participant tier, both group stores, the project store, Quilltap General, no
//! character tier). Its ops prove the resolver's pool arm (the participant vaults
//! ADMITTED — v4's `includeParticipants: true` — the covenant bypassed, the
//! stranger store still refused, the `self` token measured), the write builder,
//! the enumerator (incl. a forced covenant flag the pool arm ignores), and the
//! three handler guards (`doc_list_files` with the group tier tagged from the
//! pool and no project branch, `doc_grep`, the two blob resolvers). Each pool op
//! also compares the `DocEdit:PathResolver` log lines field for field (the new
//! `pre-built mount pool` DEBUG among them). A `nobody` actor (no pool, no
//! project, no character) is the guards' refusal leg.
//!
//! Regen (Node 24). The fixture pair is MINTED per run — rebuild, regenerate,
//! THEN `cargo test` against that SAME build, in that order. The sweep driver is
//! the sanctioned path (`recipe_sweep.py --run doc_opacity_equivalence --v4
//! <pin>`); it supplies the checkout, so this header never names one.
//!
//!     N=~/.nvm/versions/node/v24.13.1/bin ; W=${V5W:-$(git rev-parse --show-toplevel)}
//!     STAGE=/tmp/qt-oracle-stage-doc-opacity
//!     rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!     cp $W/harness/oracle/cases/doc-opacity.test.ts $STAGE/harness/oracle/cases/
//!     cp $W/harness/oracle/fixtures/doc-opacity.json $STAGE/harness/oracle/fixtures/
//!     cd ~/source/quilltap-server        # or a worktree pinned at the baseline
//!     rm -f /tmp/qt-dopa-main.db /tmp/qt-dopa-mount.db
//!     QT_FIXTURE_DOPA_MAIN=/tmp/qt-dopa-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-dopa-mount.db \
//!     $N/node --import tsx $W/harness/oracle/fixtures/build-doc-opacity-fixture.ts
//!     QT_FIXTURE_DOPA_MAIN=/tmp/qt-dopa-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-dopa-mount.db \
//!     QT_ORACLE_OUT=/tmp/oracle-doc-opacity.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!     --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "doc-opacity\.test\.ts$"
//!
//! Then:
//!
//!     QT_ORACLE_DOPA=/tmp/oracle-doc-opacity.ndjson \
//!     QT_FIXTURE_DOPA_MAIN=/tmp/qt-dopa-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-dopa-mount.db \
//!     cargo test -p quilltap-harness --test doc_opacity_equivalence -- --nocapture

use quilltap_core::db::tiered_mount_pool::{flatten_tier_pool, FlattenOptions, TieredMountPool};
use quilltap_core::db::Writer;
use quilltap_core::doc_edit::path_resolver::{
    resolve_doc_edit_path, PathResolutionContext, ResolveError,
};
use quilltap_core::doc_edit::DocEditScope;
use quilltap_core::tools::doc_edit::shared::{
    acting_character_is_opaque_to_vaults, build_read_resolution_context,
    build_write_resolution_context, collect_peer_character_ids_for_reads,
    get_accessible_mount_points, AccessibleMountPoint, AccessibleMountPointsQuery, Addressing,
};
use quilltap_core::tools::doc_edit::{
    execute_doc_edit_tool, format_doc_edit_results, DocEditToolContext,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlattenPoolSpec {
    character_mount_point_id: String,
    participant_mount_point_ids: Vec<String>,
    group_mount_point_ids: Vec<String>,
    project_mount_point_ids: Vec<String>,
    global_mount_point_id: String,
}

/// P4.D216: the `pool` actor's pre-built pool (vault placeholders resolved at run).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrebuiltPoolSpec {
    character_mount_point_id: Option<String>,
    participant_mount_point_ids: Vec<String>,
    group_mount_point_ids: Vec<String>,
    project_mount_point_ids: Vec<String>,
    global_mount_point_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Op {
    name: String,
    kind: String,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    mount_point: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    args: Option<Value>,
    #[serde(default)]
    opts: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    leilani_id: String,
    abigail_id: String,
    project_id: String,
    chat_id: String,
    group_official_mount_point_id: String,
    prebuilt_pool: PrebuiltPoolSpec,
    flatten_pool: FlattenPoolSpec,
    ops: Vec<Op>,
}

// ---------------------------------------------------------------------------
// Normalization (the doc-blob normalizer: positional `<id-N>` UUIDs + `<ts>`).
// Names / codes / messages are untouched, which is where the discriminators are.
// ---------------------------------------------------------------------------

fn normalize(v: &Value) -> Value {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut counter = 0usize;
    walk(v, &mut map, &mut counter)
}

fn walk(v: &Value, map: &mut HashMap<String, String>, counter: &mut usize) -> Value {
    match v {
        Value::String(s) => Value::String(normalize_text(s, map, counter)),
        Value::Array(a) => Value::Array(a.iter().map(|e| walk(e, map, counter)).collect()),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, val) in o {
                m.insert(k.clone(), walk(val, map, counter));
            }
            Value::Object(m)
        }
        other => other.clone(),
    }
}

fn normalize_text(s: &str, map: &mut HashMap<String, String>, counter: &mut usize) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some(len) = iso_ts_len(&bytes[i..]) {
            out.push_str("<ts>");
            i += len;
        } else if uuid_at(&bytes[i..]) {
            let raw = &s[i..i + 36];
            let token = map
                .entry(raw.to_string())
                .or_insert_with(|| {
                    let t = format!("<id-{}>", *counter);
                    *counter += 1;
                    t
                })
                .clone();
            out.push_str(&token);
            i += 36;
        } else {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn uuid_at(b: &[u8]) -> bool {
    if b.len() < 36 {
        return false;
    }
    let dashes = [8, 13, 18, 23];
    for (i, &c) in b[..36].iter().enumerate() {
        if dashes.contains(&i) {
            if c != b'-' {
                return false;
            }
        } else if !c.is_ascii_hexdigit() {
            return false;
        }
    }
    if b.len() > 36 && (b[36].is_ascii_hexdigit() || b[36] == b'-') {
        return false;
    }
    true
}

fn iso_ts_len(b: &[u8]) -> Option<usize> {
    const LEN: usize = 24;
    if b.len() < LEN {
        return None;
    }
    let pat = b"####-##-##T##:##:##.###Z";
    for (i, &p) in pat.iter().enumerate() {
        let c = b[i];
        let ok = match p {
            b'#' => c.is_ascii_digit(),
            other => c == other,
        };
        if !ok {
            return None;
        }
    }
    Some(LEN)
}

// ---------------------------------------------------------------------------

fn resolve_row(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    ctx: &PathResolutionContext,
    path: &str,
) -> Value {
    match resolve_doc_edit_path(
        main,
        mount,
        DocEditScope::DocumentStore,
        Some(path),
        ctx,
        None,
    ) {
        Ok(r) => json!({
            "ok": true,
            "mountPointId": r.mount_point_id,
            "mountPointName": r.mount_point_name,
            "mountType": r.mount_type,
            "relativePath": r.relative_path,
        }),
        Err(ResolveError::Path { message, code }) => json!({
            "ok": false,
            "code": code.as_str(),
            "message": message,
        }),
        Err(ResolveError::FsSeam) => json!({
            "ok": false,
            "code": "FS_SEAM",
            "message": "host filesystem unavailable",
        }),
    }
}

/// The context shape, canonicalized exactly as the oracle canonicalizes v4's
/// (which OMITS `characterIds` / `hideCharacterVaults` / `operatorOverride` when
/// undefined). The DISCRIMINATING fields — `characterId` (null before bug 152's
/// fix, set after) and `hideCharacterVaults` (absent before, true after) — are not
/// masked by the canonicalization.
fn shape_of(c: &PathResolutionContext) -> Value {
    json!({
        "projectId": c.project_id,
        "characterId": c.character_id,
        "characterIds": c.character_ids,
        "hideCharacterVaults": c.hide_character_vaults,
        "mountPoint": c.mount_point,
        "operatorOverride": c.operator_override,
        // P4.D216: the pool must ride the builders into the resolver's context.
        "mountPool": c.mount_pool,
    })
}

// ---------------------------------------------------------------------------
// P4.D216: a STRUCTURAL capture of the path resolver's lines (level, message,
// fields by name), compared with v4's `{ level, message, context }`.
// ---------------------------------------------------------------------------

const RESOLVER_TARGET: &str = "quilltap_core::doc_edit::path_resolver";

/// `(level, message, sorted (field, value) pairs)`.
type Line = (String, String, Vec<(String, String)>);

struct LineVisitor(String, Vec<(String, String)>);
impl tracing::field::Visit for LineVisitor {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.1.push((f.name().to_string(), v.to_string()));
    }
    fn record_i64(&mut self, f: &tracing::field::Field, v: i64) {
        self.1.push((f.name().to_string(), v.to_string()));
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        self.1.push((f.name().to_string(), v.to_string()));
    }
    fn record_bool(&mut self, f: &tracing::field::Field, v: bool) {
        self.1.push((f.name().to_string(), v.to_string()));
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        if f.name() == "message" {
            self.0 = format!("{v:?}");
        } else {
            self.1.push((f.name().to_string(), format!("{v:?}")));
        }
    }
}

struct ResolverCapture(std::sync::Arc<std::sync::Mutex<Vec<Line>>>);
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for ResolverCapture {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        if meta.target() != RESOLVER_TARGET {
            return;
        }
        let mut v = LineVisitor(String::new(), Vec::new());
        event.record(&mut v);
        v.1.sort();
        self.0
            .lock()
            .unwrap()
            .push((meta.level().to_string().to_lowercase(), v.0, v.1));
    }
}

/// v4's recorded lines in the capture's shape (every context value as text).
fn v4_lines(v: &Value) -> Vec<Line> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|l| {
                    let mut fields: Vec<(String, String)> = l["context"]
                        .as_object()
                        .map(|o| {
                            o.iter()
                                .map(|(k, v)| {
                                    let text = match v {
                                        Value::String(s) => s.clone(),
                                        other => other.to_string(),
                                    };
                                    (k.clone(), text)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    fields.sort();
                    (
                        l["level"].as_str().unwrap_or_default().to_string(),
                        l["message"].as_str().unwrap_or_default().to_string(),
                        fields,
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_oracle(text: &str) -> Vec<Value> {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .expect("oracle NDJSON is empty");
    let row: Value = serde_json::from_str(line).expect("parse oracle line");
    row.get("ops")
        .and_then(Value::as_array)
        .expect("oracle row has no `ops`")
        .clone()
}

#[test]
fn doc_opacity_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DOPA") else {
        eprintln!("SKIP: set QT_ORACLE_DOPA to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_DOPA_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_DOPA_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_DOPA_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_DOPA_MOUNT to the seed mount-index .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/doc-opacity.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let oracle_ops = parse_oracle(&oracle_text);
    assert_eq!(
        oracle_ops.len(),
        spec.ops.len(),
        "oracle op count {} != spec op count {}",
        oracle_ops.len(),
        spec.ops.len()
    );
    // Coverage floor — v4's two regression suites are 13 + 15 cases.
    assert!(
        spec.ops.len() >= 28,
        "the matrix must mirror v4's 28 regression cases; got {}",
        spec.ops.len()
    );

    let scratch = std::env::temp_dir().join(format!("qt-dopa-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("dopa-main.db");
    let work_mount = scratch.join("dopa-mount.db");
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    let main_w = Writer::open_writable(&work_main, &spec.test_pepper_base64).expect("open main");
    let mount_w = Writer::open_writable(&work_mount, &spec.test_pepper_base64).expect("open mount");
    let main = main_w.connection();
    let mount = mount_w.connection();

    // ---- the MINTED placeholders, read back (never transcribed) ----
    let leilani_vault = quilltap_core::doc_edit::path_resolver::resolve_self_vault_mount_point_id(
        main,
        Some(&spec.leilani_id),
    )
    .expect("leilani vault minted");
    let abigail_vault = quilltap_core::doc_edit::path_resolver::resolve_self_vault_mount_point_id(
        main,
        Some(&spec.abigail_id),
    )
    .expect("abigail vault minted");
    let repo = quilltap_core::db::doc_mount_points::DocMountPointsRepository::new(mount);
    let name_of = |id: &str| -> String {
        repo.find_by_id_for_docedit(id)
            .unwrap_or_else(|e| panic!("read mount {id}: {e}"))
            .unwrap_or_else(|| panic!("mount {id} missing"))
            .name
    };
    let subs: HashMap<&str, String> = HashMap::from([
        ("{{leilaniVaultId}}", leilani_vault.clone()),
        ("{{abigailVaultId}}", abigail_vault.clone()),
        ("{{leilaniVaultName}}", name_of(&leilani_vault)),
        ("{{abigailVaultName}}", name_of(&abigail_vault)),
        (
            "{{groupStoreId}}",
            spec.group_official_mount_point_id.clone(),
        ),
    ]);
    let sub = |v: &str| -> String { subs.get(v).cloned().unwrap_or_else(|| v.to_string()) };
    let sub_args = |args: &Value| -> Value {
        match args {
            Value::Object(o) => {
                let mut m = Map::new();
                for (k, val) in o {
                    m.insert(
                        k.clone(),
                        match val {
                            Value::String(s) => Value::String(sub(s)),
                            other => other.clone(),
                        },
                    );
                }
                Value::Object(m)
            }
            other => other.clone(),
        }
    };

    // P4.D216: the pre-built pool, its vault placeholders read back.
    let prebuilt_pool = TieredMountPool {
        character_mount_point_id: spec.prebuilt_pool.character_mount_point_id.clone(),
        participant_mount_point_ids: spec
            .prebuilt_pool
            .participant_mount_point_ids
            .iter()
            .map(|v| sub(v))
            .collect(),
        group_mount_point_ids: spec
            .prebuilt_pool
            .group_mount_point_ids
            .iter()
            .map(|v| sub(v))
            .collect(),
        project_mount_point_ids: spec
            .prebuilt_pool
            .project_mount_point_ids
            .iter()
            .map(|v| sub(v))
            .collect(),
        global_mount_point_id: spec.prebuilt_pool.global_mount_point_id.clone(),
    };
    let ctx_for = |actor: &str| {
        let character = match actor {
            "pool" | "nobody" => None,
            "abigail" => Some(spec.abigail_id.clone()),
            _ => Some(spec.leilani_id.clone()),
        };
        DocEditToolContext {
            chat_id: spec.chat_id.clone(),
            user_id: spec.user_id.clone(),
            project_id: character.as_ref().map(|_| spec.project_id.clone()),
            character_id: character,
            operator_override: false,
            files_dir: None,
            blob_webp: Default::default(),
            mount_pool: (actor == "pool").then(|| prebuilt_pool.clone()),
        }
    };

    use tracing_subscriber::layer::SubscriberExt;
    let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Line>::new()));
    let _capture_guard = tracing::subscriber::set_default(
        tracing_subscriber::registry().with(ResolverCapture(captured.clone())),
    );
    let mut pool_ops = 0usize;

    let pool = TieredMountPool {
        character_mount_point_id: Some(spec.flatten_pool.character_mount_point_id.clone()),
        participant_mount_point_ids: spec.flatten_pool.participant_mount_point_ids.clone(),
        group_mount_point_ids: spec.flatten_pool.group_mount_point_ids.clone(),
        project_mount_point_ids: spec.flatten_pool.project_mount_point_ids.clone(),
        global_mount_point_id: Some(spec.flatten_pool.global_mount_point_id.clone()),
    };

    let mut mismatches: Vec<String> = Vec::new();

    for (i, op) in spec.ops.iter().enumerate() {
        let actor = op.actor.clone().unwrap_or_else(|| "leilani".to_string());
        let ctx = ctx_for(&actor);
        let mount_point = op.mount_point.as_deref().map(sub);
        let addressing = Addressing {
            scope: Some("document_store".to_string()),
            mount_point: mount_point.clone(),
            path: op.path.clone(),
            ..Default::default()
        };

        let result: Value = match op.kind.as_str() {
            "resolve_read" => {
                let rc = build_read_resolution_context(main, &addressing, &ctx);
                resolve_row(main, mount, &rc, op.path.as_deref().unwrap())
            }
            "resolve_write" => match build_write_resolution_context(main, mount, &addressing, &ctx)
            {
                Ok(rc) => resolve_row(main, mount, &rc, op.path.as_deref().unwrap()),
                Err(message) => json!({ "ok": false, "code": "ACCESS_DENIED", "message": message }),
            },
            "context_read" => shape_of(&build_read_resolution_context(main, &addressing, &ctx)),
            "context_write" => match build_write_resolution_context(main, mount, &addressing, &ctx)
            {
                Ok(rc) => shape_of(&rc),
                Err(message) => json!({ "error": message }),
            },
            "accessible" => {
                let peers = collect_peer_character_ids_for_reads(main, &ctx);
                let hide = acting_character_is_opaque_to_vaults(main, &ctx);
                // P4.D216: an op may force the covenant flag on — a pre-built
                // pool must ignore it (the pool arm runs before the covenant).
                let forced = op
                    .opts
                    .as_ref()
                    .and_then(|o| o.get("hideCharacterVaults"))
                    .and_then(Value::as_bool)
                    .unwrap_or(hide);
                let stores = accessible_for(main, mount, &ctx, &peers, forced);
                json!({
                    "hideCharacterVaults": hide,
                    "peers": peers,
                    "stores": stores.iter().map(|m| json!({
                        "id": m.id, "name": m.name, "mountType": m.mount_type,
                    })).collect::<Vec<_>>(),
                })
            }
            "agreement" => {
                let peers = collect_peer_character_ids_for_reads(main, &ctx);
                let hide = acting_character_is_opaque_to_vaults(main, &ctx);
                let stores = accessible_for(main, mount, &ctx, &peers, hide);
                let rows: Vec<Value> = stores
                    .iter()
                    .map(|m| {
                        let a = Addressing {
                            scope: Some("document_store".to_string()),
                            mount_point: Some(m.name.clone()),
                            path: Some("notes.md".to_string()),
                            ..Default::default()
                        };
                        let rc = build_read_resolution_context(main, &a, &ctx);
                        let r = resolve_row(main, mount, &rc, "notes.md");
                        json!({
                            "name": m.name,
                            "opens": r.get("ok") == Some(&Value::Bool(true)),
                            "resolvedId": r.get("mountPointId").cloned().unwrap_or(Value::Null),
                        })
                    })
                    .collect();
                Value::Array(rows)
            }
            "tool" => {
                let args = sub_args(op.args.as_ref().unwrap_or(&Value::Null));
                let r =
                    execute_doc_edit_tool(main, mount, op.tool.as_deref().unwrap(), &args, &ctx);
                json!({
                    "output": serde_json::to_value(&r).expect("serialize result"),
                    "formatted": format_doc_edit_results(&r),
                })
            }
            "flatten" => {
                let opts = op.opts.clone().unwrap_or_else(|| json!({}));
                let include_participants = opts
                    .get("includeParticipants")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let include_character_tier = opts
                    .get("includeCharacterTier")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                Value::Array(
                    flatten_for(&pool, include_participants, include_character_tier)
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                )
            }
            other => panic!("unknown op kind: {other}"),
        };

        let got = normalize(&json!({
            "name": op.name,
            "kind": op.kind,
            "actor": actor,
            "result": result,
        }));
        // P4.D216: a pool op's path-resolver lines are compared on their own
        // (field values as text); every other op's lines are dropped, as v4's
        // oracle drops them.
        let got_lines: Vec<Line> = std::mem::take(&mut *captured.lock().unwrap());
        let mut want_raw = oracle_ops[i].clone();
        if actor == "pool" {
            pool_ops += 1;
            let want_lines = v4_lines(&normalize(want_raw.get("logs").unwrap_or(&Value::Null)));
            let got_norm: Vec<Line> = got_lines
                .into_iter()
                .map(|(l, m, f)| {
                    let norm = |t: &str| {
                        normalize(&Value::String(t.to_string()))
                            .as_str()
                            .unwrap()
                            .to_string()
                    };
                    (
                        l,
                        norm(&m),
                        f.into_iter().map(|(k, v)| (k, norm(&v))).collect(),
                    )
                })
                .collect();
            // v5's refusal WARNs carry structured fields v4 bakes into the
            // message (a v5 convenience, P4.100); a v4 line with NO context is
            // compared on level + message alone.
            let got_norm: Vec<Line> = got_norm
                .into_iter()
                .zip(want_lines.iter().map(Some).chain(std::iter::repeat(None)))
                .map(|((l, m, f), w)| match w {
                    Some((_, _, wf)) if wf.is_empty() => (l, m, Vec::new()),
                    _ => (l, m, f),
                })
                .collect();
            if got_norm != want_lines {
                mismatches.push(format!(
                    "op[{i}] {}: path-resolver log lines diverge\n  rust:   {got_norm:?}\n  oracle: {want_lines:?}",
                    op.name
                ));
            }
            if let Some(o) = want_raw.as_object_mut() {
                o.remove("logs");
            }
        }
        let mut want = normalize(&want_raw);
        // P4.D216: the pool ops list AFTER the run's own blob writes, whose
        // `modified` is the write's epoch-ms on each side (a minted clock the ISO
        // normalizer cannot see). Blanked on BOTH sides for pool ops only — the
        // discriminators (path, store, scope tag) are untouched.
        let mut got = got;
        if actor == "pool" {
            blank_modified(&mut got);
            blank_modified(&mut want);
        }
        if got != want {
            mismatches.push(format!(
                "op[{i}] {}\n  rust:   {got}\n  oracle: {want}",
                op.name
            ));
        }
    }

    // P4.100 item iii: the ACCESS_DENIED warn's `characters:` field for a
    // TWO-CHARACTER context — proven end to end through the REAL resolver.
    // The prior mutation (M7) only reddened `describe_characters`'s own
    // six-case unit table; nothing drove the join through the capture layer.
    // Abigail (transparent) admits Leilani as a peer under the chat's
    // cross-character reads, so a refused read against the out-of-scope
    // stranger store carries BOTH ids — no v4 oracle counterpart (this is a
    // v5 tracing convenience, not part of the compared Result), so it is
    // pinned here rather than as an `ops` row.
    {
        let abigail_ctx = ctx_for("abigail");
        let peers = collect_peer_character_ids_for_reads(main, &abigail_ctx);
        assert_eq!(
            peers,
            vec![spec.leilani_id.clone()],
            "the premise: Abigail's peer collection admits Leilani"
        );
        let addressing = Addressing {
            scope: Some("document_store".to_string()),
            mount_point: Some("Someone Elses Papers".to_string()),
            path: Some("notes.md".to_string()),
            ..Default::default()
        };
        let rc = build_read_resolution_context(main, &addressing, &abigail_ctx);
        let (result, logs) = quilltap_core::test_support::captured_with(|| {
            resolve_row(main, mount, &rc, "notes.md")
        });
        assert_eq!(
            result.get("ok"),
            Some(&Value::Bool(false)),
            "the premise: the stranger store is out of scope for Abigail too"
        );
        let warn = logs
            .iter()
            .find(|l| l.contains("Mount point exists but is out of scope"))
            .unwrap_or_else(|| panic!("no ACCESS_DENIED warn captured; logs: {logs:?}"));
        let expected_field = format!("characters={},{}", spec.abigail_id, spec.leilani_id);
        assert!(
            warn.contains(&expected_field),
            "the warn's `characters:` field must join BOTH acting ids: {warn}"
        );
    }

    assert!(
        mismatches.is_empty(),
        "{} of {} ops diverged:\n\n{}",
        mismatches.len(),
        spec.ops.len(),
        mismatches.join("\n\n")
    );
    assert!(
        pool_ops >= 20,
        "the P4.D216 pool arms must run; only {pool_ops} did"
    );
    eprintln!(
        "doc_opacity: {} ops matched the oracle ({pool_ops} pre-built-pool ops, their resolver lines included).",
        spec.ops.len()
    );
}

/// The enumeration the `accessible` and `agreement` ops share — the same query
/// the four production call sites build.
fn accessible_for(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    ctx: &DocEditToolContext,
    peers: &[String],
    hide: bool,
) -> Vec<AccessibleMountPoint> {
    get_accessible_mount_points(
        main,
        mount,
        AccessibleMountPointsQuery {
            project_id: ctx.project_id.as_deref(),
            character_id: ctx.character_id.as_deref(),
            extra_character_ids: peers,
            hide_character_vaults: hide,
            mount_pool: ctx.mount_pool.as_ref(),
        },
    )
}

/// Replace every numeric `modified` value with `"<ms>"`.
fn blank_modified(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for (k, val) in o.iter_mut() {
                if k == "modified" && val.is_number() {
                    *val = Value::String("<ms>".to_string());
                } else {
                    blank_modified(val);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(blank_modified),
        _ => {}
    }
}

fn flatten_for(
    pool: &TieredMountPool,
    include_participants: bool,
    include_character_tier: bool,
) -> Vec<String> {
    flatten_tier_pool(
        pool,
        FlattenOptions {
            include_participants,
            include_character_tier,
            ..Default::default()
        },
    )
}
