//! Side-effect applier — where a custom tool's resolved effects actually land
//! (v4 `lib/pascal/side-effects.ts`, NEW at `c4d4b0de`).
//!
//! The execution core ([`super::custom_tools::execute_custom_tool`]) resolves a
//! run's effects pure — which effect fires, what value it writes — and both
//! entrances hand the result here. This module decides WHERE each write goes
//! and issues the writes, batched one per touched store.
//!
//! ## "Write where it lives"
//!
//! A `state.<path>` effect lands at the tier whose top-level first path segment
//! already exists, searched in cascade-precedence order chat → project → group
//! → general (the project tier only when the cascade carries a `projectId`, the
//! group tier only under the exactly-one rule, `groupTier.status == "single"`).
//! A key found nowhere defaults to the CHAT tier — the most local store, the
//! least blast radius. The search runs against the local working copies, so an
//! earlier effect that minted a fresh key at the chat tier pins later writes of
//! that key there too.
//!
//! Known consequence, documented rather than fixed: with two or more applicable
//! groups the cascade's group tier contributes nothing (the exactly-one rule),
//! so a key living only in group state is invisible to this search and the
//! effect shadows it at the chat tier — consistent with what a read of the
//! merged cascade would have shown.
//!
//! ## Read once, never re-read
//!
//! The cascade and the metadata snapshot are read at run start and never
//! re-read — no read-your-writes anywhere. State tiers and character metadata
//! are whole-object replaces, so an effect racing a same-turn `state` call can
//! lose one side; that is the pre-existing accepted risk from
//! `state-cascade.md`, and re-reading just before the write is deliberately
//! rejected. (v4 frames this as the `BACKGROUND_JOBS_CHILD` buffered-proxy
//! contract; v5 has no forked child — the single-writer task is the same
//! discipline by another road — but the read-once rule and its consequence are
//! identical, so the behavior is unchanged.)
//!
//! ## Never fails the run
//!
//! The roll already happened and Pascal still announces. Each store's write is
//! individually caught; a failed store logs a warning and its effects drop from
//! the applied list. Nothing here returns an error to the caller.
//!
//! ## The v5 shape
//!
//! v4 awaits five repository calls through its buffered proxy. v5's four state
//! paths are heterogeneous — the chat tier is a real column, project and group
//! state go through the document-store overlay writers, general state is the
//! mount-root document, and character metadata is the `metadata.json`
//! whole-object replace — so the applier runs SYNCHRONOUSLY on a borrowed
//! [`WriterSet`], inside the one [`crate::db::runtime::Db::write`] closure each
//! entrance opens. The per-store "one write, individually fail-soft" contract is
//! expressed over those four paths rather than over five uniform repo calls.

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use super::custom_tool_types::EffectTarget;
use super::custom_tools::ResolvedEffect;
use crate::db::runtime::WriterSet;
use crate::state::cascade::StateCascadeResult;
use crate::state::paths::{get_at_path, set_at_path, PathKey};

const CONTEXT: &str = "pascal.side-effects";

/// The four stores a state effect can land in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectTier {
    Chat,
    Project,
    Group,
    General,
}

impl EffectTier {
    pub fn as_str(self) -> &'static str {
        match self {
            EffectTier::Chat => "chat",
            EffectTier::Project => "project",
            EffectTier::Group => "group",
            EffectTier::General => "general",
        }
    }
}

/// One write that actually landed — the shape `pascalMeta.effects` records.
///
/// Serialization is payload: this rides into a persisted `pascalMeta`, so the
/// key order is v4's object literal — `target`, then `previous` ONLY when the
/// store held something, then `next`, then `tier` on state writes alone.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedEffect {
    /// The effect's raw target (`"state.encounter.count"`, `"metadata.lockpick"`).
    pub target: String,
    /// What the store held before, when it held anything. `Some(Value::Null)` is
    /// a stored `null` and DOES serialize; `None` is v4's `undefined` and does
    /// not.
    pub previous: Option<Value>,
    /// What was written.
    pub next: Value,
    /// Which store a state write landed in. Absent on metadata writes.
    pub tier: Option<EffectTier>,
}

