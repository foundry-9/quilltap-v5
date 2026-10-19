//! The proving bench's wardrobe lens (v4 `lib/pascal/bench-wardrobe.ts`,
//! `5cb0e0190`) — which garment a dry run's wardrobe gestures WOULD have found,
//! had the character in the bench's chair rolled.
//!
//! The bench's doctrine is "the very same machinery a live chat uses", so this
//! resolves through the applier's own front half: [`load_wearable_pool`] for the
//! pool, [`find_in_pool`] for a take-off (archived items may come off) and
//! [`resolve_wearable`] for a put-on (which carries the archived and slot
//! refusals in their own words). It never calls a wear, a displacement or a
//! wardrobe notification: a read, and nothing else.
//!
//! The pool has no project tier — the bench has no chat — exactly as a run in a
//! chat with no project resolves.
//!
//! The result is the WIRE (`wardrobeResolution`, the preview body's LAST key),
//! built as `Value`s in v4's key order rather than a typed struct: the SPA owns
//! the type (`PreviewWardrobeResolution`), and the bytes are the contract.

use serde_json::{json, Map, Value};

use super::custom_tool_types::WardrobeEffectAction;
use super::custom_tools::ResolvedEffect;
use crate::db::runtime::Db;
use crate::wear_ops::{
    find_in_pool, resolve_wearable, wardrobe_item_not_found_message, PutOnMode, WearableRef,
    WearableRefusal, WearableResolution,
};
use crate::wearable_pool::{load_wearable_pool, LoadWearablePoolOptions, WearablePool};

const CONTEXT: &str = "custom-tools.preview";

/// v4 `foundOf(ref, item)` — `{ ref, found: { id, title, types, origin,
/// imageFileId } }`. `origin` is the pool item's own object (v4's
/// `WardrobeOrigin`, key order `scope, id, name`); an absent `imageFileId` is
/// `null`.
fn found_of(r: &str, item: &Value) -> Value {
    let mut found = Map::new();
    found.insert("id".into(), item.get("id").cloned().unwrap_or(Value::Null));
    found.insert(
        "title".into(),
        item.get("title").cloned().unwrap_or(Value::Null),
    );
    found.insert(
        "types".into(),
        item.get("types").cloned().unwrap_or_else(|| json!([])),
    );
    found.insert(
        "origin".into(),
        item.get("origin").cloned().unwrap_or(Value::Null),
    );
    found.insert(
        "imageFileId".into(),
        item.get("imageFileId")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(Value::Null),
    );
    json!({ "ref": r, "found": Value::Object(found) })
}

/// A miss, in v4's key order `ref, found, reason, message`.
fn miss(r: &str, reason: WearableRefusal, message: String) -> Value {
    json!({ "ref": r, "found": null, "reason": reason.as_str(), "message": message })
}

/// v4 `resolveBenchWardrobeAgainst(pool, effects)` — resolve every wardrobe
/// gesture's items against an already-loaded pool. PURE.
///
/// Skips every non-wardrobe effect and every `clear_slot` (a cleared slot
/// names no item; there is nothing to find). Each RENDERED ref is looked up as
/// both an id and a title; a `take_off` goes through [`find_in_pool`] (archived
/// items ARE found), a put-on through [`resolve_wearable`] with `mode ?? wear`.
/// The not-found sentence is v4's `wardrobeItemNotFoundMessage(null, ref)` —
/// `Wardrobe item not found with title "<ref>"` — deliberately NOT
/// `resolve_wearable`'s own, which would name the ref twice (id AND title).
/// Each entry carries the DEFINITION effect's index, so skipped effects keep
/// their numbers; a gesture whose items rendered empty still pushes `[]`.
pub fn resolve_bench_wardrobe_against(
    pool: &WearablePool,
    effects: &[ResolvedEffect],
) -> Vec<Value> {
    let mut out = Vec::new();
    for effect in effects {
        let ResolvedEffect::Wardrobe { index, wardrobe } = effect else {
            continue;
        };
        // A cleared slot names no item; there is nothing to find.
        if wardrobe.action == WardrobeEffectAction::ClearSlot {
            continue;
        }
        let items: Vec<Value> = wardrobe
            .items
            .iter()
            .map(|r| {
                let lookup = WearableRef {
                    item_id: Some(r),
                    item_title: Some(r),
                };
                if wardrobe.action == WardrobeEffectAction::TakeOff {
                    return match find_in_pool(pool, lookup) {
                        Some(item) => found_of(r, item),
                        None => miss(
                            r,
                            WearableRefusal::NotFound,
                            wardrobe_item_not_found_message(None, Some(r)),
                        ),
                    };
                }
                match resolve_wearable(
                    pool,
                    lookup,
                    wardrobe.mode.unwrap_or(PutOnMode::Wear),
                    wardrobe.slot.as_deref(),
                ) {
                    WearableResolution::Ok(item) => found_of(r, item),
                    WearableResolution::Refused { reason, message } => {
                        let message = if reason == WearableRefusal::NotFound {
                            wardrobe_item_not_found_message(None, Some(r))
                        } else {
                            message
                        };
                        miss(r, reason, message)
                    }
                }
            })
            .collect();
        out.push(json!({ "index": index, "items": items }));
    }
    out
}

