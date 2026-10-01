//! P4.83 prompt-templates differential — v5's `api::prompt_templates::*`
//! handlers against v4's REAL route handlers
//! (`app/api/v1/prompt-templates/route.ts` + `[id]/route.ts`) over a FRESH copy
//! of the /tmp fixture per case.
//!
//! Four comparands per case: the HTTP **status**, the response **body** bytes
//! (after normalizing minted ids and timestamps), the captured **log lines**
//! (the seed line and the route's `[Prompt Templates v1] …` family — see the
//! handler's module doc for the repository-level lines that have no v5
//! counterpart), and the whole **`prompt_templates` table** afterwards. The
//! table is what proves the two halves the body alone cannot: that a Zod-refused
//! POST/PUT never seeded anything (`table` stays at 1 row), and — since v4
//! `c3eefa752` (P4.D237) — that a built-in already on file is REFRESHED to the
//! shipped text (content / description / category / modelHint, `updatedAt`
//! moved, `createdAt` not) when it differs, and left byte-identical when it
//! does not. (Before `c3eefa752` the same case pinned the opposite: v4 never
//! overwrote a sample prompt already on file.)
//!
//! The fixture is a CLEAN PROVISIONED instance holding one user template — so
//! the first list is the one that seeds, which is the whole point. The other
//! shapes (a stale built-in, a user template named like a built-in, a
//! schema-invalid plant, and P4.D237's per-field / NULL / exact / duplicate
//! built-in plants) are PER-CASE seeds applied identically on both sides. The
//! "shipped" values of those plants come from v5's vendored catalogue here and
//! from v4's real registry in the oracle — the table comparand proves the two
//! agree (and `builtin_prompt_templates_guard` proves it independently).
//!
//! The spec's pinned `ts` survives normalization (like the fixture's ids), so a
//! refreshed row's `createdAt` reads back as that literal while its `updatedAt`
//! reads `<ts>`, and an untouched row keeps both.
//!
//! **The capture layer is process-global on purpose.** The seed lines fire
//! inside `db.write(...)`, i.e. on the writer THREAD, which a
//! `set_default`-scoped subscriber can never see. This binary therefore holds
//! exactly ONE `#[test]`, runs its cases sequentially, and installs one global
//! subscriber that appends every event to a shared buffer cleared per case.
//! Adding a second test to this file would make the two race for that buffer.
//!
//! The fixture rule (P4.D241, measured at `ca363178d`): build the FIXTURE and
//! run the ORACLE from ONE v4 tree. P4.D237's two-worktree rule (fixture at
//! `acadcc7cd`, oracle at `c3eefa752`) is retired: P4.D235's tolerant chat read
//! opens a post-`f7f3d7bf0` fixture, and the fixture holds no `chats` table
//! anyway. When v4 HEAD is past the baseline, run both lines below from one
//! pinned worktree (drift-ledger §5.1).
//!
//! Regenerate + run (Node 24, from the v4 checkout — mirror to /tmp; jest
//! ignores .claude/):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-prompt-templates-oracle
//!   mkdir -p /tmp/qt-prompt-templates-oracle/cases /tmp/qt-prompt-templates-oracle/fixtures
//!   cp $V5W/harness/oracle/cases/prompt-templates-routes.test.ts /tmp/qt-prompt-templates-oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/prompt-templates-routes.json /tmp/qt-prompt-templates-oracle/fixtures/
//!   QT_FIXTURE_PT_ROUTES_MAIN=/tmp/qt-pt-routes-fixture.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-prompt-templates-routes-fixture.ts
//!   QT_FIXTURE_PT_ROUTES_MAIN=/tmp/qt-pt-routes-fixture.db \
//!   QT_ORACLE_OUT=/tmp/oracle-prompt-templates-routes.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=600000 \
//!       --roots "$PWD" --roots /tmp/qt-prompt-templates-oracle/cases -- "prompt-templates-routes\.test\.ts$"
//!   cd $V5W
//!   QT_ORACLE_PT_ROUTES=/tmp/oracle-prompt-templates-routes.ndjson \
//!   QT_FIXTURE_PT_ROUTES=/tmp/qt-pt-routes-fixture.db \
//!     cargo test -p quilltap-harness --test prompt_templates_routes_equivalence

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::api::prompt_templates as pt;
use quilltap_core::api::types::{ErrorKind, Request, Response};
use quilltap_core::db::prompt_templates::{CreateOptions, PromptTemplatesRepository, PtCreate};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::builtin_prompt_templates::{catalogue, seed_description};
use regex::Regex;
use serde_json::{json, Map, Value};

