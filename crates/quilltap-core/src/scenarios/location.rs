//! Scenario Location — v4 `lib/scenarios/location.ts` (`545e299bb`, pinned at
//! `83334b9a5`): the one answer to "where does this scenario live, and how do
//! I read and write there".
//!
//! A location is one owner's `Scenarios/` folder: a character's vault, a
//! project's official store, a group's official store, or Quilltap General.
//! Routes, the chat-side resolver, the instance-wide lister and the startup
//! heal all address a scenario through this; nobody switches on scope to pick
//! a reader or a writer, and a `ScenarioRef` (`{ mountPointId, path }`) is the
//! one pointer that names a scenario anywhere.

use crate::db::scenarios::{resolve_scenario_path, ScenarioPathResolution, SCENARIOS_FOLDER};
use crate::scenarios::decode_uri::{js_decode_uri_component, UriMalformed};
use crate::vault_overlay::stable_uuid_from_string;

/// v4 `characterScenarioIdForPath(mountPointId, path)` (`location.ts:157-159`):
/// ref → character-scenario id, a pure function of the ref —
/// `stableUuidFromString('scenario:<mountPointId>:<path>')`, byte-identical to
/// the id the vault parser gives the same file.
pub fn character_scenario_id_for_path(mount_point_id: &str, path: &str) -> String {
    stable_uuid_from_string(&format!("scenario:{mount_point_id}:{path}"))
}

/// v4 `normalizeScenarioPath(path)` (`location.ts:192-196`) =
/// `resolveScenarioPath(path, 'Scenarios')`: `decodeURIComponent` FIRST, then
/// trim, then the `Scenarios/<file>.md` rules. The outer `Err` is V8's
/// `URIError` — v4 THROWS on a malformed escape rather than refusing, so every
/// caller propagates it (R-Z); the inner resolution is ok-or-reason.
///
/// This is the decode the item routes' boundary seam
/// ([`crate::db::scenarios::resolve_scenario_path`], no decode) relies on
/// (ruling L-6): v4 decodes an item route's `[scenarioPath]` twice — once for
/// the HTTP encoding the factory undoes, once here — and v5's dispatch never
/// HTTP-encodes, so this single decode equals v4's pair on the logical path.
pub fn normalize_scenario_path(path: &str) -> Result<ScenarioPathResolution, UriMalformed> {
    let decoded = js_decode_uri_component(path)?;
    Ok(resolve_scenario_path(&decoded, SCENARIOS_FOLDER))
}
