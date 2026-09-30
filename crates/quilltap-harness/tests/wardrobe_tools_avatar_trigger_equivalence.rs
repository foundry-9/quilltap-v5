//! Tier-2 differential: the wardrobe tools' AVATAR TRIGGER (P4.123; the P4.D238
//! Tier-3 item 8 census). v4's `wardrobe_create` (`equip_now`, for the RECIPIENT),
//! `wardrobe_wear` / `wardrobe_take_off` (`appliedCount > 0`) and
//! `wardrobe_archive` (`wasEquipped`) each await `triggerAvatarGenerationIfEnabled`
//! inside the handler; v5 fires it from the executor's `run_wardrobe_*` runners
//! after the writer closure resolves.
//!
//! Both sides run the SAME scenarios (`wardrobe-tools-avatar-trigger.json`, one
//! chat per scenario so the pending-job dedup never crosses scenarios) over the
//! flag-ON fixture. v4's side drives its REAL handlers (real trigger, real
//! `enqueueCharacterAvatarGeneration`) under tsx; v5's side drives the tools
//! THROUGH THE EXECUTOR (`BuiltInToolRunner`) — the direct `execute` calls of
//! `wardrobe_tools_equivalence` cannot see the trigger. Compared per scenario:
//! each op's `success`, the pending-announcement set, the equipped outfit, and
//! the `background_jobs` rows the scenario's chat produced (`type`, `priority`,
//! `maxAttempts`, `payload`). `status` / `attempts` are NOT compared: v4's
//! in-process queue starts a fresh job within milliseconds (the oracle dumps show
//! `PROCESSING`), where v5's enqueue leaves it `PENDING` — asserted separately.
//!
//! The committed `wardrobe-tools` corpus keeps `avatarGenerationEnabled: false`
//! (a verified no-op for the trigger); [`the_committed_wardrobe_corpus_stays_flag_off`]
//! guards that by reading its builder, so a rebuild that flips it is caught.
//!
//! ORDERING DIVERGENCE (recorded, unpinnable): v4 triggers BEFORE its state
//! re-read; v5 triggers after the whole write commits — observable only when the
//! re-read itself fails, which no differential can plant.
//!
//! Also here (the trigger's own fallback reads, P4.D238 item (b)): v4's
//! `findById` / `findAll` are FALLBACK reads (`null` / `[]` + a repository ERROR),
//! so a read failure takes `chat-not-found` / the next profile tier, never the
//! `Failed to enqueue` catch.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout / a 97b25fc53 pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_WT_AVATAR_SPEC=$V5/harness/oracle/fixtures/wardrobe-tools-avatar-trigger.json \
//!   QT_FIXTURE_WT_MAIN=/tmp/qt-wta-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wta-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-wardrobe-tools-fixture.ts
//!   QT_FIXTURE_WTA_MAIN=/tmp/qt-wta-main.db QT_FIXTURE_WTA_MOUNT=/tmp/qt-wta-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/wardrobe-tools-avatar-trigger.ts \
//!     > /tmp/oracle-wta.ndjson
//! Run:
//!   QT_ORACLE_WTA=/tmp/oracle-wta.ndjson \
//!   QT_FIXTURE_WTA_MAIN=/tmp/qt-wta-main.db QT_FIXTURE_WTA_MOUNT=/tmp/qt-wta-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_tools_avatar_trigger_equivalence -- --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::avatar_generation::{
    trigger_avatar_generation, AvatarGenerationParams, AvatarGenerationResult,
};
use quilltap_core::services::tool_execution::{ToolCall, ToolExecutionContext, ToolRunner};
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct BaseSpec {
    #[serde(rename = "testPepperBase64")]
    pepper: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "callerCharacterId")]
    caller: String,
}

#[derive(Deserialize)]
struct AvSpec {
    #[serde(rename = "imageProfile")]
    image_profile: Value,
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    tool: String,
    args: Value,
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

fn load_specs() -> (BaseSpec, AvSpec) {
    let base = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("wardrobe-tools.json")).expect("base spec"),
    )
    .expect("parse base spec");
    let av = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("wardrobe-tools-avatar-trigger.json"))
            .expect("avatar spec"),
    )
    .expect("parse avatar spec");
    (base, av)
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

