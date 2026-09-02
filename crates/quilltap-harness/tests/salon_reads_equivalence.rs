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
                o.remove("renderedHtml");
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
    // P4.D143 §H (v4 `c43d3b1b4`): the Quick-hide probe. v4 answers true iff
    // ANY chat takes the uncensored route — Flagged or Uncensored — so a
    // preserved TRUE label under a Vouched Safe override must NOT count. The
    // jest side gets a fresh fixture copy per case; the Rust side shares one
    // Db, so each arm resets all three chats to Monitored first.
    let reset = |rt: &tokio::runtime::Runtime, db: &Db| {
        rt.block_on(db.write(|w| {
            w.main().connection().execute_batch(
                "UPDATE \"chats\" SET \"conciergeOverride\" = NULL, \"isDangerousChat\" = 0, \
                 \"dangerCategories\" = NULL",
            )?;
            Ok(())
        }))
        .expect("reset concierge states");
    };
    for (name, paint) in [
        ("has_dangerous_none", None),
        (
            "has_dangerous_vouched_only",
            Some("\"conciergeOverride\" = 'OFF', \"isDangerousChat\" = 1"),
        ),
        (
            "has_dangerous_flagged",
            Some("\"conciergeOverride\" = NULL, \"isDangerousChat\" = 1"),
        ),
        (
            "has_dangerous_uncensored",
            Some("\"conciergeOverride\" = 'UNCENSORED', \"isDangerousChat\" = 0"),
        ),
    ] {
        reset(&rt, &db);
        if let Some(set) = paint {
            let sql = format!("UPDATE \"chats\" SET {set} WHERE \"id\" = '{solo}'");
            rt.block_on(db.write(move |w| {
                w.main().connection().execute_batch(&sql)?;
                Ok(())
            }))
            .expect("paint has-dangerous state");
        }
        let got = response_data(&salon::chats_has_dangerous(&db, uid));
        let want = oracle[name]["body"].clone();
        cases.push((name.into(), got, want));
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

    // P4.D143 §H: v4's unknown-action refusal on the same collection route. The
    // 400 is answered at the REST edge (v5 has no core verb for it), so the
    // differential's job is to pin v4's exact BYTES; `chats_routes.rs` builds
    // the same sentence from `CHAT_GET_ACTIONS` and the wire test
    // `chats_collection_route` proves the edge emits it.
    {
        let rec = &oracle["has_dangerous_unknown_action"];
        let want_status = rec["status"].as_i64().unwrap_or(0);
        let want_msg = rec["body"]["error"].as_str().unwrap_or("");
        if want_status != 400
            || want_msg != "Unknown action: no-such-action. Available actions: has-dangerous"
        {
            eprintln!(
                "[has_dangerous_unknown_action] MISMATCH: v4 answers {want_status} {want_msg:?}"
            );
            failed.push("has_dangerous_unknown_action".into());
        } else {
            eprintln!("[has_dangerous_unknown_action] OK (400).");
        }
    }

    assert!(failed.is_empty(), "salon-reads FAILED: {failed:?}");
}
