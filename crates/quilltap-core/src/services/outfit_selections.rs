//! Per-character outfit application for chat creation / participant add — v4's
//! `lib/wardrobe/apply-outfit-selections.ts` + the `chooseLLMOutfit` cheap-LLM
//! task (`lib/memory/cheap-llm-tasks/outfit-selection.ts`).
//!
//! [`apply_outfit_selections`] resolves each character's `OutfitSelection` to a
//! concrete equipped-slots record and persists it via
//! [`ChatOutfitsRepository::set_equipped_outfit`]. Modes: `default` (the
//! wardrobe items marked default), `manual` (the passed slot assignments),
//! `none` (undressed), `previous_chat` (copy from `source_chat_id`, else
//! default), and `llm_choose` (ask a cheap LLM, else default). An outfit failure
//! is swallowed by the create handler's try/catch — never fatal.
//!
//! The `6bf88959` progress narration: only the slow `llm_choose` path narrates —
//! a `wardrobe-start` frame before the call and a `wardrobe-result` frame (the
//! decided four-slot preview) afterwards; the fallback path resolves the panel
//! with a `log` warning + a `wardrobe-result`.
//!
//! ## The `8bb1a958` tri-tier contract (P4.D39)
//!
//! Every "what can this character wear?" question here asks the MERGED pool —
//! the character's vault under Quilltap General and the chat project's stores
//! ([`crate::db::wardrobe_read::find_wearable_pool_for_character`]) — never the
//! vault alone. Tiers merge on the FULL lists and `isDefault` is filtered last,
//! so a personal `isDefault: false` copy can shadow a shared default (the
//! opt-out). Defaults from different tiers LAYER in a slot, ordered
//! `createdAt` ascending. Resolution runs concurrently and commits serially;
//! each consult is bounded by [`OUTFIT_LLM_TIMEOUT_MS`]; an all-empty answer
//! counts only when the model flags it `"deliberate": true`.
//!
//! ## Documented seam — the malformed-JSON parse path
//!
//! v4's outfit parser calls `JSON.parse` (which THROWS on malformed JSON →
//! `executeCheapLLMTask` catches → task failure → the caller falls back to
//! default). The ported [`CheapLlmTaskExecutor`] takes an infallible parser, so a
//! provider SUCCESS is always a task success; the parser here returns empty slots
//! on unparseable content (a valid-but-non-object JSON → empty slots is v4-faithful,
//! but a genuinely malformed JSON diverges — empty-outfit success vs v4's
//! default-fallback). The corpus keeps canned responses valid JSON and drives the
//! default-fallback branch via a provider FAILURE instead (identical on both
//! sides), so the divergent path is never exercised.

use rusqlite::Connection;
use serde_json::Value;

use crate::cheap_llm::CheapLlmSelection;
use crate::db::archetype_wardrobe::{find_archetypes, find_archetypes_in_mounts};
use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::tiered_mount_pool::resolve_group_mount_point_ids_for_character;
use crate::db::wardrobe_read::{find_by_character_id, find_wearable_pool_for_character};
use crate::db::{characters_read, connection_profiles, DbError};
use crate::dissolve_bundles::{dissolve_bundles_in_slots, WearableLookup};
use crate::memory_tasks::strip_code_fences;
use crate::model::completion::{CompletionMessage, CompletionProvider, CompletionRole};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::cheap_llm_exec::CheapLlmTaskOptions;
use crate::services::creation_progress::{
    CreationProgressEmitter, LogLevel, OutfitPreviewEntry, OutfitPreviewSlots,
};
use crate::services::image_job_common::build_cheap_llm_selection;
use crate::tools::wardrobe_shared::resolve_equipped_outfit_leaf_values;
use crate::wardrobe::{sort_for_default_outfit, Slots, WARDROBE_SLOT_TYPES};
use crate::wardrobe_instructions::resolve_wardrobe_instructions;
use crate::wardrobe_tiers::{shared_wardrobe_tiers_for_character, SharedWardrobeTiers};
use crate::wearable_pool::{is_archived_truthy, merge_wearable_pool};

/// v4 `OUTFIT_SELECTION_PROMPT` (byte-exact).
const OUTFIT_SELECTION_PROMPT: &str = "You are a wardrobe assistant for a roleplay character. Your job is to choose what a character should wear at the start of a scene, based on:
- The character's available wardrobe items
- The scenario/setting description
- The character's personality
- The character's own dressing instructions, when provided — these describe what the character prefers to wear and under what circumstances; weigh them heavily, above general appropriateness guesses

Choose items that are contextually appropriate. For example, formal wear for a business meeting, casual clothes for relaxing at home, or era-appropriate costume for a historical setting.

You MUST respond with ONLY a JSON object mapping slot names to ARRAYS of wardrobe item IDs. Valid slots are: \"top\", \"bottom\", \"footwear\", \"accessories\", \"hair\". Use an empty array [] for any slot you want to leave empty. The \"hair\" slot holds a hairstyle or hairdo (braided, permed, elegantly coiffed, a wig) — not the hair itself. Leave \"hair\" empty for the character's natural, unstyled hair.

You may put multiple items in the same slot to layer them (e.g. a t-shirt under a sweater); list them inner-to-outer.

If the available wardrobe contains a composite item (its description mentions it bundles other items, or its title implies an outfit set), you may pick that composite directly — equipping it places it in all the slots it covers.

