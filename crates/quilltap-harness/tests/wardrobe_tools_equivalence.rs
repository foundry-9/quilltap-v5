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
//!   QT_WT_TOOLS_CORPUS=1 QT_FIXTURE_WT_MAIN=/tmp/qt-wt-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wt-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-wardrobe-tools-fixture.ts
//!   QT_FIXTURE_WT_MAIN=/tmp/qt-wt-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wt-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/wardrobe-tools.ts > /tmp/oracle-wardrobe-tools.ndjson
//! Run:
//!   QT_ORACLE_WT=/tmp/oracle-wardrobe-tools.ndjson \
//!   QT_FIXTURE_WT_MAIN=/tmp/qt-wt-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wt-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_tools_equivalence
//!
//! P4.D262 (v4 `3ee3b1342` + `b3f937076`): the formatters run at the spec's
//! pinned `nowMs` (`format_at`, v4's second parameter); the wear ledger and one
//! item's picture pointer are PLANTED before the ops (each side through its
//! real repository — v4's `incrementWears` / `wardrobe.update`); the read-back
//! gains every ledger row (minted item ids keyed by title); and a third oracle
//! line carries the switch-ON `pictureScenario` — the same composition the
//! executor runs (the handler, then `maybe_queue_wardrobe_tool_image` once the
//! write commits, then the format) — and the queued `background_jobs` rows
//! (type / status / maxAttempts / payload, compared parsed). v4's job host is
//! held off in the oracle (no runner on either side), so every job is PENDING.
//!
//! P4.D280 (v4 `4a38222e6`): every op runs through the PRODUCTION table —
//! `wardrobe_tool_table::run_wardrobe_tool_handler` (the handler, as inside the
//! executor's write) then `finish_wardrobe_tool` (the outfit-change effects for
//! the changed character over the ONE announcement set, the picture, the
//! `… completed` INFO) — with the turn's set handed to all seven tools, as v4's
//! executor hands it (the oracle passes it to all seven too). The fixture is
//! built with `QT_WT_TOOLS_CORPUS=1` (a FILED item, an archived General item, a
//! vault-less participant). Every op ALSO compares its log lines: the seven
//! handlers' own lines (`context` = `wardrobe-<x>-handler`) whole — level,
//! message, every field in v4's order, the `…Json` suffix stripped and v4's
//! `message` key read as v5's `errorMessage` (a tracing field named `message`
//! replaces the file record's envelope text — a recorded key rename) — and the
//! keystone's tagged lines (`[WearablePool]`, `[ItemMutations]`,
//! `[applyDisplacement]`, `[WardrobeLocation]`) by TAG only (R-H). Every
//! diverged op is reported before the test fails.

use std::collections::HashMap;

use quilltap_core::db::archetype_wardrobe::read_general_wardrobe;
use quilltap_core::db::chat_settings::{update_for_user, SettingsColVal};
use quilltap_core::db::chat_wardrobe_history::{
    ChatWardrobeHistoryRepository, WardrobeHistoryIntent, WardrobeHistoryProvenance,
    WardrobeHistorySource,
};
use quilltap_core::db::chats_outfits::ChatOutfitsRepository;
use quilltap_core::db::doc_mount_documents::DocMountDocumentsRepository;
use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::image_profiles::{
    CreateOptions as IpCreateOptions, ImageProfilesRepository, IpCreate,
};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::vault_wardrobe_public::{update_vault_wardrobe_item, WardrobePatch};
use quilltap_core::db::wardrobe_read::find_by_character_id;
use quilltap_core::db::wardrobe_wear_stats::{WardrobeWearIncrement, WardrobeWearStatsRepository};
use quilltap_core::db::Writer;
use quilltap_core::services::outfit_change_effects::{
    new_wardrobe_turn_containers, PendingWardrobeAnnouncements, PendingWardrobeHistory,
};
use quilltap_core::test_support::global_capture;
use quilltap_core::tools::wardrobe_list;
use quilltap_core::tools::wardrobe_tool_table::{
    finish_wardrobe_tool, run_wardrobe_tool_handler, WardrobeToolContext, WardrobeToolOutput,
};
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
    #[serde(rename = "nowMs")]
    now_ms: String,
    #[serde(rename = "departedCharacterId")]
    departed_character_id: String,
    #[serde(rename = "wearPlant")]
    wear_plant: Vec<WearPlant>,
    #[serde(rename = "picturePlant")]
    picture_plant: PicturePlant,
    #[serde(rename = "pictureScenario")]
    picture_scenario: PictureScenario,
    /// [P4.D316] the planted timeline + the two tools' ops.
    #[serde(rename = "historyScenario")]
    history_scenario: Value,
}

#[derive(Deserialize)]
struct WearPlant {
    #[serde(rename = "itemId")]
    item_id: String,
    wearer: Option<String>,
    #[serde(rename = "chatId")]
    chat_id: Option<String>,
    at: String,
}

