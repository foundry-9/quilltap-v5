//! Self-Inventory — the `holdings` section (v4 `lib/tools/handlers/
//! self-inventory/holdings.ts`, `074e1e01a`).
//!
//! Counts what is within the character's reach, read through the same doors
//! the tools themselves use, so a count never disagrees with what the
//! character can actually touch:
//!  - wardrobe: [`load_wearable_pool`] (roster-gated project tier, archived
//!    dropped),
//!  - Inventory: [`load_inventory_context`] (the character's own reach,
//!    roster-gated),
//!  - custom tools: [`resolve_custom_tool_roster`] (shadowing, gates and
//!    `disabled`).
//!
//! Each part fails soft on its own (`{ available: false, message }`). The
//! wearable pool and the roster never fail in v5, so only the Inventory part
//! (and a pool checkout) can reach the unavailable arm through real reads;
//! v4's whole-section `.catch(() => ({ includedParts }))` guards only these
//! three caught parts and the DEBUG line, so it has no reachable v5 twin.

use serde::Serialize;
use serde_json::Value;

use super::self_inventory::{has_section, AvailFalse, AvailTrue};
use crate::collation::locale_compare;
use crate::db::runtime::Db;
use crate::db::tiered_mount_pool::MountTier;
use crate::db::DbError;
use crate::inventory::context::load_inventory_context;
use crate::inventory::types::{InventoryFields, InventoryKind};
use crate::pascal::roster::{resolve_custom_tool_roster, RosterContext};
use crate::wearable_pool::{load_wearable_pool, LoadWearablePoolOptions};

/// v4 `SelfInventoryTierCounts` — keys in v4's order.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub struct TierCounts {
    pub character: i64,
    pub group: i64,
    pub project: i64,
    pub general: i64,
}

impl TierCounts {
    fn bump(&mut self, scope: &str) {
        match scope {
            "group" => self.group += 1,
            "project" => self.project += 1,
            "general" => self.general += 1,
            _ => self.character += 1,
        }
    }
}

/// Tally items by their origin scope; an item with no origin is the
/// character's own (v4 `countByTier`).
fn count_by_tier<'a>(scopes: impl IntoIterator<Item = Option<&'a str>>) -> TierCounts {
    let mut counts = TierCounts::default();
    for scope in scopes {
        counts.bump(scope.unwrap_or("character"));
    }
    counts
}

/// A custom tool's mount tier, in the wardrobe / Inventory vocabulary (v4
/// `tierScopeOf`).
fn tier_scope_of(tier: MountTier) -> &'static str {
    match tier {
        MountTier::Global => "general",
        MountTier::Participant | MountTier::Character => "character",
        MountTier::Group => "group",
        MountTier::Project => "project",
    }
}

/// v4 `SelfInventoryHoldingsWardrobe`.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum HoldingsWardrobe {
    Available {
        available: AvailTrue,
        garments: i64,
        outfits: i64,
        #[serde(rename = "byTier")]
        by_tier: TierCounts,
    },
    Unavailable {
        available: AvailFalse,
        message: String,
    },
}

/// v4 `SelfInventoryHoldingsInventory`.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum HoldingsInventory {
    Available {
        available: AvailTrue,
        objects: i64,
        kits: i64,
        archetypes: i64,
        #[serde(rename = "carriedHere")]
        carried_here: i64,
        #[serde(rename = "byTier")]
        by_tier: TierCounts,
    },
    Unavailable {
        available: AvailFalse,
        message: String,
    },
}

/// v4 `SelfInventoryHoldingsCustomTools`.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum HoldingsCustomTools {
    Available {
        available: AvailTrue,
        count: i64,
        names: Vec<String>,
        #[serde(rename = "byTier")]
        by_tier: TierCounts,
    },
    Unavailable {
        available: AvailFalse,
        message: String,
    },
}

/// v4 `SelfInventoryHoldingsIncludedParts`.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct HoldingsIncludedParts {
    pub wardrobe: bool,
    pub inventory: bool,
    #[serde(rename = "customTools")]
    pub custom_tools: bool,
}

/// v4 `SelfInventoryHoldingsSection`.
#[derive(Clone, Debug, Serialize)]
pub struct HoldingsSection {
    #[serde(rename = "includedParts")]
    pub included_parts: HoldingsIncludedParts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wardrobe: Option<HoldingsWardrobe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory: Option<HoldingsInventory>,
    #[serde(rename = "customTools", skip_serializing_if = "Option::is_none")]
    pub custom_tools: Option<HoldingsCustomTools>,
}