impl Serialize for AppliedEffect {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("target", &self.target)?;
        if let Some(previous) = &self.previous {
            m.serialize_entry("previous", previous)?;
        }
        m.serialize_entry("next", &self.next)?;
        if let Some(tier) = self.tier {
            m.serialize_entry("tier", tier.as_str())?;
        }
        m.end()
    }
}

/// Everything the applier needs, read once at run start.
pub struct ApplyCustomToolEffectsParams<'a> {
    pub chat_id: &'a str,
    pub tool_name: &'a str,
    /// The core's resolved effects; skipped entries are ignored here.
    pub effects: &'a [ResolvedEffect],
    /// The whole cascade, read once at run start — the RMW base for every state
    /// tier. `None` (the cascade could not be read) → state effects skip
    /// fail-soft.
    pub cascade: Option<&'a StateCascadeResult>,
    /// The rolling character. `None` → metadata effects skip fail-soft.
    pub character_id: Option<&'a str>,
    /// The character's fact sheet, hydrated at run start — the metadata RMW base.
    pub metadata_snapshot: &'a Map<String, Value>,
}

/// Every store an application may touch, for the ordered commit below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Store {
    Tier(EffectTier),
    Metadata,
}

/// What the pure planning pass worked out: the local working copies as they now
/// stand, and the applications in effect order, each tagged with the store it
/// rode.
///
/// v4 keeps planning and committing in one function because its five writes are
/// mockable repository calls; v5's four state paths are heterogeneous writer
/// calls, so the split is what makes v4's whole `side-effects.test.ts` matrix —
/// tier resolution, batching, sequential visibility through the copies, the
/// metadata RMW, the characterId gate, the cascade-null arm, the underscore
/// re-check — provable without a database. Only the COMMIT half needs one.
#[derive(Debug, Default)]
struct Plan {
    /// The four tiers as they now stand, `None` when the cascade was unreadable.
    tiers: Option<[Value; 4]>,
    /// The character's fact sheet with every metadata effect folded in, `None`
    /// when no metadata effect applied.
    metadata_next: Option<Map<String, Value>>,
    pending: Vec<(Store, AppliedEffect)>,
}

/// Apply a run's resolved effects. Returns the writes that landed, in effect
/// order, for `pascalMeta.effects`. Never fails the run.
pub fn apply_custom_tool_effects(
    writers: &mut WriterSet,
    params: ApplyCustomToolEffectsParams<'_>,
) -> Vec<AppliedEffect> {
    let chat_id = params.chat_id;
    let tool_name = params.tool_name;
    let cascade = params.cascade;
    let character_id = params.character_id;

    let Plan {
        tiers,
        metadata_next,
        pending,
    } = plan_applications(&params);
    if pending.is_empty() {
        return Vec::new();
    }

    commit_plan(
        writers,
        chat_id,
        tool_name,
        cascade,
        character_id,
        tiers,
        metadata_next,
        pending,
    )
}