#[derive(Deserialize)]
struct PicturePlant {
    #[serde(rename = "itemId")]
    item_id: String,
    #[serde(rename = "imageFileId")]
    image_file_id: String,
}

#[derive(Deserialize)]
struct PictureScenario {
    #[serde(rename = "imageProfile")]
    image_profile: Value,
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
    // Every op's lines are captured, and the list handler's INFO is pinned in
    // a sibling test over the same callsites: the global rig must own the
    // subscriber first (the `test_support` module doc's `Interest` race).
    global_capture::install();
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
    let scratch = tempfile::Builder::new()
        .prefix("qt-wt-harness-")
        .tempdir()
        .expect("scratch dir");
    let work_main = scratch.path().join("wt-main.db");
    let work_mount = scratch.path().join("wt-mount.db");
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    let main = Writer::open_writable(&work_main, &spec.test_pepper_base64).expect("open main");
    let mount = Writer::open_writable(&work_mount, &spec.test_pepper_base64).expect("open mount");
    // [P4.D316] P4.D315's one v5-side plant, on this per-case copy (R-S).
    quilltap_core::test_support::ensure_chat_wardrobe_history_on(main.connection());
    let now = quilltap_core::clock::iso_to_ms(&spec.now_ms).expect("nowMs") as f64;

    // [P4.D262] The ledger + picture plants — the oracle's rows, through the
    // repositories.
    {
        let wearer_id = |w: Option<&str>| -> Option<String> {
            match w {
                Some("caller") => Some(spec.caller_character_id.clone()),
                Some("recipient") => Some(spec.recipient_character_id.clone()),
                Some("departed") => Some(spec.departed_character_id.clone()),
                _ => None,
            }
        };
        let repo = WardrobeWearStatsRepository::new(main.connection());
        for w in &spec.wear_plant {
            repo.increment_wears(&[WardrobeWearIncrement {
                item_id: w.item_id.clone(),
                wearer_character_id: wearer_id(w.wearer.as_deref()),
                chat_id: w.chat_id.clone(),
                at: w.at.clone(),
            }])
            .expect("plant a wear");
        }
        let links = DocMountFileLinksRepository::new(mount.connection());
        let docs = DocMountDocumentsRepository::new(mount.connection());
        update_vault_wardrobe_item(
            main.connection(),
            &links,
            &docs,
            &spec.picture_plant.item_id,
            &WardrobePatch {
                image_file_id: Some(Some(spec.picture_plant.image_file_id.clone())),
                ..WardrobePatch::default()
            },
            Some(&spec.caller_character_id),
        )
        .expect("plant the picture pointer")
        .expect("the planted item exists");
    }

    // The executor's picture call needs the `Db` (an async enqueue).
    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // The turn's ONE pair of containers, handed to every tool (v4's executor
    // hands its `WardrobeToolContext` to every table entry).
    let turn = new_wardrobe_turn_containers();
    let mut failed: Vec<String> = Vec::new();

    for (i, op) in spec.ops.iter().enumerate() {
        let character_id = match op.character_id.as_deref() {
            Some("recipient") => &spec.recipient_character_id,
            _ => &spec.caller_character_id,
        };
        let (output, lines) = global_capture::capture(|| {
            run_tool(
                &rt,
                &db,
                &op.tool,
                &main,
                &mount,
                &spec.user_id,
                &spec.chat_id,
                character_id,
                &op.args,
                Some(&turn),
            )
        });
        let formatted = output.format_at(now);

        let got = normalize(&json!({
            "name": op.name,
            "tool": op.tool,
            "output": output.to_value(),
            "formatted": formatted,
        }));
        let mut want_row = oracle_returns[i].clone();
        let want_logs = want_row
            .as_object_mut()
            .and_then(|o| o.remove("logs"))
            .expect("the oracle predates P4.D280 — no per-op `logs`; regenerate it");
        let want = normalize(&want_row);
        if got != want {
            eprintln!(
                "op[{i}] {} diverged\n  rust:   {got}\n  oracle: {want}",
                op.name
            );
            failed.push(op.name.clone());
        }
        if let Some(diff) = compare_logs(&lines, &want_logs) {
            eprintln!("op[{i}] {} LOGS diverged\n{diff}", op.name);
            failed.push(format!("{}:logs", op.name));
        }

        // The reportWhenEmpty tripwire (P4.D87): a blank hair slot must vanish
        // from the per-slot STATE dump — never a "hair:" row (an "(empty)"
        // there reads as baldness). The byte-diff above proves v4-parity; this
        // catches BOTH sides agreeing on a leak (a corpus-blindness guard,
        // asserted on the two ops that leave the hair slot empty — their
        // effect lines legitimately name the slot; the dump must not).
        if op.name == "takeoff_hairdo" || op.name == "clear_hair_slot" {
            let dump = formatted
                .split("Current outfit:")
                .nth(1)
                .unwrap_or_default()
                .split("Summary:")
                .next()
                .unwrap_or_default();
            assert!(
                !dump.contains("hair:"),
                "op[{i}] {} leaked a hair row into the blank-slot dump:\n{formatted}",
                op.name
            );
        }
    }