/// v4 `resolveHoldingsIncludedParts`.
pub fn resolve_holdings_parts(requested: &Option<Vec<String>>) -> Option<HoldingsIncludedParts> {
    let all = has_section(requested, "holdings");
    let wardrobe = all || has_section(requested, "holdings.wardrobe");
    let inventory = all || has_section(requested, "holdings.inventory");
    let custom_tools = all || has_section(requested, "holdings.customTools");
    if !wardrobe && !inventory && !custom_tools {
        return None;
    }
    Some(HoldingsIncludedParts {
        wardrobe,
        inventory,
        custom_tools,
    })
}

/// v4 `buildHoldingsWardrobe`.
fn build_holdings_wardrobe(
    db: &Db,
    character_id: &str,
    chat_id: &str,
) -> Result<HoldingsWardrobe, DbError> {
    db.read_main(|main| {
        db.read_mount_index(|mount| {
            let pool = load_wearable_pool(
                main,
                mount,
                character_id,
                None,
                LoadWearablePoolOptions {
                    chat_id: Some(chat_id),
                    ..Default::default()
                },
            );
            let wearable = pool.wearable();
            let outfits = wearable
                .iter()
                .filter(|item| crate::wardrobe::is_composite(&component_ids(item)))
                .count() as i64;
            Ok(HoldingsWardrobe::Available {
                available: AvailTrue,
                garments: wearable.len() as i64 - outfits,
                outfits,
                by_tier: count_by_tier(wearable.iter().map(|item| {
                    item.get("origin")
                        .and_then(|o| o.get("scope"))
                        .and_then(Value::as_str)
                })),
            })
        })
    })
}

