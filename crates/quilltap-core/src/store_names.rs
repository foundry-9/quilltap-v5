//! Document-store naming rules — v4's `lib/mount-index/store-names.ts`
//! (`ed8b15b50`, bug 186), pure.
//!
//! Store names form one case-insensitive namespace: a store is addressed by
//! name in `qtap://` URIs and in every doc tool, so two stores sharing a name
//! make that name ambiguous. On top of that namespace sit two naming rules for
//! character vaults:
//!
//!  - The vault a character points at is named after that character:
//!    `"<Name> Character Vault"`. Two live characters sharing a name cannot
//!    both hold it, so the later one takes the standard ` (N)` suffix.
//!  - A vault no character points at any more (an earlier vault left behind
//!    by a restore, a re-provision, or a deleted character) is retired to
//!    `"<Name> Version <timestamp> Store"`, freeing the plain name for the live
//!    vault. Only a store still carrying a vault-shaped name is retired; one
//!    the operator has renamed to something of their own is left alone.
//!
//! [`plan_store_names`] turns those rules into a rename list. It is pure, so
//! the boot reconcile, the restore and the tests all share one answer — and on
//! an instance v4 and v5 share, the two MUST plan the same renames, or each
//! boot undoes the other (the `store_names_equivalence` /
//! `store_name_reconcile_equivalence` families are that proof).
//!
//! ## JS fidelity
//!
//! - `trim` is JS's ([`js_trim`]: U+FEFF trims, U+0085 does not), and
//!   `toLowerCase` is `str::to_lowercase` (byte-identical to JS).
//! - v4's `VAULT_NAME_PATTERN` (`/^(.*\S)\s+Character Vault(?:\s+\(\d+\))*$/i`,
//!   non-unicode) is matched BY HAND in [`vault_base_name`], never through the
//!   `regex` crate, whose `\s` / `(?i)` are Unicode-aware: `.` excludes only
//!   `\n \r U+2028 U+2029`; `\s`/`\S` is [`is_js_ws`]; `\d` is ASCII; and the
//!   literal `Character Vault` folds ASCII case ONLY — non-unicode `/i`
//!   canonicalises through `toUpperCase` and refuses a non-ASCII → ASCII
//!   mapping, so `ſ` (→ `S`) and the Kelvin sign never match.
//! - `localeCompare` is ICU ([`locale_compare`]), as the case repair's
//!   `keep_oldest_less`.
//! - `Date.parse` is [`js_date_parse_ms`] (V8's ISO subset). V8's legacy forms
//!   and extended years are NOT ported: a store whose `createdAt` fails
//!   `TimestampSchema` (`z.iso.datetime()`) is dropped before the plan, so they
//!   cannot reach it (`STORE_STAMP_UNREACHABLE_FORMS`, pinned both ways in the
//!   tier-1 family).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::clock::iso_from_unix_ms;
use crate::collation::locale_compare;
use crate::db::ensure_official_store::next_unique_mount_point_name;
use crate::episodic::js_date_parse_ms;
use crate::jsstr::{is_js_ws, js_trim};

/// The name a character's live vault carries (v4 `characterVaultName`):
/// `` `${(name || 'Untitled').trim()} Character Vault` ``. An empty name is
/// `Untitled`; a whitespace-only name is truthy, trims to `''`, and so yields
/// `" Character Vault"` with a LEADING space — v4's own shape.
pub fn character_vault_name(character_name: &str) -> String {
    let name = if character_name.is_empty() {
        "Untitled"
    } else {
        character_name
    };
    format!("{} Character Vault", js_trim(name))
}

