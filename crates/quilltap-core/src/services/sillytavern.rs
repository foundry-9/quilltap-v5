//! SillyTavern character-card import / export — port of the JSON legs of v4
//! `lib/sillytavern/character.ts` (`exportSTCharacter` / `importSTCharacter`).
//!
//! Both are pure transforms over `serde_json::Value`. The PNG legs
//! (`createSTCharacterPNG` / `parseSTCharacterPNG`) embed/read the card in a PNG
//! `tEXt` chunk and ride the quilltap-web multipart/binary route — they are the
//! `export-png` / multipart-import deferrals (the dispatch answers a loud
//! `not_available`; NOT stubbed here).

use serde_json::{json, Map, Value};

/// JS-truthy-string helper: `Some(non-empty &str)`, else `None`. Mirrors the
/// `x || fallback` chains in the ST transform (empty string is falsy in JS).
fn truthy_str(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// v4 `exportSTCharacter(character)` — the internal (overlaid) character → an ST
/// `chara_card_v2` card. Reads only stable fields (identity/description/
/// personality/scenarios/systemPrompts/firstMessage/exampleDialogues/title), so
/// no minted id ever reaches the output.
pub fn export_st_character(character: &Value) -> Value {
    // systemPromptContent = defaultPrompt.content || systemPrompts[0].content || ''
    let system_prompt_content = match character.get("systemPrompts").and_then(Value::as_array) {
        Some(a) if !a.is_empty() => {
            let default_content = a
                .iter()
                .find(|p| p.get("isDefault").and_then(Value::as_bool) == Some(true))
                .and_then(|p| truthy_str(p.get("content")));
            default_content
                .or_else(|| truthy_str(a[0].get("content")))
                .unwrap_or_default()
        }
        _ => String::new(),
    };

    // scenarioContent: 1 scenario → its content (may be absent → the JS ternary
    // yields undefined → the key is omitted); many → `## title\ncontent` joined
    // with a blank line; none → '' (present).
    let scenario_content: Option<Value> = match character.get("scenarios").and_then(Value::as_array)
    {
        Some(a) if !a.is_empty() => {
            if a.len() == 1 {
                a[0].get("content").cloned()
            } else {
                let joined = a
                    .iter()
                    .map(|s| {
                        format!(
                            "## {}\n{}",
                            s.get("title").and_then(Value::as_str).unwrap_or(""),
                            s.get("content").and_then(Value::as_str).unwrap_or(""),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
                Some(Value::String(joined))
            }
        }
        _ => Some(Value::String(String::new())),
    };

    // baseData = character.sillyTavernData (a truthy object) OR the default card.
    let mut data: Map<String, Value> = match character.get("sillyTavernData") {
        Some(Value::Object(o)) => o.clone(),
        _ => {
            let mut m = Map::new();
            m.insert(
                "name".into(),
                character.get("name").cloned().unwrap_or(Value::Null),
            );
            insert_or_omit(&mut m, "description", character.get("description"));
            insert_or_omit(&mut m, "personality", character.get("personality"));
            insert_or_omit_scenario(&mut m, &scenario_content);
            insert_or_omit(&mut m, "first_mes", character.get("firstMessage"));
            m.insert(
                "mes_example".into(),
                Value::String(truthy_str(character.get("exampleDialogues")).unwrap_or_default()),
            );
            m.insert("creator_notes".into(), Value::String(String::new()));
            m.insert("tags".into(), Value::Array(vec![]));
            m.insert("creator".into(), Value::String("Quilltap".into()));
            m.insert("character_version".into(), Value::String("1.0".into()));
            m.insert("extensions".into(), Value::Object(Map::new()));
            m
        }
    };

    // Overrides (v4 `{ ...baseData, name, description, ... }`). An override whose
    // value is `undefined` in JS drops the key from the final JSON — so a missing
    // character field removes that key here.
    if let Some(name) = character.get("name") {
        data.insert("name".into(), name.clone());
    } else {
        data.remove("name");
    }
    insert_or_omit(&mut data, "description", character.get("description"));
    insert_or_omit(&mut data, "personality", character.get("personality"));
    // scenario override.
    match &scenario_content {
        Some(v) => {
            data.insert("scenario".into(), v.clone());
        }
        None => {
            data.remove("scenario");
        }
    }
    insert_or_omit(&mut data, "first_mes", character.get("firstMessage"));
    data.insert(
        "mes_example".into(),
        Value::String(truthy_str(character.get("exampleDialogues")).unwrap_or_default()),
    );
    data.insert("system_prompt".into(), Value::String(system_prompt_content));
    // title: character.title || undefined.
    match truthy_str(character.get("title")) {
        Some(t) => {
            data.insert("title".into(), Value::String(t));
        }
        None => {
            data.remove("title");
        }
    }

    json!({
        "spec": "chara_card_v2",
        "spec_version": "2.0",
        "data": Value::Object(data),
    })
}

/// v4 `importSTCharacter(stData)` — an ST card (V2 wrapper or direct data) → the
/// internal character-create input (`{ name, title, description, personality,
/// scenarios, firstMessage, exampleDialogues, systemPrompts, sillyTavernData }`).
///
/// Faithful to v4 EXCEPT the minted scenario/systemPrompt `id`/`createdAt`/
/// `updatedAt`: v4 mints them (`crypto.randomUUID()` + `now`) before handing the
/// arrays to `create`, but the vault write projects only `title`/`content`
/// (scenarios) and `name`/`content`/`isDefault` (prompts) — the ids/timestamps are
/// irrelevant to the on-disk bytes (the same invariant [`character_create`] relies
/// on). So this transform stays pure and omits them.
pub fn import_st_character(st_data: &Value) -> Value {
    // v4: `const data = 'data' in stData ? stData.data : stData`. Key-presence
    // test (a present-but-null `data` still shadows the wrapper).
    let data = match st_data.as_object() {
        Some(o) if o.contains_key("data") => st_data.get("data").cloned().unwrap_or(Value::Null),
        _ => st_data.clone(),
    };

    // exampleDialogues: mes_example — array → JSON.stringify; truthy string →
    // itself; falsy → ''.
    let example_dialogues = match data.get("mes_example") {
        Some(Value::Array(a)) => serde_json::to_string(a).unwrap_or_default(),
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => String::new(),
    };

    // systemPrompts: one 'Default' prompt when `system_prompt` is a truthy string.
    let system_prompts = match truthy_str(data.get("system_prompt")) {
        Some(content) => Value::Array(vec![json!({
            "name": "Default",
            "content": content,
            "isDefault": true,
        })]),
        None => Value::Array(vec![]),
    };

    // scenarios: one 'Default' scenario when `scenario` is a truthy string.
    let scenarios = match truthy_str(data.get("scenario")) {
        Some(content) => Value::Array(vec![json!({
            "title": "Default",
            "content": content,
        })]),
        None => Value::Array(vec![]),
    };

    let mut out = Map::new();
    out.insert(
        "name".into(),
        data.get("name").cloned().unwrap_or(Value::Null),
    );
    // v4: `title: data.title || null`.
    out.insert(
        "title".into(),
        truthy_str(data.get("title"))
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    insert_or_omit(&mut out, "description", data.get("description"));
    insert_or_omit(&mut out, "personality", data.get("personality"));
    out.insert("scenarios".into(), scenarios);
    insert_or_omit(&mut out, "firstMessage", data.get("first_mes"));
    out.insert("exampleDialogues".into(), Value::String(example_dialogues));
    out.insert("systemPrompts".into(), system_prompts);
    // Store the original card data verbatim for full fidelity (the slim column).
    out.insert("sillyTavernData".into(), data);
    Value::Object(out)
}

/// `{ ...obj, key: character.field }`: set the key to the field's value when the
/// field is present (including a JSON `null`), or drop the key when the field is
/// absent (v4's `undefined` → JSON omit).
fn insert_or_omit(map: &mut Map<String, Value>, key: &str, field: Option<&Value>) {
    match field {
        Some(v) => {
            map.insert(key.to_string(), v.clone());
        }
        None => {
            map.remove(key);
        }
    }
}

/// The default-baseData `scenario` seed (parallels [`insert_or_omit`] but for the
/// already-computed `scenarioContent`).
fn insert_or_omit_scenario(map: &mut Map<String, Value>, scenario: &Option<Value>) {
    match scenario {
        Some(v) => {
            map.insert("scenario".into(), v.clone());
        }
        None => {
            map.remove("scenario");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_default_base_data_shape() {
        let character = json!({
            "name": "Aria",
            "title": "Sky-Captain",
            "description": "A daring sky-captain.",
            "personality": "Bold and curious.",
            "firstMessage": Value::Null,
            "exampleDialogues": Value::Null,
            "sillyTavernData": Value::Null,
            "scenarios": [
                { "title": "Interlude", "content": "A storm looms." },
                { "title": "Prologue", "content": "The airship departs." },
            ],
            "systemPrompts": [
                { "name": "Backup", "isDefault": false, "content": "Alt." },
                { "name": "Explorer", "isDefault": true, "content": "You are Aria." },
            ],
        });
        let card = export_st_character(&character);
        assert_eq!(card["spec"], "chara_card_v2");
        assert_eq!(card["spec_version"], "2.0");
        let d = &card["data"];
        assert_eq!(d["name"], "Aria");
        assert_eq!(d["title"], "Sky-Captain");
        assert_eq!(d["system_prompt"], "You are Aria.");
        assert_eq!(
            d["scenario"],
            "## Interlude\nA storm looms.\n\n## Prologue\nThe airship departs."
        );
        assert_eq!(d["first_mes"], Value::Null);
        assert_eq!(d["mes_example"], "");
        assert_eq!(d["creator"], "Quilltap");
        assert_eq!(d["character_version"], "1.0");
    }

    #[test]
    fn import_card_unwraps_and_maps() {
        let card = json!({
            "spec": "chara_card_v2",
            "data": {
                "name": "Fable",
                "description": "A storyteller.",
                "personality": "Wry.",
                "scenario": "Dusk falls.",
                "first_mes": "Hello.",
                "mes_example": "U: Hi\nF: Hi",
                "system_prompt": "You are Fable.",
                "title": "The Bard",
            },
        });
        let out = import_st_character(&card);
        assert_eq!(out["name"], "Fable");
        assert_eq!(out["title"], "The Bard");
        assert_eq!(out["firstMessage"], "Hello.");
        assert_eq!(out["exampleDialogues"], "U: Hi\nF: Hi");
        assert_eq!(out["scenarios"][0]["title"], "Default");
        assert_eq!(out["scenarios"][0]["content"], "Dusk falls.");
        assert_eq!(out["systemPrompts"][0]["name"], "Default");
        assert_eq!(out["systemPrompts"][0]["isDefault"], true);
        // sillyTavernData is the unwrapped card data verbatim.
        assert_eq!(out["sillyTavernData"]["name"], "Fable");
    }

    #[test]
    fn import_mes_example_array_is_stringified() {
        let direct = json!({
            "name": "Q",
            "mes_example": ["a", "b"],
        });
        let out = import_st_character(&direct);
        assert_eq!(out["exampleDialogues"], "[\"a\",\"b\"]");
        // No system_prompt / scenario → empty arrays.
        assert_eq!(out["systemPrompts"], json!([]));
        assert_eq!(out["scenarios"], json!([]));
    }
}
