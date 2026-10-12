//! Which archived store each restored entity is bound to (bug 185) — v4
//! `lib/backup/restore/store-claims.ts` (`ed8b15b50`), ported by P4.D273.
//!
//! A character's vault and a project's or group's official store travel in the
//! archive beside the entity row, and the restore brings both back. The
//! entities' ordinary create paths always mint a fresh store, which is right for
//! a new entity and wrong here: the row ends up pointing at an empty store and
//! the archive's own sits beside it, unreferenced. So the restore asks this
//! claim map first. When an entity's archived pointer names a store the archive
//! carries, the entity keeps that pointer and nothing is minted; otherwise it
//! falls through to the create path.
//!
//! The first entity to claim a store keeps it. A later claimant falls back to a
//! fresh store with a warning, so a damaged archive can never cross-link two
//! entities. A character may not claim a store recorded as anything but a
//! vault, nor a project or group a vault, for the same reason.
//!
//! The pointer and the carried ids must be in the same id space: in
//! `new-account` mode both have been through the restore's UUID remap, which
//! maps each original id to one new id wherever it appears.
//!
//! This is v5's convergence onto v4's own fix for the port's filing (dogfood
//! #141 + #159). It supersedes P4.147's `replace`-only archived-store map and
//! P4.158's `StoreClaims` (whose v5-only WARN is retired — v4's
//! `noteStoreFallback` sentence + WARN, in the orchestrator, replace it).
//!
//! ## ⚠ RULED DIVERGENCE — `DUP_STORE_ID_FIRST_ROW` (P4.D273 R-A)
//!
//! v4 builds its map with `new Map(carriedStores.map(s => [s.id, s]))`, which
//! keeps the LAST row of a duplicated store id. Both engines' 22a primary key
//! keeps the FIRST row and refuses the second (`UNIQUE constraint failed:
//! doc_mount_points.id`), so v4's map can judge a claim on a row it never
//! restores (`restore-archive-dup-store-id.zip`: Lorian's vault id re-appended
//! LAST as a `documents` row → v4 calls her `wrong-kind` and mints a fresh
//! vault while her real one restores orphaned). v5 keeps the FIRST row per id
//! (P4.158 R-B), agreeing with 22a on which row is real. Pinned both ways in
//! `store_claims_equivalence` and `system_restore_state`.

use std::collections::HashMap;

use serde_json::Value;

/// The entity kinds that own a store (v4 `StoreOwnerKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreOwnerKind {
    Character,
    Project,
    Group,
}

impl StoreOwnerKind {
    /// v4's wire spelling (`'character' | 'project' | 'group'`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Project => "project",
            Self::Group => "group",
        }
    }
}

/// Why an entity falls back to a fresh store (v4's `reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackReason {
    /// The archive gave it none.
    NoPointer,
    /// The archive does not carry the store it names.
    NotCarried,
    /// The store is the wrong kind for this entity.
    WrongKind,
    /// An earlier entity in the archive holds it.
    AlreadyClaimed,
}

impl FallbackReason {
    /// v4's wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoPointer => "no-pointer",
            Self::NotCarried => "not-carried",
            Self::WrongKind => "wrong-kind",
            Self::AlreadyClaimed => "already-claimed",
        }
    }
}

/// One claim's outcome (v4 `StoreClaim`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreClaim {
    /// The entity keeps its archived pointer; nothing is minted.
    Bound { mount_point_id: String },
    /// The entity falls through to its create path. `claimed_by` names the
    /// holder for [`FallbackReason::AlreadyClaimed`] (v4's `claimedBy`), and is
    /// `None` for every other reason.
    Fallback {
        reason: FallbackReason,
        claimed_by: Option<(StoreOwnerKind, String)>,
    },
}

/// The claim map for one restore (v4 `makeStoreClaimMap`'s closure).
pub struct StoreClaimMap {
    /// Store id → its `storeType` as the archive row carries it (raw).
    carried: HashMap<String, Value>,
    /// Store id → the entity that claimed it first.
    holders: HashMap<String, (StoreOwnerKind, String)>,
}

/// JS truthiness of a JSON value (`!x` is false), as v4's `!storeType` /
/// `!pointer` test it.
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// Refuses only a positive mismatch: an archive from before `storeType` was
/// recorded carries none, and its stores are still the entities' own.
///
/// v4 `kindFits`: a falsy `storeType` (absent, `null`, `''` — anything JS
/// reads as false) fits EVERY kind; a character fits only `'character'`
/// (strict `===`, so a non-string never fits); a project or group fits
/// anything but `'character'`.
fn kind_fits(kind: StoreOwnerKind, store_type: Option<&Value>) -> bool {
    if !js_truthy(store_type) {
        return true;
    }
    let is_vault = matches!(store_type, Some(Value::String(s)) if s == "character");
    match kind {
        StoreOwnerKind::Character => is_vault,
        StoreOwnerKind::Project | StoreOwnerKind::Group => !is_vault,
    }
}