    // Read the mutated state back and diff (the "tables" diff, read-back form).
    let main_c = main.connection();
    let docs = DocMountDocumentsRepository::new(mount.connection());
    let caller_items =
        find_by_character_id(main_c, &docs, &spec.caller_character_id, true).expect("caller items");
    let recipient_items = find_by_character_id(main_c, &docs, &spec.recipient_character_id, true)
        .expect("recipient items");
    // v4's readback is `readSharedWardrobe(generalMountPointId, true)`.
    let general_items = read_general_wardrobe(main_c, &docs, true).expect("general items");
    let equipped = ChatOutfitsRepository::new(main_c).get_equipped_outfit(&spec.chat_id);
    let mut ann: Vec<String> = turn.0.lock().unwrap().iter().cloned().collect();
    ann.sort();
    let mut pending_history: Vec<(String, Value)> = turn
        .1
        .lock()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap()))
        .collect();
    pending_history.sort_by(|a, b| a.0.cmp(&b.0));

    let wear = wear_readback(main_c, &spec, &caller_items, &recipient_items);
    let readback = json!({
        "callerItems": caller_items,
        "recipientItems": recipient_items,
        "generalItems": general_items,
        "equippedOutfit": equipped.unwrap_or(Value::Null),
        "pendingAnnouncements": ann,
        "wardrobeWear": wear,
        "pendingHistory": pending_history,
        "history": dump_history(main_c),
    });

    let got = normalize(&readback);
    let want = normalize(&oracle_readback);
    if got != want {
        eprintln!("read-back state diverged\n  rust:   {got}\n  oracle: {want}");
        failed.push("readback".to_string());
    }

    // ── [P4.D316] the history scenario: a planted timeline, then the tools ──
    {
        // v4's repository creates `background_jobs` lazily on first touch (the
        // outside-turn rollback's announcement); v5 provisions it at boot, so
        // this un-booted copy gets it from the D23 dump first.
        let fresh: Value = serde_json::from_str(include_str!(
            "../../quilltap-core/src/services/provisioning/fresh_schema.json"
        ))
        .unwrap();
        let jobs_ddl = fresh["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|s| s.starts_with("CREATE TABLE \"background_jobs\" ("))
            .unwrap();
        main_c
            .execute_batch(&jobs_ddl.replacen("CREATE TABLE", "CREATE TABLE IF NOT EXISTS", 1))
            .unwrap();
        let red_hat = find_by_character_id(main_c, &docs, &spec.caller_character_id, true)
            .expect("caller items")
            .into_iter()
            .find(|i| i["title"] == "Red Hat")
            .expect("the main ops minted a Red Hat")["id"]
            .as_str()
            .unwrap()
            .to_string();
        let want_history = parse_oracle_history(&oracle_text);
        let oracle_red_hat = oracle_readback["callerItems"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["title"] == "Red Hat")
            .expect("v4's Red Hat")["id"]
            .as_str()
            .unwrap()
            .to_string();
        let history_repo = ChatWardrobeHistoryRepository::new(main_c);
        let mut got_history: Vec<(Value, Vec<String>)> = Vec::new();
        for step in spec.history_scenario["steps"].as_array().unwrap() {
            match step["step"].as_str().unwrap() {
                kind @ ("plant" | "dress") => {
                    let mut slots = json!({ "top": [], "bottom": [], "footwear": [], "accessories": [], "hair": [] });
                    for (k, v) in step["slots"].as_object().unwrap() {
                        slots[k] = v.clone();
                    }
                    if let Some(slot) = step["redHatIn"].as_str() {
                        slots[slot].as_array_mut().unwrap().push(json!(red_hat));
                    }
                    ChatOutfitsRepository::new(main_c)
                        .set_equipped_outfit(&spec.chat_id, &spec.caller_character_id, &slots)
                        .expect("plant slots");
                    if kind == "plant" {
                        let p = &step["provenance"];
                        let source: WardrobeHistorySource =
                            serde_json::from_value(p["source"].clone()).unwrap();
                        let mut prov = WardrobeHistoryProvenance::new(source);
                        if let Some(a) = p["actorCharacterId"].as_str() {
                            prov = prov.with_actor(a);
                        }
                        if let Some(t) = p["toolName"].as_str() {
                            prov = prov.with_tool_name(t);
                        }
                        history_repo.settle(
                            &spec.chat_id,
                            &spec.caller_character_id,
                            &WardrobeHistoryIntent::Append { provenance: prov },
                        );
                    }
                }
                "deleteEntry" => {
                    main_c
                        .execute(
                            r#"DELETE FROM chat_wardrobe_history WHERE "chatId" = ?1 AND "characterId" = ?2 AND "entryIndex" = ?3"#,
                            rusqlite::params![
                                spec.chat_id,
                                spec.caller_character_id,
                                step["index"].as_i64().unwrap()
                            ],
                        )
                        .expect("drop a history entry");
                }
                "cursor" => {
                    history_repo.settle(
                        &spec.chat_id,
                        &spec.caller_character_id,
                        &WardrobeHistoryIntent::Cursor {
                            index: step["index"].as_i64().unwrap(),
                        },
                    );
                }
                _ => {
                    let character_id = if step["characterId"] == "recipient" {
                        &spec.recipient_character_id
                    } else {
                        &spec.caller_character_id
                    };
                    let tool = step["tool"].as_str().unwrap();
                    let turn = new_wardrobe_turn_containers();
                    let outside = step["outside"] == true;
                    let (output, lines) = global_capture::capture(|| {
                        run_tool(
                            &rt,
                            &db,
                            tool,
                            &main,
                            &mount,
                            &spec.user_id,
                            &spec.chat_id,
                            character_id,
                            &step["args"],
                            (!outside).then_some(&turn),
                        )
                    });
                    let mut set: Vec<String> = turn.0.lock().unwrap().iter().cloned().collect();
                    set.sort();
                    let mut map: Vec<(String, Value)> = turn
                        .1
                        .lock()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap()))
                        .collect();
                    map.sort_by(|a, b| a.0.cmp(&b.0));
                    let equipped = ChatOutfitsRepository::new(main_c)
                        .get_equipped_outfit_for_character(&spec.chat_id, &spec.caller_character_id)
                        .unwrap_or(Value::Null);
                    got_history.push((
                        json!({
                            "name": step["name"],
                            "tool": tool,
                            "output": output.to_value(),
                            "formatted": output.format_at(now),
                            "set": set,
                            "map": map,
                            "historyRows": dump_history(main_c),
                            "equipped": equipped,
                        }),
                        lines,
                    ));
                }
            }
        }
        assert_eq!(got_history.len(), want_history.len(), "history op count");
        for ((got, lines), want) in got_history.iter().zip(&want_history) {
            let mut want = want.clone();
            let want_logs = want.as_object_mut().unwrap().shift_remove("logs").unwrap();
            let name = want["name"].as_str().unwrap().to_string();
            // A row whose slots carry the main ops' MINTED Red Hat id hashes
            // a different id on each side by construction: its `slotsHash`
            // is masked (the hash itself is pinned over fixed ids by the
            // recording families).
            let mask = |v: &Value, minted: &str| -> Value {
                let mut v = v.clone();
                if let Some(rows) = v["historyRows"].as_array_mut() {
                    for r in rows {
                        if r["slots"].as_str().is_some_and(|s| s.contains(minted)) {
                            r["slotsHash"] = json!("<minted>");
                        }
                    }
                }
                v
            };
            let (g, w) = (
                normalize(&mask(got, &red_hat)),
                normalize(&mask(&want, &oracle_red_hat)),
            );
            if serde_json::to_string(&g).unwrap() != serde_json::to_string(&w).unwrap() {
                eprintln!("history op {name} diverged\n  rust:   {g}\n  oracle: {w}");
                failed.push(format!("history:{name}"));
            }
            if let Some(diff) = compare_logs(lines, &want_logs) {
                eprintln!("history op {name} LOGS diverged\n{diff}");
                failed.push(format!("history:{name}:logs"));
            }
        }
        // The scenario reached each arm it was written for.
        let all = serde_json::to_string(&want_history).unwrap();
        for marker in [
            "Now wearing:",
            "gap_before_window",
            "Wearing now: 1.",
            "is already wearing outfit",
            "There is no outfit 9",
            "No outfit history yet",
            "is already in the first outfit",
            "the operator, as",
            "Rollback left a stale item off",
            "Cannot read properties of undefined (reading 'slots')",
        ] {
            assert!(
                all.contains(marker),
                "the history scenario never reached {marker:?}"
            );
        }
    }

    // ── [P4.D262] the picture scenario: the operator switch ON ──
    let fresh: Value = serde_json::from_str(include_str!(
        "../../quilltap-core/src/services/provisioning/fresh_schema.json"
    ))
    .unwrap();
    for table in ["chat_settings", "image_profiles", "background_jobs"] {
        let head = format!("CREATE TABLE \"{table}\" (");
        let ddl = fresh["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|s| s.starts_with(&head))
            .unwrap();
        main_c
            .execute_batch(&ddl.replacen("CREATE TABLE", "CREATE TABLE IF NOT EXISTS", 1))
            .unwrap();
    }
    update_for_user(
        main_c,
        &spec.user_id,
        &[(
            "wardrobeImageSettings",
            SettingsColVal::Text(
                json!({"imageProfileId": null, "generateFromTools": true}).to_string(),
            ),
        )],
        "2026-02-01T00:00:00.000Z",
    )
    .expect("switch ON");
    {
        let ip = &spec.picture_scenario.image_profile;
        let s = |k: &str| ip[k].as_str().map(str::to_string);
        ImageProfilesRepository::new(main_c)
            .create(
                &IpCreate {
                    user_id: spec.user_id.clone(),
                    name: s("name").unwrap(),
                    provider: s("provider").unwrap(),
                    api_key_id: s("apiKeyId"),
                    base_url: s("baseUrl"),
                    model_name: s("modelName").unwrap(),
                    parameters: json!({}),
                    is_default: true,
                    is_dangerous_compatible: false,
                    tags: Vec::new(),
                },
                &IpCreateOptions {
                    id: s("id").unwrap(),
                    created_at: "2026-02-01T00:00:00.000Z".to_string(),
                    updated_at: "2026-02-01T00:00:00.000Z".to_string(),
                },
            )
            .expect("plant the image profile");
    }
    let mut pictures: Vec<Value> = Vec::new();
    let mut picture_logs: Vec<Vec<String>> = Vec::new();
    for op in &spec.picture_scenario.ops {
        let character_id = match op.character_id.as_deref() {
            Some("recipient") => &spec.recipient_character_id,
            _ => &spec.caller_character_id,
        };
        let (output, lines) = global_capture::capture(|| {
            run_tool(
                &rt,
                &db,
                &op.tool,
                &main,
                &mount,
                &spec.user_id,
                &spec.chat_id,
                character_id,
                &op.args,
                Some(&turn),
            )
        });
        pictures.push(json!({
            "name": op.name,
            "tool": op.tool,
            "output": output.to_value(),
            "formatted": output.format_at(now),
        }));
        picture_logs.push(lines);
    }
    let jobs: Vec<Value> = {
        let mut st = main_c
            .prepare(
                "SELECT \"type\", \"status\", \"maxAttempts\", \"payload\" FROM \"background_jobs\" ORDER BY rowid",
            )
            .unwrap();
        st.query_map([], |r| {
            Ok(json!({
                "type": r.get::<_, String>(0)?,
                "status": r.get::<_, String>(1)?,
                "maxAttempts": r.get::<_, f64>(2)?,
                "payload": serde_json::from_str::<Value>(&r.get::<_, String>(3)?).unwrap(),
            }))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    };
    let (want_pictures, want_jobs) = parse_oracle_pictures(&oracle_text);
    assert_eq!(pictures.len(), want_pictures.len(), "picture op count");
    for (i, (got, want)) in pictures.iter().zip(&want_pictures).enumerate() {
        let mut want = want.clone();
        let want_logs = want
            .as_object_mut()
            .and_then(|o| o.remove("logs"))
            .expect("the oracle predates P4.D280 — no per-op `logs`; regenerate it");
        let (got, want) = (normalize(got), normalize(&want));
        let name = got["name"].as_str().unwrap_or_default().to_string();
        if got != want {
            eprintln!("picture op[{i}] diverged\n  rust:   {got}\n  oracle: {want}");
            failed.push(format!("picture:{name}"));
        }
        if let Some(diff) = compare_logs(&picture_logs[i], &want_logs) {
            eprintln!("picture op[{i}] {name} LOGS diverged\n{diff}");
            failed.push(format!("picture:{name}:logs"));
        }
    }
    let want_jobs: Vec<Value> = want_jobs
        .iter()
        .map(|j| {
            json!({
                "type": j["type"],
                "status": j["status"],
                "maxAttempts": j["maxAttempts"].as_f64(),
                "payload": serde_json::from_str::<Value>(j["payload"].as_str().unwrap()).unwrap(),
            })
        })
        .collect();
    let (got_jobs, want_jobs) = (normalize(&json!(jobs)), normalize(&json!(want_jobs)));
    if got_jobs != want_jobs {
        eprintln!("queued picture jobs diverged\n  rust:   {got_jobs}\n  oracle: {want_jobs}");
        failed.push("jobs".to_string());
    }
    assert!(failed.is_empty(), "wardrobe tools diverged: {failed:?}");
    // A queued picture is PENDING (no runner on either side) and tried once.
    assert!(!jobs.is_empty(), "the switch-ON scenario queued no picture");
    // [P4.D316] the history scenario's outside-turn rollback queued ONE
    // announcement before the pictures; every other job is a picture.
    let announcements = jobs
        .iter()
        .filter(|j| j["type"] == json!("WARDROBE_OUTFIT_ANNOUNCEMENT"))
        .count();
    assert_eq!(announcements, 1, "the outside-turn rollback's announcement");
    for job in jobs
        .iter()
        .filter(|j| j["type"] != json!("WARDROBE_OUTFIT_ANNOUNCEMENT"))
    {
        assert_eq!(job["type"], json!("WARDROBE_ITEM_IMAGE_GENERATION"));
        assert_eq!(job["status"], json!("PENDING"));
        assert_eq!(job["maxAttempts"], json!(1.0));
    }

    drop(db);
    drop(main);
    drop(mount);
    drop(scratch);

    eprintln!(
        "OK: wardrobe tools matched oracle ({} ops + read-back + {} picture ops, {} jobs).",
        spec.ops.len(),
        pictures.len(),
        jobs.len()
    );
}

/// [P4.D262] Every ledger row minus the minted `id` / `createdAt` /
/// `updatedAt`, the item keyed by TITLE where a create minted its id — the
/// oracle's `wearReadback`, sorted by plain code-unit order.
fn wear_readback(
    main: &rusqlite::Connection,
    spec: &Spec,
    caller_items: &[Value],
    recipient_items: &[Value],
) -> Value {
    let spec_text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../harness/oracle/fixtures/wardrobe-tools.json"),
    )
    .unwrap();
    let _ = spec;
    let key_of = |id: &str| -> String {
        if spec_text.contains(id) {
            return id.to_string();
        }
        caller_items
            .iter()
            .chain(recipient_items)
            .find(|i| i.get("id").and_then(Value::as_str) == Some(id))
            .and_then(|i| i.get("title").and_then(Value::as_str))
            .unwrap_or(id)
            .to_string()
    };
    let mut st = main
        .prepare(
            "SELECT \"itemId\", \"wearerCharacterId\", \"wearCount\", \"firstWornAt\", \"lastWornAt\", \"lastWornChatId\" FROM \"wardrobe_wear_stats\"",
        )
        .unwrap();
    let mut rows: Vec<(String, Value)> = st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .map(|(item, wearer, count, first, last, chat)| {
            let item = key_of(&item);
            let sort_key = format!("{item}|{}", wearer.clone().unwrap_or_default());
            (
                sort_key,
                json!({
                    "itemId": item,
                    "wearerCharacterId": wearer,
                    "wearCount": count,
                    "firstWornAt": first,
                    "lastWornAt": last,
                    "lastWornChatId": chat,
                }),
            )
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Array(rows.into_iter().map(|(_, v)| v).collect())
}

/// [P4.D316] the fourth oracle line: `{ history }`.
fn parse_oracle_history(text: &str) -> Vec<Value> {
    for line in text.lines() {
        let v: Value = serde_json::from_str(line.trim()).unwrap_or(Value::Null);
        if let Some(h) = v.get("history") {
            return h.as_array().expect("history array").clone();
        }
    }
    panic!("the oracle predates P4.D316 — no `history` line; regenerate it");
}

/// The third oracle line: `{ pictures, jobs }`.
fn parse_oracle_pictures(text: &str) -> (Vec<Value>, Vec<Value>) {
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
        if let (Some(p), Some(j)) = (v.get("pictures"), v.get("jobs")) {
            return (p.as_array().unwrap().clone(), j.as_array().unwrap().clone());
        }
    }
    panic!("the oracle predates P4.D262 — no `pictures` line; regenerate it");
}

/// One op through the PRODUCTION table, composed as the executor composes it:
/// the handler half (inside the write, here on the two direct writers), then
/// the after-commit half over the turn's ONE set (the outfit-change effects,
/// the picture, the `… completed` INFO).
#[allow(clippy::too_many_arguments)]
fn run_tool(
    rt: &tokio::runtime::Runtime,
    db: &Db,
    tool: &str,
    main_w: &Writer,
    mount_w: &Writer,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
    turn: Option<&(PendingWardrobeAnnouncements, PendingWardrobeHistory)>,
) -> WardrobeToolOutput {
    let stage = run_wardrobe_tool_handler(
        tool,
        main_w.connection(),
        mount_w.connection(),
        user_id,
        chat_id,
        character_id,
        args,
    )
    .unwrap_or_else(|| panic!("the table holds no `{tool}`"));
    // v4's executor: the caller's `character` provenance (actor only when
    // truthy) and BOTH turn containers.
    let provenance = WardrobeHistoryProvenance::new(WardrobeHistorySource::Character);
    let provenance = if character_id.is_empty() {
        provenance
    } else {
        provenance.with_actor(character_id)
    };
    rt.block_on(finish_wardrobe_tool(
        db,
        &WardrobeToolContext {
            user_id,
            chat_id,
            character_id,
            provenance: &provenance,
            pending_wardrobe_announcements: turn.map(|t| &t.0),
            pending_wardrobe_history: turn.map(|t| &t.1),
        },
        stage,
    ))
}

/// [P4.D316] `chat_wardrobe_history` exactly as the oracle reads it back.
fn dump_history(c: &rusqlite::Connection) -> Value {
    let mut stmt = c
        .prepare(
            "SELECT \"chatId\", \"characterId\", \"entryIndex\", \"slots\", \"slotsHash\", \"source\", \"actorCharacterId\", \"actorToolName\", \"isCurrent\" FROM chat_wardrobe_history ORDER BY \"chatId\", \"characterId\", \"entryIndex\"",
        )
        .expect("prepare the history read-back");
    let rows = stmt
        .query_map([], |r| {
            Ok(json!({
                "chatId": r.get::<_, String>(0)?,
                "characterId": r.get::<_, String>(1)?,
                "entryIndex": r.get::<_, i64>(2)?,
                "slots": r.get::<_, String>(3)?,
                "slotsHash": r.get::<_, String>(4)?,
                "source": r.get::<_, String>(5)?,
                "actorCharacterId": r.get::<_, Option<String>>(6)?,
                "actorToolName": r.get::<_, Option<String>>(7)?,
                "isCurrent": r.get::<_, i64>(8)?,
            }))
        })
        .expect("read the history back")
        .collect::<Result<Vec<_>, _>>()
        .expect("history rows");
    Value::Array(rows)
}

/// The nine handlers' `context`s — their lines compare whole.
const HANDLER_CONTEXTS: [&str; 9] = [
    "wardrobe-list-handler",
    "wardrobe-read-handler",
    "wardrobe-create-handler",
    "wardrobe-update-handler",
    "wardrobe-archive-handler",
    "wardrobe-wear-handler",
    "wardrobe-take-off-handler",
    "wardrobe-rollback-handler",
    "wardrobe-history-handler",
];

/// The keystone's tags — their lines compare by TAG only (R-H: the bytes are
/// the keystone's families').
const KEYSTONE_TAGS: [&str; 4] = [
    "[WearablePool]",
    "[ItemMutations]",
    "[applyDisplacement]",
    "[WardrobeLocation]",
];

/// A parsed log line: `(level, message, [(key, value)])` in field order.
type LogLine = (String, String, Vec<(String, String)>);

/// A captured v5 line (`LEVEL target message k=v …`) → `(level, message,
/// [(key, value)])` in field order, the `…Json` suffix stripped and the
/// renamed `errorMessage` read under v4's `message` key.
fn parse_v5_line(line: &str) -> Option<LogLine> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let bytes = rest.as_bytes();
    // Field keys start after a space: ` <ident>=`.
    let mut keys: Vec<(usize, usize)> = Vec::new();
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i - 1] == b' ' {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i && j < bytes.len() && bytes[j] == b'=' && bytes[i].is_ascii_alphabetic() {
                keys.push((i, j));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    let message_end = keys.first().map(|(k, _)| k - 1).unwrap_or(rest.len());
    let message = rest[..message_end].to_string();
    let mut fields = Vec::new();
    for (n, (start, eq)) in keys.iter().enumerate() {
        let end = keys
            .get(n + 1)
            .map(|(next, _)| next - 1)
            .unwrap_or(rest.len());
        let key = &rest[*start..*eq];
        let key = key
            .strip_suffix("Json")
            .filter(|k| !k.is_empty())
            .unwrap_or(key);
        let key = if key == "errorMessage" {
            "message"
        } else {
            key
        };
        fields.push((key.to_string(), rest[eq + 1..end].to_string()));
    }
    Some((level.to_lowercase(), message, fields))
}

/// A v4 captured line → the same shape (values as the rig renders them).
fn v4_line(v: &Value) -> LogLine {
    let fields = v["fields"]
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, val)| {
                    let rendered = match val {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), rendered)
                })
                .collect()
        })
        .unwrap_or_default();
    (
        v["level"].as_str().unwrap_or_default().to_string(),
        v["message"].as_str().unwrap_or_default().to_string(),
        fields,
    )
}

