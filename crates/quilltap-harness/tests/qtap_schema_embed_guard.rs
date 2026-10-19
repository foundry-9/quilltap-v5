//! The vendored `.qtap` export-schema guard (P4.86, Tier 1 item 2).
//!
//! `crates/quilltap-core/src/generators/qtap-export.schema.json` is a
//! byte-copy of v4's `public/schemas/qtap-export.schema.json`, `include_str!`'d
//! into the binary — the `help/**` arrangement, because v4 reads its copy from
//! `process.cwd()` and a Rust engine has no cwd to read it from. That makes a
//! v4 commit touching the schema a **re-vendor obligation**: this test fails
//! the moment the two files differ, so the obligation cannot be missed.
//!
//! The v4 checkout is optional here — a machine without one skips (the harness
//! convention), and the SELF-consistency half (the embedded copy parses, has
//! the expected `$schema`/`$id`, and compiles) always runs.
//!
//! Run standalone:
//!   QT_V4_CHECKOUT=~/source/quilltap-server \
//!     cargo test -p quilltap-harness --test qtap_schema_embed_guard -- --nocapture

use quilltap_core::generators::qtap_schema::{validate_qtap_export, QTAP_EXPORT_SCHEMA_JSON};
use serde_json::{json, Value};

// P4.157 R-H: the ONE v4-checkout locator (`common::v4_root`).
mod common;

/// The vendored size at v4 `78b381a96` (P4.D171 — the route-trail message
/// field, `5841a8c62`; was 89,769 at `2f4254b42`). 93,384 at `31436bae4`
/// (the file-entry `generationKey`, v4 `7fbf8a55b`).
///
/// ⚠ `generators::qtap_schema`'s own `the_embedded_schema_compiles` carries a
/// SECOND copy of this number. A re-vendor moves both.
/// P4.D205: 95,266 at v4 `e7d77bb60` — the Inform re-vendor (`$defs.ChatInform`,
/// `chats.chatInforms`, `counts.chatInforms`). 93,384 at `31436bae4`.
/// P4.D225: 96,049 at v4 `49059fb14` — `8bd080267` (#73) widened the route
/// trail (`evidence` five-valued, `profileKind` between `evidence` and
/// `detail`, the three descriptions); `49059fb14` left the file alone (the
/// refusal ledger is NOT exported).
/// P4.D226: 96,967 at v4 `4d370a90f` — #75's `chats.conciergeOverride`
/// DEPRECATED (its description rewritten) and the three Concierge keys
/// `conciergeMode` / `conciergeModeSetBy` / `conciergeModeReason` after it.
/// P4.D249: 97,324 at v4 `52d6e7ecd` — `$defs.ChatInform.permanent` (a
/// boolean, default false) after `recordMessageId`, and `consumedAt` /
/// `consumedByMessageId` gaining their first-delivery descriptions.
/// P4.D264: 101,092 at v4 `f5e953a3f` — `3ee3b1342` (#81) added
/// `data.wardrobeWear` under BOTH `additionalProperties: false` data objects
/// (characters `:29`, document-stores `:183` — the one acceptance change),
/// `counts.wardrobeWear` and `$defs.WardrobeWear`; `7c8572869` (#82) added
/// `WardrobeItem.imageFileId` and `WardrobeItem._imageFiles` (documentation —
/// `$defs.WardrobeItem` was already `additionalProperties: true`).
/// P4.D272: 102,523 at v4 `01a83539d` (`d58548051`:
/// `ExportedChat.otherExtractionWatermarkMessageId`, `Memory.tier` /
/// `supersededById` / `consolidatedFrom` / `consolidatedAt`, the `source`
/// description). Both `$defs` are `additionalProperties: true`, so the move is
/// an acceptance tightening for PRESENT keys, never a new-key refusal; its
/// only behavioural consumer is the AI import's validator.
/// P4.D284: 103,075 at v4 `4a38222e6` — `$defs.WardrobeItem.folder` (a string,
/// `maxLength` 900, after `archivedAt`): the item's folder under `Wardrobe/`,
/// which importers file at the same path. `$defs.WardrobeItem` stays
/// `additionalProperties: true`, so the move tightens a PRESENT `folder` only
/// (a non-string or an over-long one).
/// P4.D294: 105,310 at v4 `83334b9a5` — three key groups, none of them
/// loosening anything: the scenario pair on `ExportedChat` after
/// `scenarioText` (`545e299bb` — `scenarioSource`, an object-or-null
/// requiring `mountPointId` (uuid format) / `path` (`maxLength` 500) /
/// `bodySha256` (`^[0-9a-f]{64}$`), and `scenarioNotes`, a string-or-null);
/// the extraction-cursor triple on `ExportedChat`
/// (`otherExtractionWatermarkMessageId` re-described as the bundle carriage
/// of the `memory_extraction_cursors` row, `episodeExtractionWatermarkMessageId`
/// from `c2795733f` / `ac6348c5f` and `turnExtractionWatermarkMessageId` from
/// `64743ee13`, each a string-or-null); and `Memory.supersededAt` after
/// `supersededById` (`c2795733f`, a Timestamp-or-null — bug 225's cold
/// retention clock). Both `$defs` stay `additionalProperties: true`, so every
/// move tightens a PRESENT key only.
/// P4.D313: 105,774 at v4 `25b8fab6a` — `pascalMeta.invokedBy` gains
/// `"trigger"` (`362426c8f`), NEW `triggerPhrase`, the `effects` description
/// (`f0db8cbb8`); the object stays open, so the move LOOSENS one enum and
/// refuses nothing new.
/// P4.D317: 108,678 at v4 `e8e17b2d9` — `7953da3a0` added `$defs.WardrobeHistory`
/// (`additionalProperties: true`; required `id`, `chatId`, `characterId`,
/// `entryIndex`, `slots`, `slotsHash` (16–16), `source` (the seven-value enum),
/// `actorCharacterId`, `isCurrent`, `createdAt`), `chats.wardrobeHistory` LAST
/// under the chats data object — `additionalProperties: false`, so a chats
/// bundle carrying history was REFUSED before (the one acceptance change) —
/// and `counts.wardrobeHistory`; `4ae20977f` added `actorToolName` (string or
/// null, NOT required) and re-described `actorCharacterId`.
const VENDORED_BYTES: usize = 108_678;