/// The pure half: decide WHERE each applicable effect goes and what it writes,
/// mutating only local copies. Never touches a store.
fn plan_applications(params: &ApplyCustomToolEffectsParams<'_>) -> Plan {
    let &ApplyCustomToolEffectsParams {
        chat_id,
        tool_name,
        effects,
        cascade,
        character_id,
        metadata_snapshot,
    } = params;

    // v4 `effects.filter(isApplicableEffect)`. The resolved value is a
    // primitive; `to_value` is the JSON form the stores actually hold.
    let applicable: Vec<(&EffectTarget, Value)> = effects
        .iter()
        .filter_map(|e| match e {
            ResolvedEffect::Applicable { target, value, .. } => Some((target, value.to_value())),
            ResolvedEffect::Skipped { .. } => None,
        })
        .collect();
    if applicable.is_empty() {
        return Plan::default();
    }

    // Local working copies — read once, mutated in effect order, committed once
    // per touched store. Sequential effects see each other's values through
    // these, never through a store re-read.
    let mut tiers: Option<[Value; 4]> = cascade.map(|c| {
        [
            object_or_empty(&c.chat_state),
            object_or_empty(&c.project_state),
            object_or_empty(&c.group_state),
            object_or_empty(&c.general_state),
        ]
    });
    let mut metadata_next: Option<Map<String, Value>> = None;

    let skip = |reason: &str, target: &str| {
        tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chat_id,
            tool = tool_name,
            reason,
            effect_target = target,
            "Custom tool effect not applied",
        );
    };

    // Applications in effect order, each tagged with the store it rode.
    let mut pending: Vec<(Store, AppliedEffect)> = Vec::new();

    for (target, value) in &applicable {
        match target {
            EffectTarget::Metadata { key, raw } => {
                let Some(_) = character_id else {
                    // A run nobody made writes to nobody's sheet.
                    skip("no rolling character, metadata effect skipped", raw);
                    continue;
                };
                let next = metadata_next.get_or_insert_with(|| metadata_snapshot.clone());
                let previous = next.get(key).cloned();
                next.insert(key.clone(), value.clone());
                pending.push((
                    Store::Metadata,
                    AppliedEffect {
                        target: raw.clone(),
                        previous,
                        next: value.clone(),
                        tier: None,
                    },
                ));
            }

            EffectTarget::State { path, raw } => {
                let (Some(tiers), Some(cascade)) = (tiers.as_mut(), cascade) else {
                    skip("state cascade unavailable, state effect skipped", raw);
                    continue;
                };

                // Underscore guard, RE-CHECKED at apply time — defense in depth
                // behind the load-time rejection in `parse_effect_target`.
                if matches!(path.first(), Some(PathKey::Prop(s)) if s.starts_with('_')) {
                    tracing::warn!(
                        target: "quilltap::pascal",
                        context = CONTEXT,
                        chat_id,
                        tool = tool_name,
                        effect_target = raw,
                        "Custom tool effect refused — underscore-guarded state key",
                    );
                    continue;
                }

                // v4 `String(first)`: a numeric first segment addresses the key
                // `"0"`, exactly as a JS property lookup would.
                let first = match path.first() {
                    Some(PathKey::Prop(s)) => s.clone(),
                    Some(PathKey::Index(i)) => i.to_string(),
                    // Unreachable: `parse_effect_target` rejects an empty path.
                    None => continue,
                };

                let tier = resolve_tier(tiers, cascade, &first);
                let store = &mut tiers[tier as usize];
                let previous = get_at_path(store, path);
                if set_at_path(store, path, value.clone()).is_err() {
                    // Unreachable — `set_at_path` only refuses an EMPTY path, and
                    // `parse_effect_target` has already rejected that. Dropping
                    // the effect rather than propagating keeps the module's
                    // never-fails-the-run promise if it ever becomes reachable.
                    skip("state path could not be written, effect skipped", raw);
                    continue;
                }
                pending.push((
                    Store::Tier(tier),
                    AppliedEffect {
                        target: raw.clone(),
                        previous,
                        next: value.clone(),
                        tier: Some(tier),
                    },
                ));
            }
        }
    }

    Plan {
        tiers,
        metadata_next,
        pending,
    }
}

