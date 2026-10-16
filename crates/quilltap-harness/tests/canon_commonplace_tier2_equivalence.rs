//! Tier-2 differential test: the Commonplace canon reads (P4.D270 — v4
//! `d58548051`, `lib/memory/cheap-llm-tasks/canon.ts`):
//! `load_commonplace_canon`, `load_canon_for_self_with_commonplace` and the
//! `pub` `load_canon_for_observer_about_subject` (with and without
//! `include_commonplace`) in `services::memory_processor`, each followed by the
//! matching renderer, over ONE vault fixture both sides read.
//!
//! Per op the oracle (v4's REAL loaders over v4's REAL vault overlay) records
//! the loader's returned value — key presence as v4 builds it (`commonplace`
//! only when truthy) — the rendered block, and the `[Canon]` lines the op
//! logged. The Rust side serializes its own value in the same shape, renders,
//! and captures its own `[Canon]` lines (the loaders log on the caller thread —
//! the vault read is a `read_mount_index`, never a writer closure).
//!
//! v4's `[Canon] Commonplace digest unavailable; canon continues without it`
//! catch is UNREACHABLE (`readVaultTextFile` catches every error itself) and is
//! not ported; this family pins it NEGATIVE: no op on either side emits it.
//!
//! Generate the fixture + oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-canon-commonplace-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-canon-commonplace-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-canon-commonplace-fixture.ts
//!   QT_FIXTURE_CANON_MAIN=/tmp/qt-canon-commonplace-main.db \
//!   QT_FIXTURE_CANON_MOUNT=/tmp/qt-canon-commonplace-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/canon-commonplace-tier2.ts \
//!     > /tmp/oracle-canon-commonplace.ndjson
//! Run:
//!   QT_ORACLE_CANON_COMMONPLACE=/tmp/oracle-canon-commonplace.ndjson \
//!   QT_FIXTURE_CANON_MAIN=/tmp/qt-canon-commonplace-main.db \
//!   QT_FIXTURE_CANON_MOUNT=/tmp/qt-canon-commonplace-mount.db \
//!     cargo test -p quilltap-harness --test canon_commonplace_tier2_equivalence

use std::collections::BTreeMap;

use quilltap_core::canon::{render_other_canon_block, render_self_canon_block};
use quilltap_core::commonplace_file::CommonplaceSubject;
use quilltap_core::db::characters_read;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::canon_loaders::{
    load_canon_for_observer_about_subject, load_canon_for_self_with_commonplace,
    load_commonplace_canon, CanonSubject,
};
use quilltap_core::test_support::global_capture;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct SubjectW {
    id: String,
    name: String,
    #[serde(default)]
    identity: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CommonplaceSubjectW {
    SelfTag(String),
    Other { id: String, name: String },
}

#[derive(Deserialize)]
struct CharacterW {
    id: String,
    name: String,
    manifesto: Option<String>,
    personality: Option<String>,
    description: Option<String>,
    identity: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpW {
    op: String,
    name: String,
    mount: String,
    #[serde(default)]
    observer: Option<String>,
    #[serde(default)]
    subject: Option<Value>,
    #[serde(default)]
    include_commonplace: Option<bool>,
    #[serde(default)]
    character: Option<CharacterW>,
    #[serde(default)]
    holder: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecW {
    test_pepper_base64: String,
    ops: Vec<OpW>,
}

#[derive(Deserialize)]
struct OracleRow {
    name: String,
    value: Value,
    block: Option<String>,
    logs: Vec<Value>,
}

/// v4's captured line → `(level, message, fields)`, field values as the rig
/// renders them.
fn oracle_line(v: &Value) -> (String, String, BTreeMap<String, String>) {
    let fields = v["fields"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, val)| {
            let rendered = match val {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), rendered)
        })
        .collect();
    (
        v["level"].as_str().unwrap().to_string(),
        v["message"].as_str().unwrap().to_string(),
        fields,
    )
}

/// A rig line (`LEVEL target message k=v …`) → the same shape; only `[Canon]`
/// lines are kept (the values here carry no spaces).
fn rust_line(line: &str) -> Option<(String, String, BTreeMap<String, String>)> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let rest = rest.trim_start();
    if !rest.starts_with("[Canon]") {
        return None;
    }
    // The message runs up to the first ` <ident>=`.
    let mut fields = BTreeMap::new();
    let parts: Vec<&str> = rest.split(' ').collect();
    let first_field = parts
        .iter()
        .position(|p| {
            p.split_once('=')
                .is_some_and(|(k, _)| !k.is_empty() && k.chars().all(char::is_alphanumeric))
        })
        .unwrap_or(parts.len());
    let message = parts[..first_field].join(" ");
    let mut key: Option<String> = None;
    let mut acc = String::new();
    for p in &parts[first_field..] {
        match p.split_once('=') {
            Some((k, v)) if !k.is_empty() && k.chars().all(char::is_alphanumeric) => {
                if let Some(prev) = key.take() {
                    fields.insert(prev, std::mem::take(&mut acc));
                }
                key = Some(k.to_string());
                acc = v.to_string();
            }
            _ => {
                acc.push(' ');
                acc.push_str(p);
            }
        }
    }
    if let Some(prev) = key {
        fields.insert(prev, acc);
    }
    Some((level.to_lowercase(), message, fields))
}

fn mount_of(db: &Db, character_id: &str, mount: &str) -> Option<String> {
    if mount == "none" {
        return None;
    }
    let id = character_id.to_string();
    let raw = db
        .read_main(|main| db.read_mount_index(|m| characters_read::find_by_id(main, m, &id)))
        .unwrap_or_else(|e| panic!("findById {character_id}: {e:?}"))
        .unwrap_or_else(|| panic!("character {character_id} missing"));
    raw["characterDocumentMountPointId"]
        .as_str()
        .map(str::to_string)
}

