//! Tier-2/3 differential: the seven wardrobe tool handlers (W4.1d batch 2; v4
//! `lib/tools/handlers/wardrobe-*-handler.ts`), ported as `quilltap_core::tools::
//! wardrobe_{list,read,create,update,archive,wear,take_off}`.
//!
//! Both sides run the SAME op sequence (`wardrobe-tools.json`) on a fresh copy of
//! the pre-seeded two-database fixture (a caller + recipient character with full
//! vaults, a Quilltap General archetype tier, a chat with participants + a seeded
//! equipped outfit). Each op's Output + the `format*` string are compared against
//! v4's, and the mutated state is read back (both wardrobes, the archetypes, the
//! chat's equipped outfit) and diffed.
//!
//! NORMALIZATION: create mints an item id + createdAt/updatedAt; update/archive
//! mint updatedAt inside the content-addressed `.md`. Every UUID string is remapped
//! to a positional `<id-N>` token (first-appearance order, one map per compared
//! value) and every ISO-8601 timestamp collapsed to `<ts>`, applied identically to
//! both sides — so a minted value on the v4 side maps to the same token as its
//! (differently-minted) Rust counterpart, while the structure (ordering, which
//! seed ids appear where) is diffed exactly. The underlying vault/`chats` table
//! bytes are inherited from the composed differentials
//! (`vault_wardrobe_public_equivalence` / `chats_outfits_tier2_equivalence`).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WT_MAIN=/tmp/qt-wt-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wt-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-wardrobe-tools-fixture.ts
//!   QT_FIXTURE_WT_MAIN=/tmp/qt-wt-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wt-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/wardrobe-tools.ts > /tmp/oracle-wardrobe-tools.ndjson
//! Run:
//!   QT_ORACLE_WT=/tmp/oracle-wardrobe-tools.ndjson \
//!   QT_FIXTURE_WT_MAIN=/tmp/qt-wt-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wt-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_tools_equivalence

use std::collections::HashMap;

use quilltap_core::db::archetype_wardrobe::find_archetypes;
use quilltap_core::db::chats_outfits::ChatOutfitsRepository;
use quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository;
use quilltap_core::db::wardrobe_read::find_by_character_id;
use quilltap_core::db::Writer;
use quilltap_core::tools::{
    wardrobe_archive, wardrobe_create, wardrobe_list, wardrobe_read, wardrobe_take_off,
    wardrobe_update, wardrobe_wear,
};
use quilltap_core::wardrobe_tiers::SharedWardrobeTiers;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "callerCharacterId")]
    caller_character_id: String,
    #[serde(rename = "recipientCharacterId")]
    recipient_character_id: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    name: String,
    tool: String,
    #[serde(default, rename = "characterId")]
    character_id: Option<String>,
    args: Value,
}

/// Recursively remap every UUID (positional `<id-N>`) and collapse every ISO
/// timestamp to `<ts>`, using one shared map per top-level value. Applied to both
/// sides so a differently-minted value maps to the same token.
fn normalize(v: &Value) -> Value {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut counter = 0usize;
    walk(v, &mut map, &mut counter)
}

fn walk(v: &Value, map: &mut HashMap<String, String>, counter: &mut usize) -> Value {
    match v {
        Value::String(s) => Value::String(normalize_text(s, map, counter)),
        Value::Array(a) => Value::Array(a.iter().map(|e| walk(e, map, counter)).collect()),
        Value::Object(o) => {
            let mut m = serde_json::Map::new();
            for (k, val) in o {
                m.insert(k.clone(), walk(val, map, counter));
            }
            Value::Object(m)
        }
        other => other.clone(),
    }
}