fn context_of(fields: &[(String, String)]) -> Option<&str> {
    fields
        .iter()
        .find(|(k, _)| k == "context")
        .map(|(_, v)| v.as_str())
}

fn keystone_tag(message: &str) -> Option<&'static str> {
    KEYSTONE_TAGS
        .iter()
        .copied()
        .find(|t| message.starts_with(t))
}

/// Compare one op's lines: the handlers' lines whole (sorted — v5 fires the
/// effects after the write, v4 inline), the keystone's by tag. `None` = same.
fn compare_logs(v5: &[String], v4: &Value) -> Option<String> {
    let v4_lines: Vec<_> = v4
        .as_array()
        .map(|a| a.iter().map(v4_line).collect())
        .unwrap_or_default();
    let v5_lines: Vec<_> = v5.iter().filter_map(|l| parse_v5_line(l)).collect();
    let handler = |lines: &[LogLine]| -> Value {
        let mut rows: Vec<Value> = lines
            .iter()
            .filter(|(_, _, f)| context_of(f).is_some_and(|c| HANDLER_CONTEXTS.contains(&c)))
            .map(|(level, message, fields)| {
                json!([
                    level,
                    message,
                    fields
                        .iter()
                        .map(|(k, v)| json!([k, v]))
                        .collect::<Vec<_>>()
                ])
            })
            .collect();
        rows.sort_by_key(|r| r.to_string());
        Value::Array(rows)
    };
    let tags = |lines: &[LogLine]| -> Vec<&'static str> {
        let mut t: Vec<&'static str> = lines
            .iter()
            .filter_map(|(_, m, _)| keystone_tag(m))
            .collect();
        t.sort_unstable();
        t.dedup();
        t
    };
    let (got, want) = (
        normalize(&handler(&v5_lines)),
        normalize(&handler(&v4_lines)),
    );
    let (got_tags, want_tags) = (tags(&v5_lines), tags(&v4_lines));
    if got == want && got_tags == want_tags {
        return None;
    }
    Some(format!(
        "  v5 handler lines: {got}\n  v4 handler lines: {want}\n  v5 tags: {got_tags:?}\n  v4 tags: {want_tags:?}"
    ))
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

