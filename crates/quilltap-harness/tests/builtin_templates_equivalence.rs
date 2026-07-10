//! Differential: the built-in roleplay-templates seeder (P4.4u3, family 1).
//!
//! Runs v5's `builtin_templates::seed_built_in_templates` over the same three
//! pre-states the v4 oracle drives through the REAL `seedBuiltInTemplates()`, and
//! diffs the resulting `roleplay_templates` dump (minted ids/timestamps
//! normalized, keyed by name+isBuiltIn). The delimiters bytes are the point:
//!
//!   - `fresh`         → INSERT path, SCHEMA-order delimiters;
//!   - `stale-builtin` → UPDATE path, SEED-LITERAL-order delimiters (v4 quirk);
//!   - `user-same-name`→ built-in inserted alongside a same-name user row.
//!
//! Generate the oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   : > /tmp/oracle-builtin-templates.ndjson
//!   for S in fresh stale-builtin user-same-name; do
//!     QT_STATE=$S $N/npx tsx \
//!       ~/source/quilltap-v5/harness/oracle/cases/builtin-templates.ts \
//!       >> /tmp/oracle-builtin-templates.ndjson
//!   done
//! Run:
//!   QT_ORACLE_BUILTIN_TEMPLATES=/tmp/oracle-builtin-templates.ndjson \
//!     cargo test -p quilltap-harness --test builtin_templates_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::db::roleplay_templates::{
    CreateOptions, DialogueDetection, RenderingPattern, RtCreate, StringOrPair, TemplateDelimiter,
};
use quilltap_core::db::Writer;
use quilltap_core::services::builtin_templates::seed_built_in_templates;
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    states: Vec<State>,
}

#[derive(Deserialize)]
struct State {
    name: String,
    pre: Vec<PreRow>,
}

#[derive(Deserialize)]
struct PreRow {
    id: String,
    #[serde(rename = "userId")]
    user_id: Option<String>,
    name: String,
    description: Option<String>,
    #[serde(rename = "systemPrompt")]
    system_prompt: String,
    #[serde(rename = "isBuiltIn")]
    is_built_in: bool,
    tags: Vec<String>,
    delimiters: Vec<TemplateDelimiter>,
    #[serde(rename = "renderingPatterns")]
    rendering_patterns: Vec<RenderingPattern>,
    #[serde(rename = "dialogueDetection")]
    dialogue_detection: Option<DialogueDetection>,
    #[serde(rename = "narrationDelimiters")]
    narration_delimiters: StringOrPair,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/builtin-templates-states.json")
}

/// Extract the `roleplay_templates` CREATE TABLE from the captured fresh schema —
/// the exact DDL v4's `ensureCollection` (generateDDL) produces, so both sides
/// store byte-identical rows.
fn roleplay_templates_ddl() -> String {
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src/services/provisioning/fresh_schema.json");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).expect("read fresh_schema"))
            .expect("parse fresh_schema");
    schema["main"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s.as_str())
        .find(|s| s.contains("CREATE TABLE \"roleplay_templates\""))
        .expect("roleplay_templates DDL present")
        .to_string()
}

/// Normalize a dump the way the oracle does: id/createdAt/updatedAt → fixed
/// placeholders, rows sorted by (name, isBuiltIn).
fn normalize(dump: &Value) -> Value {
    let mut rows: Vec<Value> = dump["rows"].as_array().unwrap().clone();
    for row in &mut rows {
        let obj = row.as_object_mut().unwrap();
        obj.insert("id".into(), Value::from("<id>"));
        obj.insert("createdAt".into(), Value::from("<ts>"));
        obj.insert("updatedAt".into(), Value::from("<ts>"));
    }
    rows.sort_by_key(|r| format!("{} {}", r["name"].as_str().unwrap_or(""), r["isBuiltIn"]));
    let mut m = Map::new();
    m.insert("table".into(), dump["table"].clone());
    m.insert("columns".into(), dump["columns"].clone());
    m.insert("rows".into(), Value::from(rows));
    Value::Object(m)
}

#[test]
fn builtin_templates_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_BUILTIN_TEMPLATES") else {
        eprintln!("SKIP: set QT_ORACLE_BUILTIN_TEMPLATES to the oracle NDJSON (see header).");
        return;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec"))
            .expect("parse spec");
    let ddl = roleplay_templates_ddl();

    // Index the oracle NDJSON by state name.
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let mut oracle_by_state = std::collections::HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        oracle_by_state.insert(v["state"].as_str().unwrap().to_string(), v);
    }

    for state in &spec.states {
        let want = oracle_by_state
            .get(&state.name)
            .unwrap_or_else(|| panic!("oracle missing state {}", state.name));

        // Fresh encrypted main DB with just the roleplay_templates table.
        let work = std::env::temp_dir().join(format!(
            "qt-builtin-tmpl-{}-{}.db",
            std::process::id(),
            state.name
        ));
        let _ = std::fs::remove_file(&work);
        let writer = Writer::open_writable(&work, &spec.test_pepper_base64).expect("open work db");
        writer.connection().execute_batch(&ddl).expect("create ddl");

        // Seed the pre-state through the real repo (pinned ids/timestamps).
        {
            let repo = writer.roleplay_templates();
            for row in &state.pre {
                repo.create(
                    &RtCreate {
                        user_id: row.user_id.clone(),
                        name: row.name.clone(),
                        description: row.description.clone(),
                        system_prompt: row.system_prompt.clone(),
                        is_built_in: row.is_built_in,
                        tags: row.tags.clone(),
                        delimiters: row.delimiters.clone(),
                        rendering_patterns: row.rendering_patterns.clone(),
                        dialogue_detection: row.dialogue_detection.clone(),
                        narration_delimiters: row.narration_delimiters.clone(),
                    },
                    &CreateOptions {
                        id: row.id.clone(),
                        created_at: row.created_at.clone(),
                        updated_at: row.updated_at.clone(),
                    },
                )
                .expect("pre-row create");
            }
        }

        // The unit under test.
        seed_built_in_templates(writer.connection()).expect("seed built-in templates");

        let got = normalize(
            &writer
                .dump_table_json("roleplay_templates", "id")
                .expect("dump"),
        );
        let want_norm = normalize(want);
        let _ = std::fs::remove_file(&work);

        assert_eq!(
            got["columns"], want_norm["columns"],
            "column set diverged for state {}",
            state.name
        );
        assert_eq!(
            got["rows"], want_norm["rows"],
            "state {} diverged\n  rust:   {}\n  oracle: {}",
            state.name, got["rows"], want_norm["rows"]
        );
        eprintln!(
            "OK: builtin-templates state '{}' matched oracle ({} rows).",
            state.name,
            got["rows"].as_array().map(|a| a.len()).unwrap_or(0)
        );
    }
}
