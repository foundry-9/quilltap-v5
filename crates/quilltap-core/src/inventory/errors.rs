//! Inventory refusals — every "no" the Inventory's mutations and equip can
//! say, each with the `code` the routes answer with (v4
//! `lib/inventory/errors.ts`, WHOLE at `322a5488e`; P4.D324). A refusal is a
//! 409 unless it says otherwise; the routes (P4.D325) map them in one place.
//!
//! v4's refusal CLASSES become constructor fns over ONE value type
//! ([`InventoryRefusal`] — `code`, `message`, `status`, `details`), so a
//! family compares the refusal VALUE (code, message, status and the details
//! bytes in v4's key order) and the route mapper spreads `{code,
//! ...details}`. All THIRTEEN codes ship here, the two title-lint codes
//! included: their constructors are pure string assembly over the title and
//! the collisions / unread pools and compile without the lint itself
//! (P4.D340 calls them).
//!
//! [`InventoryError`] is the umbrella every keystone fn returns where v4 lets
//! an `InventoryRefusal`, an `InventorySvgError`, an `InventoryFolder*Error`,
//! a `ForeignWardrobeImageError` or a `CharacterArchivedError` escape.

use serde::Serialize;
use serde_json::{json, Map, Value};

use super::folders::InventoryFolderError;
use crate::db::DbError;

/// The thirteen wire codes (v4 `InventoryRefusalCode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InventoryRefusalCode {
    OffRoster,
    ArchetypeInUse,
    ArchivedDependents,
    NotCopyable,
    CapExceeded,
    InvalidParent,
    KitNesting,
    KitStranded,
    NotEquippable,
    InvalidShape,
    NotAParticipant,
    TitleShadow,
    TitleCheckUnavailable,
}

impl InventoryRefusalCode {
    /// The SCREAMING_SNAKE wire code.
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryRefusalCode::OffRoster => "OFF_ROSTER",
            InventoryRefusalCode::ArchetypeInUse => "ARCHETYPE_IN_USE",
            InventoryRefusalCode::ArchivedDependents => "ARCHIVED_DEPENDENTS",
            InventoryRefusalCode::NotCopyable => "NOT_COPYABLE",
            InventoryRefusalCode::CapExceeded => "CAP_EXCEEDED",
            InventoryRefusalCode::InvalidParent => "INVALID_PARENT",
            InventoryRefusalCode::KitNesting => "KIT_NESTING",
            InventoryRefusalCode::KitStranded => "KIT_STRANDED",
            InventoryRefusalCode::NotEquippable => "NOT_EQUIPPABLE",
            InventoryRefusalCode::InvalidShape => "INVALID_SHAPE",
            InventoryRefusalCode::NotAParticipant => "NOT_A_PARTICIPANT",
            InventoryRefusalCode::TitleShadow => "TITLE_SHADOW",
            InventoryRefusalCode::TitleCheckUnavailable => "TITLE_CHECK_UNAVAILABLE",
        }
    }
}

/// One Inventory refusal (v4 `InventoryRefusal` and its eleven subclasses).
/// `Display` is v4's `message`.
#[derive(Debug, Clone, PartialEq)]
pub struct InventoryRefusal {
    pub code: InventoryRefusalCode,
    pub message: String,
    /// 400 or 409.
    pub status: u16,
    /// v4's `details` object, in v4's key order.
    pub details: Map<String, Value>,
}

impl std::fmt::Display for InventoryRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for InventoryRefusal {}

/// A holder of one archetype's instance (v4 `DependentHolder`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependentHolder {
    pub scope: String,
    pub owner_id: Option<String>,
    pub owner_name: Option<String>,
}

/// An archived character holding an instance (`ARCHIVED_DEPENDENTS`'s rows).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchivedDependent {
    pub id: Option<String>,
    pub name: Option<String>,
}

/// A kit a move would strand (v4 `findKitsStrandedByMove`'s rows). The
/// refusal's `details.kits` carries only `{ id, name, ownerName }` — v4's one
/// caller (`transfers.ts:91`) maps each row to those three keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrandedKit {
    pub id: String,
    pub name: Option<String>,
    pub owner_name: Option<String>,
    pub mount_point_id: String,
}

