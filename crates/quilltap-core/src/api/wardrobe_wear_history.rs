//! The `?action=wear-history` read on every tier's single-item wardrobe GET —
//! v4 `lib/wardrobe/wear-history.ts`'s `buildWearHistoryPayload` behind the
//! four item routes' `'wear-history'` action arms (`3ee3b1342`).
//!
//! The dispatch verb (contract C1 §11, landed by P4.D255's keystone with a
//! refusal body; P4.D256 owns it since) routes by scope to the tier's own arm,
//! which lives beside that tier's item GET so it resolves the item exactly as
//! the GET does. Every arm runs the tier's 404s BEFORE the ledger is read (v4
//! `wear-ledger-routes.test.ts`: an item outside the tier never reaches
//! `findHistory`), then answers `Response::WardrobeWearHistory({ history,
//! wearers, lastWornChat })` (contract C2 §3).

use super::types::{ErrorKind, Response, WardrobeContainerScope};
use crate::db::runtime::Db;

/// v4 `GET …/wardrobe/[itemId]?action=wear-history` → `Response::
/// WardrobeWearHistory({history, wearers, lastWornChat})` (contract C2 §3).
///
/// `container_id` names the character / project / group; it is ignored for
/// General. A container scope without one has no v4 analog (the REST path
/// always carries it) and answers a 400.
pub fn wardrobe_item_wear_history(
    db: &Db,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
) -> Response {
    use WardrobeContainerScope as S;
    match (scope, container_id.filter(|c| !c.is_empty())) {
        (S::General, _) => super::wardrobe::wardrobe_item_wear_history_general(db, item_id),
        (_, None) => Response::error(
            ErrorKind::BadRequest,
            "containerId is required for a character, project or group wardrobe",
        ),
        (S::Character, Some(id)) => {
            super::characters::character_wardrobe_wear_history(db, id, item_id)
        }
        (S::Project, Some(id)) => super::projects::project_wardrobe_wear_history(db, id, item_id),
        (S::Group, Some(id)) => super::groups::group_wardrobe_wear_history(db, id, item_id),
    }
}
