//! SillyTavern character-card import / export — port of v4
//! `lib/sillytavern/character.ts`.
//!
//! The JSON legs (`exportSTCharacter` / `importSTCharacter`) are pure transforms
//! over `serde_json::Value`. The PNG legs (`createSTCharacterPNG` /
//! `parseSTCharacterPNG` / the placeholder generator) embed/read the card in a
//! PNG `tEXt` chunk and are consumed by the quilltap-web multipart/binary routes
//! (`?action=export&format=png` and `?action=import`). v4 hand-rolls all of the
//! PNG machinery (CRC32, chunk framing, a solid-colour placeholder) with NO
//! image library — this port matches, keeping the whole codec pure and
//! dependency-free. The only declared seam is the placeholder's DEFLATE bytes:
//! v4 uses Node's `zlib.deflateSync`, this uses stored (uncompressed) DEFLATE
//! blocks — both valid zlib that decode to identical pixels (the differential
//! diffs the placeholder DECODED, byte-exact on the real-avatar leg).

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

// ===========================================================================
// PNG legs (v4 `parseSTCharacterPNG` / `createSTCharacterPNG` /
// `generatePlaceholderPNG` / `calculateCRC32` / `createPNGChunk`) — hand-rolled
// chunk arithmetic, no image library (matching v4).
// ===========================================================================

/// The 8-byte PNG signature (`[137,80,78,71,13,10,26,10]`).
const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// v4 `calculateCRC32` — the PNG-spec CRC32 (reflected, poly `0xedb88320`),
/// computed bytewise so it stays byte-identical to the JS bit-twiddling loop.
fn calculate_crc32(buffer: &[u8]) -> u32 {
    let mut crc: u32 = 0xffff_ffff;
    for &byte in buffer {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xffff_ffff
}

/// v4 `createPNGChunk` — `u32 len | type | data | u32 CRC32(type+data)`.
fn create_png_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(chunk_type);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&calculate_crc32(&crc_input).to_be_bytes());
    out
}

/// Adler-32 (the zlib trailer checksum) over `data`.
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