// ── The process-global capture layer (see the module doc) ───────────────────

static CAPTURED: OnceLock<Arc<Mutex<Vec<Value>>>> = OnceLock::new();

fn capture_buffer() -> Arc<Mutex<Vec<Value>>> {
    CAPTURED
        .get_or_init(|| Arc::new(Mutex::new(Vec::new())))
        .clone()
}

/// Renders one event into `{level, message, context}` — the same shape the
/// oracle emits for v4's `logger.<level>(message, bag)`.
struct EventVisitor {
    message: Option<String>,
    context: Map<String, Value>,
}

impl tracing::field::Visit for EventVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.context.insert(field.name().to_string(), json!(value));
        }
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(rendered);
        } else {
            self.context
                .insert(field.name().to_string(), json!(rendered));
        }
    }
}

struct GlobalCapture;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for GlobalCapture {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut v = EventVisitor {
            message: None,
            context: Map::new(),
        };
        event.record(&mut v);
        let Some(message) = v.message else { return };
        if !is_ported_line(&message) {
            return;
        }
        let level = match *event.metadata().level() {
            tracing::Level::ERROR => "error",
            tracing::Level::WARN => "warn",
            tracing::Level::INFO => "info",
            tracing::Level::DEBUG => "debug",
            tracing::Level::TRACE => "trace",
        };
        capture_buffer().lock().unwrap().push(json!({
            "level": level,
            "message": message,
            "context": Value::Object(v.context),
        }));
    }
}

/// The families this port owns (the oracle filters identically): the seeder's
/// two lines (both renamed / added at v4 `c3eefa752`) and the route's
/// `[Prompt Templates v1]` lines. The pre-`c3eefa752` seed sentence stays in the
/// filter so a side still emitting it shows up as a diff rather than vanishing.
fn is_ported_line(message: &str) -> bool {
    message == "Sample prompt template seeded"
        || message == "Built-in prompt template refreshed from shipped text"
        || message == "Sample prompt template seeded from plugin"
        || message.starts_with("[Prompt Templates v1] ")
}

fn install_capture() {
    use tracing_subscriber::layer::SubscriberExt;
    let _ =
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(GlobalCapture));
}

// ── Spec + fixture ──────────────────────────────────────────────────────────

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/prompt-templates-routes.json")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

/// Normalize what both sides legitimately mint: UUIDs not in the fixture spec
/// (the 21 seeded ids, plus a created template's id) and ISO timestamps other
/// than the spec's own pinned `ts` (which `known` carries, so it stays literal).
fn normalize(v: &mut Value, known: &HashSet<String>, uuid_re: &Regex, ts_re: &Regex) {
    match v {
        Value::String(s) => {
            if known.contains(s.as_str()) {
                // A pinned id or the pinned `ts` — keep it legible.
            } else if ts_re.is_match(s) {
                *s = "<ts>".to_string();
            } else if uuid_re.is_match(s) {
                *s = "<newid>".to_string();
            }
        }
        Value::Array(a) => {
            for x in a {
                normalize(x, known, uuid_re, ts_re);
            }
        }
        Value::Object(o) => {
            for x in o.values_mut() {
                normalize(x, known, uuid_re, ts_re);
            }
        }
        _ => {}
    }
}