fn open_copy(main: &str, mount: &str, pepper: &str, label: &str) -> (Db, PathBuf, PathBuf) {
    let scratch = std::env::temp_dir().join(format!("qt-wta-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch");
    let (m, mo) = (scratch.join("main.db"), scratch.join("mount.db"));
    for p in [&m, &mo] {
        let _ = std::fs::remove_file(p);
    }
    std::fs::copy(main, &m).expect("copy main");
    std::fs::copy(mount, &mo).expect("copy mount");
    let db = Db::open(
        DbPaths {
            main: m.clone(),
            mount_index: Some(mo.clone()),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open two-db");
    (db, m, mo)
}

fn cleanup(paths: &[&PathBuf]) {
    for p in paths {
        for s in ["", "-journal", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{s}", p.display()));
        }
    }
}

fn runner(db: &Db) -> BuiltInToolRunner {
    BuiltInToolRunner::new(
        db.clone(),
        SelfInventoryEnv {
            version: String::new(),
            runtime_mode: "local-dev".to_string(),
            client_shell: ClientShell::Browser,
            mount_index_degraded: false,
            release_notes: None,
            changelog: None,
            model_info: Vec::new(),
            fallback_pricing: Vec::new(),
            registry_default_context: 8192,
        },
    )
}

/// `background_jobs` rows projected to the compared columns, grouped by the
/// payload's `chatId`, each group sorted by its payload text.
fn jobs_by_chat(rows: &[Value]) -> BTreeMap<String, Vec<Value>> {
    let mut out: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for r in rows {
        let payload = match r.get("payload") {
            Some(Value::String(s)) => serde_json::from_str(s).expect("payload json"),
            Some(v) => v.clone(),
            None => Value::Null,
        };
        let chat = payload
            .get("chatId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        out.entry(chat).or_default().push(json!({
            "type": r["type"],
            "priority": r["priority"],
            "maxAttempts": r["maxAttempts"],
            "payload": payload,
        }));
    }
    for v in out.values_mut() {
        v.sort_by_key(|j| j["payload"].to_string());
    }
    out
}

/// Remap every UUID string to a positional `<id-N>` (first appearance) — a
/// minted item id on the v4 side maps to the same token as v5's differently
/// minted one, while the structure is diffed exactly.
fn norm_uuids(v: &Value) -> Value {
    fn is_uuid(s: &str) -> bool {
        s.len() == 36
            && s.bytes().enumerate().all(|(i, b)| match i {
                8 | 13 | 18 | 23 => b == b'-',
                _ => b.is_ascii_hexdigit(),
            })
    }
    fn walk(v: &Value, map: &mut BTreeMap<String, usize>) -> Value {
        match v {
            Value::String(s) if is_uuid(s) => {
                let n = map.len();
                let n = *map.entry(s.clone()).or_insert(n);
                Value::String(format!("<id-{n}>"))
            }
            Value::Array(a) => Value::Array(a.iter().map(|e| walk(e, map)).collect()),
            Value::Object(o) => {
                Value::Object(o.iter().map(|(k, e)| (k.clone(), walk(e, map))).collect())
            }
            other => other.clone(),
        }
    }
    walk(v, &mut BTreeMap::new())
}

#[test]
fn wardrobe_tools_avatar_trigger_matches_oracle() {
    let (Some(oracle_path), Some(fx_main), Some(fx_mount)) = (
        env_or_skip("QT_ORACLE_WTA"),
        env_or_skip("QT_FIXTURE_WTA_MAIN"),
        env_or_skip("QT_FIXTURE_WTA_MOUNT"),
    ) else {
        return;
    };
    let (base, av) = load_specs();

    let text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let (mut o_scen, mut o_jobs, mut o_equipped) = (None, None, None);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("oracle line");
        if let Some(s) = v.get("scenarios") {
            o_scen = Some(s.as_array().expect("scenarios").clone());
        }
        if let Some(j) = v.get("jobs") {
            o_jobs = Some(j.as_array().expect("jobs").clone());
            o_equipped = Some(v["equipped"].clone());
        }
    }
    let (o_scen, o_jobs, o_equipped) = (
        o_scen.expect("oracle missing scenarios"),
        o_jobs.expect("oracle missing jobs"),
        o_equipped.expect("oracle missing equipped"),
    );
    assert_eq!(o_scen.len(), av.scenarios.len(), "oracle scenario count");

    let (db, m, mo) = open_copy(&fx_main, &fx_mount, &base.pepper, "main");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let tools = runner(&db);

    let mut got_scen: Vec<Value> = Vec::new();
    for sc in &av.scenarios {
        let ctx = ToolExecutionContext {
            chat_id: sc.chat_id.clone(),
            user_id: base.user_id.clone(),
            character_id: Some(base.caller.clone()),
            ..Default::default()
        };
        let mut successes = Vec::new();
        for op in &sc.ops {
            let r = rt.block_on(tools.run(
                &ToolCall {
                    name: op.tool.clone(),
                    arguments: op.args.clone(),
                    call_id: None,
                },
                &ctx,
            ));
            successes.push(r.success);
        }
        let mut ann: Vec<String> = ctx
            .pending_wardrobe_announcements
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        ann.sort();
        got_scen.push(json!({
            "name": sc.name,
            "chatId": sc.chat_id,
            "successes": successes,
            "pendingAnnouncements": ann,
        }));
    }
    assert_eq!(
        Value::Array(got_scen),
        Value::Array(o_scen),
        "scenario results / announcements diverged"
    );

    // Equipped outfits per chat.
    let mut got_equipped = serde_json::Map::new();
    for sc in &av.scenarios {
        let cid = sc.chat_id.clone();
        let eq = db
            .read_main(move |c| {
                Ok(
                    quilltap_core::db::chats_outfits::ChatOutfitsRepository::new(c)
                        .get_equipped_outfit(&cid),
                )
            })
            .expect("equipped");
        got_equipped.insert(sc.chat_id.clone(), eq.unwrap_or(Value::Null));
    }
    assert_eq!(
        norm_uuids(&Value::Object(got_equipped)),
        norm_uuids(&o_equipped),
        "equipped outfits diverged"
    );

    // The jobs.
    let dump = db
        .read_main(|c| quilltap_core::db::dump_table_json_conn(c, "background_jobs", "id"))
        .expect("dump background_jobs");
    let rows = dump["rows"].as_array().expect("rows").clone();
    assert!(
        rows.iter().all(|r| r["status"] == "PENDING"),
        "v5 leaves enqueued jobs PENDING: {rows:?}"
    );
    let got = jobs_by_chat(&rows);
    let want = jobs_by_chat(&o_jobs);
    for sc in &av.scenarios {
        assert_eq!(
            got.get(&sc.chat_id),
            want.get(&sc.chat_id),
            "{}: background_jobs diverged\n  rust:   {:?}\n  oracle: {:?}",
            sc.name,
            got.get(&sc.chat_id),
            want.get(&sc.chat_id)
        );
    }
    assert_eq!(got, want, "background_jobs (whole table) diverged");
    // Exercised-count: v4 enqueues exactly these six (create self, create gifted,
    // wear, take_off, archive equipped, the deduped double wear).
    assert_eq!(want.len(), 6, "oracle enqueued jobs for six chats");
    let total: usize = want.values().map(Vec::len).sum();
    assert_eq!(total, 6, "the double wear collapses to ONE row");

    drop(tools);
    drop(db);
    cleanup(&[&m, &mo]);
    eprintln!(
        "OK: wardrobe-tool avatar trigger matched oracle ({} scenarios, {total} jobs).",
        av.scenarios.len()
    );
}