#[test]
fn the_embedded_schema_is_self_consistent() {
    assert_eq!(
        QTAP_EXPORT_SCHEMA_JSON.len(),
        VENDORED_BYTES,
        "the vendored schema's size moved — re-vendor deliberately, then update this constant"
    );
    let schema: Value = serde_json::from_str(QTAP_EXPORT_SCHEMA_JSON).expect("the schema parses");
    assert_eq!(
        schema["$schema"],
        json!("https://json-schema.org/draft/2020-12/schema")
    );
    assert_eq!(
        schema["$id"],
        json!("https://quilltap.app/schemas/qtap-export.schema.json")
    );
    // Every `$ref` is LOCAL — the engine is built with no resolver, and a
    // remote `$ref` would fail the compile rather than reach the network.
    let refs: Vec<&str> = collect_refs(&schema);
    assert!(!refs.is_empty(), "the schema carries $refs");
    for r in &refs {
        assert!(
            r.starts_with("#/$defs/"),
            "non-local $ref {r:?} — the engine has no resolver and must never need one"
        );
    }
    // It compiles (a compile failure would otherwise surface only as the
    // `Schema validation error:` refusal on every call).
    let result = validate_qtap_export(&json!({}));
    assert!(
        !result
            .errors
            .iter()
            .any(|e| e.starts_with("Schema validation error:")),
        "the vendored schema failed to compile: {:?}",
        result.errors
    );
}

#[test]
fn the_embedded_schema_equals_the_v4_checkouts() {
    let Some(root) = common::v4_root() else {
        eprintln!("SKIP: no v4 checkout (set QT_V4_CHECKOUT).");
        return;
    };
    let path = root.join("public/schemas/qtap-export.schema.json");
    let Ok(v4_bytes) = std::fs::read_to_string(&path) else {
        eprintln!("SKIP: {} is unreadable.", path.display());
        return;
    };
    assert!(
        v4_bytes == QTAP_EXPORT_SCHEMA_JSON,
        "the vendored qtap-export schema has DRIFTED from {} — v4 changed it, and the copy under \
         crates/quilltap-core/src/generators/ must be re-vendored (v4 {} bytes, vendored {} bytes)",
        path.display(),
        v4_bytes.len(),
        QTAP_EXPORT_SCHEMA_JSON.len()
    );
}

fn collect_refs(v: &Value) -> Vec<&str> {
    let mut out = Vec::new();
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
        match v {
            Value::Object(o) => {
                for (k, val) in o {
                    if k == "$ref" {
                        if let Some(s) = val.as_str() {
                            out.push(s);
                        }
                    }
                    walk(val, out);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(v, &mut out);
    out
}