/// v4's log context bags are camelCase; v5's tracing fields are snake_case (the
/// established rendering — `subprompts.rs`' `character_id` for v4's
/// `characterId`). Fold the oracle's keys into v5's spelling so the comparand is
/// the LEVEL, the SENTENCE and the field SET + VALUES, not the casing.
fn snake(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 2);
    for ch in key.chars() {
        if ch.is_ascii_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn snake_context(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, val) in o {
                out.insert(snake(k), val.clone());
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// v4's `updatedFields` is a JS array; v5 renders `?updated_fields` through
/// `Debug`, so the captured value is the Rust list literal. Compare the two by
/// re-rendering the oracle's array the same way.
fn debug_render_string_array(v: &Value) -> Value {
    match v {
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|i| match i {
                    Value::String(s) => format!("{s:?}"),
                    other => format!("{other}"),
                })
                .collect();
            json!(format!("[{}]", parts.join(", ")))
        }
        other => other.clone(),
    }
}

fn logs_for_compare(
    rows: &[Value],
    known: &HashSet<String>,
    uuid_re: &Regex,
    ts_re: &Regex,
) -> Value {
    let mut out = Vec::new();
    for row in rows {
        let mut ctx = snake_context(&row["context"]);
        if let Some(uf) = ctx.get("updated_fields").cloned() {
            if uf.is_array() {
                ctx.as_object_mut()
                    .unwrap()
                    .insert("updated_fields".into(), debug_render_string_array(&uf));
            }
        }
        let mut entry = json!({
            "level": row["level"].clone(),
            "message": row["message"].clone(),
            "context": ctx,
        });
        normalize(&mut entry, known, uuid_re, ts_re);
        out.push(entry);
    }
    Value::Array(out)
}

/// The raw table dump, in v4's `rawQuery` shape (`isBuiltIn` an INTEGER, `tags`
/// the stored TEXT) so both sides compare the persisted bytes, not the wire.
fn dump_table(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            "SELECT id, userId, name, content, description, isBuiltIn, category, modelHint, \
             tags, createdAt, updatedAt FROM prompt_templates",
        )?;
        let rows = stmt.query_map([], |r| {
            let mut o = Map::new();
            o.insert("id".into(), json!(r.get::<_, String>(0)?));
            o.insert("userId".into(), json!(r.get::<_, Option<String>>(1)?));
            o.insert("name".into(), json!(r.get::<_, String>(2)?));
            o.insert("content".into(), json!(r.get::<_, String>(3)?));
            o.insert("description".into(), json!(r.get::<_, Option<String>>(4)?));
            o.insert("isBuiltIn".into(), json!(r.get::<_, Option<i64>>(5)?));
            o.insert("category".into(), json!(r.get::<_, Option<String>>(6)?));
            o.insert("modelHint".into(), json!(r.get::<_, Option<String>>(7)?));
            o.insert("tags".into(), json!(r.get::<_, Option<String>>(8)?));
            o.insert("createdAt".into(), json!(r.get::<_, String>(9)?));
            o.insert("updatedAt".into(), json!(r.get::<_, String>(10)?));
            Ok(Value::Object(o))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(Value::Array(out))
    })
    .expect("dump prompt_templates")
}