/// JS `.` in a non-unicode regex: anything but a line terminator.
fn is_js_dot(c: char) -> bool {
    !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Non-unicode `/i` equality against an ASCII pattern char: the input folds
/// ASCII case only (a non-ASCII char whose upper case is ASCII is refused).
fn ascii_ci_eq(input: char, pattern: char) -> bool {
    input.is_ascii() && input.eq_ignore_ascii_case(&pattern)
}

/// Does `tail` match `\s+Character Vault(?:\s+\(\d+\))*$` (non-unicode `/i`)?
/// Every quantifier here is followed by a char its class excludes, so the
/// greedy match is the only one — no backtracking is needed inside the tail.
fn tail_matches(tail: &str) -> bool {
    let chars: Vec<char> = tail.chars().collect();
    let mut i = 0;
    // \s+
    let start = i;
    while i < chars.len() && is_js_ws(chars[i]) {
        i += 1;
    }
    if i == start {
        return false;
    }
    // `Character Vault` — the inner space is a literal U+0020, not `\s`.
    for p in "Character Vault".chars() {
        let Some(&c) = chars.get(i) else {
            return false;
        };
        let ok = if p == ' ' {
            c == ' '
        } else {
            ascii_ci_eq(c, p)
        };
        if !ok {
            return false;
        }
        i += 1;
    }
    // (?:\s+\(\d+\))*$
    loop {
        if i == chars.len() {
            return true;
        }
        let ws = i;
        while i < chars.len() && is_js_ws(chars[i]) {
            i += 1;
        }
        if i == ws || chars.get(i) != Some(&'(') {
            return false;
        }
        i += 1;
        let digits = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i == digits || chars.get(i) != Some(&')') {
            return false;
        }
        i += 1;
    }
}

/// The character name a vault-shaped store name was built from
/// (`"Tester Character Vault (2)"` → `"Tester"`), or `None` when the name is
/// not vault-shaped (v4 `vaultBaseName`). Trailing ` (N)` suffixes — any number
/// of them, since a restore that meets a taken `"… (2)"` adds a second one —
/// are ignored.
///
/// The greedy `(.*\S)` with backtracking yields the LONGEST prefix that ends in
/// a non-space, holds no line terminator, and leaves a matching tail; so the
/// prefix is tried from the longest down.
pub fn vault_base_name(store_name: &str) -> Option<String> {
    let s = js_trim(store_name);
    // `.` cannot cross a line terminator: the group ends before the first one.
    let limit = s
        .char_indices()
        .find(|&(_, c)| !is_js_dot(c))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let mut ends: Vec<usize> = s[..limit]
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .collect();
    ends.reverse();
    for end in ends {
        let group = &s[..end];
        let last = group.chars().next_back().expect("non-empty group");
        if is_js_ws(last) {
            continue;
        }
        if tail_matches(&s[end..]) {
            return Some(group.to_string());
        }
    }
    None
}

/// A compact UTC stamp with no colons, so the name reads cleanly in a
/// `qtap://` authority: `2026-10-09T16:45:06.123Z` → `2026-10-09T164506Z`
/// (v4 `storeNameTimestamp`). Milliseconds are DROPPED, not rounded; an offset
/// input is normalised to UTC; an unparseable one falls back to
/// `iso.replace(/:/g, '')`.
pub fn store_name_timestamp(iso: &str) -> String {
    match js_date_parse_ms(iso) {
        None => iso.replace(':', ""),
        Some(ms) => {
            let d = iso_from_unix_ms(ms); // YYYY-MM-DDTHH:MM:SS.sssZ
            format!("{}T{}{}{}Z", &d[0..10], &d[11..13], &d[14..16], &d[17..19])
        }
    }
}

/// The name a vault takes once no character points at it (v4
/// `retiredVaultName`): `` `${base.trim()} Version ${stamp} Store` ``.
pub fn retired_vault_name(base_name: &str, created_at: &str) -> String {
    format!(
        "{} Version {} Store",
        js_trim(base_name),
        store_name_timestamp(created_at)
    )
}

/// Just enough of a store row to name it (v4 `StoreNameRow`).
#[derive(Debug, Clone, PartialEq)]
pub struct StoreNameRow {
    pub id: String,
    pub name: String,
    pub store_type: Option<String>,
    pub created_at: String,
}

/// Just enough of a character row to name its vault (v4 `VaultOwnerRow`).
#[derive(Debug, Clone, PartialEq)]
pub struct VaultOwnerRow {
    pub id: String,
    pub name: String,
    pub character_document_mount_point_id: Option<String>,
    pub created_at: String,
}

/// Why a store was renamed (v4 `StoreRename['reason']`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreRenameReason {
    LiveVault,
    RetiredVault,
    Collision,
}

impl StoreRenameReason {
    pub fn as_str(self) -> &'static str {
        match self {
            StoreRenameReason::LiveVault => "live-vault",
            StoreRenameReason::RetiredVault => "retired-vault",
            StoreRenameReason::Collision => "collision",
        }
    }
}

/// One planned rename (v4 `StoreRename`).
#[derive(Debug, Clone, PartialEq)]
pub struct StoreRename {
    pub id: String,
    pub from: String,
    pub to: String,
    pub reason: StoreRenameReason,
}

impl StoreRename {
    /// v4's object, in its key order `id, from, to, reason` — the order the
    /// reconcile's INFO line spreads it in.
    pub fn to_json(&self) -> Value {
        json!({ "id": self.id, "from": self.from, "to": self.to, "reason": self.reason.as_str() })
    }
}

/// v4 `lower`: `name.trim().toLowerCase()`.
fn lower(name: &str) -> String {
    js_trim(name).to_lowercase()
}

/// v4 `byAge`: `a.createdAt.localeCompare(b.createdAt) || a.id.localeCompare(b.id)`.
fn by_age(a_created: &str, a_id: &str, b_created: &str, b_id: &str) -> Ordering {
    locale_compare(a_created, b_created).then_with(|| locale_compare(a_id, b_id))
}

