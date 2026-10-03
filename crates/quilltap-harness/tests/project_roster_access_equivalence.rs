//! Differential for the PROJECT ROSTER ACCESS gate (P4.D245 — v4 `9753d0eb2`,
//! `lib/projects/roster-access.ts`): the project roster as a TOOL-ACCESS gate
//! through one chokepoint, wired at v4's seven sites.
//!
//! Drives the ported `project_roster_admits` / `roster_gated_project_id` /
//! `get_accessible_mount_points` / `resolve_doc_edit_path` /
//! `execute_doc_edit_tool` / `resolve_shared_wardrobe_tiers_for_chat` /
//! `wardrobe_list::execute` over the SAME real two-partition fixture the oracle
//! drives v4's real functions over (`harness/oracle/cases/project-roster-access.
//! test.ts`), whose projects set `allowAnyCharacter` EXPLICITLY: Closed (`false`,
//! roster [Ada]), Open (`true`), Broken (`false`, its store wiped after the build
//! — the overlay's `properties.json missing`, v4's OUTER fail-closed line).
//!
//! **Why a new family:** v4 `9753d0eb2` flipped the create default to `true`, so
//! every existing minted fixture now bakes its projects OPEN at the target pin and
//! no existing arm can red the gate (its builders set `true` explicitly since
//! P4.D245 item 2). v4's own `roster-access.test.ts` mocks the repositories; this
//! family runs the real policy over real stores — the composition the gate lives
//! in.
//!
//! **Every op compares its LOG LINES too** — v4's `DocEdit:PathResolver` service
//! lines against v5's `quilltap_core::doc_edit::path_resolver` target, and v4's
//! ROOT-logger gate lines (`[ProjectRoster] Tool access check`, the `[Wardrobe]`
//! withheld DEBUG, the repository's two fail-closed `safeQuery` lines) against
//! v5's `project_roster_access` / `wardrobe_tiers` / `quilltap::db` targets —
//! level, message and fields by name. The line COUNT is the only witness for two
//! of v4's hunks (`handleOpenDocument` passing `characterId` to the resolver
//! makes an admitted character's new-blank open log TWO `[ProjectRoster]` lines).
//!
//! **Both directions:** the `admits` / `gated_id` ops read `{ absent: true }` from
//! an oracle regenerated at a pin before the module existed and are SKIPPED; the
//! composition arms then redden the ported v5 against that baseline oracle
//! (v4 admitted everyone), and redden unported v5 against the target oracle.
//!
//! Regen (Node 24). The fixture pair is MINTED per run — rebuild, regenerate,
//! THEN `cargo test` against that SAME build, in that order. The sweep driver is
//! the sanctioned path (`recipe_sweep.py --run project_roster_access_equivalence
//! --v4 <pin>`); it supplies the checkout, so this header never names one.
//!
//! The `V5W=${V5W:-$HOME/...}` alias below is the SANCTIONED self-referential literal: `recipe_sweep.py`
//! (`ALIAS_ASSIGN` + `--self-test`) can neutralise only that spelling, so never copy a `W=...$(git ...)` form.
//!
//!     N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!     STAGE=/tmp/qt-oracle-stage-project-roster-access
//!     rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!     cp $V5W/harness/oracle/cases/project-roster-access.test.ts $STAGE/harness/oracle/cases/
//!     cp $V5W/harness/oracle/fixtures/project-roster-access.json $STAGE/harness/oracle/fixtures/
//!     cd ~/source/quilltap-server        # or a worktree pinned at the baseline
//!     rm -f /tmp/qt-pra-main.db /tmp/qt-pra-mount.db
//!     QT_FIXTURE_PRA_MAIN=/tmp/qt-pra-main.db QT_FIXTURE_PRA_MOUNT=/tmp/qt-pra-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-project-roster-access-fixture.ts
//!     QT_FIXTURE_PRA_MAIN=/tmp/qt-pra-main.db QT_FIXTURE_PRA_MOUNT=/tmp/qt-pra-mount.db \
//!     QT_ORACLE_OUT=/tmp/oracle-project-roster-access.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!     --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "project-roster-access\.test\.ts$"
//!
//! Then:
//!
//!     QT_ORACLE_PRA=/tmp/oracle-project-roster-access.ndjson \
//!     QT_FIXTURE_PRA_MAIN=/tmp/qt-pra-main.db QT_FIXTURE_PRA_MOUNT=/tmp/qt-pra-mount.db \
//!     cargo test -p quilltap-harness --test project_roster_access_equivalence -- --nocapture