Example response:
{\"top\": [\"uuid-tshirt\", \"uuid-sweater\"], \"bottom\": [\"uuid-jeans\"], \"footwear\": [\"uuid-boots\"], \"accessories\": [], \"hair\": []}

Wearing nothing at all is a legitimate choice, but you must mark it as a choice. If the character genuinely should be unclothed — a nudist at home, a bath or swim scene, a setting where clothing would be absurd — leave every slot empty AND include \"deliberate\": true:

{\"deliberate\": true, \"top\": [], \"bottom\": [], \"footwear\": [], \"accessories\": [], \"hair\": []}

Deliberate nudity is about the clothing slots — an unclothed character may still keep a styled hairdo, so fill or empty \"hair\" on its own merits.

An all-empty response WITHOUT \"deliberate\": true is read as a failure to choose, and the character will be dressed in their default outfit instead. Do not use the empty response to opt out of the task — if you are unsure, pick something.

Do not include any other text, explanation, or markdown formatting. Just the JSON object.";

/// One character's outfit selection (v4 `OutfitSelection`).
#[derive(Clone, Debug)]
pub struct OutfitSelection {
    pub character_id: String,
    pub mode: String,
    /// The `manual` mode's slot assignments; ignored otherwise.
    pub slots: Option<Slots>,
}

/// The context the LLM path needs (v4 `OutfitSelectionContext`, minus the
/// progress emitter which is passed separately).
///
/// Deliberately NOT `Default`: v4 made `projectMountPointIds` required "so a
/// future call site that forgets it fails to compile instead of silently
/// dressing characters from one tier", and a `..Default::default()` escape
/// hatch would hand that guarantee straight back.
#[derive(Clone, Debug)]
pub struct OutfitContext<'a> {
    pub user_id: &'a str,
    /// Project document stores in scope for this chat, resolved by the caller
    /// from the project id it already holds
    /// ([`resolve_project_mount_point_ids`](crate::tools::wardrobe_shared::resolve_project_mount_point_ids)
    /// / `..._for_chat`). Folded into the shared wardrobe tier alongside
    /// Quilltap General.
    pub project_mount_point_ids: &'a [String],
    pub scenario_text: Option<&'a str>,
    /// The chat settings' `cheapLLMSettings` sub-object (v4 `buildCheapLLMConfig`
    /// source). `None` → the `DEFAULT_CHEAP_LLM_CONFIG` defaults.
    pub cheap_settings: Option<&'a Value>,
    /// The continuation source chat for `previous_chat` mode.
    pub source_chat_id: Option<&'a str>,
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// A `Slots` → the `{top,bottom,footwear,accessories,hair}` `Value` that
/// `set_equipped_outfit` stores (the canonical serialization).
fn slots_to_value(slots: &Slots) -> Value {
    slots.to_value()
}

/// v4 `resolveDefaultOutfit`'s pure half — the default outfit computed from an
/// already-merged wearable pool.
///
/// Personal and shared defaults **layer** rather than one winning the slot; only
/// an id collision shadows (a personal copy of a shared item replaces it, so a
/// personal `isDefault: false` override is how a character opts out of a shared
/// default). That is why the merge happens on the *full* pools and `isDefault`
/// is filtered here, last — filtering first would hide the opt-out.
///
/// Order is deterministic by `createdAt` ascending (slot arrays read
/// inner-to-outer); [`sort_for_default_outfit`] is mirrored by the client
/// composer so the preview and the chat that opens agree.
///
/// A bundle marked default dissolves into its parts before it is stored, same as
/// every other put-on gesture (`61574563`). The whole pool is the lookup — a
/// shared bundle's components are in it even when the character doesn't own
/// them.
pub fn default_outfit_from_pool(pool: &[Value]) -> Slots {
    let defaults: Vec<Value> = pool
        .iter()
        .filter(|i| i.get("isDefault").and_then(Value::as_bool) == Some(true))
        .filter(|i| !is_archived_truthy(i))
        .cloned()
        .collect();
    let mut slots = Slots::default();
    if defaults.is_empty() {
        return slots;
    }

    for item in sort_for_default_outfit(&defaults) {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        if let Some(types) = item.get("types").and_then(Value::as_array) {
            for t in types {
                if let Some(slot) = t.as_str() {
                    if WARDROBE_SLOT_TYPES.contains(&slot) {
                        slots.slot_mut(slot).push(id.to_string());
                    }
                }
            }
        }
    }
    dissolve_bundles_in_slots(&slots, &lookup_from(pool))
}

/// An `id → item` lookup over a wardrobe pool, for [`dissolve_bundles_in_slots`].
fn lookup_from(items: &[Value]) -> WearableLookup {
    let mut map = WearableLookup::new();
    for item in items {
        if let Some(id) = item.get("id").and_then(Value::as_str) {
            map.insert(id.to_string(), item.clone());
        }
    }
    map
}

/// v4 `resolveDefaultOutfit(characterId, repos, { projectMountPointIds })` —
/// the default outfit from every item marked default across all THREE tiers:
/// the character's own vault, the project's linked stores, and Quilltap General.
///
/// Callers already holding a merged pool (the batch in
/// [`apply_outfit_selections`] fetches the shared tier once) go straight to
/// [`default_outfit_from_pool`].
pub fn resolve_default_outfit(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    project_mount_point_ids: &[String],
) -> Result<Slots, DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let tiers =
        shared_wardrobe_tiers_for_character(main, mount, character_id, project_mount_point_ids);
    let pool = find_wearable_pool_for_character(main, &docs, character_id, &tiers)?;
    Ok(default_outfit_from_pool(&pool))
}

/// v4 `LLMOutfitChoice` — what the model decided.
///
/// `deliberately_unclothed` separates "the character should be naked" from "the
/// model produced nothing usable" — two states that were previously identical on
/// the wire (all-empty slots) and therefore indistinguishable to the caller.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LlmOutfitChoice {
    pub slots: Slots,
    /// The model explicitly flagged its empty answer as intentional.
    pub deliberately_unclothed: bool,
}

/// v4 `chooseLLMOutfit`'s response parser (over the ported executor's infallible
/// contract — see the module-header seam). Validates each candidate id against
/// the wardrobe (must exist AND cover the slot), tolerating a legacy
/// single-id/null shape, and reports the model's `deliberate` claim.
fn parse_outfit_response(content: &str, items: &[Value]) -> LlmOutfitChoice {
    let clean = strip_code_fences(content);
    let parsed: Value = serde_json::from_str(&clean).unwrap_or(Value::Null);
    if !parsed.is_object() {
        return LlmOutfitChoice::default();
    }
    // Only an explicit boolean `true` counts. A model that omits the flag, or
    // sends a stringy "true", has not made the deliberate claim — and the
    // fallback (dress them in their defaults) is the safe reading.
    let deliberately_unclothed = parsed.get("deliberate") == Some(&Value::Bool(true));
    // valid item ids + which slots each covers.
    let mut valid: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut item_slots: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for it in items {
        if let Some(id) = it.get("id").and_then(Value::as_str) {
            valid.insert(id);
            let types: Vec<&str> = it
                .get("types")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            item_slots.insert(id, types);
        }
    }

    let mut result = Slots::default();
    for slot in WARDROBE_SLOT_TYPES {
        let raw = parsed.get(slot);
        // array → as-is; null/absent → []; scalar → [scalar].
        let candidates: Vec<&Value> = match raw {
            Some(Value::Array(a)) => a.iter().collect(),
            None | Some(Value::Null) => Vec::new(),
            Some(other) => vec![other],
        };
        let target = result.slot_mut(slot);
        for cand in candidates {
            let Some(id) = cand.as_str() else { continue };
            if !valid.contains(id) {
                continue;
            }
            let covers = item_slots
                .get(id)
                .map(|v| v.contains(&slot))
                .unwrap_or(false);
            if !covers {
                continue;
            }
            if !target.iter().any(|x| x == id) {
                target.push(id.to_string());
            }
        }
    }
    LlmOutfitChoice {
        slots: result,
        deliberately_unclothed,
    }
}