/// Decide every store's name under the rules above and return the renames
/// needed to get there (empty when the instance already conforms) — v4
/// `planStoreNames`.
///
/// Order of claim on the namespace:
///  1. Live vaults, each to its character's vault name. One that already holds
///     its exact name keeps it; then the older character wins. Each further
///     namesake takes the lowest free ` (N)`, so the names stay predictable.
///  2. Every other store, oldest first, keeps its name if it is still free,
///     otherwise takes the next ` (N)` (the case-insensitive collision repair).
///     An unlinked vault with a vault-shaped name is retired first — unless a
///     character with no vault bears that name, since `ensureCharacterVault`
///     may yet adopt it.
///
/// Consequences kept from v4: archived characters ARE owners (the caller reads
/// raw); an untyped store is never retired; a `documents` store is only ever
/// suffixed; a retired name is not vault-shaped, so it is never re-retired
/// (idempotence); the output is in INPUT order, and a trim-only or re-casing
/// change IS a rename (the compare is exact).
pub fn plan_store_names(stores: &[StoreNameRow], characters: &[VaultOwnerRow]) -> Vec<StoreRename> {
    let store_ids: HashSet<&str> = stores.iter().map(|s| s.id.as_str()).collect();

    // Store id → its owner (the oldest character pointing at it, should a
    // damaged instance have two).
    let mut owner_by_store: HashMap<&str, &VaultOwnerRow> = HashMap::new();
    // Vault names a character without a vault could still adopt.
    let mut adoptable_names: HashSet<String> = HashSet::new();
    let mut by_age_chars: Vec<&VaultOwnerRow> = characters.iter().collect();
    by_age_chars.sort_by(|a, b| by_age(&a.created_at, &a.id, &b.created_at, &b.id));
    for character in by_age_chars {
        // JS truthiness: an empty-string pointer is no pointer.
        match character.character_document_mount_point_id.as_deref() {
            Some(pointer) if !pointer.is_empty() && store_ids.contains(pointer) => {
                owner_by_store.entry(pointer).or_insert(character);
            }
            _ => {
                adoptable_names.insert(lower(&character_vault_name(&character.name)));
            }
        }
    }

    let mut taken: HashSet<String> = HashSet::new();
    let mut final_name: HashMap<&str, (String, StoreRenameReason)> = HashMap::new();

    // 1. Live vaults.
    let owner_of = |s: &StoreNameRow| -> &VaultOwnerRow { owner_by_store[s.id.as_str()] };
    let holds_canonical =
        |s: &StoreNameRow| lower(&s.name) == lower(&character_vault_name(&owner_of(s).name));
    let mut live_vaults: Vec<&StoreNameRow> = stores
        .iter()
        .filter(|s| owner_by_store.contains_key(s.id.as_str()))
        .collect();
    live_vaults.sort_by(|a, b| {
        holds_canonical(b)
            .cmp(&holds_canonical(a))
            .then_with(|| {
                let (oa, ob) = (owner_of(a), owner_of(b));
                by_age(&oa.created_at, &oa.id, &ob.created_at, &ob.id)
            })
            .then_with(|| by_age(&a.created_at, &a.id, &b.created_at, &b.id))
    });
    for store in live_vaults {
        let canonical = character_vault_name(&owner_of(store).name);
        let name = next_unique_mount_point_name(&taken, &canonical);
        taken.insert(lower(&name));
        final_name.insert(store.id.as_str(), (name, StoreRenameReason::LiveVault));
    }

    // 2. Everything else, oldest first.
    let mut others: Vec<&StoreNameRow> = stores
        .iter()
        .filter(|s| !owner_by_store.contains_key(s.id.as_str()))
        .collect();
    others.sort_by(|a, b| by_age(&a.created_at, &a.id, &b.created_at, &b.id));
    for store in others {
        let base = if store.store_type.as_deref() == Some("character") {
            vault_base_name(&store.name)
        } else {
            None
        };
        if let Some(base) = base {
            if !adoptable_names.contains(&lower(&character_vault_name(&base))) {
                let name = next_unique_mount_point_name(
                    &taken,
                    &retired_vault_name(&base, &store.created_at),
                );
                taken.insert(lower(&name));
                final_name.insert(store.id.as_str(), (name, StoreRenameReason::RetiredVault));
                continue;
            }
        }
        let name = if taken.contains(&lower(&store.name)) {
            next_unique_mount_point_name(&taken, js_trim(&store.name))
        } else {
            store.name.clone()
        };
        taken.insert(lower(&name));
        final_name.insert(store.id.as_str(), (name, StoreRenameReason::Collision));
    }

    stores
        .iter()
        .filter_map(|store| {
            let (name, reason) = &final_name[store.id.as_str()];
            (*name != store.name).then(|| StoreRename {
                id: store.id.clone(),
                from: store.name.clone(),
                to: name.clone(),
                reason: *reason,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(id: &str, name: &str, created_at: &str, store_type: &str) -> StoreNameRow {
        StoreNameRow {
            id: id.into(),
            name: name.into(),
            store_type: Some(store_type.into()),
            created_at: created_at.into(),
        }
    }

    fn character(id: &str, name: &str, vault: Option<&str>, created_at: &str) -> VaultOwnerRow {
        VaultOwnerRow {
            id: id.into(),
            name: name.into(),
            character_document_mount_point_id: vault.map(str::to_string),
            created_at: created_at.into(),
        }
    }

    // v4 jest `store-names.test.ts:39-48`.
    #[test]
    fn builds_and_parses_vault_names() {
        assert_eq!(character_vault_name(" Tester "), "Tester Character Vault");
        assert_eq!(character_vault_name(""), "Untitled Character Vault");
        assert_eq!(character_vault_name("  "), " Character Vault");
        assert_eq!(
            vault_base_name("Tester Character Vault").as_deref(),
            Some("Tester")
        );
        assert_eq!(
            vault_base_name("tester character vault (3)").as_deref(),
            Some("tester")
        );
        assert_eq!(
            vault_base_name("Lorian Character Vault (2) (2)").as_deref(),
            Some("Lorian")
        );
        assert_eq!(vault_base_name("X\nCharacter Vault").as_deref(), Some("X"));
        assert_eq!(
            vault_base_name("Tester Version 2026-10-09T164506Z Store"),
            None
        );
        assert_eq!(vault_base_name("Group Files: Aeronauts Club"), None);
        assert_eq!(vault_base_name("Chara\u{17F}ter Vault"), None);
        assert_eq!(vault_base_name("X Chara\u{212A}ter Vault"), None);
        assert_eq!(vault_base_name("X\u{85}Character Vault"), None);
    }

    #[test]
    fn stamps_a_retired_vault_without_colons() {
        assert_eq!(
            store_name_timestamp("2026-10-09T16:45:06.123Z"),
            "2026-10-09T164506Z"
        );
        assert_eq!(
            retired_vault_name("Tester", "2026-10-09T16:45:06.123Z"),
            "Tester Version 2026-10-09T164506Z Store"
        );
        assert_eq!(store_name_timestamp("not a date: 12:30"), "not a date 1230");
    }

    // v4 jest `store-names.test.ts:52-71` — the V4test case + idempotence.
    #[test]
    fn names_the_live_vault_plainly_and_retires_the_old_ones() {
        let stores = vec![
            store(
                "old-1",
                "Lorian Character Vault",
                "2026-09-01T10:00:00.000Z",
                "character",
            ),
            store(
                "old-2",
                "Lorian Character Vault (2)",
                "2026-09-15T10:00:00.000Z",
                "character",
            ),
            store(
                "live",
                "Lorian Character Vault (3)",
                "2026-10-01T10:00:00.000Z",
                "character",
            ),
            store(
                "general",
                "Quilltap General",
                "2026-01-01T00:00:00.000Z",
                "documents",
            ),
        ];
        let chars = vec![character(
            "lorian",
            "Lorian",
            Some("live"),
            "2026-01-01T00:00:00.000Z",
        )];
        let renames = plan_store_names(&stores, &chars);
        let tos: Vec<(&str, &str, &str)> = renames
            .iter()
            .map(|r| (r.id.as_str(), r.to.as_str(), r.reason.as_str()))
            .collect();
        assert_eq!(
            tos,
            vec![
                (
                    "old-1",
                    "Lorian Version 2026-09-01T100000Z Store",
                    "retired-vault"
                ),
                (
                    "old-2",
                    "Lorian Version 2026-09-15T100000Z Store",
                    "retired-vault"
                ),
                ("live", "Lorian Character Vault", "live-vault"),
            ]
        );
        let after: Vec<StoreNameRow> = stores
            .iter()
            .map(|s| StoreNameRow {
                name: renames
                    .iter()
                    .find(|r| r.id == s.id)
                    .map(|r| r.to.clone())
                    .unwrap_or_else(|| s.name.clone()),
                ..s.clone()
            })
            .collect();
        assert!(plan_store_names(&after, &chars).is_empty());
    }

    #[test]
    fn rename_json_key_order_is_v4s() {
        let r = StoreRename {
            id: "a".into(),
            from: "b".into(),
            to: "c".into(),
            reason: StoreRenameReason::Collision,
        };
        assert_eq!(
            serde_json::to_string(&r.to_json()).unwrap(),
            r#"{"id":"a","from":"b","to":"c","reason":"collision"}"#
        );
    }
}
