//! P4.6p ROLEPLAY-TEMPLATES route-surface differential: `api::roleplay_templates::*`
//! vs v4's REAL roleplay-template route handlers. Both sides read a FRESH copy of
//! the committed groups-projects fixture per case; built-in ids (minted at fixture
//! build) are resolved from the fixture and identical on both sides. Create mints
//! id + timestamps (blanked); update mints updatedAt (blanked). Error cases assert
//! the Error kind's HTTP status + message.
//!
//! P4.D351 (v4 `778b97b2b`, the Novel template, pin `236c04e35`): both sides run
//! the REAL built-in seed on every copy (v4's `seedBuiltInTemplates`, v5's
//! `seed_built_in_templates` — production's boot step; v4's
//! `initializeDatabase` does not seed), so the Novel row exists on both with an
//! id / stamps each side mints, and the seed restamps the legacy built-ins'
//! `updatedAt` — the normaliser blanks those (and ONLY those) minted fields.
//! Every case compares the whole `{status, body}` (a `Validation error`'s
//! `details` included) AND the captured `Roleplay template …` log lines — the
//! two DEBUGs and the route-level INFOs. v4's REPOSITORY-level INFOs (`…
//! created successfully`, the field-less `… updated successfully` /
//! `… deleted successfully`) are dropped from v4's side BY NAME
//! (`REPO_LEVEL_INFO_LINES`, [`is_repo_level_info`]). v5 PORTS them at the
//! unification (`db/roleplay_templates.rs`, pinned by its
//! `repository_info_line_tests`), but they are emitted INSIDE the route's
//! `Db::write` closure — on the writer thread, invisible to this family's
//! thread-scoped `captured_with`, and under the `db` target `v5_lines` does not
//! keep. Retiring the carve needs a process-wide capture layer for this binary
//! (the `memory_housekeeping_handler.rs` template), not an oracle regen —
//! the carve STAYS until then.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-roleplay-templates-routes.ndjson npx jest -- roleplay-templates-routes
//! Run:
//!   QT_ORACLE_ROLEPLAY_ROUTES=/tmp/oracle-roleplay-templates-routes.ndjson \
//!     cargo test -p quilltap-harness --test roleplay_templates_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::roleplay_templates as rt;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::builtin_templates::seed_built_in_templates;
use quilltap_core::test_support::captured_with;
use serde::Deserialize;
use serde_json::{json, Value};

