//! Read-back differential for the PUBLIC wardrobe write path (seam #7):
//! `quilltap-core::db::vault_wardrobe_public` vs v4's REAL
//! `WardrobeRepository.create`/`.update`/`.delete`.
//!
//! Both sides start from the SAME baked fixture (a character + empty vault, spanning
//! a MAIN db + a MOUNT-INDEX db), drive the same op sequence through the public
//! path, and after each op read the target character's `Wardrobe/` back via the
//! verified-equivalent read. The comparison is the read-back item list per op
//! (plus a small result tag), with each item's minted `updatedAt` normalized —
//! the tier a byte-level table dump can't reach, because an update's fresh
//! timestamp lands inside a content-addressed `.md`. The projection primitive is
//! separately byte-verified (`vault_wardrobe_write_equivalence`).
//!
//! Generate the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WPUB_MAIN=/tmp/qt-wpub-main.db QT_FIXTURE_WPUB_MOUNT=/tmp/qt-wpub-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-vault-wardrobe-public-fixture.ts
//!   QT_FIXTURE_WPUB_MAIN=/tmp/qt-wpub-main.db QT_FIXTURE_WPUB_MOUNT=/tmp/qt-wpub-mount.db \
//!     $N/npx tsx $V5/harness/oracle/cases/vault-wardrobe-public.ts > /tmp/oracle-wpub.ndjson
//! Run:
//!   QT_ORACLE_WPUB=/tmp/oracle-wpub.ndjson \
//!   QT_FIXTURE_WPUB_MAIN=/tmp/qt-wpub-main.db QT_FIXTURE_WPUB_MOUNT=/tmp/qt-wpub-mount.db \
//!     cargo test -p quilltap-harness --test vault_wardrobe_public_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::vault_wardrobe_public::{
    create_vault_wardrobe_item, delete_vault_wardrobe_item, update_vault_wardrobe_item,
    WardrobePatch, WardrobePublicError,
};
use quilltap_core::db::{characters_read, vault_read_overlay, Writer};
use quilltap_core::vault_overlay::WardrobeItem;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "characterId")]
    character_id: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    op: String,
    #[serde(rename = "readBack")]
    read_back: String,
    // create
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    options: Option<Value>,
    // update / delete
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "characterId")]
    character_id: Option<String>,
    #[serde(default)]
    patch: Option<Value>,
}

fn str_field(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
fn str_array(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Build the `WardrobeItem` v4's public `create` materializes from `{data,
/// options}`: id/createdAt/updatedAt from options; componentItemIds/replace
/// defaulted; every other optional absent (v4 spreads only `data`'s keys).
fn item_for_create(data: &Value, options: &Value) -> WardrobeItem {
    WardrobeItem {
        id: str_field(options, "id"),
        character_id: Some(Some(str_field(data, "characterId"))),
        title: str_field(data, "title"),
        description: None,
        image_prompt: None,
        types: str_array(data, "types"),
        component_item_ids: str_array(data, "componentItemIds"),
        appropriateness: None,
        is_default: false,
        replace: data
            .get("replace")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        migrated_from_clothing_record_id: None,
        archived_at: str_field_opt(data, "archivedAt"),
        created_at: str_field(options, "createdAt"),
        updated_at: str_field(options, "updatedAt"),
    }
}

fn str_field_opt(v: &Value, k: &str) -> Option<Option<String>> {
    match v.get(k) {
        Some(Value::String(s)) => Some(Some(s.clone())),
        _ => None,
    }
}

/// Translate a JSON patch object into a typed `WardrobePatch` (only the keys the
/// corpus exercises; each present key sets its field).
fn patch_from_json(patch: &Value) -> WardrobePatch {
    let obj = patch.as_object();
    let has = |k: &str| obj.map(|o| o.contains_key(k)).unwrap_or(false);
    WardrobePatch {
        title: has("title").then(|| str_field(patch, "title")),
        types: has("types").then(|| str_array(patch, "types")),
        component_item_ids: has("componentItemIds").then(|| str_array(patch, "componentItemIds")),
        description: has("description").then(|| {
            patch
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string)
        }),
        image_prompt: has("imagePrompt").then(|| {
            patch
                .get("imagePrompt")
                .and_then(Value::as_str)
                .map(str::to_string)
        }),
        appropriateness: has("appropriateness").then(|| {
            patch
                .get("appropriateness")
                .and_then(Value::as_str)
                .map(str::to_string)
        }),
        is_default: has("isDefault").then(|| {
            patch
                .get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        }),
        replace: has("replace").then(|| {
            patch
                .get("replace")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        }),
        archived_at: has("archivedAt").then(|| {
            patch
                .get("archivedAt")
                .and_then(Value::as_str)
                .map(str::to_string)
        }),
    }
}

fn threw_reason(e: &WardrobePublicError) -> &'static str {
    match e {
        WardrobePublicError::NoMount => "nomount",
        WardrobePublicError::Cycle(_) => "cycle",
        WardrobePublicError::Db(_) => "db",
    }
}

