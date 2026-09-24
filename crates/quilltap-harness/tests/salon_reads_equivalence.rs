//! P4.6a read-surface differential: the three Salon reads
//! (`api::salon::{chat_settings, list_chats, chat_get}`) vs v4's REAL route
//! handlers (the chat-settings GET, `handleList`, `handleGet`). Both sides read a
//! FRESH copy of the committed salon fixture (identical baked ids → no remap); the
//! response body is diffed after (a) number canonicalization + key sort and
//! (b) stripping `renderedHtml` from the v4 side (the port OMITS it — the locked
//! markdown-render divergence).
//!
//! The dispatch contract wraps differently than v4's REST route, so the harness
//! extracts the comparable payload from each side: settings → the object; list →
//! v4 `body.chats` vs the Rust array; get → `{chat}` both.
//!
//! P4.65 widened the committed fixture for the `ChatListPreloaded` batching
//! port: a THIRD chat (cross-row character/project/story-background dedup,
//! later-dated messages so the sort has distinct keys), character-level tags
//! (Aria: Adventure; Elm: Mystery — the participant `_allTagIds` arm), a
//! vault-LINK avatar hit (Elm), and an unavailable-vault character (Vex — the
//! drop-vs-503 convergence pin), plus the `list_all` key-order pin.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-salon-reads.ndjson npx jest -- salon-reads
//! Run:
//!   QT_ORACLE_SALON_READS=/tmp/oracle-salon-reads.ndjson \
//!     cargo test -p quilltap-harness --test salon_reads_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::salon;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    /// P4.65: the fixture's tag table — `tags[1]` ("Mystery") lives ONLY on a
    /// character, driving the `list_exclude_character_tag` case.
    #[serde(default)]
    tags: Vec<TagSpec>,
    chats: Vec<ChatSpec>,
}
#[derive(Deserialize)]
struct TagSpec {
    id: String,
}
#[derive(Deserialize)]
struct ChatSpec {
    id: String,
    #[serde(default)]
    tags: Vec<String>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/salon.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}
fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
/// Strip `renderedHtml` from every message in a `{chat:{messages:[…]}}` body
/// (the v4 side always emits it; the port omits it — the locked divergence).
fn strip_rendered_html(v: &mut Value) {
    if let Some(msgs) = v
        .get_mut("chat")
        .and_then(|c| c.get_mut("messages"))
        .and_then(Value::as_array_mut)
    {
        for m in msgs {
            if let Some(o) = m.as_object_mut() {
                // `shift_remove`, NOT `remove`: serde_json is built with
                // `preserve_order`, so `Map::remove` is indexmap's SWAP-remove
                // — it moves the LAST key into the removed slot. Stripping
                // `renderedHtml` that way silently relocated
                // `confirmationOriginalContent` from the end of every message
                // to position 11, which made the v4 side's key order a
                // fiction. Invisible until P4.D183 added the detail key-order
                // pin below, because `norm` sorts keys away
                // (`serde-json-map-remove-is-swap-remove`).
                o.shift_remove("renderedHtml");
            }
        }
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    let sorted = sorted(&v);
    serde_json::to_string_pretty(&sorted).unwrap()
}
/// Every object's key sequence, depth-first — the shape `norm`'s sorted compare
/// deliberately throws away (mirrors `home_routes_equivalence::key_paths`).
fn key_paths(v: &Value, path: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            out.push(format!(
                "{path}: {}",
                o.keys().cloned().collect::<Vec<_>>().join(",")
            ));
            for (k, x) in o.iter() {
                key_paths(x, &format!("{path}/{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                key_paths(x, &format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

/// The Rust `data` payload (adjacently-tagged Response → `{type, data}`).
fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

#[test]
fn salon_reads_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SALON_READS") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let solo = &spec.chats[0].id;
    let group = &spec.chats[1].id;
    let tag = &spec.chats[0].tags[0];
    let uid = &spec.user_id;

    // A fresh Db over a copy of the committed fixture.
    let scratch = std::env::temp_dir().join(format!("qt-salon-reads-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("salon-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("salon-mount.db"), &mount).unwrap();
    // P4.D171: the committed `salon-{main,mount}.db` predates the two
    // `78b381a96`-round schema moves. On a real instance the boot ensures have
    // already added the columns before any Salon read runs — the same
    // repaired-at-boot idiom `web_search_runner_wire.rs` uses for the
    // connection-profiles fallback pair.
    {
        let w = quilltap_core::db::Writer::open_writable(&main, &spec.test_pepper_base64).unwrap();
        quilltap_core::db::chat_messages_route_trail_repair::
            ensure_chat_messages_route_trail_column(w.connection())
            .expect("ensure the route-trail column on the vintage fixture");
        quilltap_core::db::chats_cycle_order_repair::ensure_chats_cycle_order_column(
            w.connection(),
        )
        .expect("ensure the cycle-order column on the vintage fixture");
        // P4.D183, same idiom, one round later: `31436bae4` moved the schema
        // again (`files.generationKey`, `chats.transcriptVersion`). The
        // generation-key half is not optional here — P4.D182 put the column in
        // `FILE_ENTRY_COLUMNS`, and this family resolves message ATTACHMENTS,
        // so without the heal every chat GET answers `no such column:
        // generationKey` and the whole family reds. It stayed invisible at
        // P4.D182's gate because this test SKIPs (and passes) when its oracle
        // var is unset, and cargo captures a passing test's output.
        quilltap_core::test_support::ensure_p4d182_columns(w.connection());
        // …and the counter planted at 7 on every chat, as the oracle plants it:
        // the chat GET's `transcriptVersion` is a projected VALUE, and at the
        // healed `DEFAULT 0` a hard-coded zero would pass this family (the
        // `31436bae4` unification review's catch). The key-order pin below sees
        // paths; this makes the number itself a comparand.
        w.connection()
            .execute("UPDATE \"chats\" SET \"transcriptVersion\" = 7", [])
            .expect("plant the transcript counter on the vintage fixture");
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");

    // (name, got payload, want payload)
    let re_ts = Regex::new(r"^ignore$").unwrap();
    let _ = re_ts;
    let mut cases: Vec<(String, Value, Value)> = Vec::new();

    // settings
    {
        let got = response_data(&salon::chat_settings(&db, uid));
        let want = oracle["settings"]["body"].clone();
        cases.push(("settings".into(), got, want));
    }
    // list_all
    {
        let got = response_data(&salon::list_chats(&db, uid, &[], None, false));
        let want = oracle["list_all"]["body"]["chats"].clone();
        cases.push(("list_all".into(), got, want));
    }
    // list_exclude_tag
    {
        let got = response_data(&salon::list_chats(
            &db,
            uid,
            std::slice::from_ref(tag),
            None,
            false,
        ));
        let want = oracle["list_exclude_tag"]["body"]["chats"].clone();
        cases.push(("list_exclude_tag".into(), got, want));
    }
    // list_exclude_character_tag (P4.65): "Mystery" lives ONLY on a character
    // (Elm, in the third chat, which carries no chat-level tag) — the
    // participant half of `_allTagIds` in isolation. The list cases as a set
    // also pin the drop-vs-503 convergence: every one of them enriches the
    // third chat, whose Vex participant has an UNAVAILABLE vault — v4's
    // batched `findByIds` drops it (participant.character: null) where v5's
    // pre-P4.65 per-row `find_by_id` answered a StoreUnavailable error.
    {
        let character_tag = &spec.tags[1].id;
        let got = response_data(&salon::list_chats(
            &db,
            uid,
            std::slice::from_ref(character_tag),
            None,
            false,
        ));
        let want = oracle["list_exclude_character_tag"]["body"]["chats"].clone();
        cases.push(("list_exclude_character_tag".into(), got, want));
    }
    // list_limit1
    {
        let got = response_data(&salon::list_chats(&db, uid, &[], Some(1), false));
        let want = oracle["list_limit1"]["body"]["chats"].clone();
        cases.push(("list_limit1".into(), got, want));
    }
    // get_solo
    {
        // No probe: identical to the jest oracle's empty `ptyManager` map.
        let got = response_data(&rt.block_on(salon::chat_get(&db, uid, solo, None)));
        let mut want = oracle["get_solo"]["body"].clone();
        strip_rendered_html(&mut want);
        cases.push(("get_solo".into(), got, want));
    }
    // get_group
    {
        let got = response_data(&rt.block_on(salon::chat_get(&db, uid, group, None)));
        let mut want = oracle["get_group"]["body"].clone();
        strip_rendered_html(&mut want);
        cases.push(("get_group".into(), got, want));
    }
    // get_impersonated (P4.D60, bug 51): inject live impersonation state onto the
    // group chat, then GET — the projection must echo the seat in both fields.
    // LAST case: the shared-db mutation cannot affect the earlier reads.
    {
        let seat = "b2000000-0000-4000-8000-000000000001";
        let sql = format!(
            "UPDATE \"chats\" SET \"impersonatingParticipantIds\" = '[\"{seat}\"]', \
             \"activeTypingParticipantId\" = '{seat}' WHERE \"id\" = '{group}'"
        );
        rt.block_on(db.write(move |w| {
            w.main().connection().execute_batch(&sql)?;
            Ok(())
        }))
        .expect("inject impersonation");
        let got = response_data(&rt.block_on(salon::chat_get(&db, uid, group, None)));
        let mut want = oracle["get_impersonated"]["body"].clone();
        strip_rendered_html(&mut want);
        cases.push(("get_impersonated".into(), got, want));
    }
    // P4.D143 (v4 `c43d3b1b4`): the list carries the DERIVED `conciergeState` +
    // `dangerCategories`, never the raw pair. Paint one chat per non-Monitored
    // state with the preserved label set the wrong way round on both operator
    // rows (Vouched over a TRUE label, Uncensored over a FALSE one), so a
    // payload that leaked `isDangerousChat` would be visibly wrong rather than
    // accidentally right. Mirrors the oracle's `setConcierge` UPDATEs exactly.
    // AFTER `get_impersonated`: the shared-db mutation must not reach it.
    let third = &spec.chats[2].id;
    {
        let sql = format!(
            "UPDATE \"chats\" SET \"conciergeOverride\" = 'OFF', \"isDangerousChat\" = 1, \
             \"dangerCategories\" = NULL WHERE \"id\" = '{solo}';              UPDATE \"chats\" SET \"conciergeOverride\" = 'UNCENSORED', \"isDangerousChat\" = 0, \
             \"dangerCategories\" = NULL WHERE \"id\" = '{group}';              UPDATE \"chats\" SET \"conciergeOverride\" = NULL, \"isDangerousChat\" = 1, \
             \"dangerCategories\" = '[\"Violence\",\"Substance Use\"]' WHERE \"id\" = '{third}'"
        );
        rt.block_on(db.write(move |w| {
            w.main().connection().execute_batch(&sql)?;
            Ok(())
        }))
        .expect("paint concierge states");
        let got = response_data(&salon::list_chats(&db, uid, &[], None, false));
        let want = oracle["list_concierge_states"]["body"]["chats"].clone();
        cases.push(("list_concierge_states".into(), got, want));
    }
    // The single-chat GET is UNTOUCHED by `c43d3b1b4`: the detail view keeps
    // the raw trio for the sidebar control. Same painted state, and the body
    // must still carry `isDangerousChat` / `dangerCategories` /
    // `conciergeOverride` and NO `conciergeState`.
    {
        let sql = format!(
            "UPDATE \"chats\" SET \"dangerCategories\" = '[\"Violence\"]' WHERE \"id\" = '{solo}'"
        );
        rt.block_on(db.write(move |w| {
            w.main().connection().execute_batch(&sql)?;
            Ok(())
        }))
        .expect("paint solo categories");
        let got = response_data(&rt.block_on(salon::chat_get(&db, uid, solo, None)));
        let mut want = oracle["get_vouched_keeps_raw_trio"]["body"].clone();
        strip_rendered_html(&mut want);
        // The claim in its own right, so a normalizer change can never make it
        // vacuous: the detail body keeps all three raw keys and gains none of
        // the list's derived one.
        for key in ["isDangerousChat", "dangerCategories", "conciergeOverride"] {
            assert!(
                want["chat"].get(key).is_some(),
                "oracle's detail body lost {key} — v4's detail view is supposed to keep the raw trio"
            );
            assert!(
                got["chat"].get(key).is_some(),
                "v5's detail body lost {key}"
            );
        }
        assert!(
            want["chat"].get("conciergeState").is_none()
                && got["chat"].get("conciergeState").is_none(),
            "the detail GET must NOT carry the list's derived conciergeState"
        );
        cases.push(("get_vouched_keeps_raw_trio".into(), got, want));
    }
    // P4.D171: the route trail (on a message) and the drawn rotation (on the
    // chat) — the two `78b381a96`-round schema moves. The oracle's
    // `setRouteTrail`/`setCycleOrder` mirror these exactly. LAST case: shared
    // db, and the mutation cannot affect earlier reads. (Until P4.D220 it first
    // undid the retired `has_dangerous_*` loop's leftover concierge paint; the
    // reset below stays, restoring the PRISTINE fixture's untouched NULLs, so
    // the case cannot depend on what ran before it.)
    {
        rt.block_on(db.write(|w| {
            w.main().connection().execute_batch(
                "UPDATE \"chats\" SET \"conciergeOverride\" = NULL, \"isDangerousChat\" = NULL, \
                 \"dangerCategories\" = NULL",
            )?;
            Ok(())
        }))
        .expect("restore pristine concierge state");
        let message_id = "d1000000-0000-4000-8000-000000000002";
        let trail = serde_json::json!([
            {
                "profileId": "cccc0001-0000-4000-8000-000000000001",
                "profileName": "Primary",
                "provider": "OPENAI_COMPATIBLE",
                "modelName": "mock-model",
                "via": "primary",
                "outcome": "failed",
                "trigger": "rate-limit",
                "evidence": "finish-reason",
                "detail": "HTTP 429",
            },
            {
                "profileId": "cccc0001-0000-4000-8000-000000000001",
                "profileName": "Primary",
                "provider": "OPENAI_COMPATIBLE",
                "modelName": "mock-model",
                "via": "retry",
                "outcome": "answered",
            },
        ]);
        let trail_text = serde_json::to_string(&trail).unwrap();
        let solo_owned = solo.clone();
        rt.block_on(db.write(move |w| {
            w.main().connection().execute(
                "UPDATE \"chat_messages\" SET \"routeTrail\" = ?1 WHERE \"id\" = ?2",
                rusqlite::params![trail_text, message_id],
            )?;
            w.main().connection().execute(
                "UPDATE \"chats\" SET \"cycleOrderParticipantIds\" = ?1 WHERE \"id\" = ?2",
                rusqlite::params![
                    "[\"b1000000-0000-4000-8000-000000000002\",\"b1000000-0000-4000-8000-000000000001\"]",
                    solo_owned
                ],
            )?;
            Ok(())
        }))
        .expect("plant route trail + cycle order");
        let got = response_data(&rt.block_on(salon::chat_get(&db, uid, solo, None)));
        let mut want = oracle["get_route_trail_and_cycle_order"]["body"].clone();
        strip_rendered_html(&mut want);
        cases.push(("get_route_trail_and_cycle_order".into(), got, want));
    }

    // P4.D195 (v4 `1fefadb9a`, bug 147): the two cycle columns' non-default
    // arms — the shapes neither the committed fixture nor a JSON-encoding
    // plant can pose. Mirrors the oracle's `setCycleColumnsRaw`. Runs LAST
    // (after the rotation plant above) because each writes both columns on the
    // shared db and no later read depends on them.
    //
    // (a) EMPTY STRING in both columns: `'' ?? '[]'` is `''` in JS, so v4 puts
    //     the empty string on the wire. v5 must agree (`.filter(|v|
    //     !v.is_null())` leaves a non-null `Value::String("")` alone).
    {
        let solo_owned = solo.clone();
        rt.block_on(db.write(move |w| {
            // Undo the previous block's route-trail plant first: the oracle
            // gets a FRESH fixture copy per case and this one does not ask for
            // a trail, while the Rust side shares ONE db. NULL is the
            // fixture's untouched value for the column.
            w.main()
                .connection()
                .execute_batch("UPDATE \"chat_messages\" SET \"routeTrail\" = NULL")?;
            w.main().connection().execute(
                "UPDATE \"chats\" SET \"spokenThisCycleParticipantIds\" = '', \
                 \"cycleOrderParticipantIds\" = '' WHERE \"id\" = ?1",
                rusqlite::params![solo_owned],
            )?;
            Ok(())
        }))
        .expect("plant empty-string cycle columns");
        let got = response_data(&rt.block_on(salon::chat_get(&db, uid, solo, None)));
        let mut want = oracle["get_cycle_columns_empty_string"]["body"].clone();
        strip_rendered_html(&mut want);
        cases.push(("get_cycle_columns_empty_string".into(), got, want));
    }

    // (b) SQL NULL in both columns — the legacy row predating them. Neither
    //     side's `?? '[]'` fires (v4's chat schema `.default('[]')` and v5's
    //     `db::chats_read` coerce first); this arm proves they agree about
    //     that, and is the ONLY case exercising a NULL
    //     `spokenThisCycleParticipantIds`.
    //
    //     It also carries the READER'S GUARANTEE, stated executably. P4.D195's
    //     M2 mutation — projecting `Value::Null` instead of `'[]'` when the
    //     value is null — SURVIVED the entire differential, because no input
    //     can reach that fallback: the reader has already coerced. That is a
    //     real finding about coverage, not a proof to delete, and the honest
    //     response is to pin the reason rather than pretend the corpus covers
    //     the branch. If `db::chats_read` ever stopped coercing, THIS is what
    //     reddens, and the projection's `?? '[]'` (kept for v4 fidelity)
    //     becomes load-bearing instead of decorative.
    {
        let solo_owned = solo.clone();
        rt.block_on(db.write(move |w| {
            w.main().connection().execute(
                "UPDATE \"chats\" SET \"spokenThisCycleParticipantIds\" = NULL, \
                 \"cycleOrderParticipantIds\" = NULL WHERE \"id\" = ?1",
                rusqlite::params![solo_owned],
            )?;
            Ok(())
        }))
        .expect("plant null cycle columns");
        let raw_row = db
            .read_main(|c| quilltap_core::db::chats_read::find_by_id(c, solo))
            .expect("read the null-column row back")
            .expect("the solo chat exists");
        for key in ["spokenThisCycleParticipantIds", "cycleOrderParticipantIds"] {
            assert!(
                raw_row.get(key).is_some_and(|v| v.is_string()),
                "reader_never_yields_null_cycle_columns: `db::chats_read` handed                  `{key}` as {:?} for a NULL column — it must coerce to a string                  (`'[]'`) before the projection sees it. The projection's                  `?? '[]'` fallback has been unreachable on that guarantee; it                  is now load-bearing, so re-check `api::salon::chat_get`.",
                raw_row.get(key)
            );
        }
        let got = response_data(&rt.block_on(salon::chat_get(&db, uid, solo, None)));
        let mut want = oracle["get_cycle_columns_null"]["body"].clone();
        strip_rendered_html(&mut want);
        cases.push(("get_cycle_columns_null".into(), got, want));
    }

    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);

    let mut failed = Vec::new();
    for (name, got, want) in &cases {
        let g = norm(got);
        let w = norm(want);
        if g != w {
            eprintln!("[{name}] MISMATCH:\n{}", first_diff(&g, &w));
            failed.push(name.clone());
        } else {
            eprintln!("[{name}] OK.");
        }
    }

    // P4.65: the wire-order claim over the richest list body — the Rust key
    // sequence must match v4's `JSON.stringify` bytes, not merely carry the
    // same key SET (`norm` sorts keys away). Works because serde_json is built
    // with `preserve_order` workspace-wide (feature unification), so both the
    // parsed oracle line and the serialized Rust response retain source order.
    // Mirrors `home_routes_equivalence::check_key_order` (P4.64's precedent).
    if let Some((_, got, want)) = cases.iter().find(|(n, _, _)| n == "list_all") {
        let mut got_paths = Vec::new();
        key_paths(got, "", &mut got_paths);
        let mut want_paths = Vec::new();
        key_paths(want, "", &mut want_paths);
        if got_paths != want_paths {
            eprintln!("[list_all] KEY ORDER MISMATCH:\n got {got_paths:#?}\n want {want_paths:#?}");
            failed.push("list_all:keyOrder".into());
        } else {
            eprintln!("[list_all] key order OK ({} objects).", got_paths.len());
        }
    } else {
        failed.push("list_all:keyOrder-case-missing".into());
    }

    // P4.D183: the same wire-order claim over the chat GET's DETAIL body.
    //
    // §C.1 places `transcriptVersion` "at v4's position" — directly after
    // `messages`, before `projectId` (v4 `handlers/get.ts:375`) — and until
    // this pin existed nothing held it there: `norm` sorts keys away, and the
    // `list_all` pin above covers only the LIST. Measured by mutation at the
    // lane: moving the key two slots later left the family GREEN.
    //
    // Pinned over every detail case, not just one, because the five differ in
    // which optional keys they carry and a single case would let the others
    // drift.
    {
        let detail_cases: Vec<&(String, Value, Value)> = cases
            .iter()
            .filter(|(n, _, _)| n.starts_with("get_"))
            .collect();
        assert!(
            detail_cases.len() >= 4,
            "the chat-GET key-order pin found only {} detail case(s) — it is \
             named by the `get_` prefix and has gone vacuous",
            detail_cases.len()
        );
        for (name, got, want) in detail_cases {
            let mut got_paths = Vec::new();
            key_paths(got, "", &mut got_paths);
            let mut want_paths = Vec::new();
            key_paths(want, "", &mut want_paths);
            if got_paths != want_paths {
                eprintln!(
                    "[{name}] KEY ORDER MISMATCH:\n got {got_paths:#?}\n want {want_paths:#?}"
                );
                failed.push(format!("{name}:keyOrder"));
            } else {
                eprintln!("[{name}] key order OK ({} objects).", got_paths.len());
            }
        }
    }

    // P4.D220 (v4 `944127d9a` + `ad1c4c37f`): the collection GET's refusals.
    // The `has-dangerous` probe is RETIRED; v4's GET is now `dispatchAction(req,
    // {}, list)`, so every `?action=` shape — the retired name, an unknown one,
    // a bare `?action=`, a key-only `?action` — is the `Unknown action`
    // envelope with an EMPTY `availableActions`. The 400 is answered at the
    // REST edge (v5 has no core verb for it), so the differential's job is to
    // pin v4's exact BYTES; `chats_routes.rs` answers them through
    // `query::dispatch_action(&[])` and the wire test `chats_collection_route`
    // proves the edge emits them.
    for (name, action) in [
        ("list_action_retired_has_dangerous", "has-dangerous"),
        ("list_action_unknown", "no-such-action"),
        ("list_action_bare", ""),
        ("list_action_key_only", ""),
    ] {
        let rec = &oracle[name];
        let want = serde_json::json!({
            "error": format!("Unknown action: {action}"),
            "availableActions": [],
        });
        let status = rec["status"].as_i64().unwrap_or(0);
        if status != 400 || rec["body"] != want {
            eprintln!("[{name}] MISMATCH: v4 answers {status} {}", rec["body"]);
            failed.push(name.into());
        } else {
            eprintln!("[{name}] OK (400).");
        }
    }

    assert!(failed.is_empty(), "salon-reads FAILED: {failed:?}");
}