/// Build the two `chooseLLMOutfit` messages (system prompt + the character/
/// wardrobe/scenario user message), byte-exact to v4.
fn build_outfit_messages(
    character: &Value,
    wardrobe_items: &[Value],
    scenario_text: Option<&str>,
    dressing_instructions: Option<&str>,
) -> Vec<CompletionMessage> {
    let character_name = s(character, "name").unwrap_or_default();
    let note = |label: &str, field: &str| -> String {
        match s(character, field) {
            Some(v) if !v.trim().is_empty() => format!("\n{label}\n{v}"),
            _ => String::new(),
        }
    };
    let manifesto_note = note("Character Manifesto (foundational tenets):", "manifesto");
    let description_note = note(
        "Character Description (behaviour and mannerisms):",
        "description",
    );
    let personality_note = note("Character Personality (internal drivers):", "personality");
    let scenario_note = match scenario_text {
        Some(sc) if !sc.is_empty() => format!("\nScenario: {sc}"),
        _ => "\nScenario: (general conversation, no specific setting)".to_string(),
    };

    // v4 `b86bb1a5`: the note is emitted only for a truthy, non-blank string —
    // null, `''` and whitespace-only all produce a user message BYTE-IDENTICAL
    // to the pre-commit one (v4 pins that byte-identity). Straight quotes around
    // `"you"`, an em dash before it, and the name interpolated twice.
    let instructions_note = match dressing_instructions {
        Some(instr) if !crate::jsstr::js_trim(instr).is_empty() => format!(
            "\nDressing Instructions (addressed to {character_name} in the second person — \"you\" is {character_name}):\n{}",
            crate::jsstr::js_trim(instr)
        ),
        _ => String::new(),
    };

    let wardrobe_section = wardrobe_items
        .iter()
        .map(|item| {
            let id = s(item, "id").unwrap_or_default();
            let title = s(item, "title").unwrap_or_default();
            let types = item
                .get("types")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let appropriateness = match s(item, "appropriateness") {
                Some(a) if !a.is_empty() => format!(" [appropriate for: {a}]"),
                _ => String::new(),
            };
            let desc = match s(item, "description") {
                Some(d) if !d.is_empty() => format!(" — {d}"),
                _ => String::new(),
            };
            let component_count = item
                .get("componentItemIds")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            let composite_marker = if component_count > 0 {
                let plural = if component_count == 1 { "" } else { "s" };
                format!(" [composite — bundles {component_count} other item{plural}]")
            } else {
                String::new()
            };
            format!(
                "  - ID: {id} | \"{title}\"{composite_marker} (covers: {types}){appropriateness}{desc}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let user_content = format!(
        "Character: {character_name}{manifesto_note}{description_note}{personality_note}{scenario_note}{instructions_note}\n\nAvailable Wardrobe Items:\n{wardrobe_section}\n\nChoose what {character_name} should wear for this scene:"
    );

    vec![
        CompletionMessage {
            role: CompletionRole::System,
            content: OUTFIT_SELECTION_PROMPT.to_string(),
        },
        CompletionMessage {
            role: CompletionRole::User,
            content: user_content,
        },
    ]
}

/// v4 `toOutfitPreviewSlots` over the resolved leaf items: each item →
/// `{id, title, isComposite}`.
fn to_outfit_preview_slots(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    slots: &Slots,
    project_mount_point_ids: &[String],
) -> OutfitPreviewSlots {
    let docs = DocMountDocumentsRepository::new(mount);
    // The project tier reaches the preview too: without it a project-tier
    // garment resolved with no title in the creation dialog (v4 defect 3).
    let leaf = resolve_equipped_outfit_leaf_values(
        main,
        &docs,
        character_id,
        slots,
        // v4 `sharedWardrobeTiersForCharacter` — the group tier is per character.
        &shared_wardrobe_tiers_for_character(main, mount, character_id, project_mount_point_ids),
    )
    .unwrap_or_default();
    let map = |items: &[Value]| -> Vec<OutfitPreviewEntry> {
        items
            .iter()
            .map(|i| OutfitPreviewEntry {
                id: s(i, "id").unwrap_or_default(),
                title: s(i, "title").unwrap_or_default(),
                is_composite: i
                    .get("componentItemIds")
                    .and_then(Value::as_array)
                    .map(|a| !a.is_empty())
                    .unwrap_or(false),
            })
            .collect()
    };
    OutfitPreviewSlots {
        top: map(&leaf.top),
        bottom: map(&leaf.bottom),
        footwear: map(&leaf.footwear),
        accessories: map(&leaf.accessories),
        hair: map(&leaf.hair),
    }
}

/// How long to wait on the wardrobe LLM for one character before giving up and
/// dressing them in their defaults (v4 `OUTFIT_LLM_TIMEOUT_MS`).
///
/// A healthy call on a cheap model is a second or two. Observed in the wild: a
/// provider that took 2m32s to emit 19 tokens, which — back when this loop was
/// serial — stalled the entire chat-creation request behind one character.
/// Concurrency stops one straggler from blocking the others; this stops a
/// straggler from blocking the chat.
///
/// Note the underlying request is not aborted, only abandoned; it will still be
/// billed and still land in the LLM log.
pub const OUTFIT_LLM_TIMEOUT_MS: u64 = 60_000;

/// The batch's wearable pools, fetched lazily and memoized — v4's
/// `getSharedPool` / `getPool` promise memos.
///
/// Every read here is SYNCHRONOUS (rusqlite), so no `.await` can interleave
/// inside a fetch and two concurrent resolvers can never race into a double
/// read the way two JS callers would without the promise memo.
struct PoolCache<'a> {
    main: &'a Connection,
    mount: &'a Connection,
    docs: DocMountDocumentsRepository<'a>,
    project_mount_point_ids: &'a [String],
    chat_id: &'a str,
    shared: std::cell::RefCell<Option<std::rc::Rc<Vec<Value>>>>,
    by_character: std::cell::RefCell<std::collections::HashMap<String, std::rc::Rc<Vec<Value>>>>,
}

impl<'a> PoolCache<'a> {
    fn new(
        main: &'a Connection,
        mount: &'a Connection,
        chat_id: &'a str,
        project_mount_point_ids: &'a [String],
    ) -> Self {
        Self {
            main,
            mount,
            docs: DocMountDocumentsRepository::new(mount),
            project_mount_point_ids,
            chat_id,
            shared: std::cell::RefCell::new(None),
            by_character: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }

    /// The shared tiers, read at most once for the whole batch and only when a
    /// selection actually needs a pool — `find_archetypes` re-reads and parses
    /// every file in each shared folder, and a batch of `manual`/`none`
    /// selections shouldn't pay for a wardrobe read at all.
    fn shared(&self) -> std::rc::Rc<Vec<Value>> {
        if let Some(cached) = self.shared.borrow().clone() {
            return cached;
        }
        // The PROJECT + general tiers only: they are the same for every
        // character in this batch. The group tier is per-character and is read
        // separately in `group_tier` below.
        let tiers = SharedWardrobeTiers::project_only(self.project_mount_point_ids);
        let items = find_archetypes(self.main, &self.docs, false, &tiers)
            .unwrap_or_else(|e| {
                tracing::warn!(
                    chat_id = self.chat_id,
                    project_mount_count = self.project_mount_point_ids.len(),
                    error = %e,
                    "[applyOutfitSelections] Failed to read shared wardrobe tiers; using the character vault alone"
                );
                Vec::new()
            });
        let rc = std::rc::Rc::new(items);
        *self.shared.borrow_mut() = Some(rc.clone());
        rc
    }

    /// v4 `getGroupTier` — one character's GROUP tier. It can't join the batched
    /// read: it's keyed on the character's own memberships, so each character
    /// gets their own. Read separately and layered over the batch tiers in
    /// [`Self::get`] (group beats project beats general). Fails soft to `[]`
    /// with a warning — an unreadable group store must not cost the character
    /// the rest of their wardrobe.
    fn group_tier(&self, character_id: &str) -> Vec<Value> {
        let group_mount_point_ids =
            resolve_group_mount_point_ids_for_character(self.main, self.mount, character_id);
        if group_mount_point_ids.is_empty() {
            return Vec::new();
        }
        find_archetypes_in_mounts(&self.docs, &group_mount_point_ids, false).unwrap_or_else(|e| {
            tracing::warn!(
                chat_id = self.chat_id, character_id, error = %e,
                "[applyOutfitSelections] Failed to read group wardrobe tier; skipping it"
            );
            Vec::new()
        })
    }

    /// One character's merged pool (the shared tiers under their own vault),
    /// memoized so a character whose `llm_choose` falls back to defaults doesn't
    /// re-read their vault. Each side fails soft on its own.
    fn get(&self, character_id: &str) -> std::rc::Rc<Vec<Value>> {
        if let Some(cached) = self.by_character.borrow().get(character_id).cloned() {
            return cached;
        }
        let batch_shared = self.shared();
        let group = self.group_tier(character_id);
        // Group last: a group's livery shadows the project's copy of the same id.
        let shared: Vec<Value> = batch_shared
            .iter()
            .cloned()
            .chain(group.iter().cloned())
            .collect();
        let own = find_by_character_id(self.main, &self.docs, character_id, false)
            .unwrap_or_else(|e| {
                tracing::warn!(
                    chat_id = self.chat_id, character_id, error = %e,
                    "[applyOutfitSelections] Failed to read character wardrobe; using shared tiers alone"
                );
                Vec::new()
            });
        let pool = std::rc::Rc::new(merge_wearable_pool(&shared, &own));
        tracing::debug!(
            chat_id = self.chat_id,
            character_id,
            pool_size = pool.len(),
            shared_count = shared.len(),
            group_count = group.len(),
            own_count = own.len(),
            project_mount_count = self.project_mount_point_ids.len(),
            "[applyOutfitSelections] Resolved wearable pool"
        );
        self.by_character
            .borrow_mut()
            .insert(character_id.to_string(), pool.clone());
        pool
    }
}

/// v4 `applyOutfitSelections`: resolve + persist each character's outfit. `main`
/// is the writer connection (reads + `set_equipped_outfit`); `mount` the
/// mount-index store; `completion`/`executor` the cheap-LLM boundary for
/// `llm_choose`.
///
/// Runs in two phases, exactly as v4 does:
///
///  1. **Resolve, concurrently.** Every character's slots are worked out in
///     parallel — `llm_choose` is a network round trip apiece, and serially they
///     added up (one stalled provider held the whole cast). Nothing is written
///     here, and one character's failure can't cancel its siblings. v4 uses
///     `Promise.allSettled`; v5 uses `join_all` over per-character futures on
///     the caller's existing current-thread runtime, so network waits overlap
///     while the DB reads stay serial at await points and the single-writer rule
///     is untouched.
///  2. **Commit, serially, in the caller's order.**
///     [`ChatOutfitsRepository::set_equipped_outfit`] reads the chat's whole
///     `equippedOutfit` map, sets one key and writes it back, so concurrent
///     writers would lose every entry but the last.
///
/// Never returns `Err`: a character that can't be resolved or persisted is
/// logged and left as-is rather than failing the chat around them (v4's
/// post-`8bb1a958` contract). The `Result` stays in the signature because the
/// create handler's `let _ =` treats it as v4's try/catch.
#[allow(clippy::too_many_arguments)]
pub async fn apply_outfit_selections<C: CompletionProvider>(
    main: &Connection,
    mount: &Connection,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    chat_id: &str,
    selections: &[OutfitSelection],
    ctx: &OutfitContext<'_>,
    emitter: &CreationProgressEmitter,
) -> Result<(), DbError> {
    let outfits = ChatOutfitsRepository::new(main);
    let pools = PoolCache::new(main, mount, chat_id, ctx.project_mount_point_ids);

    // Phase 1 — resolve concurrently. `join_all` polls every future to
    // completion and surfaces each one's own `Result`, which is v4's
    // `allSettled` in Rust: a blow-up is captured per character, never
    // cancelling its siblings.
    let resolved: Vec<Result<Option<Value>, DbError>> =
        futures_util::future::join_all(selections.iter().map(|selection| {
            resolve_selection(
                main, mount, completion, executor, &outfits, &pools, chat_id, selection, ctx,
                emitter,
            )
        }))
        .await;

    // Phase 2 — commit serially, in the caller's order.
    for (selection, outcome) in selections.iter().zip(resolved) {
        let character_id = selection.character_id.as_str();
        let value = match outcome {
            Err(e) => {
                tracing::error!(
                    chat_id, character_id, error = %e,
                    "[applyOutfitSelections] Failed to resolve outfit; character left as-is"
                );
                continue;
            }
            // v4 returns null for an unrecognised mode — nothing is written.
            Ok(None) => continue,
            Ok(Some(v)) => v,
        };
        if let Err(e) = outfits.set_equipped_outfit(chat_id, character_id, &value) {
            tracing::error!(
                chat_id, character_id, error = %e,
                "[applyOutfitSelections] Failed to persist equipped outfit"
            );
        }
    }
    Ok(())
}

/// v4 `resolveSelection` — work out what a character should be wearing. `None`
/// means nothing should be written at all (an unrecognised mode).
///
/// Deliberately does **no** persistence: these run concurrently, and
/// `set_equipped_outfit` is a read-modify-write of a single JSON column keyed by
/// character, so concurrent writers would drop each other's entries. The caller
/// commits the results serially afterwards. (`outfits` is here only for the
/// `previous_chat` READ.)
#[allow(clippy::too_many_arguments)]
async fn resolve_selection<C: CompletionProvider>(
    main: &Connection,
    mount: &Connection,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    outfits: &ChatOutfitsRepository<'_>,
    pools: &PoolCache<'_>,
    chat_id: &str,
    selection: &OutfitSelection,
    ctx: &OutfitContext<'_>,
    emitter: &CreationProgressEmitter,
) -> Result<Option<Value>, DbError> {
    if let Some(value) = resolve_non_llm_selection(outfits, pools, selection, ctx) {
        return Ok(Some(value));
    }
    if selection.mode == "llm_choose" {
        return resolve_llm_choose(
            main,
            mount,
            completion,
            executor,
            pools,
            chat_id,
            &selection.character_id,
            ctx,
            Some(emitter),
        )
        .await
        .map(Some);
    }
    tracing::warn!(
        chat_id,
        character_id = selection.character_id.as_str(),
        mode = selection.mode.as_str(),
        "[applyOutfitSelections] Unknown outfit selection mode"
    );
    Ok(None)
}

/// The four modes that need no model call (`default` / `manual` / `none` /
/// `previous_chat`). `None` for `llm_choose` and for an unknown mode, which the
/// caller dispatches. The returned `Value` is what
/// `set_equipped_outfit` should store — `previous_chat` carries the source
/// chat's stored object through verbatim, as v4 does.
fn resolve_non_llm_selection(
    outfits: &ChatOutfitsRepository<'_>,
    pools: &PoolCache<'_>,
    selection: &OutfitSelection,
    ctx: &OutfitContext<'_>,
) -> Option<Value> {
    let character_id = selection.character_id.as_str();
    match selection.mode.as_str() {
        "default" => Some(slots_to_value(&default_outfit_from_pool(
            &pools.get(character_id),
        ))),
        "manual" => Some(slots_to_value(&selection.slots.clone().unwrap_or_default())),
        "none" => Some(slots_to_value(&Slots::default())),
        "previous_chat" => {
            if let Some(source) = ctx.source_chat_id {
                // v4 wraps the read in try/catch → fall back to default.
                if let Ok(Some(prev)) =
                    outfits.get_equipped_outfit_for_character(source, character_id)
                {
                    return Some(prev);
                }
            }
            Some(slots_to_value(&default_outfit_from_pool(
                &pools.get(character_id),
            )))
        }
        _ => None,
    }
}

/// The four no-model modes for a SYNC caller — the chat merge, which runs inside
/// the single-writer closure and cannot hold connections across an await
/// (P4.9E3A). Resolves through the same [`resolve_non_llm_selection`] the
/// concurrent path uses, then persists. Returns `true` when the selection was
/// handled here; `false` for `llm_choose` and for an unknown mode.
pub fn apply_outfit_selection_sync(
    main: &Connection,
    mount: &Connection,
    outfits: &ChatOutfitsRepository<'_>,
    chat_id: &str,
    selection: &OutfitSelection,
    ctx: &OutfitContext<'_>,
) -> Result<bool, DbError> {
    let pools = PoolCache::new(main, mount, chat_id, ctx.project_mount_point_ids);
    let Some(value) = resolve_non_llm_selection(outfits, &pools, selection, ctx) else {
        return Ok(false);
    };
    outfits.set_equipped_outfit(chat_id, &selection.character_id, &value)?;
    Ok(true)
}

/// The outcome of one `llm_choose` attempt (the v4 `applyOutfitSelections`
/// arm's observable states).
pub enum LlmChooseOutcome {
    /// The consult never started (missing character / empty wardrobe / no
    /// resolvable profile) — v4 announces nothing and falls to the default.
    NotAttempted,
    /// The consult ran; `choice` is `None` on a task failure or a timeout
    /// (→ default).
    Attempted {
        character_name: String,
        choice: Option<LlmOutfitChoice>,
    },
}

/// The LLM-attempt half of v4's `llm_choose` arm, over PRE-READ rows (so a
/// connection never spans the await for callers without dedicated writers).
/// `emitter` narrates `wardrobe-start` when present (chat-create's dialog);
/// the RESULT frames stay at the call sites, which hold the connections the
/// preview resolution needs.
///
/// The consult is bounded by [`OUTFIT_LLM_TIMEOUT_MS`]. v4 races the timer at
/// the `applyOutfitSelections` layer; v5 races it here instead, because all
/// three entrances (create, add-participant, merge) funnel through this one
/// function, and the observable result is the same either way — a timeout falls
/// back to the default outfit with the consult still counted as announced.
///
/// `resolve_dressing_instructions` is v4's 7th positional `dressingInstructions`
/// parameter turned inside out. v4 resolves the cascade at the
/// `applyOutfitSelections` layer, immediately AFTER `progress.wardrobeStart` and
/// only once `character && pool.length > 0 && defaultProfile` all hold — the
/// three conditions that live INSIDE this function in v5, because v5 split v4's
/// single entrance into two (`resolve_llm_choose` and `run_llm_choose_via_db`).
/// Passing the resolution as a closure invoked here keeps v4's exact ordering
/// and read count at both entrances instead of resolving eagerly (which would
/// probe up to four vault files on an instance with no connection profiles, and
/// would narrate the consult after the reads rather than before them).
#[allow(clippy::too_many_arguments)]
pub async fn choose_llm_outfit<C: CompletionProvider, F: FnOnce() -> Option<String>>(
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    character: Option<&Value>,
    wardrobe_items: &[Value],
    all_profiles: &[Value],
    cheap_settings: Option<&Value>,
    scenario_text: Option<&str>,
    resolve_dressing_instructions: F,
    character_id: &str,
    emitter: Option<&CreationProgressEmitter>,
) -> LlmChooseOutcome {
    let Some(character) = character else {
        return LlmChooseOutcome::NotAttempted;
    };
    if wardrobe_items.is_empty() {
        return LlmChooseOutcome::NotAttempted;
    }
    let selection: Option<CheapLlmSelection> =
        build_cheap_llm_selection(all_profiles, cheap_settings);
    let Some(selection) = selection else {
        return LlmChooseOutcome::NotAttempted;
    };

    let character_name = s(character, "name").unwrap_or_default();
    if let Some(emitter) = emitter {
        emitter.wardrobe_start(character_id, &character_name);
    }

    // Optional dressing instructions: nearest tier's `Wardrobe/instructions.md`
    // wins (character > group > project > general) and the search stops there.
    // Soft-fails to None — instructions never block the outfit choice.
    let dressing_instructions = resolve_dressing_instructions();

    let messages = build_outfit_messages(
        character,
        wardrobe_items,
        scenario_text,
        dressing_instructions.as_deref(),
    );
    let items_for_parse = wardrobe_items.to_vec();
    let consult = executor.execute(
        completion,
        &selection,
        messages,
        move |content| parse_outfit_response(content, &items_for_parse),
        None,
        None,
        Some(character_id),
        Some("outfit-selection"),
        CheapLlmTaskOptions::default(),
    );

    let choice = match tokio::time::timeout(
        std::time::Duration::from_millis(OUTFIT_LLM_TIMEOUT_MS),
        consult,
    )
    .await
    {
        Ok(result) if result.success => result.result,
        Ok(_) => None,
        Err(_) => {
            tracing::warn!(
                character_id,
                timeout_ms = OUTFIT_LLM_TIMEOUT_MS,
                pool_size = wardrobe_items.len(),
                "[applyOutfitSelections] LLM outfit selection timed out, falling back to defaults"
            );
            None
        }
    };

    LlmChooseOutcome::Attempted {
        character_name,
        choice,
    }
}

/// v4's read of a completed consult: which answers count as a pick.
///
/// `chooseLLMOutfit` never fails parsing — it drops ids it can't validate and
/// still reports success. An all-empty answer therefore means either "nothing
/// usable" or "naked on purpose", and only the model's own `deliberate` flag
/// tells the two apart. Without it, dress them in their defaults.
///
/// Returns `(slots, deliberately_unclothed)` when the answer counts, `None`
/// when the caller should fall back.
fn accept_llm_choice(choice: Option<&LlmOutfitChoice>) -> Option<(Slots, bool)> {
    let choice = choice?;
    // Only the *clothing* slots count here: a pick of nothing but a hairdo is
    // still "chose nothing to wear" (v4 `4423ad10` — CLOTHING_SLOT_TYPES.some).
    let picked = crate::wardrobe::CLOTHING_SLOT_TYPES
        .iter()
        .any(|slot| !choice.slots.slot(slot).is_empty());
    let declared_bare = choice.deliberately_unclothed;
    if picked || declared_bare {
        Some((choice.slots.clone(), declared_bare && !picked))
    } else {
        None
    }
}

/// v4's dressing-instructions block from `applyOutfitSelections`' `llm_choose`
/// arm (`b86bb1a5`), over connections the caller already holds.
///
/// Two quirks carried verbatim:
///   * v4 re-runs `sharedWardrobeTiersForCharacter(characterId, [])` here even
///     though the pool build already ran it with the REAL project ids — a second,
///     redundant resolve whose project half is thrown away. Only its
///     `groupMountPointIds` is used, and the PROJECT ids come from the OUTER
///     `projectMountPointIds`, not from this call.
///   * v4 wraps both calls in `.catch(...)`. v5's
///     [`shared_wardrobe_tiers_for_character`] is sync + infallible and
///     [`resolve_wardrobe_instructions`] never fails, so there is no analogue to
///     port — recorded, not silently dropped.
fn resolve_dressing_instructions_conn(
    main: &Connection,
    mount: &Connection,
    character: Option<&Value>,
    chat_id: Option<&str>,
    character_id: &str,
    project_mount_point_ids: &[String],
) -> Option<String> {
    let character_mount_point_id = character.and_then(|c| s(c, "characterDocumentMountPointId"));
    let tiers = shared_wardrobe_tiers_for_character(main, mount, character_id, &[]);
    let resolved = resolve_wardrobe_instructions(
        main,
        mount,
        character_mount_point_id.as_deref(),
        &tiers.group_mount_point_ids,
        project_mount_point_ids,
    )?;
    tracing::debug!(
        chat_id,
        character_id,
        tier = resolved.tier.as_str(),
        mount_point_id = resolved.mount_point_id,
        "[applyOutfitSelections] Dressing instructions in play"
    );
    Some(resolved.content)
}

/// [`resolve_dressing_instructions_conn`] for the out-of-create entrances, which
/// hold a [`Db`](crate::db::runtime::Db) rather than open connections.
fn resolve_dressing_instructions_for(
    db: &crate::db::runtime::Db,
    character: Option<&Value>,
    chat_id: Option<&str>,
    character_id: &str,
    project_mount_point_ids: &[String],
) -> Option<String> {
    db.read_main(|main| {
        db.read_mount_index(|mount| {
            Ok(resolve_dressing_instructions_conn(
                main,
                mount,
                character,
                chat_id,
                character_id,
                project_mount_point_ids,
            ))
        })
    })
    .ok()
    .flatten()
}

/// The host-seam contract for one out-of-create `llm_choose` pick (P4.9E3B):
/// the composing host holds the completion provider + a per-call LOGGING cheap
/// executor (the `RegenerateTitleDriver` arrangement). `None` means the pick
/// failed OR never started — v4 falls back to the default outfit either way,
/// and so do both call sites (add-participant and the merge).
#[derive(Clone, Debug)]
pub struct OutfitLlmChooseRequest {
    pub chat_id: String,
    pub character_id: String,
    pub scenario_text: Option<String>,
    /// The chat settings' `cheapLLMSettings` sub-object.
    pub cheap_settings: Option<Value>,
    /// The chat's project document stores, resolved by the caller. Folded into
    /// the shared wardrobe tier alongside Quilltap General so the candidate list
    /// spans all three tiers.
    pub project_mount_point_ids: Vec<String>,
}

pub trait OutfitLlmChooseRunner: Send + Sync {
    fn choose(
        &self,
        req: OutfitLlmChooseRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Slots>> + Send + '_>>;
}

/// The runner's core: read the character + wardrobe + profiles through pooled
/// read connections into owned rows, then run [`choose_llm_outfit`] with no
/// emitter. Shared by the production `HostOutfitLlmChooseRunner` and the
/// tier-3 differential's canned runner, so the differential drives the exact
/// composition production uses.
pub async fn run_llm_choose_via_db<C: CompletionProvider>(
    db: &crate::db::runtime::Db,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    character_id: &str,
    scenario_text: Option<&str>,
    cheap_settings: Option<&Value>,
    project_mount_point_ids: &[String],
) -> Option<Slots> {
    // v4 wraps the whole attempt in try/catch → fall to default (None).
    let cid = character_id.to_string();
    let mounts = project_mount_point_ids.to_vec();
    let read = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let docs = DocMountDocumentsRepository::new(mount);
            let character = characters_read::find_by_id(main, mount, &cid)?;
            // Candidates come from all three tiers: a character whose entire
            // wardrobe is shared used to fail the non-empty guard below and fall
            // through to (empty) defaults without the LLM ever being consulted.
            let tiers = shared_wardrobe_tiers_for_character(main, mount, &cid, &mounts);
            let items = find_wearable_pool_for_character(main, &docs, &cid, &tiers)?;
            let profiles = connection_profiles::find_all(main)?;
            Ok((character, items, profiles))
        })
    });
    let (character, items, profiles) = match read {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(character_id, error = %e,
                "[applyOutfitSelections] Error during LLM outfit selection, falling back to defaults");
            return None;
        }
    };
    match choose_llm_outfit(
        completion,
        executor,
        character.as_ref(),
        &items,
        &profiles,
        cheap_settings,
        scenario_text,
        || resolve_dressing_instructions_for(db, character.as_ref(), None, character_id, &mounts),
        character_id,
        None,
    )
    .await
    {
        LlmChooseOutcome::Attempted { choice, .. } => accept_llm_choice(choice.as_ref())
            // The same dissolution the batch arm applies (v4 `applyOutfitSelections`).
            .map(|(slots, _)| dissolve_bundles_in_slots(&slots, &lookup_from(&items))),
        LlmChooseOutcome::NotAttempted => None,
    }
}