#[test]
fn the_committed_wardrobe_corpus_stays_flag_off() {
    // The `wardrobe_tools` corpus is a neutrality leg only because its chat keeps
    // `avatarGenerationEnabled: false` (v4's trigger is then a verified no-op).
    let builder = std::fs::read_to_string(fixtures_dir().join("build-wardrobe-tools-fixture.ts"))
        .expect("builder");
    let base_chat = builder
        .split("Wardrobe Tools Fixture")
        .nth(1)
        .expect("base chat block")
        .split("equippedOutfit")
        .next()
        .unwrap();
    assert!(
        base_chat.contains("avatarGenerationEnabled: false"),
        "the base wardrobe-tools chat must stay flag-OFF:\n{base_chat}"
    );
}

// ---------------------------------------------------------------------------
// The trigger's fallback reads (P4.D238 item (b)).
// ---------------------------------------------------------------------------

fn drop_tables(db: &Db, rt: &tokio::runtime::Runtime, tables: &[&str]) {
    let sql: String = tables.iter().map(|t| format!("DROP TABLE {t};")).collect();
    rt.block_on(db.write(move |w| {
        w.main()
            .connection()
            .execute_batch(&sql)
            .map_err(quilltap_core::db::DbError::from)
    }))
    .expect("poison");
}

fn params(base: &BaseSpec, chat: &str) -> AvatarGenerationParams {
    AvatarGenerationParams {
        user_id: base.user_id.clone(),
        chat_id: chat.to_string(),
        character_id: base.caller.clone(),
        caller_context: "wardrobe-wear-handler",
        image_profile_id_override: None,
        equipped_slots_override: None,
        force: false,
    }
}