/// The oracle's per-case seeds, applied through v5's own repo (and raw SQL for
/// the deliberately schema-invalid plant, which no validating writer can store).
fn apply_seeds(rt: &tokio::runtime::Runtime, db: &Db, spec: &Value, seeds: &[String]) {
    for seed in seeds {
        let spec = spec.clone();
        let seed = seed.clone();
        rt.block_on(db.write(move |w| {
            let conn = w.main().connection();
            let ts = spec["ts"].as_str().unwrap().to_string();
            match seed.as_str() {
                "staleBuiltIn" => PromptTemplatesRepository::new(conn).create(
                    &PtCreate {
                        user_id: None,
                        name: spec["collidingName"].as_str().unwrap().to_string(),
                        content: spec["staleContent"].as_str().unwrap().to_string(),
                        description: Some("A stale plant".to_string()),
                        is_built_in: true,
                        category: Some("GENERAL".to_string()),
                        model_hint: Some("MODERN".to_string()),
                        tags: Vec::new(),
                    },
                    &CreateOptions {
                        id: spec["staleBuiltInId"].as_str().unwrap().to_string(),
                        created_at: ts.clone(),
                        updated_at: ts,
                    },
                ),
                "userClone" => PromptTemplatesRepository::new(conn).create(
                    &PtCreate {
                        user_id: Some(spec["userA"].as_str().unwrap().to_string()),
                        name: spec["collidingName"].as_str().unwrap().to_string(),
                        content: "A user template that happens to share a sample prompt name."
                            .to_string(),
                        description: None,
                        is_built_in: false,
                        category: None,
                        model_hint: None,
                        tags: Vec::new(),
                    },
                    &CreateOptions {
                        id: spec["userCloneId"].as_str().unwrap().to_string(),
                        created_at: ts.clone(),
                        updated_at: ts,
                    },
                ),
                // P4.D237: built-in plants carrying the SHIPPED values (v5's
                // vendored catalogue) with one compared field changed, or none.
                "builtinContentOnly" => plant_builtin(conn, &spec, "contentOnly", |p| {
                    p.content = spec["staleContent"].as_str().unwrap().to_string()
                }),
                "builtinDescriptionOnly" => plant_builtin(conn, &spec, "descriptionOnly", |p| {
                    p.description = Some("A stale plant".to_string())
                }),
                "builtinCategoryOnly" => plant_builtin(conn, &spec, "categoryOnly", |p| {
                    p.category = Some("COMPANION".to_string())
                }),
                "builtinModelHintOnly" => plant_builtin(conn, &spec, "modelHintOnly", |p| {
                    p.model_hint = Some("CLAUDE".to_string())
                }),
                "builtinNullDescription" => {
                    plant_builtin(conn, &spec, "nullDescription", |p| p.description = None)
                }
                "builtinExact" => plant_builtin(conn, &spec, "exact", |_| {}),
                // Every OTHER catalogue entry on file as shipped, so the only
                // thing left to find is the stale row beside it — the arm a
                // names-only probe (v5's pre-P4.D237 `needs_seeding`) cannot see.
                // Ids: the spec prefix + the catalogue position, two hex digits.
                "catalogueCurrent" => {
                    let colliding = spec["collidingName"].as_str().unwrap();
                    let prefix = spec["catalogueIdPrefix"].as_str().unwrap();
                    let repo = PromptTemplatesRepository::new(conn);
                    for (i, e) in catalogue().expect("catalogue").iter().enumerate() {
                        if e.name == colliding {
                            continue;
                        }
                        repo.create(
                            &PtCreate {
                                user_id: None,
                                name: e.name.clone(),
                                content: e.content.clone(),
                                description: Some(seed_description(&e.category, &e.model_hint)),
                                is_built_in: true,
                                category: Some(e.category.clone()),
                                model_hint: Some(e.model_hint.clone()),
                                tags: Vec::new(),
                            },
                            &CreateOptions {
                                id: format!("{prefix}{i:02x}"),
                                created_at: ts.clone(),
                                updated_at: ts.clone(),
                            },
                        )?;
                    }
                    Ok(())
                }
                "builtinDuplicate" => plant_builtin(conn, &spec, "duplicate", |p| {
                    p.content =
                        format!("{} (the duplicate)", spec["staleContent"].as_str().unwrap())
                }),
                "invalidRow" => {
                    let name: String = "\u{1F600}".repeat(101);
                    conn.execute(
                        "INSERT INTO prompt_templates (id, userId, name, content, description, \
                         isBuiltIn, category, modelHint, tags, createdAt, updatedAt) \
                         VALUES (?1, NULL, ?2, ?3, NULL, 1, NULL, NULL, '[]', ?4, ?4)",
                        rusqlite::params![
                            spec["invalidRowId"].as_str().unwrap(),
                            name,
                            "A row v4 stores but its schema refuses.",
                            ts
                        ],
                    )?;
                    Ok(())
                }
                other => panic!("unknown seed: {other}"),
            }
        }))
        .expect("apply seed");
    }
}