/// v4 `resolveBenchWardrobe(repos, characterId, effects, toolName)` — the
/// bench's wardrobe resolution for `character_id`, or `None` when the run
/// carries no wardrobe gesture (no read, no line — no key on the wire, never
/// `[]`) or the wardrobe could not be read (WARN, fail-soft — the roll result
/// still returns).
///
/// v4's catch is reachable only by mock: its real `loadWearablePool` never
/// throws, and neither does [`load_wearable_pool`]. The v5 home of the WARN is
/// the one failure left — the read connection itself (`read_main` /
/// `read_mount_index` `Err`, e.g. a degraded mount index).
pub fn resolve_bench_wardrobe(
    db: &Db,
    character_id: &str,
    effects: &[ResolvedEffect],
    tool_name: &str,
) -> Option<Vec<Value>> {
    let gestures: Vec<ResolvedEffect> = effects
        .iter()
        .filter(|effect| match effect {
            ResolvedEffect::Wardrobe { wardrobe, .. } => {
                wardrobe.action != WardrobeEffectAction::ClearSlot
            }
            _ => false,
        })
        .cloned()
        .collect();
    if gestures.is_empty() {
        return None;
    }

    // No project mounts and no chat: the lens has no project tier.
    let read = db.read_main(|main| {
        db.read_mount_index(|mount| {
            Ok(load_wearable_pool(
                main,
                mount,
                character_id,
                Some(&[]),
                LoadWearablePoolOptions {
                    operator: true,
                    ..Default::default()
                },
            ))
        })
    });
    let pool = match read {
        Ok(pool) => pool,
        Err(error) => {
            tracing::warn!(
                target: "quilltap::pascal",
                context = CONTEXT,
                tool = tool_name,
                characterId = character_id,
                error = %crate::db::fallback::error_text(&error),
                "Workbench preview: wardrobe resolution skipped — the wardrobe could not be read",
            );
            return None;
        }
    };

    let resolution = resolve_bench_wardrobe_against(&pool, &gestures);
    let (mut found, mut missing) = (0u64, 0u64);
    for entry in &resolution {
        for item in entry["items"].as_array().into_iter().flatten() {
            if item["found"].is_null() {
                missing += 1;
            } else {
                found += 1;
            }
        }
    }
    tracing::debug!(
        target: "quilltap::pascal",
        context = CONTEXT,
        tool = tool_name,
        characterId = character_id,
        found,
        missing,
        "Workbench preview: wardrobe gestures resolved against the chair",
    );
    Some(resolution)
}

#[cfg(test)]
mod tests {
    //! The lens's READ half (`resolve_bench_wardrobe`), over a real provisioned
    //! `Db`: v4's DEBUG on a seated read, the fail-soft WARN at its v5 home
    //! (R-10: a degraded mount index — v4 reaches its catch only by mock), and
    //! the silence legs. The pure half is `pascal_bench_wardrobe_equivalence`.
    use super::*;
    use crate::pascal::custom_tools::ResolvedWardrobeGesture;
    use crate::test_support::global_capture;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR: &str = "a1000000-0000-4000-8000-00000000000a";