use quilltap_core::db::Writer;
use quilltap_core::doc_edit::path_resolver::{
    resolve_doc_edit_path, PathResolutionContext, ResolveError,
};
use quilltap_core::doc_edit::DocEditScope;
use quilltap_core::tools::doc_edit::shared::{
    get_accessible_mount_points, AccessibleMountPointsQuery,
};
use quilltap_core::tools::doc_edit::{
    execute_doc_edit_tool, format_doc_edit_results, DocEditToolContext,
};
use quilltap_core::tools::wardrobe_list;
use quilltap_core::wardrobe_tiers::{
    resolve_shared_wardrobe_tiers_for_chat, SharedWardrobeTierOptions,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Op {
    name: String,
    kind: String,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    chat: Option<String>,
    #[serde(default)]
    operator: Option<bool>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    mount_point: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    args: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    ada_id: String,
    bea_id: String,
    closed_project_id: String,
    open_project_id: String,
    broken_project_id: String,
    missing_project_id: String,
    closed_chat_id: String,
    open_chat_id: String,
    no_project_chat_id: String,
    missing_chat_id: String,
    ops: Vec<Op>,
}

// ---------------------------------------------------------------------------
// Normalization (the doc-fs idiom): positional `<id-N>` UUIDs, `<ts>` ISO
// timestamps, `<mtime>` for minted file times, `__ROOT__` for the scratch root.
// Names / codes / messages / scope tags survive, which is where every
// discriminator lives.
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

/// Replace every `mtime`/`modified` value with `<mtime>` (minted on-disk times).
fn placeholder_fs_times(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for (k, val) in o.iter_mut() {
                if (k == "mtime" || k == "modified") && (val.is_number() || val.is_string()) {
                    *val = Value::String("<mtime>".into());
                } else {
                    placeholder_fs_times(val);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(placeholder_fs_times),
        _ => {}
    }
}

/// An ORACLE-side artifact, measured 2026-10-03: under jest, an error thrown by
/// Node's own `fs/promises` is created in the outer realm, so the handler's
/// `error instanceof Error` is FALSE inside the test's VM context and v4 renders
/// `String(error)` — `Error: ENOENT: …` — where plain Node (the production path,
/// probed with the same call) renders `.message`: `ENOENT: …`. v5 renders the
/// production bytes. The prefix is stripped from the oracle's new-blank failure
/// message ONLY (the one place it arises: a new blank document in a project whose
/// official store is a DATABASE store, which v4 then tries to `writeFile('')` —
/// a pre-existing v4 shape both sides share).
fn strip_jest_realm_error_prefix(v: &mut Value) {
    const PREFIX: &str = "Failed to create blank document: Error: ENOENT";
    const FIXED: &str = "Failed to create blank document: ENOENT";
    match v {
        Value::String(s) => {
            if s.contains(PREFIX) {
                *s = s.replace(PREFIX, FIXED);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(strip_jest_realm_error_prefix),
        Value::Object(o) => o.values_mut().for_each(strip_jest_realm_error_prefix),
        _ => {}
    }
}

/// Collapse `mtime: <digits>` inside a formatted string to `mtime: <mtime>`.
fn collapse_mtime_strings(v: &mut Value) {
    fn collapse(s: &str) -> String {
        let needle = "mtime: ";
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(pos) = rest.find(needle) {
            out.push_str(&rest[..pos + needle.len()]);
            let after = &rest[pos + needle.len()..];
            let digits: usize = after.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 {
                out.push_str("<mtime>");
                rest = &after[digits..];
            } else {
                rest = after;
            }
        }
        out.push_str(rest);
        out
    }
    match v {
        Value::String(s) => *s = collapse(s),
        Value::Array(a) => a.iter_mut().for_each(collapse_mtime_strings),
        Value::Object(o) => o.values_mut().for_each(collapse_mtime_strings),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// A STRUCTURAL capture of the four targets' lines (src, level, message, sorted
// (field, value) pairs), compared with v4's `{ src, level, message, context }`.
// ---------------------------------------------------------------------------

const RESOLVER_TARGET: &str = "quilltap_core::doc_edit::path_resolver";
const ROOT_TARGETS: &[&str] = &[
    "quilltap_core::project_roster_access",
    "quilltap_core::wardrobe_tiers",
    "quilltap::db",
];
/// The ROOT-logger lines the gate emits (the oracle records exactly these).
const ROOT_MESSAGES: &[&str] = &[
    "[ProjectRoster] Tool access check",
    "[Wardrobe] Character off project roster — project wardrobe withheld",
    "[Wardrobe] Project lookup for chat failed",
    "Error finding entity by ID",
    "Error checking character participation",
];

/// `(src, level, message, sorted (field, value) pairs)`.
type Line = (String, String, String, Vec<(String, String)>);

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

struct Capture(std::sync::Arc<std::sync::Mutex<Vec<Line>>>);
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        let target = meta.target();
        let src = if target == RESOLVER_TARGET {
            "resolver"
        } else if ROOT_TARGETS.contains(&target) {
            "root"
        } else {
            return;
        };
        let mut v = LineVisitor(String::new(), Vec::new());
        event.record(&mut v);
        if src == "root" && !ROOT_MESSAGES.contains(&v.0.as_str()) {
            return;
        }
        v.1.sort();
        self.0.lock().unwrap().push((
            src.to_string(),
            meta.level().to_string().to_lowercase(),
            v.0,
            v.1,
        ));
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
                        l["src"].as_str().unwrap_or_default().to_string(),
                        l["level"].as_str().unwrap_or_default().to_string(),
                        l["message"].as_str().unwrap_or_default().to_string(),
                        fields,
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn lines_to_value(lines: &[Line]) -> Value {
    Value::Array(
        lines
            .iter()
            .map(|(s, l, m, f)| {
                json!({
                    "src": s, "level": l, "message": m,
                    "context": f
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                        .collect::<Map<String, Value>>(),
                })
            })
            .collect(),
    )
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

fn resolve_row(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    scope: DocEditScope,
    ctx: &PathResolutionContext,
    path: &str,
    files_dir: &std::path::Path,
) -> Value {
    match resolve_doc_edit_path(main, mount, scope, Some(path), ctx, Some(files_dir)) {
        Ok(r) => json!({
            "ok": true,
            "scope": r.scope.as_str(),
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

#[test]
fn project_roster_access_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_PRA") else {
        eprintln!("SKIP: set QT_ORACLE_PRA to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_PRA_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_PRA_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_PRA_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_PRA_MOUNT to the seed mount-index .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/project-roster-access.json"),
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

    // CANONICAL scratch root (realpath), the doc-fs idiom: the new-blank General
    // document lands under `<root>/files`, and the sentinel must cover the prefix
    // the resolver's realpath produces.
    let scratch_raw = std::env::temp_dir().join(format!("qt-pra-rust-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch_raw);
    std::fs::create_dir_all(&scratch_raw).expect("scratch dir");
    let root = std::fs::canonicalize(&scratch_raw).unwrap();
    let files_dir = root.join("files");
    // `files/_general` up front, as the oracle materializes it (see its header).
    std::fs::create_dir_all(files_dir.join("_general")).unwrap();
    let work_main = root.join("pra-main.db");
    let work_mount = root.join("pra-mount.db");
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    let main_w = Writer::open_writable(&work_main, &spec.test_pepper_base64).expect("open main");
    let mount_w = Writer::open_writable(&work_mount, &spec.test_pepper_base64).expect("open mount");
    let main = main_w.connection();
    let mount = mount_w.connection();

    // ---- the MINTED placeholders, read back (never transcribed) ----
    let official_of = |project_id: &str| -> String {
        quilltap_core::db::projects::find_official_mount_point_id_raw(main, project_id)
            .expect("read project")
            .flatten()
            .unwrap_or_else(|| panic!("project {project_id} has no official store"))
    };
    let closed_store_id = official_of(&spec.closed_project_id);
    let open_store_id = official_of(&spec.open_project_id);
    let repo = quilltap_core::db::doc_mount_points::DocMountPointsRepository::new(mount);
    let closed_store_name = repo
        .find_by_id_for_docedit(&closed_store_id)
        .expect("read closed store")
        .expect("closed store minted")
        .name;
    let subs: HashMap<&str, String> = HashMap::from([
        ("{{closedStoreId}}", closed_store_id.clone()),
        ("{{openStoreId}}", open_store_id.clone()),
        ("{{closedStoreName}}", closed_store_name.clone()),
    ]);
    let sub = |v: &str| -> String { subs.get(v).cloned().unwrap_or_else(|| v.to_string()) };

    let project_id_of = |p: Option<&str>| -> Option<String> {
        match p {
            Some("closed") => Some(spec.closed_project_id.clone()),
            Some("open") => Some(spec.open_project_id.clone()),
            Some("broken") => Some(spec.broken_project_id.clone()),
            Some("missing") => Some(spec.missing_project_id.clone()),
            Some("empty") => Some(String::new()),
            Some("none") | None => None,
            Some(other) => panic!("unknown project {other}"),
        }
    };
    let character_id_of = |a: Option<&str>| -> Option<String> {
        match a {
            Some("ada") => Some(spec.ada_id.clone()),
            Some("bea") => Some(spec.bea_id.clone()),
            Some("empty") => Some(String::new()),
            Some("none") | None => None,
            Some(other) => panic!("unknown actor {other}"),
        }
    };
    let chat_id_of = |c: Option<&str>| -> String {
        match c {
            Some("closed") => spec.closed_chat_id.clone(),
            Some("open") => spec.open_chat_id.clone(),
            Some("noproject") => spec.no_project_chat_id.clone(),
            Some("missing") => spec.missing_chat_id.clone(),
            Some("empty") => String::new(),
            None => String::new(),
            Some(other) => panic!("unknown chat {other}"),
        }
    };

    use tracing_subscriber::layer::SubscriberExt;
    let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Line>::new()));
    let _capture_guard = tracing::subscriber::set_default(
        tracing_subscriber::registry().with(Capture(captured.clone())),
    );

    let root_str = root.to_string_lossy().to_string();
    let sentinelize = |v: &Value| -> Value {
        let s = serde_json::to_string(v)
            .unwrap()
            .replace(&root_str, "__ROOT__");
        serde_json::from_str(&s).unwrap()
    };

    let mut mismatches: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    let mut compared = 0usize;

    for (i, op) in spec.ops.iter().enumerate() {
        let project_id = project_id_of(op.project.as_deref());
        let character_id = character_id_of(op.actor.as_deref());
        captured.lock().unwrap().clear();

        let want_raw = oracle_ops[i].clone();
        if want_raw["result"].get("absent") == Some(&Value::Bool(true)) {
            // The oracle ran at a pin before the chokepoint existed: no twin.
            skipped += 1;
            continue;
        }

        let result: Value = match op.kind.as_str() {
            "admits" => json!({
                "allowed": quilltap_core::project_roster_access::project_roster_admits(
                    main, mount, project_id.as_deref(), character_id.as_deref(),
                ),
            }),
            "gated_id" => json!({
                "projectId": quilltap_core::project_roster_access::roster_gated_project_id(
                    main, mount, project_id.as_deref(), character_id.as_deref(),
                ),
            }),
            "accessible" => {
                let stores = get_accessible_mount_points(
                    main,
                    mount,
                    AccessibleMountPointsQuery {
                        project_id: project_id.as_deref(),
                        character_id: character_id.as_deref(),
                        extra_character_ids: &[],
                        hide_character_vaults: false,
                        mount_pool: None,
                    },
                );
                json!({
                    "stores": stores.iter().map(|m| json!({
                        "id": m.id, "name": m.name, "mountType": m.mount_type,
                    })).collect::<Vec<_>>(),
                })
            }
            "resolve" => {
                let scope = match op.scope.as_deref() {
                    Some("project") => DocEditScope::Project,
                    Some("document_store") => DocEditScope::DocumentStore,
                    Some("general") => DocEditScope::General,
                    other => panic!("unknown scope {other:?}"),
                };
                let ctx = PathResolutionContext {
                    project_id: project_id.clone(),
                    character_id: character_id.clone(),
                    mount_point: op.mount_point.as_deref().map(sub),
                    ..Default::default()
                };
                resolve_row(
                    main,
                    mount,
                    scope,
                    &ctx,
                    op.path.as_deref().unwrap(),
                    &files_dir,
                )
            }
            "tool" => {
                let ctx = DocEditToolContext {
                    chat_id: chat_id_of(op.chat.as_deref()),
                    user_id: spec.user_id.clone(),
                    project_id: project_id.clone(),
                    character_id: character_id.clone(),
                    operator_override: false,
                    files_dir: Some(files_dir.clone()),
                    blob_webp: Default::default(),
                    mount_pool: None,
                };
                let args = op.args.clone().unwrap_or_else(|| json!({}));
                let r =
                    execute_doc_edit_tool(main, mount, op.tool.as_deref().unwrap(), &args, &ctx);
                json!({
                    "output": serde_json::to_value(&r).expect("serialize result"),
                    "formatted": format_doc_edit_results(&r),
                })
            }
            "wardrobe_tiers" => {
                let tiers = resolve_shared_wardrobe_tiers_for_chat(
                    main,
                    mount,
                    &chat_id_of(op.chat.as_deref()),
                    character_id.as_deref().unwrap_or(""),
                    SharedWardrobeTierOptions {
                        operator: op.operator.unwrap_or(false),
                    },
                );
                json!({
                    "groupMountPointIds": tiers.group_mount_point_ids,
                    "projectMountPointIds": tiers.project_mount_point_ids,
                })
            }
            "wardrobe_list" => {
                let args = op.args.clone().unwrap_or_else(|| json!({}));
                let out = wardrobe_list::execute(
                    main,
                    mount,
                    &spec.user_id,
                    &chat_id_of(op.chat.as_deref()),
                    character_id.as_deref().unwrap_or(""),
                    &args,
                );
                json!({
                    "output": serde_json::to_value(&out).expect("serialize"),
                    "formatted": wardrobe_list::format(&out),
                })
            }
            other => panic!("unknown op kind: {other}"),
        };
        let got_lines: Vec<Line> = std::mem::take(&mut *captured.lock().unwrap());
        compared += 1;

        // v4's resolver WARNs bake their fields into the message (no context);
        // v5's carry structured fields as a convenience (P4.100). A v4 line with
        // NO context is compared on src + level + message alone.
        let want_lines = v4_lines(&want_raw["logs"]);
        let got_lines: Vec<Line> = got_lines
            .into_iter()
            .zip(want_lines.iter().map(Some).chain(std::iter::repeat(None)))
            .map(|((s, l, m, f), w)| match w {
                Some((_, _, _, wf)) if wf.is_empty() => (s, l, m, Vec::new()),
                _ => (s, l, m, f),
            })
            .collect();

        let mut got = json!({
            "name": op.name,
            "kind": op.kind,
            "result": result,
            "logs": lines_to_value(&got_lines),
        });
        let mut want = json!({
            "name": want_raw["name"],
            "kind": want_raw["kind"],
            "result": want_raw["result"],
            "logs": lines_to_value(&want_lines),
        });
        got = sentinelize(&got);
        placeholder_fs_times(&mut got);
        collapse_mtime_strings(&mut got);
        placeholder_fs_times(&mut want);
        collapse_mtime_strings(&mut want);
        strip_jest_realm_error_prefix(&mut want);
        let got = normalize(&got);
        let want = normalize(&want);
        if got != want {
            mismatches.push(format!(
                "op[{i}] {}\n  rust:   {got}\n  oracle: {want}",
                op.name
            ));
        }
    }

    drop(main_w);
    drop(mount_w);
    let _ = std::fs::remove_dir_all(&root);

    eprintln!(
        "project_roster_access: {compared} ops compared, {skipped} skipped (chokepoint absent at the oracle's pin), {} diverged.",
        mismatches.len()
    );
    // The direct chokepoint rows are either ALL compared (an oracle from a pin
    // with the module) or ALL skipped (one from before it) — never a mixture,
    // and never silently fewer than the spec carries.
    let direct = spec
        .ops
        .iter()
        .filter(|o| o.kind == "admits" || o.kind == "gated_id")
        .count();
    assert!(
        direct >= 14,
        "the spec must carry v4's nine shapes and the gated-id arms; got {direct}"
    );
    assert!(
        skipped == 0 || skipped == direct,
        "{skipped} direct rows skipped of {direct}: the oracle's pin is neither before nor after the module"
    );
    assert!(
        mismatches.is_empty(),
        "{} of {} ops diverged:\n\n{}",
        mismatches.len(),
        compared,
        mismatches.join("\n\n")
    );
}