/// Why an object cannot be carried (`NOT_EQUIPPABLE`'s `reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotEquippableReason {
    Archetype,
    Archived,
    Kit,
    Missing,
}

impl NotEquippableReason {
    pub fn as_str(self) -> &'static str {
        match self {
            NotEquippableReason::Archetype => "archetype",
            NotEquippableReason::Archived => "archived",
            NotEquippableReason::Kit => "kit",
            NotEquippableReason::Missing => "missing",
        }
    }
}

/// One protected string a candidate title landed on (v4
/// `InventoryTitleCollision` — the lint itself is P4.D340's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryTitleCollision {
    KindNameShadow {
        kind_name: String,
    },
    LiveGateShadow {
        tool_name: String,
        reference: String,
        lands_on: Vec<String>,
    },
}

impl InventoryTitleCollision {
    fn is_kind(&self) -> bool {
        matches!(self, InventoryTitleCollision::KindNameShadow { .. })
    }

    /// v4's object literal (`{reason, kindName}` / `{reason, toolName, ref,
    /// landsOn}`).
    pub fn to_value(&self) -> Value {
        match self {
            InventoryTitleCollision::KindNameShadow { kind_name } => {
                json!({"reason": "kind-name shadow", "kindName": kind_name})
            }
            InventoryTitleCollision::LiveGateShadow {
                tool_name,
                reference,
                lands_on,
            } => json!({
                "reason": "live gate shadow",
                "toolName": tool_name,
                "ref": reference,
                "landsOn": lands_on,
            }),
        }
    }
}

/// What the title lint could not read (v4 `InventoryTitleLintUnread`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InventoryTitleLintUnread {
    /// The store (and file, when one file failed) that could not be read.
    pub pool: String,
    pub reason: String,
}

fn details(pairs: Vec<(&str, Value)>) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn refusal(
    code: InventoryRefusalCode,
    message: String,
    status: u16,
    details: Map<String, Value>,
) -> InventoryRefusal {
    InventoryRefusal {
        code,
        message,
        status,
        details,
    }
}

impl InventoryRefusal {
    /// `OFF_ROSTER` (v4 `InventoryOffRosterError`).
    pub fn off_roster(item_names: &[String]) -> Self {
        let quoted = item_names
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let verb = if item_names.len() == 1 { "is" } else { "are" };
        refusal(
            InventoryRefusalCode::OffRoster,
            format!(
                "{quoted} {verb} on the project's shelf, and you are not on this project's character roster. Ask the user to add you to the roster in the project's Characters card."
            ),
            409,
            details(vec![("itemNames", json!(item_names))]),
        )
    }

    /// `ARCHETYPE_IN_USE` (v4 `InventoryArchetypeInUseError`).
    pub fn archetype_in_use(dependents: usize, holders: &[DependentHolder]) -> Self {
        let noun = if dependents == 1 {
            "object derives"
        } else {
            "objects derive"
        };
        refusal(
            InventoryRefusalCode::ArchetypeInUse,
            format!("{dependents} {noun} from it"),
            409,
            details(vec![
                ("dependents", json!(dependents)),
                ("holders", json!(holders)),
            ]),
        )
    }

    /// `ARCHIVED_DEPENDENTS` (v4 `InventoryArchivedDependentsError`).
    pub fn archived_dependents(characters: &[ArchivedDependent]) -> Self {
        let names = characters
            .iter()
            .map(|c| {
                c.name
                    .clone()
                    .or_else(|| c.id.clone())
                    .unwrap_or_else(|| "unknown".to_string())
            })
            .collect::<Vec<_>>()
            .join(", ");
        refusal(
            InventoryRefusalCode::ArchivedDependents,
            format!(
                "Instances live in archived characters' vaults, which cannot be changed: {names}"
            ),
            409,
            details(vec![("characters", json!(characters))]),
        )
    }

    /// `NOT_COPYABLE` (v4 `InventoryNotCopyableError`).
    pub fn not_copyable(item_ids: &[String]) -> Self {
        refusal(
            InventoryRefusalCode::NotCopyable,
            format!("Cannot be copied: {}", item_ids.join(", ")),
            409,
            details(vec![("itemIds", json!(item_ids))]),
        )
    }

