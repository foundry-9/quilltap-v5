//! Tier-1 differential (exact): the Inventory's image phrase and where it
//! lands in the pictures (P4.D326 item 16; v4 `4fbdad186` + `d88c1ce39` at
//! `322a5488e`).
//!
//! Both sides run `inventory-image-description.json`:
//!   - `phrases` → `inventory::image_description::render_carrying_phrase` vs v4's
//!     REAL `renderCarryingPhrase` (absent / null / empty / whitespace
//!     descriptions, kits, the trailing `.!?` RUN strip — a description that is
//!     ONLY punctuation is an EMPTY element — base-sensitivity sorting with ties
//!     in input order, a title over a name, 40 items);
//!   - `scene` → `appearance_resolution::scene_description_parts` vs
//!     `sceneDescriptionParts` (the one-UTF-16-unit capitalisation: `ß` → `SS`,
//!     an astral first character unchanged, `ǆ` → `Ǆ`, a leading combining mark);
//!   - `expansion` → `generate_image::build_expansion_context`'s placeholder
//!     `clothing` vs `buildExpansionContext`'s (the `Carried` entry, the key
//!     absent when there is neither outfit nor carried object);
//!   - `avatar` → `avatar_prompt::build_character_avatar_prompt` with no
//!     equipped slots vs `buildCharacterAvatarPrompt({}, character, {carrying})`
//!     (no repository read on either side; the scratch connections stay empty).
//!
//! Red-first was a MUTATION run (the family cannot run before the code it
//! calls exists): see the P4.D326 lane record.
//!
//! Generate the oracle (Node 24, from the v4 checkout — PIN REQUIRED when v4
//! HEAD is past the baseline; stage OUTSIDE any `.claude` path). See the oracle
//! header (`harness/oracle/cases/inventory-image-description.test.ts`). Run:
//!   QT_ORACLE_INVENTORY_IMAGE_DESCRIPTION=/tmp/oracle-inventory-image-description.ndjson \
//!     cargo test -p quilltap-harness --test inventory_image_description_equivalence

use std::collections::HashMap;

use quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository;
use quilltap_core::inventory::image_description::render_carrying_phrase;
use quilltap_core::inventory::types::ResolvedInventoryItem;
use quilltap_core::services::appearance_resolution::{
    scene_description_parts, ResolvedCharacterAppearance,
};
use quilltap_core::services::avatar_prompt::{build_character_avatar_prompt, AvatarPromptOptions};
use quilltap_core::tools::generate_image::{build_expansion_context, ResolvedPlaceholder};
use serde_json::{json, Value};

fn corpus() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/inventory-image-description.json"),
        )
        .expect("read the corpus"),
    )
    .expect("parse the corpus")
}

fn appearance(v: &Value) -> ResolvedCharacterAppearance {
    let s = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    ResolvedCharacterAppearance {
        character_id: s("characterId"),
        character_name: s("characterName"),
        physical_description: s("physicalDescription"),
        physical_description_name: s("physicalDescriptionName"),
        clothing_description: s("clothingDescription"),
        clothing_source: s("clothingSource"),
        carrying_description: v
            .get("carryingDescription")
            .and_then(Value::as_str)
            .map(str::to_string),
        was_sanitized: false,
        needs_concealment: false,
    }
}

