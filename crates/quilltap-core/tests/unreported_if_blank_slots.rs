//! Regression net for "unreported-if-blank" wardrobe slots (v4
//! `__tests__/unit/lib/wardrobe/unreported-if-blank-slots.test.ts`, mirrored).
//!
//! A slot whose `report_when_empty` is false (today: `hair`) must vanish from
//! EVERY report when it is empty — prose summaries, image prompts, Aurora
//! whispers, and the per-slot dumps in wardrobe tool results. Emptiness there
//! does not mean absence: a character with no hair item has ordinary hair, and
//! an image model told "hair: (empty)" or "no hairdo" will happily render
//! someone bald.
//!
//! These tests exist to fail loudly if a future slot list, prompt, or dump
//! starts announcing the blank. (v4's `image-analysis.test.ts` sibling is
//! skipped — that surface is unported.)

use quilltap_core::services::aurora_notifications::{
    build_opening_outfit_content, build_opening_outfit_opaque_content, build_outfit_change_content,
    build_outfit_change_opaque_content,
};
use quilltap_core::tools::{wardrobe_create, wardrobe_take_off, wardrobe_wear};
use quilltap_core::wardrobe::{
    build_outfit_slot_values, describe_outfit, is_slot_reported_when_empty, OutfitSlotValues,
    Slots, CLOTHING_SLOT_TYPES, UNREPORTED_IF_BLANK_SLOT_TYPES, WARDROBE_SLOT_META,
    WARDROBE_SLOT_TYPES,
};
use serde_json::json;

/// Words that would tell a reader or an image model the character has no hair.
const BALDNESS_TELLS: [&str; 6] = [
    "hair",
    "bald",
    "hairless",
    "unstyled",
    "no hairdo",
    "no hairstyle",
];

fn expect_no_hair_mention(text: &str, label: &str) {
    let lowered = text.to_lowercase();
    for tell in BALDNESS_TELLS {
        assert!(
            !lowered.contains(tell),
            "[{label}] leaked {tell:?} into a report with a blank hair slot:\n{text}"
        );
    }
}

fn outfit(f: impl FnMut(&str) -> Vec<String>) -> OutfitSlotValues {
    build_outfit_slot_values(f)
}

// ── the slot registry ────────────────────────────────────────────────────────

#[test]
fn registry_marks_hair_unreported_and_every_garment_slot_reported() {
    assert!(!is_slot_reported_when_empty("hair"));
    assert_eq!(UNREPORTED_IF_BLANK_SLOT_TYPES, ["hair"]);
    for slot in CLOTHING_SLOT_TYPES {
        assert!(is_slot_reported_when_empty(slot));
    }
}

#[test]
fn empty_fallback_is_non_null_exactly_for_reported_slots() {
    for (slot, meta) in WARDROBE_SLOT_TYPES.iter().zip(WARDROBE_SLOT_META.iter()) {
        assert_eq!(
            meta.empty_fallback.is_none(),
            !meta.report_when_empty,
            "slot {slot}"
        );
    }
}

// ── describe_outfit never announces a blank hair slot ───────────────────────

#[test]
fn dressed_character_with_no_hairdo_says_nothing_about_hair() {
    let out = describe_outfit(&outfit(|slot| match slot {
        "top" => vec!["linen shirt".into()],
        "bottom" => vec!["wool trousers".into()],
        "footwear" => vec!["oxfords".into()],
        "accessories" => vec!["pocket watch".into()],
        _ => vec![],
    }));
    expect_no_hair_mention(&out, "dressed, no hairdo");
}

#[test]
fn partly_dressed_character_with_no_hairdo_keeps_garment_negatives_only() {
    let out = describe_outfit(&outfit(|slot| {
        if slot == "bottom" {
            vec!["wool trousers".into()]
        } else {
            vec![]
        }
    }));
    // The garment negatives are still there — those ARE information.
    assert!(out.contains("topless"));
    assert!(out.contains("barefoot"));
    expect_no_hair_mention(&out, "partly dressed, no hairdo");
}

#[test]
fn fully_undressed_character_collapses_with_no_hair_mention() {
    let out = describe_outfit(&OutfitSlotValues::default());
    assert_eq!(out, "- completely naked and unadorned\n");
    expect_no_hair_mention(&out, "fully undressed");
}