/// v4's `llm_choose` arm of `resolveSelection`: consult the cheap LLM over the
/// merged pool, honour a deliberate-nudity claim, and otherwise fall back to the
/// default outfit. Returns the value to persist — never `None`, because v4 always
/// lands on *something* here. Resolution only; the caller commits.
#[allow(clippy::too_many_arguments)]
async fn resolve_llm_choose<C: CompletionProvider>(
    main: &Connection,
    mount: &Connection,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    pools: &PoolCache<'_>,
    chat_id: &str,
    character_id: &str,
    ctx: &OutfitContext<'_>,
    emitter: Option<&CreationProgressEmitter>,
) -> Result<Value, DbError> {
    let mut chosen: Option<Slots> = None;
    let mut deliberately_unclothed = false;
    // Track whether we announced a consult so the fallback path can still
    // resolve the dialog's panel instead of leaving it spinning.
    let mut consulted = false;
    let mut consulted_name = String::new();

    // v4 wraps the whole attempt in try/catch → fall to default; a read error
    // here propagates to the create handler's try/catch (as v4's throws do).
    let character = characters_read::find_by_id(main, mount, character_id)?;
    let wardrobe_items = pools.get(character_id);
    let all_profiles = connection_profiles::find_all(main)?;

    if let LlmChooseOutcome::Attempted {
        character_name,
        choice,
    } = choose_llm_outfit(
        completion,
        executor,
        character.as_ref(),
        &wardrobe_items,
        &all_profiles,
        ctx.cheap_settings,
        ctx.scenario_text,
        || {
            resolve_dressing_instructions_conn(
                main,
                mount,
                character.as_ref(),
                Some(chat_id),
                character_id,
                ctx.project_mount_point_ids,
            )
        },
        character_id,
        emitter,
    )
    .await
    {
        consulted = true;
        consulted_name = character_name;
        match accept_llm_choice(choice.as_ref()) {
            Some((slots, bare)) => {
                // The prompt lets the model pick a bundle outright; break it
                // into its parts before it's stored, so the wardrobe reads as
                // garments rather than an opaque card (`61574563`).
                chosen = Some(dissolve_bundles_in_slots(
                    &slots,
                    &lookup_from(&wardrobe_items),
                ));
                deliberately_unclothed = bare;
            }
            None => {
                tracing::warn!(
                    chat_id,
                    character_id,
                    pool_size = wardrobe_items.len(),
                    "[applyOutfitSelections] LLM outfit selection failed, falling back to defaults"
                );
            }
        }
    }

    if let Some(slots) = chosen {
        if deliberately_unclothed {
            tracing::info!(
                chat_id,
                character_id,
                "[applyOutfitSelections] Character is deliberately unclothed by LLM choice"
            );
            if let Some(emitter) = emitter {
                emitter.log(
                    format!("{consulted_name} chose to wear nothing at all."),
                    Some(LogLevel::Info),
                );
            }
        }
        publish_preview(
            main,
            mount,
            emitter,
            character_id,
            &consulted_name,
            &slots,
            ctx.project_mount_point_ids,
        );
        return Ok(slots_to_value(&slots));
    }

    let slots = default_outfit_from_pool(&wardrobe_items);
    // If we already told the dialog we were consulting this character, resolve
    // their panel with the default we fell back to (and note it).
    if consulted {
        if let Some(emitter) = emitter {
            emitter.log(
                format!("{consulted_name} settled on their usual attire."),
                Some(LogLevel::Warn),
            );
            publish_preview(
                main,
                mount,
                Some(emitter),
                character_id,
                &consulted_name,
                &slots,
                ctx.project_mount_point_ids,
            );
        }
    }
    Ok(slots_to_value(&slots))
}