/// The impure half: at most one write per touched store, in the fixed order
/// chat → project → group → general → metadata, each individually caught. A
/// failed store's effects drop from the applied list.
#[allow(clippy::too_many_arguments)]
fn commit_plan(
    writers: &mut WriterSet,
    chat_id: &str,
    tool_name: &str,
    cascade: Option<&StateCascadeResult>,
    character_id: Option<&str>,
    tiers: Option<[Value; 4]>,
    metadata_next: Option<Map<String, Value>>,
    pending: Vec<(Store, AppliedEffect)>,
) -> Vec<AppliedEffect> {
    let touched: Vec<Store> = {
        let mut seen: Vec<Store> = Vec::new();
        for (store, _) in &pending {
            if !seen.contains(store) {
                seen.push(*store);
            }
        }
        seen
    };
    let mut failed: Vec<Store> = Vec::new();

    let commit =
        |store: Store, failed: &mut Vec<Store>, write: &mut dyn FnMut() -> Result<(), String>| {
            if !touched.contains(&store) {
                return;
            }
            if let Err(error) = write() {
                failed.push(store);
                tracing::warn!(
                    target: "quilltap::pascal",
                    context = CONTEXT,
                    chat_id,
                    tool = tool_name,
                    store = ?store,
                    error,
                    "Custom tool effect write failed; those effects were dropped",
                );
            }
        };

    // The fixed commit order: chat → project → group → general → metadata.
    if let (Some(tiers), Some(cascade)) = (tiers.as_ref(), cascade) {
        commit(
            Store::Tier(EffectTier::Chat),
            &mut failed,
            &mut || -> Result<(), String> {
                crate::tools::state::write_chat_state(
                    writers,
                    chat_id,
                    &tiers[EffectTier::Chat as usize],
                )
                .map_err(|e| e.to_string())
            },
        );
        commit(
            Store::Tier(EffectTier::Project),
            &mut failed,
            &mut || -> Result<(), String> {
                // Only touched when `resolve_tier` chose the project tier, which
                // requires the cascade to carry a `projectId`.
                let project_id = cascade
                    .project_id
                    .as_deref()
                    .ok_or_else(|| "project tier touched with no projectId".to_string())?;
                crate::tools::state::write_project_state(
                    writers,
                    project_id,
                    &tiers[EffectTier::Project as usize],
                )
                .map_err(|e| e.to_string())
            },
        );
        commit(
            Store::Tier(EffectTier::Group),
            &mut failed,
            &mut || -> Result<(), String> {
                let group_id = cascade
                    .group_tier
                    .applied_group_id
                    .as_deref()
                    .ok_or_else(|| "group tier touched with no appliedGroupId".to_string())?;
                crate::tools::state::write_group_state(
                    writers,
                    group_id,
                    &tiers[EffectTier::Group as usize],
                )
                .map_err(|e| e.to_string())
            },
        );
        commit(
            Store::Tier(EffectTier::General),
            &mut failed,
            &mut || -> Result<(), String> {
                let Some(mount_w) = writers.mount_index() else {
                    // Degraded open: v4 throws from `writeGeneralState`.
                    return Err("Quilltap General mount has not been provisioned yet".to_string());
                };
                let mount_c = mount_w.connection();
                let main_c = writers.main().connection();
                crate::services::mount_index::general_state::write_general_state(
                    main_c,
                    mount_c,
                    &tiers[EffectTier::General as usize],
                )
                .map_err(|e| e.to_string())
            },
        );
    }

    if let (Some(next), Some(character_id)) = (metadata_next.as_ref(), character_id) {
        commit(
            Store::Metadata,
            &mut failed,
            &mut || -> Result<(), String> {
                let Some(mount_w) = writers.mount_index() else {
                    return Err("character metadata requires mount-index".to_string());
                };
                let mount_c = mount_w.connection();
                let main_c = writers.main().connection();
                let mut patch = Map::new();
                patch.insert("metadata".to_string(), Value::Object(next.clone()));
                crate::db::vault_character_update::update_character(
                    main_c,
                    mount_c,
                    character_id,
                    &patch,
                )
                .map(|_| ())
                .map_err(|e| e.to_string())
            },
        );
    }

    let applied: Vec<AppliedEffect> = pending
        .into_iter()
        .filter(|(store, _)| !failed.contains(store))
        .map(|(_, entry)| entry)
        .collect();

    if !applied.is_empty() {
        tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chat_id,
            tool = tool_name,
            applied = ?applied.iter().map(|e| e.target.as_str()).collect::<Vec<_>>(),
            "Custom tool effects applied",
        );
    }

    applied
}

