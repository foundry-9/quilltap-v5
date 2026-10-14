//! Scenario source remapping — P4.D288 (C1-11), porting v4
//! `lib/scenarios/source-remap.ts` (`545e299bb`, scenarios from anywhere) at
//! the `83334b9a5` pin.
//!
//! "Carrying a chat's `scenarioSource` across an import or a new-account
//! restore, where mount-point ids change. A source is only worth keeping while
//! its mount exists: one whose mount did not come across (a chat-only export,
//! say) is dropped to null rather than left dangling, and the in-chat control
//! then falls back to inferring the preset by body." Pure.

use serde_json::Value;

use crate::db::chats::ScenarioSource;

/// v4 `readScenarioSource(value)`: whatever a bundle carried in
/// `scenarioSource` — an object, a JSON string from a raw row, or nothing — as
/// a valid source, or `None`. `null`, an absent key (pass [`Value::Null`]) and
/// `""` are nothing; a string is `JSON.parse`d (unparseable → `None`); the
/// result must pass `ScenarioSourceSchema` (L-4's
/// [`ScenarioSource::parse`]), which strips unknown keys.
pub fn read_scenario_source(value: &Value) -> Option<ScenarioSource> {
    let parsed;
    let candidate = match value {
        Value::Null => return None,
        Value::String(s) if s.is_empty() => return None,
        Value::String(s) => {
            parsed = serde_json::from_str::<Value>(s).ok()?;
            &parsed
        }
        other => other,
    };
    ScenarioSource::parse(candidate, &[]).ok()
}

/// v4 `remapScenarioSource(value, mapMount)`: re-point a source at its
/// mount's new id. `map_mount` answers the new id, or `None` when the mount is
/// not in this instance — then (and for an empty id, v4's falsy check) the
/// source is dropped. The answer is the PARSED source (unknown keys stripped)
/// with the mapped `mountPointId`.
pub fn remap_scenario_source(
    value: &Value,
    map_mount: impl Fn(&str) -> Option<String>,
) -> Option<ScenarioSource> {
    let source = read_scenario_source(value)?;
    let mount_point_id = map_mount(&source.mount_point_id).filter(|id| !id.is_empty())?;
    Some(ScenarioSource {
        mount_point_id,
        ..source
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn nothing_reads_as_none_and_a_string_is_parsed() {
        assert_eq!(read_scenario_source(&Value::Null), None);
        assert_eq!(read_scenario_source(&json!("")), None);
        assert_eq!(read_scenario_source(&json!("{not json")), None);
        let obj = json!({
            "mountPointId": "11111111-1111-4111-8111-111111111111",
            "path": "Scenarios/a.md",
            "bodySha256": SHA,
            "extra": 1,
        });
        let want = read_scenario_source(&obj).expect("valid");
        assert_eq!(
            read_scenario_source(&json!(obj.to_string())),
            Some(want.clone())
        );
        assert_eq!(
            remap_scenario_source(&obj, |_| Some(
                "22222222-2222-4222-8222-222222222222".into()
            ))
            .map(|s| s.mount_point_id),
            Some("22222222-2222-4222-8222-222222222222".to_string())
        );
        assert_eq!(remap_scenario_source(&obj, |_| None), None);
        assert_eq!(remap_scenario_source(&obj, |_| Some(String::new())), None);
    }
}