/// Wrap `raw` in a valid zlib stream using stored (uncompressed) DEFLATE blocks.
/// The declared placeholder seam: v4 emits `zlib.deflateSync(raw)` (compressed);
/// this emits stored blocks. Both are valid zlib and inflate to the identical
/// `raw` bytes, so the placeholder PNG decodes to identical pixels — only the
/// compressed IDAT bytes differ (never asserted byte-exact for the placeholder).
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    // zlib header: CMF=0x78 (deflate, 32 KiB window), FLG=0x01 (no preset dict,
    // fastest level) — `0x7801` is divisible by 31, the required check.
    let mut out = Vec::with_capacity(raw.len() + (raw.len() / 65_535 + 1) * 5 + 6);
    out.push(0x78);
    out.push(0x01);
    if raw.is_empty() {
        out.push(0x01); // BFINAL=1, BTYPE=00 (stored)
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(!0u16).to_le_bytes());
    } else {
        let mut i = 0;
        while i < raw.len() {
            let chunk = (raw.len() - i).min(65_535);
            let is_last = i + chunk >= raw.len();
            out.push(if is_last { 0x01 } else { 0x00 });
            let len = chunk as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(&raw[i..i + chunk]);
            i += chunk;
        }
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

/// v4 `generatePlaceholderPNG` — a 256×256 solid-colour RGB PNG whose colour is a
/// deterministic hash of the character name (the placeholder does NOT draw the
/// initial, despite v4's comment). Used when no avatar is supplied.
fn generate_placeholder_png(character_name: &str) -> Vec<u8> {
    // v4's name-hash palette.
    const COLORS: [[u8; 3]; 8] = [
        [74, 144, 226], // Blue
        [80, 200, 120], // Green
        [255, 149, 0],  // Orange
        [175, 82, 222], // Purple
        [255, 59, 48],  // Red
        [90, 200, 250], // Teal
        [255, 204, 0],  // Yellow
        [88, 86, 214],  // Indigo
    ];
    // `hash = ((hash << 5) - hash) + charCodeAt(i); hash = hash & hash` — 32-bit
    // signed integer arithmetic over UTF-16 code units.
    let mut hash: i32 = 0;
    for unit in character_name.encode_utf16() {
        hash = (hash << 5).wrapping_sub(hash).wrapping_add(unit as i32);
    }
    // `Math.abs(hash) % colors.length` (unsigned_abs handles i32::MIN like JS).
    let color_index = (hash.unsigned_abs() as usize) % COLORS.len();
    let [r, g, b] = COLORS[color_index];

    const WIDTH: usize = 256;
    const HEIGHT: usize = 256;

    // IHDR: 8-bit depth, colour type 2 (RGB), no compression/filter/interlace.
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(WIDTH as u32).to_be_bytes());
    ihdr.extend_from_slice(&(HEIGHT as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let ihdr_chunk = create_png_chunk(b"IHDR", &ihdr);

    // Raw scanlines: a leading filter byte (0 = none) then RGB per pixel.
    let mut raw = vec![0u8; HEIGHT * (1 + WIDTH * 3)];
    for y in 0..HEIGHT {
        let row_offset = y * (1 + WIDTH * 3);
        raw[row_offset] = 0;
        for x in 0..WIDTH {
            let p = row_offset + 1 + x * 3;
            raw[p] = r;
            raw[p + 1] = g;
            raw[p + 2] = b;
        }
    }
    let idat_chunk = create_png_chunk(b"IDAT", &zlib_stored(&raw));
    let iend_chunk = create_png_chunk(b"IEND", &[]);

    let mut out = Vec::with_capacity(8 + ihdr_chunk.len() + idat_chunk.len() + iend_chunk.len());
    out.extend_from_slice(&PNG_SIGNATURE);
    out.extend_from_slice(&ihdr_chunk);
    out.extend_from_slice(&idat_chunk);
    out.extend_from_slice(&iend_chunk);
    out
}

/// v4 `createSTCharacterPNG` — `exportSTCharacter` → `JSON.stringify` → embed as a
/// `chara`-keyword `tEXt` chunk inserted immediately after IHDR of the avatar PNG
/// (or a generated placeholder when `avatar` is `None`).
///
/// Byte-exact with v4 on the real-avatar leg (pure chunk math + the same
/// insertion-ordered `JSON.stringify`); the placeholder leg differs only in the
/// declared DEFLATE seam (see [`zlib_stored`]).
pub fn create_st_character_png(character: &Value, avatar: Option<&[u8]>) -> Vec<u8> {
    let st_card = export_st_character(character);
    // `JSON.stringify(stCard)` — compact, insertion-ordered (preserve_order).
    let json_data = serde_json::to_string(&st_card).unwrap_or_default();

    let placeholder;
    let avatar_buffer: &[u8] = match avatar {
        Some(bytes) => bytes,
        None => {
            let name = character
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("Character");
            placeholder = generate_placeholder_png(name);
            &placeholder
        }
    };

    // tEXt chunk data: `chara` + NUL + raw UTF-8 JSON.
    let mut chunk_data = Vec::with_capacity(6 + json_data.len());
    chunk_data.extend_from_slice(b"chara");
    chunk_data.push(0);
    chunk_data.extend_from_slice(json_data.as_bytes());
    let text_chunk = create_png_chunk(b"tEXt", &chunk_data);

    // Insert after the PNG signature + IHDR chunk. v4 reads the IHDR length from
    // the avatar and advances `8 + 12 + ihdrLength`. A truncated avatar (< 12
    // bytes) would panic v4's `readUInt32BE`; we guard and clamp instead.
    let insert_offset = if avatar_buffer.len() >= 12 {
        let ihdr_length = u32::from_be_bytes([
            avatar_buffer[8],
            avatar_buffer[9],
            avatar_buffer[10],
            avatar_buffer[11],
        ]) as usize;
        (8 + 12 + ihdr_length).min(avatar_buffer.len())
    } else {
        avatar_buffer.len()
    };

    let mut out = Vec::with_capacity(avatar_buffer.len() + text_chunk.len());
    out.extend_from_slice(&avatar_buffer[..insert_offset]);
    out.extend_from_slice(&text_chunk);
    out.extend_from_slice(&avatar_buffer[insert_offset..]);
    out
}

/// v4 `parseSTCharacterPNG` — walk the PNG chunks for a `chara`/`ccv2` `tEXt`
/// chunk and return its embedded card data (`.data` of a `chara_card_v2` wrapper,
/// or a bare object with a `name`). `None` on any failure (bad signature,
/// truncated length, malformed JSON) — v4's `catch → return null`.
///
/// Faithful to v4 including the null-terminator `continue` that does NOT advance
/// the offset: a `tEXt` chunk with no NUL byte loops forever in v4 too. Valid PNG
/// text chunks always carry a keyword/NUL separator, so this is unreachable in
/// practice; documented, not silently altered.
pub fn parse_st_character_png(buffer: &[u8]) -> Option<Value> {
    if buffer.len() < 8 || buffer[..8] != PNG_SIGNATURE {
        return None;
    }
    let mut offset = 8usize;
    while offset < buffer.len() {
        // readUInt32BE(offset): needs 4 bytes, else v4 throws → null.
        if offset + 4 > buffer.len() {
            return None;
        }
        let length = u32::from_be_bytes([
            buffer[offset],
            buffer[offset + 1],
            buffer[offset + 2],
            buffer[offset + 3],
        ]) as usize;
        // type = ascii(offset+4, offset+8) — Node clamps the slice to len.
        let type_end = (offset + 8).min(buffer.len());
        let chunk_type = &buffer[(offset + 4).min(buffer.len())..type_end];

        if chunk_type == b"tEXt" {
            // chunkData = slice(offset+8, offset+8+length) — clamped to len.
            let data_start = (offset + 8).min(buffer.len());
            let data_end = (offset + 8 + length).min(buffer.len());
            let chunk_data = &buffer[data_start..data_end];
            match chunk_data.iter().position(|&b| b == 0) {
                None => continue, // v4: `continue` WITHOUT advancing offset.
                Some(null_index) => {
                    let keyword = String::from_utf8_lossy(&chunk_data[..null_index]);
                    if keyword == "chara" || keyword == "ccv2" {
                        let json_bytes = &chunk_data[null_index + 1..];
                        let json_str = String::from_utf8_lossy(json_bytes);
                        // JSON.parse throw → caught → null for the whole function.
                        let parsed: Value = serde_json::from_str(&json_str).ok()?;
                        if parsed.get("spec").and_then(Value::as_str) == Some("chara_card_v2")
                            && parsed.get("data").map(is_truthy).unwrap_or(false)
                        {
                            return parsed.get("data").cloned();
                        } else if parsed.get("name").map(is_truthy).unwrap_or(false) {
                            return Some(parsed);
                        }
                    }
                }
            }
        }
        offset += 12 + length;
    }
    None
}

/// JS truthiness for a `serde_json::Value` (the `parsed.data` / `parsed.name`
/// guards): `null`/`false`/`0`/`""`/absent are falsy, everything else truthy.
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
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
    fn png_round_trips_card_through_text_chunk() {
        // A minimal valid avatar (1×1 red PNG, standard 13-byte IHDR).
        let avatar: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92,
            0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        let character = json!({
            "name": "Aria",
            "description": "A sky-captain.",
            "personality": "Bold.",
            "firstMessage": "Ahoy.",
            "exampleDialogues": "",
            "scenarios": [{ "title": "Default", "content": "A storm looms." }],
            "systemPrompts": [{ "name": "Default", "isDefault": true, "content": "You are Aria." }],
        });
        let png = create_st_character_png(&character, Some(avatar));
        // tEXt inserted after IHDR: the avatar bytes are a prefix up to offset 33.
        assert_eq!(&png[..33], &avatar[..33]);
        let parsed = parse_st_character_png(&png).expect("round-trips");
        assert_eq!(parsed["name"], "Aria");
        assert_eq!(parsed["scenario"], "A storm looms.");
        assert_eq!(parsed["system_prompt"], "You are Aria.");
    }

    #[test]
    fn placeholder_is_a_valid_256_rgb_png_with_card() {
        let character = json!({ "name": "Bartholomew", "description": "A butler." });
        let png = create_st_character_png(&character, None);
        assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        // IHDR width/height = 256, colour type 2.
        assert_eq!(
            u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
            256
        );
        assert_eq!(
            u32::from_be_bytes([png[20], png[21], png[22], png[23]]),
            256
        );
        assert_eq!(png[25], 2); // colour type RGB
                                // The embedded card still round-trips.
        let parsed = parse_st_character_png(&png).expect("placeholder carries the card");
        assert_eq!(parsed["name"], "Bartholomew");
    }

    #[test]
    fn parse_rejects_non_png_and_bad_json() {
        assert_eq!(parse_st_character_png(b"not a png"), None);
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