#[test]
fn still_renders_the_hairdo_when_one_is_set() {
    let out = describe_outfit(&outfit(|slot| {
        if slot == "hair" {
            vec!["marcel waves".into()]
        } else {
            vec![]
        }
    }));
    assert!(out.contains("- **hair:** marcel waves"));
}

// ── Aurora's whispers never announce a blank hair slot ──────────────────────

type WhisperBuilder = fn(&str, &OutfitSlotValues) -> String;

#[test]
fn aurora_whispers_omit_hair_entirely_when_the_slot_is_blank() {
    let o = outfit(|slot| match slot {
        "top" => vec!["linen shirt".into()],
        "bottom" => vec!["wool trousers".into()],
        _ => vec![],
    });
    let builders: [(&str, WhisperBuilder); 4] = [
        ("opening", build_opening_outfit_content),
        ("opening (opaque)", build_opening_outfit_opaque_content),
        ("change", build_outfit_change_content),
        ("change (opaque)", build_outfit_change_opaque_content),
    ];
    for (label, build) in builders {
        expect_no_hair_mention(&build("Bertie", &o), label);
    }
}

// ── wardrobe tool results never announce a blank hair slot ──────────────────

fn state_with(slot: &str, ids: &[&str]) -> serde_json::Value {
    let mut slots = Slots::fresh();
    for id in ids {
        slots.slot_mut(slot).push(id.to_string());
    }
    slots.to_value()
}

#[test]
fn wear_dump_omits_the_hair_row_when_hair_is_empty() {
    let text = wardrobe_wear::format(&wardrobe_wear::WardrobeWearToolOutput {
        success: true,
        operations: Vec::new(),
        current_state: state_with("top", &["shirt-1"]),
        coverage_summary: "- **top:** linen shirt\n".to_string(),
        error: None,
    });
    assert!(text.contains("top: shirt-1"));
    assert!(text.contains("accessories: (empty)"));
    assert!(!text.contains("hair"));
}

#[test]
fn wear_dump_still_lists_the_hair_row_when_a_hairdo_is_worn() {
    let text = wardrobe_wear::format(&wardrobe_wear::WardrobeWearToolOutput {
        success: true,
        operations: Vec::new(),
        current_state: state_with("hair", &["braid-1"]),
        coverage_summary: "- **hair:** braided crown\n".to_string(),
        error: None,
    });
    assert!(text.contains("hair: braid-1"));
}

#[test]
fn take_off_dump_omits_the_hair_row_when_hair_is_empty() {
    let text = wardrobe_take_off::format(&wardrobe_take_off::WardrobeTakeOffToolOutput {
        success: true,
        operations: Vec::new(),
        current_state: state_with("top", &["shirt-1"]),
        coverage_summary: "- **top:** linen shirt\n".to_string(),
        error: None,
    });
    assert!(text.contains("top: shirt-1"));
    assert!(text.contains("accessories: (empty)"));
    assert!(!text.contains("hair"));
}

#[test]
fn create_dump_omits_the_hair_row_when_hair_is_empty() {
    let text = wardrobe_create::format(&wardrobe_create::WardrobeCreateToolOutput {
        success: true,
        item_id: "item-1".to_string(),
        title: "Linen Shirt".to_string(),
        equipped: true,
        effect: Some("layered".to_string()),
        effect_summary: None,
        is_composite: None,
        resolved_types: None,
        resolved_component_item_ids: None,
        recipient_name: None,
        current_state: Some(state_with("top", &["item-1"])),
        error: None,
    });
    assert!(text.contains("top: item-1"));
    assert!(text.contains("accessories: (empty)"));
    assert!(!text.contains("hair"));
}

#[test]
fn create_dump_still_lists_the_hair_row_when_a_hairdo_is_worn() {
    let text = wardrobe_create::format(&wardrobe_create::WardrobeCreateToolOutput {
        success: true,
        item_id: "braid-1".to_string(),
        title: "Braided Crown".to_string(),
        equipped: true,
        effect: Some("layered".to_string()),
        effect_summary: None,
        is_composite: None,
        resolved_types: None,
        resolved_component_item_ids: None,
        recipient_name: None,
        current_state: Some(json!({
            "top": [], "bottom": [], "footwear": [], "accessories": [],
            "hair": ["braid-1"],
        })),
        error: None,
    });
    assert!(text.contains("hair: braid-1"));
}