    /// `CAP_EXCEEDED` (v4 `InventoryCapExceededError`).
    pub fn cap_exceeded(item_id: &str, item_name: &str, cap: i64) -> Self {
        refusal(
            InventoryRefusalCode::CapExceeded,
            format!("\"{item_name}\" can be carried at most {cap} at a time"),
            409,
            details(vec![
                ("itemId", json!(item_id)),
                ("itemName", json!(item_name)),
                ("cap", json!(cap)),
            ]),
        )
    }

    /// `INVALID_PARENT` (v4 `InventoryInvalidParentError`) — the message and
    /// details are each site's own.
    pub fn invalid_parent(message: impl Into<String>, details: Map<String, Value>) -> Self {
        refusal(
            InventoryRefusalCode::InvalidParent,
            message.into(),
            409,
            details,
        )
    }

    /// `KIT_NESTING` (v4 `InventoryKitNestingError`).
    pub fn kit_nesting(member_ids: &[String]) -> Self {
        refusal(
            InventoryRefusalCode::KitNesting,
            format!(
                "A kit cannot contain another kit: {}",
                member_ids.join(", ")
            ),
            409,
            details(vec![("memberIds", json!(member_ids))]),
        )
    }

    /// `KIT_STRANDED` (v4 `InventoryKitStrandedError`).
    pub fn kit_stranded(item_id: &str, kits: &[StrandedKit]) -> Self {
        let n = kits.len();
        let names = kits
            .iter()
            .map(|k| k.name.clone().unwrap_or_else(|| k.id.clone()))
            .collect::<Vec<_>>()
            .join(", ");
        let plural = if n == 1 { "" } else { "s" };
        refusal(
            InventoryRefusalCode::KitStranded,
            format!("Moving this would strand it from {n} kit{plural} that name it: {names}"),
            409,
            details(vec![
                ("itemId", json!(item_id)),
                (
                    "kits",
                    Value::Array(
                        kits.iter()
                            .map(|k| json!({ "id": k.id, "name": k.name, "ownerName": k.owner_name }))
                            .collect(),
                    ),
                ),
            ]),
        )
    }

    /// `NOT_EQUIPPABLE` (v4 `InventoryNotEquippableError`).
    pub fn not_equippable(item_id: &str, reason: NotEquippableReason) -> Self {
        refusal(
            InventoryRefusalCode::NotEquippable,
            format!("Object {item_id} cannot be carried ({})", reason.as_str()),
            409,
            details(vec![
                ("itemId", json!(item_id)),
                ("reason", json!(reason.as_str())),
            ]),
        )
    }

    /// `INVALID_SHAPE` (v4 `InventoryShapeError`, a 400) — v4's callers
    /// build the message (`issues.join('; ')`, `Kit members not reachable
    /// from this tier: …`, …) and the details.
    pub fn shape(message: impl Into<String>, details: Map<String, Value>) -> Self {
        refusal(
            InventoryRefusalCode::InvalidShape,
            message.into(),
            400,
            details,
        )
    }

    /// v4 `InventoryTransferSameLocationError` — an `INVALID_SHAPE` with
    /// `Source and destination are the same` and no details.
    pub fn transfer_same_location() -> Self {
        Self::shape("Source and destination are the same", Map::new())
    }

    /// `NOT_A_PARTICIPANT` (v4 `InventoryNotAParticipantError`, a 400).
    pub fn not_a_participant(character_id: &str) -> Self {
        refusal(
            InventoryRefusalCode::NotAParticipant,
            format!("Character {character_id} is not a participant of this chat"),
            400,
            details(vec![("characterId", json!(character_id))]),
        )
    }

