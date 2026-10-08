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

use std::collections::HashMap;

use quilltap_core::db::archetype_wardrobe::find_archetypes;
use quilltap_core::db::chat_settings::{update_for_user, SettingsColVal};
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
use quilltap_core::services::tool_image_generation::{
    maybe_queue_wardrobe_tool_image, QueueWardrobeToolImageArgs,
};
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
            now,
            &rt,
            &db,
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
    let general_items =
        find_archetypes(main_c, &docs, true, &SharedWardrobeTiers::none()).expect("general items");
    let equipped = ChatOutfitsRepository::new(main_c).get_equipped_outfit(&spec.chat_id);
    let mut ann: Vec<String> = announce.into_iter().collect();
    ann.sort();

    let wear = wear_readback(main_c, &spec, &caller_items, &recipient_items);
    let readback = json!({
        "callerItems": caller_items,
        "recipientItems": recipient_items,
        "generalItems": general_items,
        "equippedOutfit": equipped.unwrap_or(Value::Null),
        "pendingAnnouncements": ann,
        "wardrobeWear": wear,
    });

    let got = normalize(&readback);
    let want = normalize(&oracle_readback);
    assert_eq!(
        got, want,
        "read-back state diverged\n  rust:   {got}\n  oracle: {want}"
    );

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
    for op in &spec.picture_scenario.ops {
        let character_id = match op.character_id.as_deref() {
            Some("recipient") => &spec.recipient_character_id,
            _ => &spec.caller_character_id,
        };
        let (output, formatted) = run_picture_op(
            &rt,
            &db,
            &op.tool,
            &main,
            &mount,
            &spec.user_id,
            &spec.chat_id,
            character_id,
            &op.args,
            now,
        );
        pictures.push(json!({
            "name": op.name,
            "tool": op.tool,
            "output": output,
            "formatted": formatted,
        }));
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
    for (i, (got, want)) in pictures.iter().zip(&want_pictures).enumerate() {
        let (got, want) = (normalize(got), normalize(want));
        assert_eq!(
            got, want,
            "picture op[{i}] diverged\n  rust:   {got}\n  oracle: {want}"
        );
    }
    assert_eq!(pictures.len(), want_pictures.len(), "picture op count");
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
    assert_eq!(
        got_jobs, want_jobs,
        "queued picture jobs diverged\n  rust:   {got_jobs}\n  oracle: {want_jobs}"
    );
    // A queued picture is PENDING (no runner on either side) and tried once.
    assert!(!jobs.is_empty(), "the switch-ON scenario queued no picture");
    for job in &jobs {
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

/// [P4.D262] One switch-ON op, composed exactly as the executor composes it:
/// the handler (inside the write), then — for a successful create / update —
/// `maybe_queue_wardrobe_tool_image` once the write has committed, then the
/// format.
#[allow(clippy::too_many_arguments)]
fn run_picture_op(
    rt: &tokio::runtime::Runtime,
    db: &Db,
    tool: &str,
    main_w: &Writer,
    mount_w: &Writer,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
    now: f64,
) -> (Value, String) {
    let main = main_w.connection();
    let mount = mount_w.connection();
    match tool {
        "wardrobe_create" => {
            let mut out =
                wardrobe_create::execute(main, mount, user_id, chat_id, character_id, args);
            if out.success {
                if let Some(recipient) = out.picture_character_id.clone() {
                    out.image_generation = rt.block_on(maybe_queue_wardrobe_tool_image(
                        db,
                        QueueWardrobeToolImageArgs {
                            user_id: user_id.to_string(),
                            chat_id: chat_id.to_string(),
                            character_id: recipient,
                            item_id: out.item_id.clone(),
                            requested: out.generate_image,
                            default_when_enabled: true,
                            caller_context: "wardrobe-create-handler",
                        },
                    ));
                }
            }
            (to_value(&out), wardrobe_create::format(&out))
        }
        "wardrobe_update" => {
            let (mut out, claim) = wardrobe_update::execute_with_picture_claim(
                main,
                mount,
                user_id,
                chat_id,
                character_id,
                args,
            );
            if let Some(claim) = claim {
                out.image_generation = rt.block_on(maybe_queue_wardrobe_tool_image(
                    db,
                    QueueWardrobeToolImageArgs {
                        user_id: user_id.to_string(),
                        chat_id: chat_id.to_string(),
                        character_id: character_id.to_string(),
                        item_id: claim.item_id,
                        requested: claim.requested,
                        default_when_enabled: claim.changes_look,
                        caller_context: "wardrobe-update-handler",
                    },
                ));
            }
            (to_value(&out), wardrobe_update::format(&out))
        }
        "wardrobe_list" => {
            let out = wardrobe_list::execute(main, mount, user_id, chat_id, character_id, args);
            (to_value(&out), wardrobe_list::format_at(&out, now))
        }
        "wardrobe_read" => {
            let out = wardrobe_read::execute(main, mount, user_id, chat_id, character_id, args);
            (to_value(&out), wardrobe_read::format_at(&out, now))
        }
        other => panic!("no picture-scenario arm for {other}"),
    }
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
    now: f64,
    rt: &tokio::runtime::Runtime,
    db: &Db,
) -> (Value, String) {
    let main = main_w.connection();
    let mount = mount_w.connection();
    match tool {
        // The four picture-aware tools compose exactly as the executor does
        // (the switch is OFF in the main sequence — no settings row — so only a
        // `generate_image: true` ask reaches an outcome: `not-enabled`).
        "wardrobe_list" | "wardrobe_read" | "wardrobe_create" | "wardrobe_update" => {
            run_picture_op(
                rt,
                db,
                tool,
                main_w,
                mount_w,
                user_id,
                chat_id,
                character_id,
                args,
                now,
            )
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