/// Build the claim map for one restore from the stores the archive carries —
/// the raw `doc_mount_points` rows (each row's `id` and `storeType`, read
/// RAW). A row whose `id` is not a string can never be named by a pointer
/// (a v5 archive row's ids are strings) and is skipped. A duplicated id keeps
/// its FIRST row — see the module's `DUP_STORE_ID_FIRST_ROW`.
pub fn make_store_claim_map(carried_stores: &[Value]) -> StoreClaimMap {
    let mut carried: HashMap<String, Value> = HashMap::new();
    for store in carried_stores {
        let Some(id) = store.get("id").and_then(Value::as_str) else {
            continue;
        };
        carried
            .entry(id.to_string())
            .or_insert_with(|| store.get("storeType").cloned().unwrap_or(Value::Null));
    }
    StoreClaimMap {
        carried,
        holders: HashMap::new(),
    }
}

impl StoreClaimMap {
    /// Claim the store `pointer` names for `entity_id`, in v4's order: no
    /// pointer (`None` / `""`) → `no-pointer`; not carried → `not-carried`;
    /// the wrong kind → `wrong-kind`; held → `already-claimed` (naming the
    /// holder); else the holder is recorded — at claim time, so a create that
    /// later refuses still holds it — and the entity is bound.
    pub fn claim(
        &mut self,
        kind: StoreOwnerKind,
        entity_id: &str,
        pointer: Option<&str>,
    ) -> StoreClaim {
        let fallback = |reason| StoreClaim::Fallback {
            reason,
            claimed_by: None,
        };
        let Some(pointer) = pointer.filter(|p| !p.is_empty()) else {
            return fallback(FallbackReason::NoPointer);
        };
        let Some(store_type) = self.carried.get(pointer) else {
            return fallback(FallbackReason::NotCarried);
        };
        if !kind_fits(kind, Some(store_type)) {
            return fallback(FallbackReason::WrongKind);
        }
        if let Some(holder) = self.holders.get(pointer) {
            return StoreClaim::Fallback {
                reason: FallbackReason::AlreadyClaimed,
                claimed_by: Some(holder.clone()),
            };
        }
        self.holders
            .insert(pointer.to_string(), (kind, entity_id.to_string()));
        StoreClaim::Bound {
            mount_point_id: pointer.to_string(),
        }
    }