/// Normalize a read-back items array for comparison: sort by id, blank each
/// minted `updatedAt` to `<ts>`.
fn normalize_items(items: &mut Value) {
    if let Some(arr) = items.as_array_mut() {
        for it in arr.iter_mut() {
            if let Some(o) = it.as_object_mut() {
                o.insert("updatedAt".to_string(), Value::String("<ts>".to_string()));
            }
        }
        arr.sort_by(|a, b| {
            a.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .cmp(b.get("id").and_then(Value::as_str).unwrap_or_default())
        });
    }
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/vault-wardrobe-public-tier2.json")
}

#[test]
fn vault_wardrobe_public_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_WPUB") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_WPUB to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_WPUB_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_WPUB_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_WPUB_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_WPUB_MOUNT to the mount fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");
    let want_results = oracle
        .get("results")
        .and_then(Value::as_array)
        .expect("oracle has results array");
    assert_eq!(
        want_results.len(),
        spec.ops.len(),
        "oracle op count != corpus"
    );

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-wpub-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-wpub-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    // Resolve the read-back character's mount once (same for every op here).
    let owner = characters_read::find_by_id_raw(main.connection(), &spec.character_id)
        .expect("find owner")
        .expect("owner exists");
    let mount_point_id = owner
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)
        .expect("owner has vault mount")
        .to_string();

    let mut got_results: Vec<Value> = Vec::new();
    for op in &spec.ops {
        let links = mount.doc_mount_file_links();
        let docs = mount.doc_mount_documents();
        let result = match op.op.as_str() {
            "create" => {
                let item = item_for_create(
                    op.data.as_ref().expect("create data"),
                    op.options.as_ref().expect("create options"),
                );
                match create_vault_wardrobe_item(main.connection(), &links, &docs, &item) {
                    Ok(_) => json!({ "kind": "ok" }),
                    Err(e) => json!({ "kind": "threw", "reason": threw_reason(&e) }),
                }
            }
            "update" => {
                let patch = patch_from_json(op.patch.as_ref().expect("update patch"));
                match update_vault_wardrobe_item(
                    main.connection(),
                    &links,
                    &docs,
                    op.id.as_deref().expect("update id"),
                    &patch,
                    op.character_id.as_deref().expect("update characterId"),
                ) {
                    Ok(Some(_)) => json!({ "kind": "ok" }),
                    Ok(None) => json!({ "kind": "none" }),
                    Err(e) => json!({ "kind": "threw", "reason": threw_reason(&e) }),
                }
            }
            "delete" => match delete_vault_wardrobe_item(
                main.connection(),
                &links,
                &docs,
                op.id.as_deref().expect("delete id"),
                op.character_id.as_deref().expect("delete characterId"),
            ) {
                Ok(b) => json!({ "kind": "deleted", "value": b }),
                Err(e) => json!({ "kind": "threw", "reason": threw_reason(&e) }),
            },
            other => panic!("unknown op {other}"),
        };

        // Read the target character's Wardrobe/ back through the verified read.
        let vault = vault_read_overlay::read_character_vault_wardrobe(
            &mount.doc_mount_documents(),
            &mount_point_id,
            &op.read_back,
        )
        .unwrap_or_else(|e| panic!("read-back: {e:?}"));
        let items = vault
            .and_then(|v| v.get("items").cloned())
            .unwrap_or_else(|| json!([]));
        got_results.push(json!({ "op": op.op, "result": result, "items": items }));
    }
    drop(main);
    drop(mount);
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    for (i, want) in want_results.iter().enumerate() {
        let mut got = got_results[i].clone();
        let mut want = want.clone();
        normalize_items(&mut got["items"]);
        normalize_items(&mut want["items"]);
        assert_eq!(
            got, want,
            "op {i} ({}) diverged\n  rust:   {got}\n  oracle: {want}",
            spec.ops[i].op
        );
    }

    eprintln!(
        "OK: vault-wardrobe-public matched oracle ({} ops).",
        spec.ops.len()
    );
}