fn component_ids(item: &Value) -> Vec<String> {
    item.get("componentItemIds")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// v4 `buildHoldingsInventory`. `pool.items` also carries the character's own
/// archived objects (so a reference to one can be answered); a count of what
/// is in reach skips them.
fn build_holdings_inventory(
    db: &Db,
    character_id: &str,
    chat_id: &str,
) -> Result<HoldingsInventory, DbError> {
    db.read_main(|main| {
        db.read_mount_index(|mount| {
            let inventory = load_inventory_context(main, mount, character_id, Some(chat_id))?;
            let live: Vec<_> = inventory
                .pool
                .items
                .iter()
                .filter(|item| !item.is_archived())
                .collect();
            let (mut objects, mut kits, mut archetypes) = (0i64, 0i64, 0i64);
            for item in &live {
                if item.is_archetype() {
                    archetypes += 1;
                } else if item.kind() == InventoryKind::Kit {
                    kits += 1;
                } else {
                    objects += 1;
                }
            }
            let scopes: Vec<Option<&'static str>> = live
                .iter()
                .map(|item| item.origin().map(|o| o.scope.as_str()))
                .collect();
            Ok(HoldingsInventory::Available {
                available: AvailTrue,
                objects,
                kits,
                archetypes,
                carried_here: inventory.equipped_ids.len() as i64,
                by_tier: count_by_tier(scopes),
            })
        })
    })
}

/// v4 `buildHoldingsCustomTools`: switched off for the instance → the
/// unavailable arm WITHOUT resolving a roster; else the roster's names sorted
/// by `localeCompare`.
fn build_holdings_custom_tools(
    db: &Db,
    character: &Value,
    user_id: &str,
    chat_id: &str,
    project_id: Option<&str>,
    now_ms: i64,
) -> Result<HoldingsCustomTools, DbError> {
    db.read_main(|main| {
        let settings = crate::db::chat_settings::find_by_user_id_or_none(main, user_id);
        if settings
            .as_ref()
            .and_then(|s| s.get("customTools"))
            .is_some_and(|v| v == &Value::Bool(false))
        {
            return Ok(HoldingsCustomTools::Unavailable {
                available: AvailFalse,
                message: "Custom tools are switched off for this instance.".to_string(),
            });
        }
        db.read_mount_index(|mount| {
            let project_id: Option<String> = match project_id {
                Some(p) => Some(p.to_string()),
                None => crate::db::chats_read::find_by_id_or_none(main, chat_id).and_then(|c| {
                    c.get("projectId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                }),
            };
            let str_of = |k: &str| character.get(k).and_then(Value::as_str).map(str::to_string);
            let roster = resolve_custom_tool_roster(
                &RosterContext {
                    user_id: user_id.to_string(),
                    chat_id: chat_id.to_string(),
                    character_id: str_of("id"),
                    character_mount_point_id: str_of("characterDocumentMountPointId"),
                    project_id,
                    metadata: character
                        .get("metadata")
                        .and_then(Value::as_object)
                        .cloned(),
                    ..Default::default()
                },
                main,
                mount,
                now_ms,
            );
            let mut names: Vec<String> = roster
                .tools
                .iter()
                .map(|(_, entry)| entry.definition.name.clone())
                .collect();
            names.sort_by(|a, b| locale_compare(a, b));
            Ok(HoldingsCustomTools::Available {
                available: AvailTrue,
                count: roster.tools.len() as i64,
                names,
                by_tier: count_by_tier(
                    roster
                        .tools
                        .iter()
                        .map(|(_, entry)| Some(tier_scope_of(entry.tier))),
                ),
            })
        })
    })
}

/// The DEBUG line's `available ?? null`.
fn available_of(available: Option<bool>) -> &'static str {
    match available {
        Some(true) => "true",
        Some(false) => "false",
        None => "null",
    }
}

/// v4 `buildHoldingsSection`: each requested part, each failing soft to
/// `{ available: false, message }`, then DEBUG `self_inventory: holdings
/// resolved` (each part's `available ?? null`).
pub fn build_holdings_section(
    db: &Db,
    character: &Value,
    user_id: &str,
    chat_id: &str,
    project_id: Option<&str>,
    now_ms: i64,
    parts: HoldingsIncludedParts,
) -> HoldingsSection {
    let character_id = character
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let wardrobe = parts.wardrobe.then(|| {
        build_holdings_wardrobe(db, &character_id, chat_id).unwrap_or_else(|e| {
            HoldingsWardrobe::Unavailable {
                available: AvailFalse,
                message: e.to_string(),
            }
        })
    });
    let inventory = parts.inventory.then(|| {
        build_holdings_inventory(db, &character_id, chat_id).unwrap_or_else(|e| {
            HoldingsInventory::Unavailable {
                available: AvailFalse,
                message: e.to_string(),
            }
        })
    });
    let custom_tools = parts.custom_tools.then(|| {
        build_holdings_custom_tools(db, character, user_id, chat_id, project_id, now_ms)
            .unwrap_or_else(|e| HoldingsCustomTools::Unavailable {
                available: AvailFalse,
                message: e.to_string(),
            })
    });
    assemble_holdings_section(&character_id, parts, wardrobe, inventory, custom_tools)
}

/// The section from its resolved parts, with the DEBUG line (split from the
/// reads so a unit test can drive each part's unavailable arm).
fn assemble_holdings_section(
    character_id: &str,
    parts: HoldingsIncludedParts,
    wardrobe: Option<HoldingsWardrobe>,
    inventory: Option<HoldingsInventory>,
    custom_tools: Option<HoldingsCustomTools>,
) -> HoldingsSection {
    tracing::debug!(
        context = "self-inventory-handler",
        characterId = %character_id,
        wardrobe = available_of(wardrobe.as_ref().map(|w| matches!(w, HoldingsWardrobe::Available { .. }))),
        inventory = available_of(inventory.as_ref().map(|i| matches!(i, HoldingsInventory::Available { .. }))),
        customTools = available_of(custom_tools.as_ref().map(|c| matches!(c, HoldingsCustomTools::Available { .. }))),
        "self_inventory: holdings resolved"
    );
    HoldingsSection {
        included_parts: parts,
        wardrobe,
        inventory,
        custom_tools,
    }
}

// ===========================================================================
// RENDER (v4 formatters.ts `formatTierCounts` / `formatHoldings*`)
// ===========================================================================

/// v4 `formatTierCounts`: the non-zero tiers, labelled, joined `, `; all zero
/// → `none`.
fn format_tier_counts(counts: &TierCounts) -> String {
    let labels: [(i64, &str); 4] = [
        (counts.character, "your vault"),
        (counts.group, "groups"),
        (counts.project, "project"),
        (counts.general, "Quilltap General"),
    ];
    let parts: Vec<String> = labels
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{label} {n}"))
        .collect();
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

fn n(v: i64) -> String {
    super::self_inventory::format_number(v)
}

fn format_holdings_wardrobe(w: &HoldingsWardrobe) -> String {
    match w {
        HoldingsWardrobe::Unavailable { message, .. } => {
            format!("### Wardrobe\nUnavailable \u{2014} {message}")
        }
        HoldingsWardrobe::Available {
            garments,
            outfits,
            by_tier,
            ..
        } => [
            "### Wardrobe".to_string(),
            format!("- Garments: {}", n(*garments)),
            format!("- Outfits: {}", n(*outfits)),
            format!("- By tier: {}", format_tier_counts(by_tier)),
        ]
        .join("\n"),
    }
}

fn format_holdings_inventory(inv: &HoldingsInventory) -> String {
    match inv {
        HoldingsInventory::Unavailable { message, .. } => {
            format!("### Inventory\nUnavailable \u{2014} {message}")
        }
        HoldingsInventory::Available {
            objects,
            kits,
            archetypes,
            carried_here,
            by_tier,
            ..
        } => [
            "### Inventory".to_string(),
            format!("- Objects: {}", n(*objects)),
            format!("- Kits: {}", n(*kits)),
            format!("- Archetypes: {}", n(*archetypes)),
            format!("- Carrying in this chat: {}", n(*carried_here)),
            format!("- By tier: {}", format_tier_counts(by_tier)),
        ]
        .join("\n"),
    }
}

fn format_holdings_custom_tools(t: &HoldingsCustomTools) -> String {
    match t {
        HoldingsCustomTools::Unavailable { message, .. } => {
            format!("### Custom Tools\nUnavailable \u{2014} {message}")
        }
        HoldingsCustomTools::Available {
            count,
            names,
            by_tier,
            ..
        } => {
            let mut lines = vec![
                "### Custom Tools".to_string(),
                format!("- Available: {}", n(*count)),
            ];
            if *count > 0 {
                lines.push(format!("- By tier: {}", format_tier_counts(by_tier)));
                lines.push(format!(
                    "- Names (run with run_custom): {}",
                    names.join(", ")
                ));
            }
            lines.join("\n")
        }
    }
}

/// v4 `formatHoldingsSection` — the report's LAST section.
pub fn format_holdings_section(section: &HoldingsSection) -> String {
    let mut parts: Vec<String> = vec!["## Holdings".to_string()];
    if let Some(w) = &section.wardrobe {
        parts.push(String::new());
        parts.push(format_holdings_wardrobe(w));
    }
    if let Some(i) = &section.inventory {
        parts.push(String::new());
        parts.push(format_holdings_inventory(i));
    }
    if let Some(c) = &section.custom_tools {
        parts.push(String::new());
        parts.push(format_holdings_custom_tools(c));
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    //! The arms the corpus cannot reach through v4's real code (P4.D350 R-5):
    //! v4's "one failing part does not take the others down" mocks a throw;
    //! v5's wearable pool and roster never fail, so each part's unavailable
    //! arm is driven here through the assembler, with the DEBUG line's
    //! `available ?? null` rendering pinned.
    use super::*;
    use crate::test_support::global_capture;

    fn parts() -> HoldingsIncludedParts {
        HoldingsIncludedParts {
            wardrobe: true,
            inventory: true,
            custom_tools: false,
        }
    }

    #[test]
    fn one_failing_part_does_not_take_the_others_down() {
        global_capture::install();
        let (section, lines) = global_capture::capture(|| {
            assemble_holdings_section(
                "c1",
                parts(),
                Some(HoldingsWardrobe::Unavailable {
                    available: AvailFalse,
                    message: "boom".to_string(),
                }),
                Some(HoldingsInventory::Available {
                    available: AvailTrue,
                    objects: 1,
                    kits: 0,
                    archetypes: 0,
                    carried_here: 0,
                    by_tier: TierCounts {
                        character: 1,
                        ..Default::default()
                    },
                }),
                None,
            )
        });
        assert_eq!(
            serde_json::to_string(&section).unwrap(),
            r#"{"includedParts":{"wardrobe":true,"inventory":true,"customTools":false},"wardrobe":{"available":false,"message":"boom"},"inventory":{"available":true,"objects":1,"kits":0,"archetypes":0,"carriedHere":0,"byTier":{"character":1,"group":0,"project":0,"general":0}}}"#
        );
        assert_eq!(
            format_holdings_section(&section),
            "## Holdings\n\n### Wardrobe\nUnavailable \u{2014} boom\n\n### Inventory\n- Objects: 1\n- Kits: 0\n- Archetypes: 0\n- Carrying in this chat: 0\n- By tier: your vault 1"
        );
        let line: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("self_inventory: holdings resolved"))
            .collect();
        assert_eq!(line.len(), 1, "{lines:?}");
        assert!(
            line[0].ends_with(
                "self_inventory: holdings resolved context=self-inventory-handler characterId=c1 wardrobe=false inventory=true customTools=null"
            ),
            "{}",
            line[0]
        );
    }

    #[test]
    fn tier_counts_render_non_zero_tiers_or_none() {
        assert_eq!(format_tier_counts(&TierCounts::default()), "none");
        assert_eq!(
            format_tier_counts(&TierCounts {
                character: 2,
                group: 0,
                project: 1,
                general: 3
            }),
            "your vault 2, project 1, Quilltap General 3"
        );
        assert_eq!(tier_scope_of(MountTier::Global), "general");
        assert_eq!(tier_scope_of(MountTier::Participant), "character");
    }

    #[test]
    fn no_holdings_part_requested_is_none() {
        assert_eq!(resolve_holdings_parts(&Some(vec!["vault".into()])), None);
        assert_eq!(
            resolve_holdings_parts(&None),
            Some(HoldingsIncludedParts {
                wardrobe: true,
                inventory: true,
                custom_tools: true,
            })
        );
    }
}