/// One P4.D237 built-in plant under `collidingName`: the catalogue's shipped
/// content / description / category / modelHint, `over` applied, pinned id
/// `builtinPlantIds[key]` and the spec's `ts` for both timestamps.
fn plant_builtin(
    conn: &rusqlite::Connection,
    spec: &Value,
    key: &str,
    over: impl FnOnce(&mut PtCreate),
) -> Result<(), quilltap_core::db::DbError> {
    let name = spec["collidingName"].as_str().unwrap();
    let shipped = catalogue()
        .expect("the vendored catalogue parses")
        .into_iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("the catalogue has no {name}"));
    let mut plant = PtCreate {
        user_id: None,
        name: name.to_string(),
        content: shipped.content.clone(),
        description: Some(seed_description(&shipped.category, &shipped.model_hint)),
        is_built_in: true,
        category: Some(shipped.category.clone()),
        model_hint: Some(shipped.model_hint.clone()),
        tags: Vec::new(),
    };
    over(&mut plant);
    let ts = spec["ts"].as_str().unwrap().to_string();
    PromptTemplatesRepository::new(conn).create(
        &plant,
        &CreateOptions {
            id: spec["builtinPlantIds"][key].as_str().unwrap().to_string(),
            created_at: ts.clone(),
            updated_at: ts,
        },
    )
}

/// Response → `(body, status)`, mapping v5's `ErrorKind` to v4's wire status.
fn response_to_body_status(resp: Response) -> (Value, u16) {
    match resp {
        Response::PromptTemplates(v)
        | Response::PromptTemplate(v)
        | Response::PromptTemplateDeleted(v) => (v, 200),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                _ => 500,
            };
            let body = match e.validation_wire_body() {
                Some(b) => b,
                None => json!({ "error": e.message }),
            };
            (body, status)
        }
        other => panic!("unexpected response variant: {other:?}"),
    }
}

/// Decode the oracle's body through the SAME `Request` serde the wire uses, so a
/// present-`null` can never quietly collapse to key-absent in the harness alone.
fn decode_flat(kind: &str, id: Option<&str>, body: &Value) -> Request {
    let mut req = Map::new();
    req.insert("type".into(), json!(kind));
    if let Some(id) = id {
        req.insert("id".into(), json!(id));
    }
    if let Some(o) = body.as_object() {
        for key in ["name", "content", "description", "category", "modelHint"] {
            if let Some(v) = o.get(key) {
                req.insert(key.to_string(), v.clone());
            }
        }
    }
    serde_json::from_value(Value::Object(req))
        .unwrap_or_else(|e| panic!("{kind} body must decode through Request: {e}"))
}