    /// [`Self::claim`] over a raw archive value: a falsy pointer is
    /// `no-pointer` (v4's `!pointer`); a truthy NON-string can name no carried
    /// store (the carried ids are strings), so it is `not-carried`.
    pub fn claim_value(
        &mut self,
        kind: StoreOwnerKind,
        entity_id: &str,
        pointer: Option<&Value>,
    ) -> StoreClaim {
        match pointer {
            Some(Value::String(s)) => self.claim(kind, entity_id, Some(s)),
            other if !js_truthy(other) => self.claim(kind, entity_id, None),
            _ => StoreClaim::Fallback {
                reason: FallbackReason::NotCarried,
                claimed_by: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    //! v4's five jest cases (`__tests__/unit/lib/backup/store-claims.test.ts`
    //! `:17-49`) + the duplicate-id first-row row (R-A) + a claim held across
    //! a later refusal.
    use super::*;
    use serde_json::json;

    fn bound(id: &str) -> StoreClaim {
        StoreClaim::Bound {
            mount_point_id: id.to_string(),
        }
    }

    fn fell(reason: FallbackReason) -> StoreClaim {
        StoreClaim::Fallback {
            reason,
            claimed_by: None,
        }
    }

    /// v4's jest fixture: a vault, a documents store and an untyped store.
    fn claims() -> StoreClaimMap {
        make_store_claim_map(&[
            json!({"id": "vault", "storeType": "character"}),
            json!({"id": "docs", "storeType": "documents"}),
            json!({"id": "untyped", "storeType": null}),
        ])
    }

    #[test]
    fn binds_an_entity_to_the_carried_store_its_pointer_names() {
        let mut map = claims();
        assert_eq!(
            map.claim(StoreOwnerKind::Character, "c1", Some("vault")),
            bound("vault")
        );
        assert_eq!(
            map.claim(StoreOwnerKind::Project, "p1", Some("docs")),
            bound("docs")
        );
    }

    #[test]
    fn falls_through_when_there_is_no_pointer_or_the_store_is_not_carried() {
        let mut map = claims();
        assert_eq!(
            map.claim(StoreOwnerKind::Character, "c1", None),
            fell(FallbackReason::NoPointer)
        );
        assert_eq!(
            map.claim(StoreOwnerKind::Group, "g1", Some("elsewhere")),
            fell(FallbackReason::NotCarried)
        );
        // v4's `!pointer`: an empty string is no pointer either.
        assert_eq!(
            map.claim(StoreOwnerKind::Group, "g1", Some("")),
            fell(FallbackReason::NoPointer)
        );
    }

    #[test]
    fn refuses_a_vault_to_a_project_or_group_and_a_documents_store_to_a_character() {
        let mut map = claims();
        assert_eq!(
            map.claim(StoreOwnerKind::Group, "g1", Some("vault")),
            fell(FallbackReason::WrongKind)
        );
        assert_eq!(
            map.claim(StoreOwnerKind::Character, "c1", Some("docs")),
            fell(FallbackReason::WrongKind)
        );
    }

    #[test]
    fn accepts_a_store_whose_kind_the_archive_never_recorded() {
        assert_eq!(
            claims().claim(StoreOwnerKind::Character, "c1", Some("untyped")),
            bound("untyped")
        );
        assert_eq!(
            claims().claim(StoreOwnerKind::Project, "p1", Some("untyped")),
            bound("untyped")
        );
    }

    #[test]
    fn gives_a_store_to_the_first_claimant_only_across_entity_kinds() {
        let mut map = claims();
        assert!(matches!(
            map.claim(StoreOwnerKind::Project, "p1", Some("untyped")),
            StoreClaim::Bound { .. }
        ));
        assert_eq!(
            map.claim(StoreOwnerKind::Character, "c1", Some("untyped")),
            StoreClaim::Fallback {
                reason: FallbackReason::AlreadyClaimed,
                claimed_by: Some((StoreOwnerKind::Project, "p1".to_string())),
            }
        );
    }

    /// R-A: a duplicated id keeps the FIRST row (v4 keeps the last).
    #[test]
    fn a_duplicated_store_id_keeps_its_first_row() {
        let mut m = make_store_claim_map(&[
            json!({"id": "vault-1", "storeType": "character"}),
            json!({"id": "vault-1", "storeType": "documents"}),
        ]);
        assert_eq!(
            m.claim(StoreOwnerKind::Character, "c1", Some("vault-1")),
            bound("vault-1")
        );
    }

    /// The holder is recorded at claim time: a create that later refuses
    /// still holds the store, and the next claimant is told so.
    #[test]
    fn a_claim_is_held_across_a_later_refusal() {
        let mut m = make_store_claim_map(&[json!({"id": "store-1", "storeType": "documents"})]);
        assert_eq!(
            m.claim(StoreOwnerKind::Project, "p-refused", Some("store-1")),
            bound("store-1")
        );
        // (the bound create refuses here — the map is not told)
        assert_eq!(
            m.claim(StoreOwnerKind::Group, "g1", Some("store-1")),
            StoreClaim::Fallback {
                reason: FallbackReason::AlreadyClaimed,
                claimed_by: Some((StoreOwnerKind::Project, "p-refused".to_string())),
            }
        );
    }

    #[test]
    fn non_string_store_types_follow_js_truthiness() {
        let mut m = make_store_claim_map(&[
            json!({"id": "n0", "storeType": 0}),
            json!({"id": "n5", "storeType": 5}),
            json!({"id": "f", "storeType": false}),
        ]);
        assert_eq!(
            m.claim(StoreOwnerKind::Character, "c1", Some("n0")),
            bound("n0")
        );
        assert_eq!(
            m.claim(StoreOwnerKind::Character, "c2", Some("n5")),
            fell(FallbackReason::WrongKind)
        );
        assert_eq!(
            m.claim(StoreOwnerKind::Project, "p1", Some("n5")),
            bound("n5")
        );
        assert_eq!(
            m.claim(StoreOwnerKind::Character, "c3", Some("f")),
            bound("f")
        );
    }

    #[test]
    fn claim_value_reads_raw_pointers() {
        let mut m = make_store_claim_map(&[json!({"id": "vault-1", "storeType": "character"})]);
        assert_eq!(
            m.claim_value(StoreOwnerKind::Character, "c1", Some(&json!(null))),
            fell(FallbackReason::NoPointer)
        );
        assert_eq!(
            m.claim_value(StoreOwnerKind::Character, "c1", Some(&json!(7))),
            fell(FallbackReason::NotCarried)
        );
        assert_eq!(
            m.claim_value(StoreOwnerKind::Character, "c1", Some(&json!("vault-1"))),
            bound("vault-1")
        );
    }
}