    /// `TITLE_SHADOW` (v4 `InventoryTitleShadowError`): the title and every
    /// collision, one line each, then what to do.
    pub fn title_shadow(title: &str, collisions: &[InventoryTitleCollision]) -> Self {
        let mut lines = vec![format!("title rejected: \"{title}\"")];
        for (i, collision) in collisions.iter().enumerate() {
            let lead = if i == 0 { "collides with" } else { "and with" };
            match collision {
                InventoryTitleCollision::KindNameShadow { kind_name } => lines.push(format!(
                    "{lead} KIND NAME \"{kind_name}\" (reason: kind-name shadow)"
                )),
                InventoryTitleCollision::LiveGateShadow {
                    tool_name,
                    reference,
                    lands_on,
                } => {
                    lines.push(format!(
                        "{lead} a gate key on tool \"{tool_name}\" (reason: live gate shadow)"
                    ));
                    let landed = lands_on
                        .iter()
                        .map(|n| format!("\"{n}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(format!(
                        "  that gate's \"{reference}\" already lands on {landed}"
                    ));
                }
            }
        }
        let kind = collisions.iter().any(InventoryTitleCollision::is_kind);
        let gate = collisions.iter().any(|c| !c.is_kind());
        lines.push(
            if kind && gate {
                "choose a title that is not a kind name and not a gate key."
            } else if kind {
                "choose a title that is not a kind name."
            } else {
                "choose a title that is not a gate key."
            }
            .to_string(),
        );
        refusal(
            InventoryRefusalCode::TitleShadow,
            lines.join("\n"),
            409,
            details(vec![
                ("title", json!(title)),
                (
                    "collisions",
                    Value::Array(collisions.iter().map(|c| c.to_value()).collect()),
                ),
            ]),
        )
    }

    /// `TITLE_CHECK_UNAVAILABLE` (v4 `InventoryTitleCheckUnavailableError`):
    /// the lint could not build its protected set, so the write is refused
    /// rather than passed unchecked.
    pub fn title_check_unavailable(title: &str, unread: &[InventoryTitleLintUnread]) -> Self {
        let mut lines = vec![format!("title rejected: \"{title}\"")];
        for u in unread {
            lines.push(format!(
                "the title check could not read {} ({})",
                u.pool, u.reason
            ));
        }
        lines.push(
            "a title is not accepted unchecked; repair what could not be read, then try again."
                .to_string(),
        );
        refusal(
            InventoryRefusalCode::TitleCheckUnavailable,
            lines.join("\n"),
            409,
            details(vec![("title", json!(title)), ("unread", json!(unread))]),
        )
    }

    /// The refusal as v4's `{code, …details}` object (what a route answers
    /// beside `error`), in v4's key order: `code` first, then the details.
    pub fn code_and_details(&self) -> Map<String, Value> {
        let mut out = Map::new();
        out.insert("code".into(), json!(self.code.as_str()));
        for (k, v) in &self.details {
            out.insert(k.clone(), v.clone());
        }
        out
    }
}

/// The glyph is not an SVG, or is too large once cleaned (v4
/// `InventorySvgError`, a 400 at the route). The payload is v4's sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventorySvgError(pub String);

impl std::fmt::Display for InventorySvgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InventorySvgError {}

/// Everything an Inventory keystone fn can fail with — the one type every
/// route maps (P4.D325's `inventory_error_response`).
#[derive(Debug)]
pub enum InventoryError {
    /// A coded refusal.
    Refusal(InventoryRefusal),
    /// An unsafe or oversize glyph (a 400).
    Svg(InventorySvgError),
    /// A folder rule / missing folder / conflict / foreign id.
    Folder(InventoryFolderError),
    /// v4 `ForeignWardrobeImageError` — the picture is not one of the
    /// object's own (`File ${fileId} is not an image of wardrobe item
    /// ${itemId}`, v4's text — the Inventory reuses the wardrobe's class).
    ForeignImage { item_id: String, file_id: String },
    /// v4 `CharacterArchivedError` — a write to an archived character's vault.
    CharacterArchived { character_id: String },
    /// A storage failure.
    Db(DbError),
    /// A plain v4 `Error` (`Character ${id} has no linked vault`, `Failed to
    /// remove the object from its source after the move`, …).
    Other(String),
}