/// v4 `publishPreview` — publish a resolved outfit to the status dialog. Inert
/// without an emitter, and never fails (the resolve degrades to empty slots).
fn publish_preview(
    main: &Connection,
    mount: &Connection,
    emitter: Option<&CreationProgressEmitter>,
    character_id: &str,
    character_name: &str,
    slots: &Slots,
    project_mount_point_ids: &[String],
) {
    let Some(emitter) = emitter else { return };
    let preview =
        to_outfit_preview_slots(main, mount, character_id, slots, project_mount_point_ids);
    emitter.wardrobe_result(character_id, character_name, preview);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::completion::{CompletionError, CompletionParams, CompletionResponse};
    use serde_json::json;

    fn item(id: &str, types: &[&str], is_default: bool) -> Value {
        json!({ "id": id, "title": id, "types": types, "isDefault": is_default })
    }

    #[test]
    fn parse_validates_ids_and_slot_coverage() {
        let items = vec![
            item("shirt", &["top"], false),
            item("jeans", &["bottom"], false),
            item("dress", &["top", "bottom"], false),
        ];
        // Valid ids in the right slots; a bogus id + wrong-slot id dropped.
        let content = r#"{"top":["shirt","dress","nope","jeans"],"bottom":["dress"],"footwear":[],"accessories":[]}"#;
        let choice = parse_outfit_response(content, &items);
        assert_eq!(choice.slots.top, vec!["shirt", "dress"]); // jeans (bottom-only) dropped from top
        assert_eq!(choice.slots.bottom, vec!["dress"]);
        assert!(choice.slots.footwear.is_empty());
        assert!(!choice.deliberately_unclothed);
    }

    #[test]
    fn parse_tolerates_scalar_and_non_object() {
        let items = vec![item("shirt", &["top"], false)];
        // Legacy single-id shape.
        let s1 = parse_outfit_response(r#"{"top":"shirt"}"#, &items);
        assert_eq!(s1.slots.top, vec!["shirt"]);
        // Non-object → empty, and never a deliberate claim.
        let s2 = parse_outfit_response("[1,2,3]", &items);
        assert!(s2.slots.top.is_empty() && s2.slots.bottom.is_empty());
        assert!(!s2.deliberately_unclothed);
    }

    #[test]
    fn parse_dedupes_within_a_slot_and_strips_fences() {
        let items = vec![item("shirt", &["top"], false)];
        let dedup = parse_outfit_response(r#"{"top":["shirt","shirt"]}"#, &items);
        assert_eq!(dedup.slots.top, vec!["shirt"]);
        let fenced = parse_outfit_response("```json\n{\"top\":[\"shirt\"]}\n```", &items);
        assert_eq!(fenced.slots.top, vec!["shirt"]);
    }

    // ── v4's `deliberate` contract (outfit-selection.test.ts) ────────────────

    #[test]
    fn a_flagged_empty_answer_is_deliberate() {
        let items = vec![item("shirt", &["top"], false)];
        let choice = parse_outfit_response(
            r#"{"deliberate":true,"top":[],"bottom":[],"footwear":[],"accessories":[]}"#,
            &items,
        );
        assert!(choice.deliberately_unclothed);
        // And it counts as an answer: empty slots, honoured, no default fallback.
        let (slots, bare) = accept_llm_choice(Some(&choice)).expect("flagged empty is accepted");
        assert!(bare);
        assert_eq!(slots, Slots::default());
    }

    #[test]
    fn an_unflagged_empty_answer_falls_back_to_defaults() {
        let items = vec![item("shirt", &["top"], false)];
        let choice = parse_outfit_response(r#"{"top":[],"bottom":[]}"#, &items);
        assert!(!choice.deliberately_unclothed);
        assert!(accept_llm_choice(Some(&choice)).is_none());
    }

    #[test]
    fn a_stringy_true_is_not_a_deliberate_claim() {
        let items = vec![item("shirt", &["top"], false)];
        for raw in [
            r#"{"deliberate":"true","top":[]}"#,
            r#"{"deliberate":1,"top":[]}"#,
        ] {
            let choice = parse_outfit_response(raw, &items);
            assert!(
                !choice.deliberately_unclothed,
                "only a real boolean counts: {raw}"
            );
            assert!(accept_llm_choice(Some(&choice)).is_none());
        }
    }

    #[test]
    fn picked_items_survive_alongside_a_deliberate_flag() {
        let items = vec![item("shirt", &["top"], false)];
        let choice = parse_outfit_response(r#"{"deliberate":true,"top":["shirt"]}"#, &items);
        assert!(choice.deliberately_unclothed);
        // Something WAS picked, so the "chose to wear nothing at all" line must
        // not fire — v4's `declaredBare && !picked`.
        let (slots, bare) = accept_llm_choice(Some(&choice)).expect("a pick is accepted");
        assert_eq!(slots.top, vec!["shirt"]);
        assert!(!bare);
    }

    #[test]
    fn a_missing_choice_falls_back() {
        assert!(accept_llm_choice(None).is_none());
    }

    // ── The tri-tier default resolution (v4's tiers tests, at the pure layer) ─

    #[test]
    fn defaults_layer_across_tiers_in_created_at_order() {
        // Created oldest-first: general, then project, then personal.
        let pool = vec![
            json!({ "id": "general-shirt", "types": ["top"], "isDefault": true, "createdAt": "2026-01-01T00:00:01.000Z" }),
            json!({ "id": "project-waistcoat", "types": ["top"], "isDefault": true, "createdAt": "2026-01-01T00:00:02.000Z" }),
            json!({ "id": "own-jacket", "types": ["top"], "isDefault": true, "createdAt": "2026-01-01T00:00:03.000Z" }),
        ];
        let slots = default_outfit_from_pool(&pool);
        assert_eq!(
            slots.top,
            vec!["general-shirt", "project-waistcoat", "own-jacket"]
        );
    }

    #[test]
    fn a_shadowing_is_default_false_copy_equips_nothing() {
        // What the pool hands over after `merge_wearable_pool` shadowed the
        // shared default with the character's opt-out copy.
        let pool = vec![json!({ "id": "livery", "types": ["top"], "isDefault": false })];
        assert_eq!(default_outfit_from_pool(&pool), Slots::default());
    }

    #[test]
    fn an_archived_default_is_excluded() {
        let pool = vec![
            json!({ "id": "archived-cloak", "types": ["top"], "isDefault": true, "archivedAt": "2026-02-02T00:00:00.000Z" }),
            json!({ "id": "live-cloak", "types": ["top"], "isDefault": true }),
        ];
        assert_eq!(default_outfit_from_pool(&pool).top, vec!["live-cloak"]);
    }

    #[test]
    fn a_multi_slot_default_lands_in_every_slot_it_covers() {
        let pool = vec![json!({ "id": "dress", "types": ["top", "bottom"], "isDefault": true })];
        let slots = default_outfit_from_pool(&pool);
        assert_eq!(slots.top, vec!["dress"]);
        assert_eq!(slots.bottom, vec!["dress"]);
        assert!(slots.footwear.is_empty());
    }

    // ── The consult's timing contract (v4's concurrency + timeout tests) ─────
    //
    // Not differential-reachable: an oracle diff can observe the outfits that
    // land, never the interleaving that produced them, and the harness's canned
    // runners resolve instantly so the timer never fires there. v4 pins these
    // with fake timers and instrumented fakes too
    // (`apply-outfit-selections.concurrency.test.ts`).

    /// A provider whose reply is delayed by `delay_ms`, or never arrives at all
    /// (`delay_ms: None`) — v4's "never settles; the real-world case was 2m32s
    /// for 19 tokens".
    struct SlowProvider {
        delay_ms: Option<u64>,
        in_flight: std::sync::Arc<std::sync::Mutex<(usize, usize)>>,
    }

    impl CompletionProvider for SlowProvider {
        async fn send_message(
            &self,
            _provider: &str,
            _base_url: Option<&str>,
            _params: &CompletionParams,
        ) -> Result<CompletionResponse, CompletionError> {
            {
                let mut g = self.in_flight.lock().unwrap();
                g.0 += 1;
                g.1 = g.1.max(g.0);
            }
            match self.delay_ms {
                Some(ms) => tokio::time::sleep(std::time::Duration::from_millis(ms)).await,
                // Never settles.
                None => std::future::pending::<()>().await,
            }
            self.in_flight.lock().unwrap().0 -= 1;
            Ok(CompletionResponse {
                content: r#"{"top":["shirt"]}"#.to_string(),
                usage: None,
                finish_reason: None,
                attachment_results: None,
                cache_usage: None,
            })
        }
    }

    fn consult_inputs() -> (Value, Vec<Value>, Vec<Value>) {
        (
            json!({ "id": "c1", "name": "Bertie" }),
            vec![item("shirt", &["top"], false)],
            vec![json!({ "id": "p1", "isDefault": true, "provider": "OPENAI", "model": "gpt-x" })],
        )
    }

    #[tokio::test(start_paused = true)]
    async fn a_stalled_provider_gives_up_at_the_timeout() {
        let (character, items, profiles) = consult_inputs();
        let provider = SlowProvider {
            delay_ms: None,
            in_flight: Default::default(),
        };
        let executor = CheapLlmTaskExecutor::new();
        let started = tokio::time::Instant::now();
        let outcome = choose_llm_outfit(
            &provider,
            &executor,
            Some(&character),
            &items,
            &profiles,
            None,
            None,
            || None,
            "c1",
            None,
        )
        .await;
        // The consult is abandoned, not awaited forever, and reads as "no
        // usable answer" so the caller dresses them in their defaults.
        match outcome {
            LlmChooseOutcome::Attempted { choice, .. } => assert!(choice.is_none()),
            LlmChooseOutcome::NotAttempted => panic!("the consult did start"),
        }
        // P4.D42 (v4 `74ec93b5`) bounded the consult TWICE, and on a REMOTE
        // profile the INNER bound used to be the tighter one: `executeCheapLLMTask`
        // abandoned its own attempt at 45 s, inside the 60 s phase ceiling.
        //
        // **P4.D136 (v4 `a1d88aa3a`, bug 107) INVERTED that layering**, and v4
        // inverted it too. The shared background tier is 90 s now, and the outfit
        // consult passes NO latency class — v4's `applyOutfitSelections`
        // (`lib/wardrobe/apply-outfit-selections.ts:390`) calls `chooseLLMOutfit`
        // through `withTimeout(…, OUTFIT_LLM_TIMEOUT_MS)` and never reaches for
        // the options bag — so both sides take the `background` default and
        // `OUTFIT_LLM_TIMEOUT_MS` (60 s, untouched by that commit) is now the
        // binding bound on a remote profile.
        //
        // Unlike the memory recap, which v4 deliberately declares `interactive`
        // precisely to keep its phase ceiling above its legs, v4 left this one to
        // invert. So v5 reproduces the inversion rather than protecting against
        // it. The literals stay spelled out rather than read back from the
        // constants, so moving either bound has to be a deliberate edit here too.
        assert_eq!(
            started.elapsed().as_millis() as u64,
            60_000,
            "a remote consult now gives up at the OUTFIT phase ceiling, which the \
             90 s background attempt deadline no longer sits inside"
        );
    }

    /// The other side of that layering: on a LOCAL profile the attempt deadline
    /// is 180 s, so `OUTFIT_LLM_TIMEOUT_MS` is once again the binding one and
    /// the phase ceiling still fires at exactly v4's literal. (Without this the
    /// P4.D42 attempt deadline would have shadowed the outfit ceiling entirely
    /// and nothing would have noticed.)
    #[tokio::test(start_paused = true)]
    async fn a_stalled_local_provider_gives_up_at_the_outfit_phase_ceiling() {
        let (character, items, _) = consult_inputs();
        let profiles = vec![json!({
            "id": "p1", "isDefault": true, "provider": "OLLAMA", "model": "qwen3"
        })];
        let provider = SlowProvider {
            delay_ms: None,
            in_flight: Default::default(),
        };
        let executor = CheapLlmTaskExecutor::new();
        let started = tokio::time::Instant::now();
        let outcome = choose_llm_outfit(
            &provider,
            &executor,
            Some(&character),
            &items,
            &profiles,
            None,
            None,
            || None,
            "c1",
            None,
        )
        .await;
        match outcome {
            LlmChooseOutcome::Attempted { choice, .. } => assert!(choice.is_none()),
            LlmChooseOutcome::NotAttempted => panic!("the consult did start"),
        }
        assert_eq!(
            started.elapsed().as_millis() as u64,
            60_000,
            "gave up at exactly v4's OUTFIT_LLM_TIMEOUT_MS"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_batch_of_consults_is_bounded_by_the_slowest_not_their_sum() {
        let (character, items, profiles) = consult_inputs();
        let in_flight: std::sync::Arc<std::sync::Mutex<(usize, usize)>> = Default::default();
        let provider = SlowProvider {
            delay_ms: Some(40_000),
            in_flight: in_flight.clone(),
        };
        let executor = CheapLlmTaskExecutor::new();

        let started = tokio::time::Instant::now();
        let outcomes = futures_util::future::join_all((0..4).map(|_| {
            choose_llm_outfit(
                &provider,
                &executor,
                Some(&character),
                &items,
                &profiles,
                None,
                None,
                || None,
                "c1",
                None,
            )
        }))
        .await;

        assert_eq!(outcomes.len(), 4);
        for outcome in outcomes {
            match outcome {
                LlmChooseOutcome::Attempted { choice, .. } => {
                    assert_eq!(choice.expect("a reply landed").slots.top, vec!["shirt"]);
                }
                LlmChooseOutcome::NotAttempted => panic!("every consult should have run"),
            }
        }
        // Four 40s calls: ~40s concurrent, ~160s serial.
        assert!(
            started.elapsed().as_millis() < 120_000,
            "four consults took {:?} — they ran serially",
            started.elapsed()
        );
        // And they really were in flight together.
        assert_eq!(in_flight.lock().unwrap().1, 4);
    }

    #[test]
    fn message_layout_is_byte_exact() {
        let character = json!({
            "name": "Aria",
            "manifesto": "Honor.",
            "description": "",
            "personality": "Bold."
        });
        let items = vec![
            json!({ "id": "a1", "title": "Cloak", "types": ["top"], "componentItemIds": [] }),
            json!({ "id": "a2", "title": "Set", "types": ["top", "bottom"], "componentItemIds": ["x", "y"], "description": "a bundle" }),
        ];
        let msgs = build_outfit_messages(&character, &items, Some("A keep."), None);
        assert_eq!(msgs[0].content, OUTFIT_SELECTION_PROMPT);
        let u = &msgs[1].content;
        assert!(
            u.starts_with("Character: Aria\nCharacter Manifesto (foundational tenets):\nHonor.")
        );
        // Empty description note omitted; personality note present.
        assert!(!u.contains("Character Description (behaviour and mannerisms):"));
        assert!(u.contains("\nCharacter Personality (internal drivers):\nBold."));
        assert!(u.contains("\nScenario: A keep."));
        assert!(u.contains("  - ID: a1 | \"Cloak\" (covers: top)"));
        assert!(u.contains("  - ID: a2 | \"Set\" [composite — bundles 2 other items] (covers: top, bottom) — a bundle"));
        assert!(u.ends_with("Choose what Aria should wear for this scene:"));
    }

    /// v4 `b86bb1a5`'s two `chooseLLMOutfit` unit arms, over
    /// [`build_outfit_messages`] directly.
    ///
    /// The blank-instructions arm is UNREACHABLE from production — the cascade's
    /// [`read_wardrobe_instructions_file`](crate::wardrobe_instructions::read_wardrobe_instructions_file)
    /// already answers `None` for a file that trims empty, so no caller ever
    /// hands this function a whitespace-only string. v4 pins the byte-identity
    /// the same way (a direct call), and so does this: the guard is the reason a
    /// caller who DOES pass `""` cannot slip an empty header into the prompt.
    #[test]
    fn dressing_instructions_note_is_byte_exact_and_omitted_when_blank() {
        let character = json!({ "name": "Aria", "description": "", "personality": "" });
        let items =
            vec![json!({ "id": "a1", "title": "Cloak", "types": ["top"], "componentItemIds": [] })];
        let user = |instr: Option<&str>| {
            build_outfit_messages(&character, &items, Some("A keep."), instr)[1]
                .content
                .clone()
        };

        let with = user(Some("  You wear tweed.  "));
        assert!(with.contains(
            "\nDressing Instructions (addressed to Aria in the second person — \"you\" is Aria):\nYou wear tweed.\n\nAvailable Wardrobe Items:"
        ));

        // null / empty / whitespace-only all produce the SAME message, and it is
        // the one the pre-`b86bb1a5` builder produced.
        let none = user(None);
        assert_eq!(none, user(Some("")));
        assert_eq!(none, user(Some("   \n  ")));
        assert!(!none.contains("Dressing Instructions"));
    }
}