/// The shape both entrances call: `result.effects?.length ? await
/// applyCustomToolEffects({…}) : []`, wrapped in the one
/// [`Db::write`](crate::db::runtime::Db::write) closure the applier needs.
///
/// Shared rather than written twice because v4 writes the same six lines in
/// `run-custom-handler.ts` and the chat route, and the ONE deliberate asymmetry
/// between them (`characterId`) is the caller's argument, not this function's
/// business — the manual route passes `None` for a run nobody made, so an
/// unattributed operator roll cannot edit an arbitrary character's sheet.
pub async fn apply_effects_for_run(
    db: &crate::db::runtime::Db,
    chat_id: &str,
    result: &super::custom_tools::CustomToolRunResult,
    cascade: Option<StateCascadeResult>,
    character_id: Option<String>,
    metadata_snapshot: &Map<String, Value>,
) -> Vec<AppliedEffect> {
    let effects = match &result.effects {
        Some(e) if !e.is_empty() => e.clone(),
        _ => return Vec::new(),
    };

    let chat_id_owned = chat_id.to_string();
    let tool_name = result.tool.clone();
    let metadata = metadata_snapshot.clone();
    let outcome = db
        .write(move |writers| {
            Ok(apply_custom_tool_effects(
                writers,
                ApplyCustomToolEffectsParams {
                    chat_id: &chat_id_owned,
                    tool_name: &tool_name,
                    effects: &effects,
                    cascade: cascade.as_ref(),
                    character_id: character_id.as_deref(),
                    metadata_snapshot: &metadata,
                },
            ))
        })
        .await;

    match outcome {
        Ok(applied) => applied,
        // v5-only: the writer task is gone. v4 has no analogue, and the
        // module's promise is that nothing here fails a roll that already
        // happened — so this lands where a failed store lands, with the effects
        // dropped from the record and a warning in the log.
        Err(e) => {
            tracing::warn!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chat_id,
                tool = %result.tool,
                error = %e,
                "Custom tool effects could not be applied; the writer was unavailable",
            );
            Vec::new()
        }
    }
}

/// v4's `asStateObject` shape guard, applied to the local working copies: a
/// non-object tier spreads to `{}` in JS (`{...5}` is `{}`), so a defensive
/// coercion here is faithful rather than additive.
fn object_or_empty(v: &Value) -> Value {
    if v.is_object() {
        v.clone()
    } else {
        Value::Object(Map::new())
    }
}

/// "Write where it lives": the first tier, in cascade-precedence order, whose
/// top-level object already carries the key. Found nowhere → the chat tier.
fn resolve_tier(
    tiers: &[Value; 4],
    cascade: &StateCascadeResult,
    first_segment: &str,
) -> EffectTier {
    let mut searchable = vec![EffectTier::Chat];
    if cascade.project_id.is_some() {
        searchable.push(EffectTier::Project);
    }
    if cascade.group_tier.status == "single" {
        searchable.push(EffectTier::Group);
    }
    searchable.push(EffectTier::General);

    for tier in searchable {
        // `Object.prototype.hasOwnProperty` — presence, NOT truthiness, so a key
        // holding `null`/`0`/`false` still pins the tier.
        if tiers[tier as usize]
            .as_object()
            .is_some_and(|o| o.contains_key(first_segment))
        {
            return tier;
        }
    }
    EffectTier::Chat
}

#[cfg(test)]
mod tests {
    use super::super::custom_tool_types::parse_effect_target;
    use super::super::expressions::ExprValue;
    use super::*;
    use crate::state::cascade::GroupTier;
    use serde_json::json;

    /// The "write where it lives" matrix, over the pure resolver. v4 pins these
    /// arms through its whole applier against mocked repositories; v5's applier
    /// takes real writer connections, so the SEARCH is pinned here and the
    /// WRITES are pinned end-to-end by `pascal_custom_tools_route_equivalence`
    /// and `pascal_run_custom_handler_equivalence` over the rebuilt fixture.
    fn cascade(
        project: Option<&str>,
        group_status: &'static str,
        group: Option<&str>,
    ) -> StateCascadeResult {
        StateCascadeResult {
            chat_state: json!({}),
            project_state: json!({}),
            group_state: json!({}),
            general_state: json!({}),
            merged: json!({}),
            group_tier: GroupTier {
                status: group_status,
                candidates: Vec::new(),
                applied_group_id: group.map(str::to_string),
            },
            project_id: project.map(str::to_string),
        }
    }

    fn tiers(chat: Value, project: Value, group: Value, general: Value) -> [Value; 4] {
        [chat, project, group, general]
    }

    #[test]
    fn writes_a_key_back_to_the_tier_it_already_lives_in() {
        let c = cascade(Some("p1"), "single", Some("g1"));
        let t = tiers(
            json!({"a": 1}),
            json!({"b": 2}),
            json!({"c": 3}),
            json!({"d": 4}),
        );
        assert_eq!(resolve_tier(&t, &c, "a"), EffectTier::Chat);
        assert_eq!(resolve_tier(&t, &c, "b"), EffectTier::Project);
        assert_eq!(resolve_tier(&t, &c, "c"), EffectTier::Group);
        assert_eq!(resolve_tier(&t, &c, "d"), EffectTier::General);
    }