impl std::fmt::Display for InventoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InventoryError::Refusal(r) => r.fmt(f),
            InventoryError::Svg(e) => e.fmt(f),
            InventoryError::Folder(e) => e.fmt(f),
            InventoryError::ForeignImage { item_id, file_id } => {
                write!(
                    f,
                    "File {file_id} is not an image of wardrobe item {item_id}"
                )
            }
            InventoryError::CharacterArchived { character_id } => DbError::CharacterArchived {
                character_id: character_id.clone(),
            }
            .fmt(f),
            InventoryError::Db(e) => e.fmt(f),
            InventoryError::Other(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for InventoryError {}

impl From<InventoryRefusal> for InventoryError {
    fn from(r: InventoryRefusal) -> Self {
        InventoryError::Refusal(r)
    }
}

impl From<InventorySvgError> for InventoryError {
    fn from(e: InventorySvgError) -> Self {
        InventoryError::Svg(e)
    }
}

impl From<InventoryFolderError> for InventoryError {
    fn from(e: InventoryFolderError) -> Self {
        InventoryError::Folder(e)
    }
}

impl From<DbError> for InventoryError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::CharacterArchived { character_id } => {
                InventoryError::CharacterArchived { character_id }
            }
            other => InventoryError::Db(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(r: &InventoryRefusal) -> (String, String, u16, String) {
        (
            r.code.as_str().to_string(),
            r.message.clone(),
            r.status,
            serde_json::to_string(&r.details).unwrap(),
        )
    }

    /// Every code's message, status and details bytes — v4 `errors.ts`
    /// verbatim (the two lint codes included).
    #[test]
    fn the_thirteen_codes() {
        let rows = vec![
            (
                InventoryRefusal::off_roster(&["Lamp".into()]),
                (
                    "OFF_ROSTER",
                    "\"Lamp\" is on the project's shelf, and you are not on this project's character roster. Ask the user to add you to the roster in the project's Characters card.",
                    409,
                    r#"{"itemNames":["Lamp"]}"#,
                ),
            ),
            (
                InventoryRefusal::off_roster(&["A".into(), "B".into()]),
                (
                    "OFF_ROSTER",
                    "\"A\", \"B\" are on the project's shelf, and you are not on this project's character roster. Ask the user to add you to the roster in the project's Characters card.",
                    409,
                    r#"{"itemNames":["A","B"]}"#,
                ),
            ),
            (
                InventoryRefusal::archetype_in_use(
                    1,
                    &[DependentHolder {
                        scope: "character".into(),
                        owner_id: Some("c1".into()),
                        owner_name: None,
                    }],
                ),
                (
                    "ARCHETYPE_IN_USE",
                    "1 object derives from it",
                    409,
                    r#"{"dependents":1,"holders":[{"scope":"character","ownerId":"c1","ownerName":null}]}"#,
                ),
            ),
            (
                InventoryRefusal::archetype_in_use(2, &[]),
                ("ARCHETYPE_IN_USE", "2 objects derive from it", 409, r#"{"dependents":2,"holders":[]}"#),
            ),
            (
                InventoryRefusal::archived_dependents(&[
                    ArchivedDependent { id: Some("c1".into()), name: Some("Vi".into()) },
                    ArchivedDependent { id: Some("c2".into()), name: None },
                    ArchivedDependent { id: None, name: None },
                ]),
                (
                    "ARCHIVED_DEPENDENTS",
                    "Instances live in archived characters' vaults, which cannot be changed: Vi, c2, unknown",
                    409,
                    r#"{"characters":[{"id":"c1","name":"Vi"},{"id":"c2","name":null},{"id":null,"name":null}]}"#,
                ),
            ),
            (
                InventoryRefusal::not_copyable(&["a".into(), "b".into()]),
                ("NOT_COPYABLE", "Cannot be copied: a, b", 409, r#"{"itemIds":["a","b"]}"#),
            ),
            (
                InventoryRefusal::cap_exceeded("i", "Lantern", 2),
                (
                    "CAP_EXCEEDED",
                    "\"Lantern\" can be carried at most 2 at a time",
                    409,
                    r#"{"itemId":"i","itemName":"Lantern","cap":2}"#,
                ),
            ),
            (
                InventoryRefusal::invalid_parent(
                    "An archived archetype cannot be instantiated",
                    details(vec![("archetypeId", json!("a"))]),
                ),
                (
                    "INVALID_PARENT",
                    "An archived archetype cannot be instantiated",
                    409,
                    r#"{"archetypeId":"a"}"#,
                ),
            ),
            (
                InventoryRefusal::kit_nesting(&["k".into()]),
                ("KIT_NESTING", "A kit cannot contain another kit: k", 409, r#"{"memberIds":["k"]}"#),
            ),
            (
                InventoryRefusal::kit_stranded(
                    "i",
                    &[StrandedKit {
                        id: "k".into(),
                        name: None,
                        owner_name: Some("Vi".into()),
                        mount_point_id: "mp".into(),
                    }],
                ),
                (
                    "KIT_STRANDED",
                    "Moving this would strand it from 1 kit that name it: k",
                    409,
                    r#"{"itemId":"i","kits":[{"id":"k","name":null,"ownerName":"Vi"}]}"#,
                ),
            ),
            (
                InventoryRefusal::not_equippable("i", NotEquippableReason::Archived),
                (
                    "NOT_EQUIPPABLE",
                    "Object i cannot be carried (archived)",
                    409,
                    r#"{"itemId":"i","reason":"archived"}"#,
                ),
            ),
            (
                InventoryRefusal::transfer_same_location(),
                ("INVALID_SHAPE", "Source and destination are the same", 400, "{}"),
            ),
            (
                InventoryRefusal::not_a_participant("c"),
                (
                    "NOT_A_PARTICIPANT",
                    "Character c is not a participant of this chat",
                    400,
                    r#"{"characterId":"c"}"#,
                ),
            ),
            (
                InventoryRefusal::title_shadow(
                    "Hat",
                    &[
                        InventoryTitleCollision::KindNameShadow { kind_name: "hat".into() },
                        InventoryTitleCollision::LiveGateShadow {
                            tool_name: "t".into(),
                            reference: "Hat".into(),
                            lands_on: vec!["x".into(), "y".into()],
                        },
                    ],
                ),
                (
                    "TITLE_SHADOW",
                    "title rejected: \"Hat\"\ncollides with KIND NAME \"hat\" (reason: kind-name shadow)\nand with a gate key on tool \"t\" (reason: live gate shadow)\n  that gate's \"Hat\" already lands on \"x\", \"y\"\nchoose a title that is not a kind name and not a gate key.",
                    409,
                    r#"{"title":"Hat","collisions":[{"reason":"kind-name shadow","kindName":"hat"},{"reason":"live gate shadow","toolName":"t","ref":"Hat","landsOn":["x","y"]}]}"#,
                ),
            ),
            (
                InventoryRefusal::title_check_unavailable(
                    "Hat",
                    &[InventoryTitleLintUnread { pool: "General".into(), reason: "gone".into() }],
                ),
                (
                    "TITLE_CHECK_UNAVAILABLE",
                    "title rejected: \"Hat\"\nthe title check could not read General (gone)\na title is not accepted unchecked; repair what could not be read, then try again.",
                    409,
                    r#"{"title":"Hat","unread":[{"pool":"General","reason":"gone"}]}"#,
                ),
            ),
        ];
        for (r, (code, message, status, det)) in rows {
            assert_eq!(
                bytes(&r),
                (
                    code.to_string(),
                    message.to_string(),
                    status,
                    det.to_string()
                )
            );
        }
        let kind_only = InventoryRefusal::title_shadow(
            "x",
            &[InventoryTitleCollision::KindNameShadow {
                kind_name: "x".into(),
            }],
        );
        assert!(kind_only
            .message
            .ends_with("\nchoose a title that is not a kind name."));
        let gate_only = InventoryRefusal::title_shadow(
            "x",
            &[InventoryTitleCollision::LiveGateShadow {
                tool_name: "t".into(),
                reference: "r".into(),
                lands_on: vec![],
            }],
        );
        assert!(gate_only
            .message
            .ends_with("\nchoose a title that is not a gate key."));
        assert_eq!(
            serde_json::to_string(&InventoryRefusal::cap_exceeded("i", "n", 1).code_and_details())
                .unwrap(),
            r#"{"code":"CAP_EXCEEDED","itemId":"i","itemName":"n","cap":1}"#
        );
    }
}