fn opt(v: &Option<String>) -> Value {
    v.as_ref().map(|s| json!(s)).unwrap_or(Value::Null)
}

#[test]
fn canon_commonplace_tier2_matches_oracle() {
    global_capture::install();
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_CANON_COMMONPLACE"),
        std::env::var("QT_FIXTURE_CANON_MAIN"),
        std::env::var("QT_FIXTURE_CANON_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_CANON_COMMONPLACE + QT_FIXTURE_CANON_MAIN/MOUNT (see header)."
        );
        return;
    };
    let spec: SpecW = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../harness/oracle/fixtures/canon-commonplace-tier2.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let oracle: Vec<OracleRow> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        spec.ops.len(),
        oracle.len(),
        "op-count mismatch — regenerate"
    );

    let scratch = tempfile::Builder::new()
        .prefix("qt-canon-commonplace-rust-")
        .tempdir()
        .unwrap();
    let work_main = scratch.path().join("main.db");
    let work_mount = scratch.path().join("mount.db");
    std::fs::copy(&fixture_main, &work_main).unwrap();
    std::fs::copy(&fixture_mount, &work_mount).unwrap();
    let db = Db::open(
        DbPaths {
            main: work_main,
            mount_index: Some(work_mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    let mut failed: Vec<String> = Vec::new();
    let mut digest_ops = 0usize;
    for (op, want) in spec.ops.into_iter().zip(oracle) {
        assert_eq!(op.name, want.name, "op order mismatch");
        let name = op.name.clone();
        let ((value, block), lines) = global_capture::capture(|| match op.op.as_str() {
            "other" => {
                let observer = op.observer.clone().unwrap();
                let mount = mount_of(&db, &observer, &op.mount);
                let s: SubjectW = serde_json::from_value(op.subject.clone().unwrap()).unwrap();
                let subject = CanonSubject {
                    id: s.id,
                    name: s.name,
                    identity: s.identity,
                    description: s.description,
                };
                let canon = load_canon_for_observer_about_subject(
                    &db,
                    mount.as_deref(),
                    &observer,
                    &subject,
                    op.include_commonplace.unwrap_or(true),
                );
                let mut v = Map::new();
                v.insert("characterId".into(), json!(canon.character_id));
                v.insert("characterName".into(), json!(canon.character_name));
                v.insert("body".into(), opt(&canon.body));
                v.insert("source".into(), json!(canon.source.as_str()));
                if let Some(c) = &canon.commonplace {
                    v.insert("commonplace".into(), json!(c));
                }
                (Value::Object(v), Some(render_other_canon_block(&canon)))
            }
            "self" => {
                let c = op.character.as_ref().unwrap();
                let mount = mount_of(&db, &c.id, &op.mount);
                let canon = load_canon_for_self_with_commonplace(
                    &db,
                    &c.id,
                    &c.name,
                    c.manifesto.as_deref(),
                    c.personality.as_deref(),
                    c.description.as_deref(),
                    c.identity.as_deref(),
                    mount.as_deref(),
                );
                let mut v = Map::new();
                v.insert("characterId".into(), json!(canon.character_id));
                v.insert("characterName".into(), json!(canon.character_name));
                v.insert("manifesto".into(), opt(&canon.manifesto));
                v.insert("personality".into(), opt(&canon.personality));
                v.insert("description".into(), opt(&canon.description));
                v.insert("identity".into(), opt(&canon.identity));
                if let Some(cp) = &canon.commonplace {
                    v.insert("commonplace".into(), json!(cp));
                }
                (Value::Object(v), Some(render_self_canon_block(&canon)))
            }
            _ => {
                let holder = op.holder.clone().unwrap();
                let mount = mount_of(&db, &holder, &op.mount);
                let subject = match serde_json::from_value::<CommonplaceSubjectW>(
                    op.subject.clone().unwrap(),
                )
                .unwrap()
                {
                    CommonplaceSubjectW::SelfTag(t) => {
                        assert_eq!(t, "self");
                        CommonplaceSubject::SelfFile
                    }
                    CommonplaceSubjectW::Other { id, name } => {
                        CommonplaceSubject::Other { id, name }
                    }
                };
                let text = load_commonplace_canon(&db, &holder, mount.as_deref(), &subject);
                (opt(&text), None)
            }
        });
        let got_lines: Vec<_> = lines.iter().filter_map(|l| rust_line(l)).collect();
        let want_lines: Vec<_> = want.logs.iter().map(oracle_line).collect();
        if !want_lines.is_empty() {
            digest_ops += 1;
        }
        let ok_value = value == want.value;
        let ok_block = block == want.block;
        let ok_lines = got_lines == want_lines;
        for (_, message, _) in got_lines.iter().chain(want_lines.iter()) {
            assert_ne!(
                message, "[Canon] Commonplace digest unavailable; canon continues without it",
                "{name}: the unreachable catch line fired"
            );
        }
        if !(ok_value && ok_block && ok_lines) {
            eprintln!(
                "{name}: value {ok_value} block {ok_block} lines {ok_lines}\n  got value {value}\n want value {}\n  got block {block:?}\n want block {:?}\n  got lines {got_lines:?}\n want lines {want_lines:?}",
                want.value, want.block
            );
            failed.push(name);
        }
    }
    assert!(
        failed.is_empty(),
        "{} op(s) diverge: {failed:?}",
        failed.len()
    );
    assert!(
        digest_ops >= 8,
        "too few digest-loading ops ({digest_ops}) — stale oracle?"
    );
    eprintln!("OK: canon-commonplace matched the oracle");
}