const RT_USER_1: &str = "a4000000-0000-4000-8000-000000000001";
const RT_USER_2: &str = "a4000000-0000-4000-8000-000000000002";
const BOGUS: &str = "a4000000-0000-4000-8000-0000000000ff";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec() -> Spec {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/groups-projects.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).expect("spec")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

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
fn blank_keys(v: &mut Value, keys: &[&str]) {
    match v {
        Value::Object(o) => {
            for k in keys {
                if o.contains_key(*k) {
                    o.insert((*k).to_string(), Value::String(format!("<{k}>")));
                }
            }
            o.iter_mut().for_each(|(_, x)| blank_keys(x, keys));
        }
        Value::Array(a) => a.iter_mut().for_each(|x| blank_keys(x, keys)),
        _ => {}
    }
}
fn norm(v: &Value, blank: &[&str]) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_keys(&mut v, blank);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical)".to_string()
}
fn response_data(r: &Response) -> Value {
    serde_json::to_value(r)
        .unwrap()
        .get("data")
        .cloned()
        .unwrap_or(Value::Null)
}
fn http_for(kind: ErrorKind) -> i64 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// A fixture copy in a scratch dir removed on drop (the `Db` drops first).
struct ScratchDb {
    db: Db,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for ScratchDb {
    type Target = Db;
    fn deref(&self) -> &Db {
        &self.db
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> ScratchDb {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-rt-{tag}-"))
        .tempdir()
        .expect("tempdir");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("groups-projects-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("groups-projects-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    // Production's boot step (v4 `seed-initial-data.ts` →
    // `seedBuiltInTemplates`; v5 `host.rs`): Novel inserted, legacy cells
    // rewritten — the oracle runs v4's real seed on its copy the same way.
    db.write_blocking(|ws| seed_built_in_templates(ws.main().connection()))
        .expect("seed the built-in templates");
    ScratchDb { db, _dir: scratch }
}

fn builtin_id(db: &Db, name: &str) -> String {
    db.read_main(|m| {
        m.query_row(
            "SELECT id FROM roleplay_templates WHERE isBuiltIn = 1 AND name = ?1",
            rusqlite::params![name],
            |r| r.get::<_, String>(0),
        )
        .map_err(Into::into)
    })
    .expect("resolve built-in id")
}

/// Blank the fields the seed mints per copy: every built-in's `updatedAt`
/// (the every-boot `$set` restamps the legacy two — measured), and the Novel
/// row's `id` / `createdAt` (inserted by the seed).
fn blank_minted_builtins(v: &mut Value) {
    match v {
        Value::Object(o) => {
            if o.get("isBuiltIn") == Some(&Value::Bool(true)) {
                let novel = o.get("name") == Some(&Value::String("Novel".into()));
                let keys: &[&str] = if novel {
                    &["id", "createdAt", "updatedAt"]
                } else {
                    &["updatedAt"]
                };
                for k in keys {
                    if o.contains_key(*k) {
                        o.insert((*k).to_string(), Value::String(format!("<builtin-{k}>")));
                    }
                }
            }
            o.iter_mut().for_each(|(_, x)| blank_minted_builtins(x));
        }
        Value::Array(a) => a.iter_mut().for_each(blank_minted_builtins),
        _ => {}
    }
}

/// v4's REPOSITORY-level INFO lines (`roleplay-templates.repository.ts:351`,
/// `:392`, `:424`) — ported in `db/roleplay_templates.rs`, but invisible to
/// this binary's capture (the writer thread) — see the module docs.
fn is_repo_level_info(line: &str) -> bool {
    line.starts_with("INFO Roleplay template created successfully ")
        || ((line.starts_with("INFO Roleplay template updated successfully templateId=")
            || line.starts_with("INFO Roleplay template deleted successfully templateId="))
            && line.split(' ').count() == 6)
}

/// Mask a minted `templateId=` value (anything but the fixture's two user ids).
fn mask_minted(line: &str) -> String {
    line.split(' ')
        .map(|tok| match tok.strip_prefix("templateId=") {
            Some(id) if id != RT_USER_1 && id != RT_USER_2 => "templateId=<minted>".to_string(),
            _ => tok.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// v5's captured lines as v4 renders them: `LEVEL message k=v…` — the target
/// token dropped, only this module's `Roleplay template …` lines kept.
fn v5_lines(raw: &[String]) -> Vec<String> {
    raw.iter()
        .filter_map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next()?;
            let target = parts.next()?;
            let rest = parts.next()?;
            (target == "quilltap_core::api::roleplay_templates"
                && rest.starts_with("Roleplay template "))
            .then(|| mask_minted(&format!("{level} {rest}")))
        })
        .collect()
}

fn v4_lines(row: &Value) -> Vec<String> {
    row["lines"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter(|l| !is_repo_level_info(l))
                .map(mask_minted)
                .collect()
        })
        .unwrap_or_default()
}

/// One route call, v5's side.
enum Call {
    List,
    Get(String),
    Create(Value),
    Update(String, Value),
    Delete(String),
}

/// A v5 response as v4's `{status, body}`.
fn as_status_body(call: &Call, r: &Response) -> Value {
    match r {
        Response::Error(e) => {
            let mut body = serde_json::Map::new();
            body.insert("error".into(), Value::String(e.message.clone()));
            if let Some(d) = &e.details {
                body.insert("details".into(), (**d).clone());
            }
            json!({ "status": http_for(e.kind), "body": Value::Object(body) })
        }
        _ => {
            let status = if matches!(call, Call::Create(_)) {
                201
            } else {
                200
            };
            json!({ "status": status, "body": response_data(r) })
        }
    }
}

#[test]
fn roleplay_templates_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_ROLEPLAY_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_ROLEPLAY_ROUTES (see test header).");
        return;
    };
    let spec = spec();
    let uid = spec.user_id.clone();
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let rt_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // Resolve built-in ids (the fixture's two are stable across copies).
    let standard = builtin_id(&fresh_db(&spec, "resolve"), "Standard");
    let quilltap_rp = builtin_id(&fresh_db(&spec, "resolve2"), "Quilltap RP");

    let run = |db: &Db, call: &Call| -> Response {
        match call {
            Call::List => rt::roleplay_template_list(db, &uid),
            Call::Get(id) => rt::roleplay_template_get(db, id),
            Call::Create(b) => rt_rt.block_on(rt::roleplay_template_create(db, &uid, b.clone())),
            Call::Update(id, b) => {
                rt_rt.block_on(rt::roleplay_template_update(db, &uid, id, b.clone()))
            }
            Call::Delete(id) => rt_rt.block_on(rt::roleplay_template_delete(db, id)),
        }
    };

    let mut failed: Vec<String> = Vec::new();
    let ran = std::cell::Cell::new(0usize);
    let check = |name: &str, calls: Vec<Call>, blank: &[&str], failed: &mut Vec<String>| {
        let Some(row) = oracle.get(name) else {
            eprintln!("[{name}] MISSING from the oracle");
            failed.push(name.to_string());
            return;
        };
        ran.set(ran.get() + 1);
        let db = fresh_db(&spec, name);
        let (got, raw_lines) = captured_with(|| {
            let outs: Vec<Value> = calls
                .iter()
                .map(|c| as_status_body(c, &run(&db, c)))
                .collect();
            match outs.as_slice() {
                [one] => one.clone(),
                [a, b] => json!({ "status": b["status"], "body": { "first": a, "second": b } }),
                _ => unreachable!("one or two calls per case"),
            }
        });
        let mut want = json!({ "status": row["status"], "body": row["body"] });
        let mut got = got;
        blank_minted_builtins(&mut want);
        blank_minted_builtins(&mut got);
        let (g, w) = (norm(&got, blank), norm(&want, blank));
        let mut ok = true;
        if g != w {
            eprintln!("[{name}] MISMATCH:\n{}", first_diff(&g, &w));
            ok = false;
        }
        let (gl, wl) = (v5_lines(&raw_lines), v4_lines(row));
        if gl != wl {
            eprintln!("[{name}] LINES MISMATCH:\n  v5: {gl:#?}\n  v4: {wl:#?}");
            ok = false;
        }
        if ok {
            eprintln!("[{name}] OK.");
        } else {
            failed.push(name.to_string());
        }
    };

    const CREATED: &[&str] = &["id", "createdAt", "updatedAt"];
    const UPDATED: &[&str] = &["updatedAt"];
    let u1 = || RT_USER_1.to_string();
    let u2 = || RT_USER_2.to_string();
    let wrap_q = json!([{ "kind": "wrap", "name": "Q", "buttonName": "Q", "delimiters": "\"", "style": "qt-rp-q" }]);

    // --- Reads ---
    check("list", vec![Call::List], &[], &mut failed);
    check("get_user", vec![Call::Get(u1())], &[], &mut failed);
    check("get_regen", vec![Call::Get(u2())], &[], &mut failed);
    check(
        "get_builtin",
        vec![Call::Get(standard.clone())],
        &[],
        &mut failed,
    );
    check(
        "get_missing",
        vec![Call::Get(BOGUS.into())],
        &[],
        &mut failed,
    );
    {
        // The seeded Novel row: its id is minted per copy, so resolve it on the
        // case's own copy (the oracle does the same on its copy).
        let name = "get_builtin_novel";
        let row = oracle.get(name).cloned();
        let db = fresh_db(&spec, name);
        let novel = builtin_id(&db, "Novel");
        let (resp, raw) = captured_with(|| rt::roleplay_template_get(&db, &novel));
        let mut got = as_status_body(&Call::Get(novel.clone()), &resp);
        match row {
            Some(row) => {
                ran.set(ran.get() + 1);
                let mut want = json!({ "status": row["status"], "body": row["body"] });
                blank_minted_builtins(&mut want);
                blank_minted_builtins(&mut got);
                if norm(&got, &[]) != norm(&want, &[]) || v5_lines(&raw) != v4_lines(&row) {
                    eprintln!(
                        "[{name}] MISMATCH:\n{}",
                        first_diff(&norm(&got, &[]), &norm(&want, &[]))
                    );
                    failed.push(name.into());
                } else {
                    eprintln!("[{name}] OK.");
                }
            }
            None => failed.push(format!("{name} (missing)")),
        }
    }

    // --- Create ---
    check(
        "create_happy",
        vec![Call::Create(json!({
            "name": "Fresh Template",
            "description": "  A fresh one.  ",
            "systemPrompt": "  Write freshly.  ",
            "narrationDelimiters": "*",
            "delimiters": [
                { "kind": "wrap", "name": "Emph", "buttonName": "Em", "delimiters": "~", "style": "qt-rp-emph" },
                { "kind": "linePrefix", "name": "OOC", "buttonName": "O", "marker": "// ", "style": "qt-rp-ooc" }
            ]
        }))],
        CREATED,
        &mut failed,
    );
    check(
        "create_narration_array",
        vec![Call::Create(json!({
            "name": "Bracketed",
            "systemPrompt": "Use brackets.",
            "narrationDelimiters": ["[", "]"],
            "delimiters": [
                { "kind": "wrap", "name": "Nar", "buttonName": "N", "delimiters": ["[", "]"], "style": "qt-rp-nar" }
            ]
        }))],
        CREATED,
        &mut failed,
    );
    check(
        "create_name_required",
        vec![Call::Create(json!({}))],
        &[],
        &mut failed,
    );
    check(
        "create_sysprompt_required",
        vec![Call::Create(json!({ "name": "X" }))],
        &[],
        &mut failed,
    );
    check(
        "create_narration_required",
        vec![Call::Create(json!({ "name": "X", "systemPrompt": "Y" }))],
        &[],
        &mut failed,
    );
    check(
        "create_dup",
        vec![Call::Create(
            json!({ "name": "Bracket Prose", "systemPrompt": "Y", "narrationDelimiters": "*" }),
        )],
        &[],
        &mut failed,
    );
    check(
        "create_null_narration",
        vec![Call::Create(json!({
            "name": "Plain Prose",
            "systemPrompt": "Write plainly.",
            "narrationDelimiters": null,
            "delimiters": [{ "kind": "wrap", "name": "Speech", "buttonName": "S", "delimiters": "\"", "style": "qt-chat-dialogue" }]
        }))],
        CREATED,
        &mut failed,
    );
    for (name, nd) in [
        ("create_narration_empty_string", json!("")),
        ("create_narration_zero", json!(0)),
        ("create_narration_false", json!(false)),
        ("create_narration_object", json!({ "a": 1 })),
    ] {
        check(
            name,
            vec![Call::Create(
                json!({ "name": "X", "systemPrompt": "Y", "narrationDelimiters": nd }),
            )],
            &[],
            &mut failed,
        );
    }
    let glyph = "\u{1D4D2}";
    for (name, unmarked) in [
        ("create_unmarked_style_bad_empty", json!({ "style": "" })),
        ("create_unmarked_style_bad_type", json!(5)),
        ("create_unmarked_style_missing_style", json!({})),
        (
            "create_unmarked_style_too_long",
            json!({ "style": glyph.repeat(51) }),
        ),
        (
            "create_unmarked_style_bad_addons",
            json!({ "style": "x", "addOns": 5 }),
        ),
        (
            "create_unmarked_style_bad_addons_enum",
            json!({ "style": "x", "addOns": { "underline": "wavy" } }),
        ),
        (
            "create_unmarked_style_addons_null",
            json!({ "style": "x", "addOns": null }),
        ),
    ] {
        check(
            name,
            vec![Call::Create(
                json!({ "name": "X", "systemPrompt": "Y", "narrationDelimiters": null, "unmarkedStyle": unmarked }),
            )],
            &[],
            &mut failed,
        );
    }
    check(
        "create_unmarked_style_fifty_code_points_ok",
        vec![Call::Create(json!({
            "name": "Fifty Glyphs", "systemPrompt": "Y", "narrationDelimiters": null,
            "unmarkedStyle": { "style": glyph.repeat(50) }
        }))],
        CREATED,
        &mut failed,
    );
    check(
        "create_unmarked_null_explicit",
        vec![Call::Create(
            json!({ "name": "Null Unmarked", "systemPrompt": "Y", "narrationDelimiters": "*", "unmarkedStyle": null }),
        )],
        CREATED,
        &mut failed,
    );
    check(
        "create_unmarked_partial_addons",
        vec![Call::Create(json!({
            "name": "Partial Addons", "systemPrompt": "Y", "narrationDelimiters": null,
            "unmarkedStyle": { "style": "qt-roleplay-plain", "addOns": { "italic": true } }
        }))],
        CREATED,
        &mut failed,
    );
    for (name, md) in [
        ("create_markdown_asterisks_not_bool", json!("yes")),
        ("create_markdown_asterisks_null", Value::Null),
    ] {
        check(
            name,
            vec![Call::Create(
                json!({ "name": "X", "systemPrompt": "Y", "narrationDelimiters": "*", "markdownAsterisks": md }),
            )],
            &[],
            &mut failed,
        );
    }
    check(
        "create_markdown_false_regen",
        vec![Call::Create(json!({
            "name": "No Markdown Stars", "systemPrompt": "Y", "narrationDelimiters": "*", "markdownAsterisks": false,
            "delimiters": [{ "kind": "wrap", "name": "Emph", "buttonName": "E", "delimiters": "_", "style": "qt-rp-emph" }]
        }))],
        CREATED,
        &mut failed,
    );
    {
        // Create, then GET the created row (the id is minted — resolve it from
        // the 201 inside the case).
        let name = "create_unmarked_only_regen";
        let row = oracle.get(name).cloned().expect("oracle row");
        ran.set(ran.get() + 1);
        let db = fresh_db(&spec, name);
        let (got, raw) = captured_with(|| {
            let c = Call::Create(json!({
                "name": "Unmarked Only", "systemPrompt": "Y", "narrationDelimiters": null,
                "unmarkedStyle": { "style": "qt-roleplay-plain" }
            }));
            let a = as_status_body(&c, &run(&db, &c));
            let id = a["body"]["id"].as_str().unwrap_or_default().to_string();
            let g = Call::Get(id);
            let b = as_status_body(&g, &run(&db, &g));
            json!({ "status": b["status"], "body": { "first": a, "second": b } })
        });
        let want = json!({ "status": row["status"], "body": row["body"] });
        if norm(&got, CREATED) != norm(&want, CREATED) || v5_lines(&raw) != v4_lines(&row) {
            eprintln!(
                "[{name}] MISMATCH:\n{}\n  v5 lines: {:?}\n  v4 lines: {:?}",
                first_diff(&norm(&got, CREATED), &norm(&want, CREATED)),
                v5_lines(&raw),
                v4_lines(&row)
            );
            failed.push(name.into());
        } else {
            eprintln!("[{name}] OK.");
        }
    }
    check(
        "create_empty_patterns_no_regen_input",
        vec![Call::Create(
            json!({ "name": "Nothing To Derive", "systemPrompt": "Y", "narrationDelimiters": null }),
        )],
        CREATED,
        &mut failed,
    );
    {
        // v4's REAL NOVEL_TEMPLATE authoring fields — read back from the
        // keystone's seeded Novel row (the same fields, dumped from v4).
        let rows: Vec<Value> = serde_json::from_str(include_str!(
            "../../quilltap-core/src/services/builtin_templates.json"
        ))
        .unwrap();
        let novel = rows.iter().find(|r| r["name"] == "Novel").unwrap().clone();
        check(
            "create_novel_like",
            vec![Call::Create(json!({
                "name": "My Novel",
                "description": novel["description"],
                "systemPrompt": novel["systemPrompt"],
                "delimiters": novel["delimiters"],
                "dialogueDetection": novel["dialogueDetection"],
                "narrationDelimiters": novel["narrationDelimiters"],
                "unmarkedStyle": novel["unmarkedStyle"],
                "markdownAsterisks": novel["markdownAsterisks"],
            }))],
            CREATED,
            &mut failed,
        );
    }
    check(
        "create_wrap_curly_not_bool",
        vec![Call::Create(json!({
            "name": "X", "systemPrompt": "Y", "narrationDelimiters": "*",
            "delimiters": [{ "kind": "wrap", "name": "Q", "buttonName": "Q", "delimiters": "\"", "style": "qt-rp-q", "matchCurlyQuotes": "yes" }]
        }))],
        &[],
        &mut failed,
    );
    check(
        "create_wrap_close_optional_null",
        vec![Call::Create(json!({
            "name": "X", "systemPrompt": "Y", "narrationDelimiters": "*",
            "delimiters": [{ "kind": "wrap", "name": "Q", "buttonName": "Q", "delimiters": "\"", "style": "qt-rp-q", "closeOptional": null }]
        }))],
        &[],
        &mut failed,
    );
    check(
        "create_wrap_flags_kept",
        vec![Call::Create(json!({
            "name": "Flagged Wrap", "systemPrompt": "Y", "narrationDelimiters": null,
            "delimiters": [{ "kind": "wrap", "name": "Q", "buttonName": "Q", "delimiters": "\"", "style": "qt-rp-q", "matchCurlyQuotes": true, "closeOptional": false }]
        }))],
        CREATED,
        &mut failed,
    );

    // --- Update ---
    let upd = |id: String, b: Value| vec![Call::Update(id, b)];
    check(
        "update_happy",
        upd(
            u1(),
            json!({ "name": "Renamed Prose", "systemPrompt": "  Trimmed.  " }),
        ),
        UPDATED,
        &mut failed,
    );
    check(
        "update_builtin_403",
        upd(standard.clone(), json!({ "name": "Nope" })),
        &[],
        &mut failed,
    );
    check(
        "update_dup",
        upd(u2(), json!({ "name": "Bracket Prose" })),
        &[],
        &mut failed,
    );
    check(
        "update_regen_delims",
        upd(
            u1(),
            json!({ "name": "Bracket Prose", "systemPrompt": "Keep prose.", "delimiters": wrap_q }),
        ),
        UPDATED,
        &mut failed,
    );
    check(
        "update_regen_narration",
        upd(
            u1(),
            json!({ "name": "Bracket Prose", "systemPrompt": "Keep prose.", "narrationDelimiters": "|" }),
        ),
        UPDATED,
        &mut failed,
    );
    check(
        "update_null_description",
        upd(
            u1(),
            json!({ "name": "Bracket Prose", "systemPrompt": "Keep prose.", "description": null }),
        ),
        UPDATED,
        &mut failed,
    );
    check(
        "update_missing_required",
        upd(u1(), json!({ "delimiters": wrap_q })),
        &[],
        &mut failed,
    );
    check(
        "update_narration_null",
        upd(
            u1(),
            json!({ "name": "Bracket Prose", "systemPrompt": "Keep prose.", "narrationDelimiters": null }),
        ),
        UPDATED,
        &mut failed,
    );
    check(
        "update_regen_unmarked_only",
        upd(
            u1(),
            json!({
                "name": "Bracket Prose", "systemPrompt": "Keep prose.",
                "unmarkedStyle": { "style": "qt-roleplay-plain", "addOns": { "bold": true } }
            }),
        ),
        UPDATED,
        &mut failed,
    );
    check(
        "update_regen_markdown_only",
        upd(
            u2(),
            json!({ "name": "Quiet Draft", "systemPrompt": "Keep quiet.", "markdownAsterisks": false }),
        ),
        UPDATED,
        &mut failed,
    );
    check(
        "update_unmarked_null_clears",
        vec![
            Call::Update(
                u1(),
                json!({ "name": "Bracket Prose", "systemPrompt": "Keep prose.", "unmarkedStyle": { "style": "qt-roleplay-plain" } }),
            ),
            Call::Update(
                u1(),
                json!({ "name": "Bracket Prose", "systemPrompt": "Keep prose.", "unmarkedStyle": null }),
            ),
        ],
        UPDATED,
        &mut failed,
    );
    check(
        "update_keeps_unmarked_when_absent",
        vec![
            Call::Update(
                u1(),
                json!({
                    "name": "Bracket Prose", "systemPrompt": "Keep prose.",
                    "unmarkedStyle": { "style": "qt-roleplay-plain" }, "markdownAsterisks": false
                }),
            ),
            Call::Update(
                u1(),
                json!({ "name": "Bracket Prose", "systemPrompt": "Keep prose.", "narrationDelimiters": "~" }),
            ),
        ],
        UPDATED,
        &mut failed,
    );
    for (name, extra) in [
        (
            "update_narration_empty_string",
            json!({ "narrationDelimiters": "" }),
        ),
        (
            "update_narration_bad_pair",
            json!({ "narrationDelimiters": ["[", ""] }),
        ),
        (
            "update_narration_number",
            json!({ "narrationDelimiters": 7 }),
        ),
        (
            "update_narration_triple",
            json!({ "narrationDelimiters": ["a", "b", "c"] }),
        ),
        (
            "update_narration_single_elem",
            json!({ "narrationDelimiters": ["a"] }),
        ),
        (
            "update_narration_pair_nonstring",
            json!({ "narrationDelimiters": [1, ""] }),
        ),
        (
            "update_unmarked_style_bad",
            json!({ "unmarkedStyle": { "style": "" } }),
        ),
        (
            "update_markdown_asterisks_not_bool",
            json!({ "markdownAsterisks": "yes" }),
        ),
        (
            "update_two_new_key_refusals",
            json!({ "narrationDelimiters": "", "markdownAsterisks": 1 }),
        ),
    ] {
        let mut b = json!({ "name": "Bracket Prose", "systemPrompt": "K" });
        for (k, v) in extra.as_object().unwrap() {
            b[k] = v.clone();
        }
        check(name, upd(u1(), b), &[], &mut failed);
    }
    check(
        "update_patterns_sent_no_regen",
        upd(
            u1(),
            json!({
                "name": "Bracket Prose", "systemPrompt": "Keep prose.", "markdownAsterisks": false,
                "renderingPatterns": [{ "pattern": "x", "className": "qt-x" }]
            }),
        ),
        UPDATED,
        &mut failed,
    );
    check(
        "update_name_only_no_regen",
        upd(
            u1(),
            json!({ "name": "Renamed Prose", "systemPrompt": "Keep prose." }),
        ),
        UPDATED,
        &mut failed,
    );

    // --- Delete ---
    check("delete_happy", vec![Call::Delete(u2())], &[], &mut failed);
    check(
        "delete_builtin_403",
        vec![Call::Delete(quilltap_rp.clone())],
        &[],
        &mut failed,
    );
    check(
        "delete_missing",
        vec![Call::Delete(BOGUS.into())],
        &[],
        &mut failed,
    );

    let ran = ran.get();
    eprintln!("ran {ran} of {} oracle cases", oracle.len());
    assert_eq!(ran, oracle.len(), "every oracle case must be driven");
    assert!(
        failed.is_empty(),
        "roleplay-templates-routes FAILED: {failed:?}"
    );
}