fn line<'a>(lines: &'a [String], needle: &str) -> &'a String {
    lines
        .iter()
        .find(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no line containing {needle:?}: {lines:#?}"))
}

#[test]
fn trigger_read_failures_take_v4_fallback_arms() {
    let (Some(fx_main), Some(fx_mount)) = (
        env_or_skip("QT_FIXTURE_WTA_MAIN"),
        env_or_skip("QT_FIXTURE_WTA_MOUNT"),
    ) else {
        return;
    };
    let (base, av) = load_specs();
    let chat = av.scenarios[3].chat_id.clone(); // a flag-ON salon chat
    let profile_id = av.image_profile["id"].as_str().unwrap().to_string();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // Silence leg: an intact fixture queues with none of the repository lines.
    let (db, m, mo) = open_copy(&fx_main, &fx_mount, &base.pepper, "fb-ok");
    let (r, lines) = quilltap_core::test_support::captured_with(|| {
        rt.block_on(trigger_avatar_generation(&db, &params(&base, &chat)))
    });
    assert_eq!(r, AvatarGenerationResult::Queued);
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("Error finding") || l.contains("Failed to enqueue")),
        "{lines:#?}"
    );
    drop(db);
    cleanup(&[&m, &mo]);

    // chats table gone → v4's `findById` answers null (+ ERROR) → chat-not-found.
    let (db, m, mo) = open_copy(&fx_main, &fx_mount, &base.pepper, "fb-chats");
    drop_tables(&db, &rt, &["chats"]);
    let (r, lines) = quilltap_core::test_support::captured_with(|| {
        rt.block_on(trigger_avatar_generation(&db, &params(&base, &chat)))
    });
    match &r {
        AvatarGenerationResult::NotQueued { reason, .. } => assert_eq!(reason, "chat-not-found"),
        other => panic!("expected chat-not-found, got {other:?}"),
    }
    let l = line(&lines, "Error finding entity by ID");
    assert!(l.starts_with("ERROR "), "{l}");
    assert!(l.contains("collection=chats"), "{l}");
    assert!(
        !lines.iter().any(|l| l.contains("Failed to enqueue")),
        "no catch WARN: {lines:#?}"
    );
    drop(db);
    cleanup(&[&m, &mo]);

    // image_profiles gone → the override lookup, then `findAll`, both fall back:
    // the override WARN, both repository ERRORs, and `no-image-profile`.
    let (db, m, mo) = open_copy(&fx_main, &fx_mount, &base.pepper, "fb-profiles");
    drop_tables(&db, &rt, &["image_profiles"]);
    let mut p = params(&base, &chat);
    p.image_profile_id_override = Some(profile_id.clone());
    let (r, lines) = quilltap_core::test_support::captured_with(|| {
        rt.block_on(trigger_avatar_generation(&db, &p))
    });
    match &r {
        AvatarGenerationResult::NotQueued { reason, .. } => assert_eq!(reason, "no-image-profile"),
        other => panic!("expected no-image-profile, got {other:?}"),
    }
    let l = line(&lines, "Error finding entity by ID");
    assert!(
        l.starts_with("ERROR ") && l.contains("collection=image_profiles"),
        "{l}"
    );
    assert!(l.contains(&format!("id={profile_id}")), "{l}");
    let l = line(&lines, "Error finding all entities");
    assert!(
        l.starts_with("ERROR ") && l.contains("collection=image_profiles"),
        "{l}"
    );
    assert!(
        line(&lines, "override profile not found").starts_with("WARN "),
        "{lines:#?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("Failed to enqueue")),
        "no catch WARN: {lines:#?}"
    );
    drop(db);
    cleanup(&[&m, &mo]);

    // The catch keeps its ONE reachable leg: the enqueue's own write failing.
    let (db, m, mo) = open_copy(&fx_main, &fx_mount, &base.pepper, "fb-enqueue");
    drop_tables(&db, &rt, &["background_jobs"]);
    let (r, lines) = quilltap_core::test_support::captured_with(|| {
        rt.block_on(trigger_avatar_generation(&db, &params(&base, &chat)))
    });
    match &r {
        AvatarGenerationResult::NotQueued { reason, .. } => assert_eq!(reason, "error"),
        other => panic!("expected error, got {other:?}"),
    }
    let l = line(&lines, "Failed to enqueue avatar generation");
    assert!(
        l.starts_with("WARN ") && l.contains("context=wardrobe-wear-handler"),
        "{l}"
    );
    drop(db);
    cleanup(&[&m, &mo]);
}