/// P4.D275 R-B — v4's INFO `Wardrobe list completed` (`wardrobe-list-
/// handler.ts:171-185`, `ed8b15b50`), capture-pinned on the test thread over
/// the family's fixture with its ledger planted: all thirteen keys in v4's
/// order, every count over the FINAL items (after `include_equipped`), and v4's
/// JS truthiness — `hasTypeFilter` is TRUE for `[]`, `hasAppropriatenessFilter`
/// FALSE for `''`. The new `neverWornByCallerCount` counts the items the CALLER
/// has never worn (bug 184), beside the household's `neverWornCount`.
#[test]
fn the_list_completed_line_is_v4s() {
    global_capture::install();
    let (Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_FIXTURE_WT_MAIN"),
        std::env::var("QT_FIXTURE_WT_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_FIXTURE_WT_MAIN + QT_FIXTURE_WT_MOUNT (see header).");
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/wardrobe-tools.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let (work_main, work_mount) = (
        scratch.path().join("main.db"),
        scratch.path().join("mount.db"),
    );
    std::fs::copy(&fixture_main, &work_main).unwrap();
    std::fs::copy(&fixture_mount, &work_mount).unwrap();
    let main = Writer::open_writable(&work_main, &spec.test_pepper_base64).unwrap();
    let mount = Writer::open_writable(&work_mount, &spec.test_pepper_base64).unwrap();
    let repo = WardrobeWearStatsRepository::new(main.connection());
    for w in &spec.wear_plant {
        let wearer = match w.wearer.as_deref() {
            Some("caller") => Some(spec.caller_character_id.clone()),
            Some("recipient") => Some(spec.recipient_character_id.clone()),
            Some("departed") => Some(spec.departed_character_id.clone()),
            _ => None,
        };
        repo.increment_wears(&[WardrobeWearIncrement {
            item_id: w.item_id.clone(),
            wearer_character_id: wearer,
            chat_id: w.chat_id.clone(),
            at: w.at.clone(),
        }])
        .unwrap();
    }
    let list = |args: Value| {
        global_capture::capture(|| {
            wardrobe_list::execute(
                main.connection(),
                mount.connection(),
                &spec.user_id,
                &spec.chat_id,
                &spec.caller_character_id,
                &args,
            )
        })
    };
    let tool_lines = |lines: Vec<String>| -> Vec<String> {
        lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::tools "))
            .collect()
    };
    let completed = |out: &wardrobe_list::WardrobeListToolOutput,
                     total: usize,
                     has_type: bool,
                     has_appr: bool,
                     include: bool| {
        let items = &out.items;
        format!(
            "INFO quilltap::tools Wardrobe list completed context=wardrobe-list-handler userId={} characterId={} chatId={} totalItems={total} filteredCount={} hasTypeFilter={has_type} hasAppropriatenessFilter={has_appr} includeEquipped={include} compositeCount={} neverWornCount={} neverWornByCallerCount={} withPictureCount={}",
            spec.user_id,
            spec.caller_character_id,
            spec.chat_id,
            items.len(),
            items.iter().filter(|i| i.is_composite == Some(true)).count(),
            items.iter().filter(|i| i.wear_count == 0).count(),
            items.iter().filter(|i| i.worn_by_you == 0).count(),
            items.iter().filter(|i| i.image_file_id.as_deref().is_some_and(|f| !f.is_empty())).count(),
        )
    };

    let (all, lines) = list(json!({}));
    assert!(all.success);
    let total = all.items.len();
    // The fixture's shape, so the counts below are not vacuous: a shared item
    // the household wore and the caller did not.
    assert!(all
        .items
        .iter()
        .any(|i| i.wear_count > 0 && i.worn_by_you == 0));
    assert_eq!(
        tool_lines(lines),
        vec![completed(&all, total, false, false, true)]
    );

    let (some, lines) = list(json!({
        "type_filter": [],
        "appropriateness_filter": "",
        "include_equipped": false
    }));
    assert!(some.items.len() < total, "the equipped item is dropped");
    assert_eq!(
        tool_lines(lines),
        vec![completed(&some, total, true, false, false)]
    );

    let (_, lines) = list(json!({ "appropriateness_filter": " " }));
    assert!(tool_lines(lines)[0].contains(" hasAppropriatenessFilter=true "));

    // Silence: a refused input logs only the WARN, never `completed`.
    let (_, lines) = list(json!({ "include_equipped": "yes" }));
    let lines = tool_lines(lines);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("WARN quilltap::tools Wardrobe list tool validation failed "));
}