    fn db(with_mount: bool) -> (Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: with_mount.then(|| data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (db, dir)
    }

    fn gesture(action: WardrobeEffectAction, items: &[&str], index: usize) -> ResolvedEffect {
        ResolvedEffect::Wardrobe {
            index,
            wardrobe: ResolvedWardrobeGesture {
                action,
                items: items.iter().map(|s| s.to_string()).collect(),
                mode: None,
                slot: None,
            },
        }
    }

    fn pascal_lines(lines: Vec<String>) -> Vec<String> {
        lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::pascal "))
            .collect()
    }

    /// A seated read: every ref misses an empty wardrobe, and v4's DEBUG counts
    /// hits and misses across ALL entries; the cleared slot contributes none.
    #[test]
    fn a_seated_read_resolves_and_logs_the_counts() {
        global_capture::install();
        let (db, _dir) = db(true);
        let effects = vec![
            gesture(WardrobeEffectAction::Wear, &["Sundress", "Cloak"], 0),
            gesture(WardrobeEffectAction::ClearSlot, &[], 1),
            gesture(WardrobeEffectAction::TakeOff, &["Old Coat"], 2),
        ];
        let (out, lines) =
            global_capture::capture(|| resolve_bench_wardrobe(&db, CHAR, &effects, "dresser"));
        let out = out.expect("a resolution");
        assert_eq!(
            serde_json::to_string(&out).unwrap(),
            r#"[{"index":0,"items":[{"ref":"Sundress","found":null,"reason":"not_found","message":"Wardrobe item not found with title \"Sundress\""},{"ref":"Cloak","found":null,"reason":"not_found","message":"Wardrobe item not found with title \"Cloak\""}]},{"index":2,"items":[{"ref":"Old Coat","found":null,"reason":"not_found","message":"Wardrobe item not found with title \"Old Coat\""}]}]"#
        );
        assert_eq!(
            pascal_lines(lines),
            vec![format!(
                "DEBUG quilltap::pascal Workbench preview: wardrobe gestures resolved against \
                 the chair context=custom-tools.preview tool=dresser characterId={CHAR} \
                 found=0 missing=3"
            )]
        );
    }

    /// R-10: the wardrobe cannot be read (no mount index — P4.159's degraded
    /// shape): v4's WARN with the bare message, `None` (no key on the wire),
    /// and no DEBUG. At the route this arm is not reachable through a SEATED
    /// preview in v5 — the sheet's hydration reads the mount index first and
    /// answers 500 — so it is pinned here, at the lens.
    #[test]
    fn an_unreadable_wardrobe_warns_and_skips() {
        global_capture::install();
        let (db, _dir) = db(false);
        let effects = vec![gesture(WardrobeEffectAction::Wear, &["Sundress"], 0)];
        let (out, lines) =
            global_capture::capture(|| resolve_bench_wardrobe(&db, CHAR, &effects, "dresser"));
        assert!(out.is_none());
        let lines = pascal_lines(lines);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(&format!(
                "WARN quilltap::pascal Workbench preview: wardrobe resolution skipped — the \
                 wardrobe could not be read context=custom-tools.preview tool=dresser \
                 characterId={CHAR} error="
            )),
            "{}",
            lines[0]
        );
        assert!(
            !lines[0].contains("sqlite error"),
            "the bare message: {}",
            lines[0]
        );
    }

    /// The silence leg: no qualifying gesture (only a cleared slot, or no
    /// wardrobe effect at all) → `None` with NO read and NO line — even on a
    /// `Db` whose wardrobe could not be read.
    #[test]
    fn no_gesture_reads_nothing_and_says_nothing() {
        global_capture::install();
        let (db, _dir) = db(false);
        for effects in [
            vec![],
            vec![gesture(WardrobeEffectAction::ClearSlot, &[], 0)],
            vec![ResolvedEffect::Skipped {
                index: 0,
                reason: "condition did not hold".into(),
            }],
        ] {
            let (out, lines) =
                global_capture::capture(|| resolve_bench_wardrobe(&db, CHAR, &effects, "dresser"));
            assert!(out.is_none());
            assert!(pascal_lines(lines).is_empty());
        }
    }
}