#[test]
fn prompt_templates_routes_match_v4() {
    let (Some(oracle_path), Some(fixture)) = (
        env_or_skip("QT_ORACLE_PT_ROUTES"),
        env_or_skip("QT_FIXTURE_PT_ROUTES"),
    ) else {
        return;
    };
    install_capture();

    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).expect("read prompt-templates-routes.json"),
    )
    .expect("parse spec");
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let user_a = spec["userA"].as_str().unwrap().to_string();
    let user_b = spec["userB"].as_str().unwrap().to_string();

    // Every id the fixture or a per-case seed pins stays literal; anything else
    // that looks like a UUID was minted by the run and collapses.
    let known: HashSet<String> = [
        "userA",
        "userB",
        "userTemplateId",
        "staleBuiltInId",
        "userCloneId",
        "invalidRowId",
    ]
    .iter()
    .filter_map(|k| spec[*k].as_str().map(str::to_string))
    .chain(
        spec["builtinPlantIds"]
            .as_object()
            .expect("spec.builtinPlantIds — regenerate the fixture")
            .values()
            .filter_map(|v| v.as_str().map(str::to_string)),
    )
    .chain(std::iter::once(
        "5e800000-0000-4000-8000-0000000000ff".to_string(),
    ))
    // The pinned `ts` stays literal too (see `normalize`).
    .chain(spec["ts"].as_str().map(str::to_string))
    .collect();

    let uuid_re = Regex::new(
        r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
    )
    .unwrap();
    let ts_re = Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$").unwrap();

    let rt = rt();
    let oracle = std::fs::read_to_string(&oracle_path).expect("read oracle ndjson");
    assert!(
        !oracle.trim().is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );

    let mut driven: BTreeSet<String> = BTreeSet::new();
    let mut oracle_names: BTreeSet<String> = BTreeSet::new();
    let mut failed: BTreeMap<String, String> = BTreeMap::new();
    let mut n = 0usize;

    for line in oracle.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let name = row["name"].as_str().unwrap().to_string();
        oracle_names.insert(name.clone());
        let req = &row["req"];
        let user = if req["user"].as_str() == Some("B") {
            &user_b
        } else {
            &user_a
        };
        let param_id = req["paramId"].as_str();
        let body = if req["bodyAbsent"].as_bool() == Some(true) {
            json!({})
        } else {
            req["body"].clone()
        };
        let seeds: Vec<String> = req["seeds"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        // A fresh copy per case; every case mutates.
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main.db");
        std::fs::copy(&fixture, &main).unwrap();
        let db = Db::open(
            DbPaths {
                main,
                mount_index: None,
                llm_logs: None,
            },
            &pepper,
        )
        .expect("open db");
        apply_seeds(&rt, &db, &spec, &seeds);

        let route = req["route"].as_str().unwrap();
        let method = req["method"].as_str().unwrap();

        let run = |db: &Db| -> (Value, u16) {
            match (route, method) {
                ("collection", "GET") => {
                    response_to_body_status(rt.block_on(pt::prompt_template_list(db, user)))
                }
                ("collection", "POST") => {
                    // v4's `schema.parse(<non-object>)` — the REST edge's one arm
                    // (the flat variant cannot say "the body itself was null").
                    if !body.is_object() {
                        return (
                            json!({
                                "error": "Validation error",
                                "details": pt::body_not_object_details(&body),
                            }),
                            400,
                        );
                    }
                    let decoded = decode_flat("promptTemplateCreate", None, &body);
                    let Request::PromptTemplateCreate {
                        name,
                        content,
                        description,
                        category,
                        model_hint,
                    } = decoded
                    else {
                        unreachable!("tagged decode can only answer this variant")
                    };
                    let folded = pt::flat_body(name, content, description, category, model_hint);
                    let (body, status) = response_to_body_status(
                        rt.block_on(pt::prompt_template_create(db, user, &folded)),
                    );
                    // v4 `NextResponse.json({ template }, { status: 201 })`. The
                    // handler answers a `Response`, so the 201 is the REST
                    // edge's one status choice — applied here, and proven to be
                    // the edge's ACTUAL behaviour by
                    // `crates/quilltap-web/tests/prompt_templates_web_routes.rs`.
                    (body, if status == 200 { 201 } else { status })
                }
                ("item", "GET") => response_to_body_status(rt.block_on(pt::prompt_template_get(
                    db,
                    user,
                    param_id.unwrap(),
                ))),
                ("item", "PUT") => {
                    if !body.is_object() {
                        return (
                            json!({
                                "error": "Validation error",
                                "details": pt::body_not_object_details(&body),
                            }),
                            400,
                        );
                    }
                    let decoded =
                        decode_flat("promptTemplateUpdate", Some(param_id.unwrap()), &body);
                    let Request::PromptTemplateUpdate {
                        id,
                        name,
                        content,
                        description,
                        category,
                        model_hint,
                    } = decoded
                    else {
                        unreachable!("tagged decode can only answer this variant")
                    };
                    let folded = pt::flat_body(name, content, description, category, model_hint);
                    response_to_body_status(
                        rt.block_on(pt::prompt_template_update(db, user, &id, &folded)),
                    )
                }
                ("item", "DELETE") => response_to_body_status(
                    rt.block_on(pt::prompt_template_delete(db, user, param_id.unwrap())),
                ),
                other => panic!("unhandled route/method: {other:?}"),
            }
        };

        if req["runTwice"].as_bool() == Some(true) {
            let _ = run(&db);
        }
        capture_buffer().lock().unwrap().clear();
        let (mut got_body, got_status) = run(&db);
        let got_logs_raw = capture_buffer().lock().unwrap().clone();
        let mut got_table = dump_table(&db);
        drop(db);

        let mut want_body = row["body"].clone();
        let mut want_table = row["table"].clone();
        normalize(&mut got_body, &known, &uuid_re, &ts_re);
        normalize(&mut want_body, &known, &uuid_re, &ts_re);
        normalize(&mut got_table, &known, &uuid_re, &ts_re);
        normalize(&mut want_table, &known, &uuid_re, &ts_re);
        let want_logs = logs_for_compare(
            row["logs"].as_array().map(|a| a.as_slice()).unwrap_or(&[]),
            &known,
            &uuid_re,
            &ts_re,
        );
        let got_logs = logs_for_compare(&got_logs_raw, &known, &uuid_re, &ts_re);

        let want_status = row["status"].as_u64().unwrap() as u16;
        let mut problems = Vec::new();
        if want_status != got_status {
            problems.push(format!("status: oracle {want_status}, got {got_status}"));
        }
        if want_body != got_body {
            problems.push(format!(
                "body:\n  oracle: {want_body}\n  got:    {got_body}"
            ));
        }
        if want_table != got_table {
            problems.push(format!(
                "table:\n  oracle: {want_table}\n  got:    {got_table}"
            ));
        }
        if want_logs != got_logs {
            problems.push(format!(
                "logs:\n  oracle: {want_logs}\n  got:    {got_logs}"
            ));
        }
        if !problems.is_empty() {
            failed.insert(name.clone(), problems.join("\n"));
        }
        driven.insert(name);
        n += 1;
    }

    // Coverage by SHAPE: the oracle's names and the driven names must agree.
    let missing: Vec<&String> = oracle_names.difference(&driven).collect();
    assert!(
        missing.is_empty(),
        "oracle cases never driven by the Rust side: {missing:?}"
    );
    // A floor that a stale oracle (predating P4.D237's refresh arms) cannot
    // satisfy: 31 through P4.83 + 9 refresh cases.
    assert!(
        n >= 40,
        "expected >= 40 cases, got {n} — regenerate the oracle"
    );
    for want in [
        "list_first_seeds_21",
        "list_second_seeds_nothing",
        "list_stale_builtin_refreshed",
        "list_second_refreshes_nothing",
        "list_all_on_file_one_stale_refreshed",
        "list_builtin_content_only_refreshed",
        "list_builtin_description_only_refreshed",
        "list_builtin_category_only_refreshed",
        "list_builtin_model_hint_only_refreshed",
        "list_builtin_null_description_refreshed",
        "list_builtin_exact_untouched",
        "list_duplicate_builtins_first_refreshed",
        "list_user_named_like_builtin_still_seeds",
        "list_drops_schema_invalid_row",
        "create_minimal_201",
        "put_builtin_403",
        "delete_builtin_403",
        "put_zod_beats_missing_404",
    ] {
        assert!(
            driven.contains(want),
            "the oracle is missing the load-bearing case {want} — regenerate it"
        );
    }
    assert!(
        failed.is_empty(),
        "{} case(s) failed:\n{}",
        failed.len(),
        failed
            .iter()
            .map(|(k, v)| format!("[{k}]\n{v}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    );
    eprintln!("prompt-templates routes differential: {n} cases matched");
}