/// Replace UUID / ISO-timestamp substrings inside a string (positional tokens for
/// UUIDs, `<ts>` for timestamps). Handles standalone ids AND ids embedded in
/// rendered text.
fn normalize_text(s: &str, map: &mut HashMap<String, String>, counter: &mut usize) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some(len) = iso_ts_len(&bytes[i..]) {
            out.push_str("<ts>");
            i += len;
        } else if uuid_at(&bytes[i..]) {
            let raw = &s[i..i + 36];
            let token = map
                .entry(raw.to_string())
                .or_insert_with(|| {
                    let t = format!("<id-{}>", *counter);
                    *counter += 1;
                    t
                })
                .clone();
            out.push_str(&token);
            i += 36;
        } else {
            // Copy one UTF-8 char.
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// A UUID (`8-4-4-4-12` hex) starting at `b[0]`.
fn uuid_at(b: &[u8]) -> bool {
    if b.len() < 36 {
        return false;
    }
    let dashes = [8, 13, 18, 23];
    for (i, &c) in b[..36].iter().enumerate() {
        if dashes.contains(&i) {
            if c != b'-' {
                return false;
            }
        } else if !is_hex(c) {
            return false;
        }
    }
    // Not part of a longer hex run (avoid clipping).
    if b.len() > 36 && (is_hex(b[36]) || b[36] == b'-') {
        return false;
    }
    true
}

/// The length of an ISO-8601 `YYYY-MM-DDTHH:MM:SS.sssZ` timestamp at `b[0]`, else
/// `None`.
fn iso_ts_len(b: &[u8]) -> Option<usize> {
    const LEN: usize = 24;
    if b.len() < LEN {
        return None;
    }
    let d = |i: usize| b[i].is_ascii_digit();
    let ok = d(0)
        && d(1)
        && d(2)
        && d(3)
        && b[4] == b'-'
        && d(5)
        && d(6)
        && b[7] == b'-'
        && d(8)
        && d(9)
        && b[10] == b'T'
        && d(11)
        && d(12)
        && b[13] == b':'
        && d(14)
        && d(15)
        && b[16] == b':'
        && d(17)
        && d(18)
        && b[19] == b'.'
        && d(20)
        && d(21)
        && d(22)
        && b[23] == b'Z';
    if ok {
        Some(LEN)
    } else {
        None
    }
}

#[test]
fn wardrobe_tools_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WT") else {
        eprintln!("SKIP: set QT_ORACLE_WT to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_WT_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_WT_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_WT_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_WT_MOUNT to the seed mount-index .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/wardrobe-tools.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let (oracle_returns, oracle_readback) = parse_oracle(&oracle_text);

    // Copy the fixtures and open two writers (both connections held together).
    let scratch = std::env::temp_dir().join(format!("qt-wt-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("wt-main.db");
    let work_mount = scratch.join("wt-mount.db");
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    let main = Writer::open_writable(&work_main, &spec.test_pepper_base64).expect("open main");
    let mount = Writer::open_writable(&work_mount, &spec.test_pepper_base64).expect("open mount");

    let mut announce: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (i, op) in spec.ops.iter().enumerate() {
        let character_id = match op.character_id.as_deref() {
            Some("recipient") => &spec.recipient_character_id,
            _ => &spec.caller_character_id,
        };
        let (output, formatted) = run_op(
            &op.tool,
            &main,
            &mount,
            &spec.user_id,
            &spec.chat_id,
            character_id,
            &op.args,
            &mut announce,
        );

        let got = normalize(&json!({
            "name": op.name,
            "tool": op.tool,
            "output": output,
            "formatted": formatted,
        }));
        let want = normalize(&oracle_returns[i]);
        assert_eq!(
            got, want,
            "op[{i}] {} diverged\n  rust:   {got}\n  oracle: {want}",
            op.name
        );
    }

    // Read the mutated state back and diff (the "tables" diff, read-back form).
    let main_c = main.connection();
    let docs = DocMountDocumentsRepository::new(mount.connection());
    let caller_items =
        find_by_character_id(main_c, &docs, &spec.caller_character_id, true).expect("caller items");
    let recipient_items = find_by_character_id(main_c, &docs, &spec.recipient_character_id, true)
        .expect("recipient items");
    let general_items =
        find_archetypes(main_c, &docs, true, &SharedWardrobeTiers::none()).expect("general items");
    let equipped = ChatOutfitsRepository::new(main_c)
        .get_equipped_outfit(&spec.chat_id)
        .expect("equipped");
    let mut ann: Vec<String> = announce.into_iter().collect();
    ann.sort();

    let readback = json!({
        "callerItems": caller_items,
        "recipientItems": recipient_items,
        "generalItems": general_items,
        "equippedOutfit": equipped.unwrap_or(Value::Null),
        "pendingAnnouncements": ann,
    });

    let got = normalize(&readback);
    let want = normalize(&oracle_readback);
    assert_eq!(
        got, want,
        "read-back state diverged\n  rust:   {got}\n  oracle: {want}"
    );

    drop(main);
    drop(mount);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    eprintln!(
        "OK: wardrobe tools matched oracle ({} ops + read-back).",
        spec.ops.len()
    );
}

#[allow(clippy::too_many_arguments)]
fn run_op(
    tool: &str,
    main_w: &Writer,
    mount_w: &Writer,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
    announce: &mut std::collections::HashSet<String>,
) -> (Value, String) {
    let main = main_w.connection();
    let mount = mount_w.connection();
    match tool {
        "wardrobe_list" => {
            let out = wardrobe_list::execute(main, mount, user_id, chat_id, character_id, args);
            (to_value(&out), wardrobe_list::format(&out))
        }
        "wardrobe_read" => {
            let out = wardrobe_read::execute(main, mount, user_id, chat_id, character_id, args);
            (to_value(&out), wardrobe_read::format(&out))
        }
        "wardrobe_create" => {
            let out = wardrobe_create::execute(main, mount, user_id, chat_id, character_id, args);
            (to_value(&out), wardrobe_create::format(&out))
        }
        "wardrobe_update" => {
            let out = wardrobe_update::execute(main, mount, user_id, chat_id, character_id, args);
            (to_value(&out), wardrobe_update::format(&out))
        }
        "wardrobe_archive" => {
            let (out, ids) =
                wardrobe_archive::execute(main, mount, user_id, chat_id, character_id, args);
            announce.extend(ids);
            (to_value(&out), wardrobe_archive::format(&out))
        }
        "wardrobe_wear" => {
            let (out, ids) =
                wardrobe_wear::execute(main, mount, user_id, chat_id, character_id, args);
            announce.extend(ids);
            (to_value(&out), wardrobe_wear::format(&out))
        }
        "wardrobe_take_off" => {
            let (out, ids) =
                wardrobe_take_off::execute(main, mount, user_id, chat_id, character_id, args);
            announce.extend(ids);
            (to_value(&out), wardrobe_take_off::format(&out))
        }
        other => panic!("unknown tool {other}"),
    }
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("serialize output")
}

/// Extract the `returns` array + the `readback` object from the oracle NDJSON.
fn parse_oracle(text: &str) -> (Vec<Value>, Value) {
    let mut returns = None;
    let mut readback = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
        if let Some(r) = v.get("returns") {
            returns = Some(r.as_array().expect("returns array").clone());
        }
        if let Some(r) = v.get("readback") {
            readback = Some(r.clone());
        }
    }
    (
        returns.expect("oracle missing returns"),
        readback.expect("oracle missing readback"),
    )
}
