//! Wardrobe item drafts, server side (v4 `lib/wardrobe/item-draft.ts`, `600c2906f`;
//! P4.D377 item 7, the survey's R-D).
//!
//! In v4 the draft is the one form-state shape the item editor, Duplicate and
//! Save-as-outfit start from, and `draftToPayload` is the one way it becomes a
//! create body. v4's module is client-safe, and its server route
//! (`handleDuplicate` in `wardrobe-route-factory.ts`) imports it too: the copy is
//! built SERVER-SIDE so its title is chosen against the TIER's own titles
//! (`nextCopyTitle` over `location.readItems(true)`), not against the merged list
//! a browser happens to have on display. This twin is what the wardrobe
//! `POST ?action=duplicate` calls.
//!
//! Only what the duplicate route calls is ported: [`draft_from_item`] and
//! [`draft_to_payload`]. v4's `emptyDraft`, `payloadForEdit` and `validateDraft`
//! are the editor's alone; the SPA keeps them in `apps/web/src/app/wardrobe/
//! item-draft.ts` (P4.D377 Tier 3, item 25).
//!
//! The truthiness guards are the point: `?? ''` / `?? false` / `?? []` turn an
//! absent or null field into the form's empty value, and `blankToNull` sends a
//! whitespace-only text field as `null` — over JS `.trim()` ([`js_trim`]), never
//! Rust's `str::trim` (U+FEFF trims in JS; U+0085 does not).

use serde_json::{json, Value};

use crate::jsstr::js_trim;
use crate::vault_overlay::WardrobeItem;

/// Editable fields of a wardrobe item, as a form holds them (strings, never
/// null) — v4 `WardrobeItemDraft`, fields in v4's declaration order.
#[derive(Debug, Clone, PartialEq)]
pub struct WardrobeItemDraft {
    pub title: String,
    pub description: String,
    pub image_prompt: String,
    pub appropriateness: String,
    pub is_default: bool,
    pub types: Vec<String>,
    /// Non-empty makes the item a composite (an outfit).
    pub component_item_ids: Vec<String>,
    /// Composite-only: clear the designated slots on equip instead of layering.
    pub replace: bool,
    /// Who the item is offered to. Empty = everyone.
    pub pronoun_guidance: Vec<String>,
    /// Folder under `Wardrobe/` (`''` = root). Filing only.
    pub folder: String,
}

/// v4 `draftFromItem`: a draft holding an existing item's editable fields.
pub fn draft_from_item(item: &WardrobeItem) -> WardrobeItemDraft {
    let text = |v: &Option<Option<String>>| v.clone().flatten().unwrap_or_default();
    WardrobeItemDraft {
        title: item.title.clone(),
        description: text(&item.description),
        image_prompt: text(&item.image_prompt),
        appropriateness: text(&item.appropriateness),
        is_default: item.is_default,
        types: item.types.clone(),
        component_item_ids: item.component_item_ids.clone(),
        replace: item.replace,
        pronoun_guidance: item.pronoun_guidance.clone().unwrap_or_default(),
        folder: item.folder.clone(),
    }
}

/// v4 `isComposite` (`wardrobe.types.ts`): `(componentItemIds?.length ?? 0) > 0`.
fn is_composite(draft: &WardrobeItemDraft) -> bool {
    !draft.component_item_ids.is_empty()
}

/// v4 `draftToPayload`: the request body for a draft. Optional text fields left
/// blank go as `null`; `replace` is sent only for a composite (a leaf always
/// replaces its slots). Keys in v4's object-literal order (`title`,
/// `description`, `imagePrompt`, `types`, `appropriateness`, `isDefault`,
/// `componentItemIds`, `replace`, `pronounGuidance`, `folder`) — `serde_json`'s
/// `preserve_order` keeps them.
pub fn draft_to_payload(draft: &WardrobeItemDraft) -> Value {
    let blank_to_null = |value: &str| -> Value {
        if js_trim(value).is_empty() {
            Value::Null
        } else {
            Value::String(value.to_string())
        }
    };
    json!({
        "title": js_trim(&draft.title),
        "description": blank_to_null(&draft.description),
        "imagePrompt": blank_to_null(&draft.image_prompt),
        "types": draft.types,
        "appropriateness": blank_to_null(&draft.appropriateness),
        "isDefault": draft.is_default,
        "componentItemIds": draft.component_item_ids,
        "replace": if is_composite(draft) { draft.replace } else { false },
        "pronounGuidance": draft.pronoun_guidance,
        "folder": draft.folder,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(v: Value) -> WardrobeItem {
        WardrobeItem::from_read_value(&v)
    }

    #[test]
    fn payload_keys_follow_v4s_literal_order() {
        let d = draft_from_item(&item(json!({"title": " Hat ", "types": ["top"]})));
        let s = serde_json::to_string(&draft_to_payload(&d)).unwrap();
        assert_eq!(
            s,
            r#"{"title":"Hat","description":null,"imagePrompt":null,"types":["top"],"appropriateness":null,"isDefault":false,"componentItemIds":[],"replace":false,"pronounGuidance":[],"folder":""}"#
        );
    }

    #[test]
    fn replace_rides_only_a_composite() {
        let leaf = draft_from_item(&item(json!({"title": "A", "types": [], "replace": true})));
        assert_eq!(draft_to_payload(&leaf)["replace"], json!(false));
        let outfit = draft_from_item(&item(
            json!({"title": "A", "types": [], "replace": true, "componentItemIds": ["x"]}),
        ));
        assert_eq!(draft_to_payload(&outfit)["replace"], json!(true));
    }

    #[test]
    fn blank_to_null_uses_js_trim() {
        // U+FEFF is JS whitespace; U+0085 is not.
        let d = draft_from_item(&item(json!({
            "title": "A", "types": [], "description": "\u{feff} ", "imagePrompt": "\u{85}"
        })));
        let p = draft_to_payload(&d);
        assert_eq!(p["description"], Value::Null);
        assert_eq!(p["imagePrompt"], json!("\u{85}"));
    }
}