    #[test]
    fn a_key_found_nowhere_defaults_to_the_chat_tier() {
        let c = cascade(Some("p1"), "single", Some("g1"));
        let t = tiers(json!({}), json!({}), json!({}), json!({}));
        assert_eq!(resolve_tier(&t, &c, "brand_new"), EffectTier::Chat);
    }

    #[test]
    fn the_chat_tier_wins_when_a_key_lives_at_several() {
        let c = cascade(Some("p1"), "single", Some("g1"));
        let t = tiers(
            json!({"k": 1}),
            json!({"k": 2}),
            json!({"k": 3}),
            json!({"k": 4}),
        );
        assert_eq!(resolve_tier(&t, &c, "k"), EffectTier::Chat);
    }

    #[test]
    fn the_project_tier_is_never_searched_without_a_project_id() {
        let c = cascade(None, "single", Some("g1"));
        let t = tiers(json!({}), json!({"k": 2}), json!({}), json!({}));
        // The project store carries the key, but the cascade names no project,
        // so the search skips that tier entirely and the write defaults to chat.
        assert_eq!(resolve_tier(&t, &c, "k"), EffectTier::Chat);
    }

    #[test]
    fn an_ambiguous_group_tier_is_invisible_and_the_key_shadows_at_chat() {
        // The documented consequence of the exactly-one rule: with two or more
        // applicable groups the cascade contributes no group state, so a
        // group-only key is unreachable and the effect shadows it at chat —
        // consistent with what a read of the merged cascade would have shown.
        let c = cascade(None, "ambiguous", None);
        let t = tiers(json!({}), json!({}), json!({"k": 3}), json!({}));
        assert_eq!(resolve_tier(&t, &c, "k"), EffectTier::Chat);
    }

    #[test]
    fn presence_pins_the_tier_even_when_the_key_holds_a_falsy_value() {
        // `hasOwnProperty`, not truthiness — a key holding null / 0 / false
        // still says "this is where it lives".
        let c = cascade(Some("p1"), "single", Some("g1"));
        for held in [json!(null), json!(0), json!(false), json!("")] {
            let t = tiers(json!({}), json!({ "k": held }), json!({}), json!({}));
            assert_eq!(resolve_tier(&t, &c, "k"), EffectTier::Project);
        }
    }

    // ---------------------------------------------------------------------
    // v4's `side-effects.test.ts` matrix, over the pure planning pass.
    //
    // v4 mocks its five repository calls, so its whole matrix runs in one unit
    // file. v5's writes are heterogeneous writer calls, so the matrix splits:
    // everything below is planning, and the COMMIT half (one write per touched
    // store, per-store failure isolation) is proven end-to-end by
    // `pascal_custom_tools_route_equivalence` / `pascal_run_custom_handler_equivalence`
    // over the rebuilt fixture.
    // ---------------------------------------------------------------------

    fn full_cascade(
        chat: Value,
        project: Value,
        group: Value,
        general: Value,
    ) -> StateCascadeResult {
        StateCascadeResult {
            chat_state: chat,
            project_state: project,
            group_state: group,
            general_state: general,
            merged: json!({}),
            group_tier: GroupTier {
                status: "single",
                candidates: Vec::new(),
                applied_group_id: Some("g1".to_string()),
            },
            project_id: Some("p1".to_string()),
        }
    }

    fn state_effect(index: usize, target: &str, value: ExprValue) -> ResolvedEffect {
        ResolvedEffect::Applicable {
            index,
            target: parse_effect_target(target).expect("target parses"),
            value,
        }
    }

    fn plan(
        effects: &[ResolvedEffect],
        cascade: Option<&StateCascadeResult>,
        character_id: Option<&str>,
        metadata: &Map<String, Value>,
    ) -> Plan {
        plan_applications(&ApplyCustomToolEffectsParams {
            chat_id: "chat-1",
            tool_name: "probe",
            effects,
            cascade,
            character_id,
            metadata_snapshot: metadata,
        })
    }

