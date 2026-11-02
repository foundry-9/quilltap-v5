//! Which memories a bulk policy path may remove — P4.D288 (C1-7), porting v4
//! `lib/memory/policy-protection.ts` (bug 201) at the `83334b9a5` pin; the
//! keystone class from P4.D341 (v4 `236c04e35`).
//!
//! "A chat wipe (`DELETE /memories?chatId=`, `MEMORY_REGENERATE_CHAT`) and the
//! near-duplicate folding tool act on what extraction wrote, and only on that.
//! Four kinds of row are out of their reach:
//!  - keystones — standing facts the operator marked (inner-circle-recall.md
//!    Part C); nothing automatic removes, folds or moves one;
//!  - MANUAL rows — written by hand; extraction will never write them back.
//!  - digests (`source: 'CONSOLIDATED'`) — their members point at them through
//!    `supersededById`; removing one leaves every member dangling.
//!  - cold rows — superseded members belong to their digest, and demoted rows
//!    are kept on purpose; both still block re-insertion at the gate.
//!
//! The per-row delete (`DELETE /memories/[id]`) is the true purge and does not
//! consult this." Pure.

/// v4 `PolicyProtection` — `'keystone' | 'manual' | 'digest' | 'cold'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PolicyProtection {
    Keystone,
    Manual,
    Digest,
    Cold,
}

impl PolicyProtection {
    /// v4's literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keystone => "keystone",
            Self::Manual => "manual",
            Self::Digest => "digest",
            Self::Cold => "cold",
        }
    }
}

/// v4 `Record<PolicyProtection, number>` — key order `keystone, manual,
/// digest, cold` (v4's initial `{ keystone: 0, manual: 0, digest: 0, cold: 0 }`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SparedBy {
    pub keystone: usize,
    pub manual: usize,
    pub digest: usize,
    pub cold: usize,
}

impl SparedBy {
    /// One more row spared for `protection`.
    pub fn count(&mut self, protection: PolicyProtection) {
        match protection {
            PolicyProtection::Keystone => self.keystone += 1,
            PolicyProtection::Manual => self.manual += 1,
            PolicyProtection::Digest => self.digest += 1,
            PolicyProtection::Cold => self.cold += 1,
        }
    }

    /// v4's object, in its key order.
    pub fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "keystone": self.keystone,
            "manual": self.manual,
            "digest": self.digest,
            "cold": self.cold,
        })
    }
}

/// v4 `policyProtectionOf(memory)`: why a bulk policy path must leave the row
/// alone, or `None` when it may act — `keystone` FIRST (v4 `if
/// (memory.keystone) return 'keystone'`), then `source` MANUAL, then
/// CONSOLIDATED, then `tier` cold, in that order.
pub fn policy_protection_of(
    source: Option<&str>,
    tier: Option<&str>,
    keystone: bool,
) -> Option<PolicyProtection> {
    if keystone {
        return Some(PolicyProtection::Keystone);
    }
    match source {
        Some("MANUAL") => Some(PolicyProtection::Manual),
        Some("CONSOLIDATED") => Some(PolicyProtection::Digest),
        _ if tier == Some("cold") => Some(PolicyProtection::Cold),
        _ => None,
    }
}

/// v4 `partitionByPolicyProtection(memories)`: `(actionable, spared, sparedBy)`
/// in input order; `of` reads each item's `(source, tier, keystone)`.
pub fn partition_by_policy_protection<T: Clone>(
    memories: &[T],
    of: impl Fn(&T) -> (Option<&str>, Option<&str>, bool),
) -> (Vec<T>, Vec<T>, SparedBy) {
    let mut actionable = Vec::new();
    let mut spared = Vec::new();
    let mut spared_by = SparedBy::default();
    for memory in memories {
        let (source, tier, keystone) = of(memory);
        match policy_protection_of(source, tier, keystone) {
            Some(protection) => {
                spared.push(memory.clone());
                spared_by.count(protection);
            }
            None => actionable.push(memory.clone()),
        }
    }
    (actionable, spared, spared_by)
}