#[test]
fn inventory_image_description_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_INVENTORY_IMAGE_DESCRIPTION") else {
        eprintln!("SKIP: QT_ORACLE_INVENTORY_IMAGE_DESCRIPTION unset");
        return;
    };
    let spec = corpus();
    let mut oracle: HashMap<(String, String), Value> = HashMap::new();
    for line in std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        let key = (
            v["kind"].as_str().expect("kind").to_string(),
            v["label"].as_str().expect("label").to_string(),
        );
        oracle.insert(key, v);
    }
    let want = |kind: &str, label: &str| -> &Value {
        oracle
            .get(&(kind.to_string(), label.to_string()))
            .unwrap_or_else(|| panic!("no oracle {kind} row for {label}"))
    };
    let mut compared = 0usize;

    // --- phrases ---
    for row in spec["phrases"].as_array().expect("phrases") {
        let label = row["label"].as_str().unwrap();
        let items: Vec<ResolvedInventoryItem> = row["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| ResolvedInventoryItem(v.as_object().unwrap().clone()))
            .collect();
        let got = render_carrying_phrase(&items);
        let expected = want("phrase", label)["phrase"].as_str().map(str::to_string);
        assert_eq!(got, expected, "phrase {label}");
        compared += 1;
    }

    // --- scene description parts ---
    for row in spec["scene"].as_array().expect("scene") {
        let label = row["label"].as_str().unwrap();
        let mut v = row.clone();
        v["characterId"] = json!("c");
        let got = scene_description_parts(
            &appearance(&v),
            row["genderPrefix"].as_str().unwrap_or_default(),
        );
        let expected: Vec<String> =
            serde_json::from_value(want("scene", label)["parts"].clone()).expect("scene parts");
        assert_eq!(got, expected, "scene {label}");
        compared += 1;
    }

    // --- the image tool's expansion context ---
    for row in spec["expansion"].as_array().expect("expansion") {
        let label = row["label"].as_str().unwrap();
        let app = appearance(&row["appearance"]);
        let placeholder = format!("{{{{{}}}}}", app.character_name);
        let context = build_expansion_context(
            &format!("Paint {placeholder}"),
            &[ResolvedPlaceholder {
                placeholder: placeholder.clone(),
                name: app.character_name.clone(),
                entity_id: Some(app.character_id.clone()),
                physical_description: None,
                pronouns: None,
            }],
            "OPENAI",
            None,
            Some(std::slice::from_ref(&app)),
        );
        let clothing = &context.placeholders[0].clothing;
        let o = want("expansion", label);
        assert_eq!(
            !clothing.is_empty(),
            o["hasClothing"].as_bool().unwrap(),
            "expansion {label}: the clothing key's presence"
        );
        let got: Vec<Value> = clothing
            .iter()
            .map(|c| json!({"name": c.name, "usageContext": c.usage_context, "description": c.description}))
            .collect();
        let expected: Vec<Value> = o["clothing"].as_array().cloned().unwrap_or_default();
        // Byte comparison (serde_json's Map equality ignores key order).
        assert_eq!(
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&expected).unwrap(),
            "expansion {label}"
        );
        compared += 1;
    }

    // --- the avatar portrait (no equipped slots: no repository read) ---
    let main = rusqlite::Connection::open_in_memory().unwrap();
    let mount = rusqlite::Connection::open_in_memory().unwrap();
    let docs = DocMountDocumentsRepository::new(&mount);
    for row in spec["avatar"].as_array().expect("avatar") {
        let label = row["label"].as_str().unwrap();
        let out = build_character_avatar_prompt(
            &main,
            &mount,
            &docs,
            &row["character"],
            &AvatarPromptOptions {
                carrying: row["carrying"].as_str().map(str::to_string),
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("avatar {label}: {e}"));
        let o = want("avatar", label);
        assert_eq!(
            out.prompt,
            o["prompt"].as_str().unwrap(),
            "avatar {label}: prompt"
        );
        assert_eq!(
            out.has_appearance,
            o["hasAppearance"].as_bool().unwrap(),
            "avatar {label}: hasAppearance"
        );
        compared += 1;
    }

    // Floors: every corpus row compared, and the phrase present where it must be.
    let rows = ["phrases", "scene", "expansion", "avatar"]
        .iter()
        .map(|k| spec[*k].as_array().unwrap().len())
        .sum::<usize>();
    assert_eq!(compared, rows, "every corpus row must be compared");
    assert_eq!(
        oracle.len(),
        rows,
        "the oracle must carry one row per corpus row"
    );
    assert!(
        oracle
            .values()
            .filter(|v| v["kind"] == "phrase")
            .filter(|v| v["phrase"].is_string())
            .count()
            >= 8,
        "the phrase rows must mostly render a phrase"
    );
}