    #[test]
    fn issues_one_application_per_effect_but_batches_by_store() {
        let c = full_cascade(json!({"a": 1}), json!({}), json!({}), json!({}));
        let effects = [
            state_effect(0, "state.a", ExprValue::Number(2.0)),
            state_effect(1, "state.a", ExprValue::Number(3.0)),
            state_effect(2, "state.b", ExprValue::Bool(true)),
        ];
        let p = plan(&effects, Some(&c), None, &Map::new());
        // Three applications in EFFECT order...
        assert_eq!(p.pending.len(), 3);
        // ...but all on ONE store, so the commit issues one write.
        let stores: Vec<Store> = p.pending.iter().map(|(s, _)| *s).collect();
        assert!(stores.iter().all(|s| *s == Store::Tier(EffectTier::Chat)));
        // The last write wins in the local copy.
        assert_eq!(p.tiers.unwrap()[0], json!({"a": 3, "b": true}));
    }

    #[test]
    fn sequential_effects_see_each_other_through_the_local_copies() {
        let c = full_cascade(json!({}), json!({}), json!({}), json!({}));
        let effects = [
            state_effect(0, "state.fresh", ExprValue::Number(1.0)),
            state_effect(1, "state.fresh", ExprValue::Number(2.0)),
        ];
        let p = plan(&effects, Some(&c), None, &Map::new());
        // The SECOND application's `previous` is the FIRST's write, read back
        // from the local copy — never from a store re-read.
        assert_eq!(p.pending[0].1.previous, None);
        assert_eq!(p.pending[1].1.previous, Some(json!(1)));
    }

    #[test]
    fn an_earlier_effect_minting_a_chat_key_pins_later_writes_there() {
        // `k` lives only in GENERAL. The first effect writes `k`, which resolves
        // to general; a DIFFERENT key `j` lives nowhere and mints at chat; then
        // a second write of `j` must stay at chat because the local copy now
        // carries it.
        let c = full_cascade(json!({}), json!({}), json!({}), json!({"k": 1}));
        let effects = [
            state_effect(0, "state.k", ExprValue::Number(2.0)),
            state_effect(1, "state.j", ExprValue::Number(3.0)),
            state_effect(2, "state.j", ExprValue::Number(4.0)),
        ];
        let p = plan(&effects, Some(&c), None, &Map::new());
        assert_eq!(p.pending[0].1.tier, Some(EffectTier::General));
        assert_eq!(p.pending[1].1.tier, Some(EffectTier::Chat));
        assert_eq!(p.pending[2].1.tier, Some(EffectTier::Chat));
        assert_eq!(p.pending[2].1.previous, Some(json!(3)));
    }

    #[test]
    fn metadata_read_modify_writes_the_snapshot_as_one_whole_object() {
        let mut snapshot = Map::new();
        snapshot.insert("keep".into(), json!("me"));
        snapshot.insert("lockpick".into(), json!("whole"));
        let effects = [
            ResolvedEffect::Applicable {
                index: 0,
                target: parse_effect_target("metadata.lockpick").unwrap(),
                value: ExprValue::String("broken pick".into()),
            },
            ResolvedEffect::Applicable {
                index: 1,
                target: parse_effect_target("metadata.tally").unwrap(),
                value: ExprValue::Number(1.0),
            },
        ];
        let p = plan(&effects, None, Some("char-1"), &snapshot);
        let next = p.metadata_next.expect("metadata folded");
        // Untouched keys SURVIVE — the write is a whole-object replace of a
        // read-modified copy, not a wipe.
        assert_eq!(next.get("keep"), Some(&json!("me")));
        assert_eq!(next.get("lockpick"), Some(&json!("broken pick")));
        assert_eq!(next.get("tally"), Some(&json!(1)));
        assert_eq!(p.pending[0].1.previous, Some(json!("whole")));
        assert_eq!(p.pending[1].1.previous, None);
        assert_eq!(p.pending[0].1.tier, None);
    }

    #[test]
    fn metadata_effects_skip_when_no_character_rolled() {
        let effects = [ResolvedEffect::Applicable {
            index: 0,
            target: parse_effect_target("metadata.lockpick").unwrap(),
            value: ExprValue::Bool(true),
        }];
        let p = plan(&effects, None, None, &Map::new());
        assert!(p.pending.is_empty());
        assert!(p.metadata_next.is_none());
    }

    #[test]
    fn state_effects_skip_when_the_cascade_could_not_be_read_keeping_metadata() {
        let effects = [
            state_effect(0, "state.a", ExprValue::Number(1.0)),
            ResolvedEffect::Applicable {
                index: 1,
                target: parse_effect_target("metadata.b").unwrap(),
                value: ExprValue::Bool(true),
            },
        ];
        let p = plan(&effects, None, Some("char-1"), &Map::new());
        assert_eq!(p.pending.len(), 1);
        assert_eq!(p.pending[0].0, Store::Metadata);
        assert!(p.tiers.is_none());
    }

    #[test]
    fn a_user_only_first_segment_is_refused_at_apply_time_too() {
        // UNREACHABLE from a loadable definition — `parse_effect_target` rejects
        // `state._x` at load, and this is the defense-in-depth re-check behind
        // it. v4 reaches it the same way: by handing the applier a resolved
        // effect the loader would never have produced.
        let c = full_cascade(json!({}), json!({}), json!({}), json!({}));
        let effects = [ResolvedEffect::Applicable {
            index: 0,
            target: EffectTarget::State {
                path: vec![
                    PathKey::Prop("_secrets".into()),
                    PathKey::Prop("combo".into()),
                ],
                raw: "state._secrets.combo".into(),
            },
            value: ExprValue::Number(1.0),
        }];
        let p = plan(&effects, Some(&c), None, &Map::new());
        assert!(p.pending.is_empty(), "the refused effect writes nothing");
        // And nothing was mutated in the local copy either.
        assert_eq!(p.tiers.unwrap()[0], json!({}));
    }

    #[test]
    fn skipped_resolutions_are_ignored_and_nothing_applies() {
        let effects = [
            ResolvedEffect::Skipped {
                index: 0,
                reason: "condition did not hold".into(),
            },
            ResolvedEffect::Skipped {
                index: 1,
                reason: "expression did not evaluate: …".into(),
            },
        ];
        let p = plan(&effects, None, Some("char-1"), &Map::new());
        assert!(p.pending.is_empty());
        assert!(p.tiers.is_none() && p.metadata_next.is_none());
    }

    #[test]
    fn a_nested_path_writes_into_the_tier_its_first_segment_lives_in() {
        let c = full_cascade(
            json!({}),
            json!({"encounter": {"count": 1}}),
            json!({}),
            json!({}),
        );
        let effects = [state_effect(
            0,
            "state.encounter.count",
            ExprValue::Number(2.0),
        )];
        let p = plan(&effects, Some(&c), None, &Map::new());
        assert_eq!(p.pending[0].1.tier, Some(EffectTier::Project));
        assert_eq!(p.pending[0].1.previous, Some(json!(1)));
        assert_eq!(p.tiers.unwrap()[1], json!({"encounter": {"count": 2}}));
    }

    #[test]
    fn applied_effect_serializes_in_v4s_key_order() {
        let with_previous = AppliedEffect {
            target: "state.encounter.count".into(),
            previous: Some(json!(2)),
            next: json!(3),
            tier: Some(EffectTier::Project),
        };
        assert_eq!(
            serde_json::to_string(&with_previous).unwrap(),
            r#"{"target":"state.encounter.count","previous":2,"next":3,"tier":"project"}"#
        );

        // `previous` is ABSENT when the store held nothing (v4 `undefined`), and
        // `tier` is absent on a metadata write.
        let fresh_metadata = AppliedEffect {
            target: "metadata.lockpick".into(),
            previous: None,
            next: json!("broken pick"),
            tier: None,
        };
        assert_eq!(
            serde_json::to_string(&fresh_metadata).unwrap(),
            r#"{"target":"metadata.lockpick","next":"broken pick"}"#
        );

        // A stored `null` is NOT absent — it serializes.
        let previously_null = AppliedEffect {
            target: "state.k".into(),
            previous: Some(Value::Null),
            next: json!(1),
            tier: Some(EffectTier::Chat),
        };
        assert_eq!(
            serde_json::to_string(&previously_null).unwrap(),
            r#"{"target":"state.k","previous":null,"next":1,"tier":"chat"}"#
        );
    }
}
