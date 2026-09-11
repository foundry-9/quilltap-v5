//! P4.D180 IN-SCENE VOICE-REHEARSAL tier-3 (mocked-LLM) differential:
//! `services::announcer::in_scene_voiced::generate_in_scene_voiced_line` and
//! `api::chat_post_office::chat_impersonation_voice_preview` vs v4's REAL
//! `generateInSceneVoicedLine` / `handleImpersonationVoicePreview` (both NEW at
//! v4 `686954937`), over a FRESH copy of the committed
//! `in-scene-voiced-{main,mount}.db` fixture per case.
//!
//! The same canned completion is injected on both sides, keyed by the oracle's
//! RECORDED call (`provider|model|temperature|messages`) — so **the assembled
//! system prompt, the shaped transcript and the user message ARE the diffed
//! evidence**. A recording wrapper sits in front of the canned provider so a
//! prompt mismatch surfaces as a readable diff instead of a bare lookup miss
//! (memory note `tier3-completion-oracle`).
//!
//! ⚠ **`max_tokens` is recorded as its own field**, because the canned key does
//! NOT contain it. On the sibling announcer family that blindness is real and
//! measured (2048 → 99 leaves it green); here the ceiling is the whole point of
//! `max_tokens_for_seed`, so the recording wrapper captures it and it is
//! compared like any other byte.
//!
//! Then a **tier-2 diff on the writes**: the service "persists nothing", but the
//! Commonplace recall it runs bumps `lastAccessedAt` on the memories it
//! returned. The dump is clock-free (`bumped` = `lastAccessedAt` past the
//! fixture's seed timestamp) so it diffs with no normalization — and it
//! discriminates, 2-of-3 on a hitting seed against 0-of-3 on a missing one.
//!
//! Coverage is asserted by shape — every oracle case name must be driven here
//! and vice versa (`harness-corpus-shape-constants-rot`).
//!
//! ## What the FIXTURE cannot express, and why (measured, not assumed)
//!
//! 1. **A non-CHARACTER seat.** v4's own `ChatParticipantSchema` types `type` as
//!    a one-member enum and `characterId` as a required `UUIDSchema`, so BOTH
//!    halves of v4's `Only a character seat can be spoken for.` guard are
//!    unreachable through v4's write path. The builder was written with such a
//!    seat and v4's `chats.create` refused it. Pinned by unit test instead
//!    ([`only_a_character_seat_refusal_is_unreachable_through_v4s_writer`]), the
//!    way P4.D112 pinned its unreachable boundary escapes.
//! 2. **The `Failed to restate the line in character.` default.**
//!    `executeVoiceRewrite` always returns a NON-empty `error`
//!    (`llmResult.error || 'The LLM returned no content.'`), so v4's
//!    `result.error || <default>` never takes its right-hand side on this path.
//!    The `route_failure_default_sentence` row therefore measures the propagated
//!    sentence; the default itself is pinned by unit test
//!    ([`the_empty_error_default_sentence`]).
//!
//! ## What is NOT observable here (measured, then recorded)
//!
//! v4's `subprompts: precompiledIdentityStack ? null : subprompts` is
//! **defensive in both implementations**. `build_system_prompt` uses a present
//! stack VERBATIM (`system_prompt.rs:589` — `Some(s) => s.to_string()`), so the
//! `subprompts` argument can only ever reach the `None` branch; passing them
//! alongside a stack changes no byte. Mutation-measured: replacing the whole
//! conditional with a bare `params.subprompts` leaves all 27 cases GREEN.
//!
//! What IS pinned is the branch itself, by the pair of rows the fixture exists
//! for: `service_with_precompiled_stack` carries the seeded stack sentinel and
//! NO subprompt text, `service_without_precompiled_stack` carries the subprompt
//! and no sentinel, and forcing `get_compiled_identity_stack` to `None` reddens
//! exactly the with-stack rows. The conditional is kept because it is v4's, and
//! because it stops being inert the moment the builder learns to merge.
//!
//! ## Minted values
//!
//! None. Every id in the fixture is pinned by the builder, the clock is frozen
//! at [`NOW_MS`] on both sides, and the only write either side makes is the
//! recall's `lastAccessedAt` bump — which is compared as a boolean against the
//! seed timestamp rather than as a timestamp.
//!
//! Fixture (Node 24, from the v4 checkout — see the builder's header):
//!   TZ=UTC QT_FIXTURE_ISV_MAIN=…/in-scene-voiced-main.db \
//!   QT_FIXTURE_ISV_MOUNT=…/in-scene-voiced-mount.db \
//!     node --import tsx harness/oracle/fixtures/build-in-scene-voiced-fixture.ts
//! Generate the oracle (see the .ts header, **TZ=UTC**):
//!   … QT_ORACLE_OUT=/tmp/oracle-in-scene-voiced.ndjson TZ=UTC npx jest -- in-scene-voiced-tier3
//! Run:
//!   QT_ORACLE_IN_SCENE_VOICED=/tmp/oracle-in-scene-voiced.ndjson \
//!     cargo test -p quilltap-harness --test in_scene_voiced_tier3_equivalence -- --nocapture

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::api::chat_post_office::{
    chat_impersonation_voice_preview, InSceneVoiceDriver, InSceneVoiceRunner,
};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionError, CompletionMessage, CompletionParams,
    CompletionProvider, CompletionResponse, CompletionRole,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::announcer::in_scene_voiced::{
    generate_in_scene_voiced_line, InSceneVoicedLineParams,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use serde::Deserialize;
use serde_json::{json, Value};

const SEED_TS: &str = "2026-05-01T00:00:00.000Z";
const NOW_MS: i64 = 1_777_939_200_000; // 2026-05-05T00:00:00.000Z

const CONN_SEAT: &str = "c0000000-0000-4000-8000-000000000001";
const CONN_OVERRIDE: &str = "c0000000-0000-4000-8000-000000000002";

const CHAT_STACK: &str = "c1000000-0000-4000-8000-000000000001";
const CHAT_NOSTACK: &str = "c1000000-0000-4000-8000-000000000002";
const CHAT_NOHIST: &str = "c1000000-0000-4000-8000-000000000003";
const CHAT_UNC: &str = "c1000000-0000-4000-8000-000000000004";
const CHAT_UNC_OK: &str = "c1000000-0000-4000-8000-000000000005";
const CHAT_NOPROF: &str = "c1000000-0000-4000-8000-000000000006";
const MISSING_CHAT: &str = "99999999-9999-4999-8999-999999999999";

const P_VESPER: &str = "e1000000-0000-4000-8000-000000000001";
const P_BRAM: &str = "e1000000-0000-4000-8000-000000000002";
const P_ESME: &str = "e1000000-0000-4000-8000-000000000005";
const P_GHOST: &str = "e1000000-0000-4000-8000-000000000007";
const P_MISSING: &str = "e1000000-0000-4000-8000-0000000000de";

const SP_DEFAULT: &str = "52000000-0000-4000-8000-000000000002";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    user_id_no_default: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/in-scene-voiced.json")
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

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-isv-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("in-scene-voiced-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("in-scene-voiced-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// Wraps the canned provider and records each call the way the oracle's mock
/// records it — INCLUDING `max_tokens`, which the canned key omits.
struct RecordingProvider {
    inner: CannedCompletionProvider,
    calls: Mutex<Vec<Value>>,
}

impl CompletionProvider for RecordingProvider {
    async fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        let messages: Vec<Value> = params
            .messages
            .iter()
            .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
            .collect();
        let result = self.inner.send_message(provider, base_url, params).await;
        self.calls.lock().unwrap().push(json!({
            "provider": provider,
            "model": params.model,
            "temperature": params.temperature,
            "maxTokens": params.max_tokens,
            "messages": messages,
            "response": result.as_ref().map(|r| r.content.clone()).unwrap_or_default(),
        }));
        result
    }
}

/// The memories table's access-bump evidence, clock-free — mirrors the oracle's
/// `readMemoryBumps`.
fn dump_memory_bumps(db: &Db) -> Value {
    db.read_main(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, characterId, \
             CASE WHEN lastAccessedAt IS NOT NULL AND lastAccessedAt > ?1 THEN 1 ELSE 0 END AS bumped \
             FROM memories ORDER BY id",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![SEED_TS], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "characterId": r.get::<_, Option<String>>(1)?,
                    "bumped": r.get::<_, i64>(2)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, quilltap_core::db::DbError>(Value::Array(rows))
    })
    .unwrap()
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
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
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

fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatPostOffice(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 423,
                ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

/// The oracle's per-case inputs, mirrored here (the oracle's `CaseSpec`).
struct Case {
    name: &'static str,
    via_route: bool,
    /// The session user is `userIdNoDefault` — the no-profile 400.
    no_default_user: bool,
    chat_id: &'static str,
    participant_id: &'static str,
    /// `None` renders as a JSON non-string (the Zod wrong-type arm).
    seed_markdown: Option<&'static str>,
    connection_profile_id: Option<&'static str>,
    system_prompt_id: Option<&'static str>,
    /// SERVICE rows only — what the caller resolved.
    service_profile_id: &'static str,
    service_system_prompt_id: Option<&'static str>,
    service_subprompt_ids: &'static [&'static str],
}

const BASE: Case = Case {
    name: "",
    via_route: false,
    no_default_user: false,
    chat_id: CHAT_NOSTACK,
    participant_id: P_VESPER,
    seed_markdown: Some(""),
    connection_profile_id: None,
    system_prompt_id: None,
    service_profile_id: CONN_SEAT,
    service_system_prompt_id: Some(SP_DEFAULT),
    service_subprompt_ids: &[],
};

/// Matches the oracle's `ASTRAL_SEED` — 3,000 U+1F4DC = 6,000 UTF-16 units.
fn astral_seed() -> String {
    "\u{1F4DC}".repeat(3000)
}
fn long_seed() -> String {
    "The lamps are out on the eastern quay. ".repeat(154)
}
fn huge_seed() -> String {
    "The lamps are out on the eastern quay. ".repeat(316)
}

const CASES: &[Case] = &[
    Case {
        name: "service_with_precompiled_stack",
        chat_id: CHAT_STACK,
        seed_markdown: Some("  Tell them the lamps are out and I am seeing to it.  "),
        service_subprompt_ids: &["measure"],
        ..BASE
    },
    Case {
        name: "service_without_precompiled_stack",
        seed_markdown: Some("  Tell them the lamps are out and I am seeing to it.  "),
        service_subprompt_ids: &["measure"],
        ..BASE
    },
    Case {
        name: "service_no_recall_hits",
        seed_markdown: Some("Zzyzx qwertyuiop."),
        ..BASE
    },
    Case {
        name: "service_presence_windows",
        chat_id: CHAT_NOHIST,
        seed_markdown: Some("The rings are loose and the smith is late."),
        ..BASE
    },
    Case {
        name: "service_astral_seed_budget",
        seed_markdown: None, // supplied by `astral_seed()`
        ..BASE
    },
    Case {
        name: "service_long_seed_budget",
        seed_markdown: None,
        ..BASE
    },
    Case {
        name: "service_huge_seed_caps",
        seed_markdown: None,
        ..BASE
    },
    Case {
        name: "service_empty_completion",
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
    // ── ACTION rows ─────────────────────────────────────────────────────────
    Case {
        name: "route_ok_seat_profile",
        via_route: true,
        chat_id: CHAT_STACK,
        seed_markdown: Some("Tell them the lamps are out and I am seeing to it."),
        ..BASE
    },
    Case {
        name: "route_ok_override_profile",
        via_route: true,
        chat_id: CHAT_STACK,
        seed_markdown: Some("The lamps are out."),
        connection_profile_id: Some(CONN_OVERRIDE),
        ..BASE
    },
    Case {
        name: "route_ok_system_prompt_override",
        via_route: true,
        chat_id: CHAT_STACK,
        seed_markdown: Some("The lamps are out."),
        system_prompt_id: Some(SP_DEFAULT),
        ..BASE
    },
    Case {
        name: "route_missing_chat",
        via_route: true,
        chat_id: MISSING_CHAT,
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
    Case {
        // The dispatcher's chat 404 must beat the body parse.
        name: "route_missing_chat_beats_bad_body",
        via_route: true,
        chat_id: MISSING_CHAT,
        participant_id: "not-a-uuid",
        seed_markdown: Some(""),
        ..BASE
    },
    Case {
        name: "route_zod_blank_seed",
        via_route: true,
        chat_id: CHAT_STACK,
        seed_markdown: Some(""),
        ..BASE
    },
    Case {
        // A non-string `seedMarkdown`: v4's Zod refuses, and so must v5 —
        // measured through the same handler, not through the transport's decode.
        name: "route_zod_non_string_seed",
        via_route: true,
        chat_id: CHAT_STACK,
        seed_markdown: None,
        ..BASE
    },
    Case {
        name: "route_zod_non_uuid_profile",
        via_route: true,
        chat_id: CHAT_STACK,
        seed_markdown: Some("The lamps are out."),
        connection_profile_id: Some("not-a-uuid"),
        ..BASE
    },
    Case {
        // The parse must beat the participant find.
        name: "route_zod_beats_missing_participant",
        via_route: true,
        chat_id: CHAT_STACK,
        participant_id: "not-a-uuid",
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
    Case {
        name: "route_missing_participant",
        via_route: true,
        chat_id: CHAT_STACK,
        participant_id: P_MISSING,
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
    Case {
        name: "route_absent_seat",
        via_route: true,
        chat_id: CHAT_STACK,
        participant_id: P_ESME,
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
    Case {
        name: "route_not_impersonated",
        via_route: true,
        chat_id: CHAT_STACK,
        participant_id: P_BRAM,
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
    Case {
        name: "route_missing_character",
        via_route: true,
        chat_id: CHAT_STACK,
        participant_id: P_GHOST,
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
    Case {
        name: "route_character_default_profile",
        via_route: true,
        chat_id: CHAT_NOPROF,
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
    Case {
        name: "route_instance_default_profile",
        via_route: true,
        chat_id: CHAT_NOPROF,
        participant_id: P_BRAM,
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
    Case {
        name: "route_no_profile_anywhere",
        via_route: true,
        no_default_user: true,
        chat_id: CHAT_NOPROF,
        participant_id: P_BRAM,
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
    Case {
        name: "route_uncensored_reroutes",
        via_route: true,
        chat_id: CHAT_UNC,
        seed_markdown: Some("Say the part you were not going to say."),
        ..BASE
    },
    Case {
        name: "route_uncensored_compatible_does_not_reroute",
        via_route: true,
        chat_id: CHAT_UNC_OK,
        seed_markdown: Some("Say the part you were not going to say."),
        ..BASE
    },
    Case {
        name: "route_failure_default_sentence",
        via_route: true,
        chat_id: CHAT_STACK,
        seed_markdown: Some("The lamps are out."),
        ..BASE
    },
];

/// The seed a case actually sends. The three budget rows build theirs.
fn seed_for(case: &Case) -> Option<String> {
    match case.name {
        "service_astral_seed_budget" => Some(astral_seed()),
        "service_long_seed_budget" => Some(long_seed()),
        "service_huge_seed_caps" => Some(huge_seed()),
        // The Zod wrong-type arm sends a NUMBER, which v5's typed request cannot
        // carry — the handler is driven with the sentinel below instead; see the
        // call site.
        "route_zod_non_string_seed" => None,
        _ => case.seed_markdown.map(str::to_string),
    }
}

#[test]
fn in_scene_voiced_tier3_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_IN_SCENE_VOICED") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec")).expect("spec");
    let oracle: std::collections::HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .expect("oracle ndjson")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("oracle row");
            (v["name"].as_str().expect("name").to_string(), v)
        })
        .collect();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();

    for case in CASES {
        driven.insert(case.name.to_string());
        let Some(want) = oracle.get(case.name) else {
            failed.push(format!("{}_MISSING_FROM_ORACLE", case.name));
            continue;
        };

        let mut canned = CannedCompletionProvider::new();
        for call in want["canned"].as_array().expect("canned array") {
            let messages: Vec<CompletionMessage> = call["messages"]
                .as_array()
                .expect("messages array")
                .iter()
                .map(|m| CompletionMessage {
                    role: match m["role"].as_str().unwrap() {
                        "system" => CompletionRole::System,
                        "assistant" => CompletionRole::Assistant,
                        _ => CompletionRole::User,
                    },
                    content: m["content"].as_str().unwrap().to_string(),
                })
                .collect();
            canned = canned.with_response(
                call["provider"].as_str().unwrap(),
                call["model"].as_str().unwrap(),
                call["temperature"].as_f64(),
                &messages,
                call["response"].as_str().unwrap().to_string(),
                None,
            );
        }
        let seed = seed_for(case);
        let embedding = CannedEmbeddingProvider::new()
            .with_vector(seed.clone().unwrap_or_default(), vec![1.0, 0.0, 0.0, 0.0]);
        // No logging: the fixture family carries no llm-logs partition, and the
        // oracle mocks `logLLMCall` away to match. The LIVE log row is proven by
        // the web-venue wire test instead.
        let executor = CheapLlmTaskExecutor::new();

        let db = fresh_db(&spec, case.name);
        let provider = Arc::new(RecordingProvider {
            inner: canned,
            calls: Mutex::new(Vec::new()),
        });

        if case.via_route {
            let driver: Arc<dyn InSceneVoiceDriver> = Arc::new(InSceneVoiceRunner {
                db: db.clone(),
                completion: provider.clone(),
                embedding,
                executor: Arc::new(executor),
                now_ms: Some(NOW_MS as f64),
            });
            let user = if case.no_default_user {
                &spec.user_id_no_default
            } else {
                &spec.user_id
            };
            // The wrong-TYPE seed cannot ride v5's `String` field at all, which
            // IS the port's answer: the transport refuses it before the handler.
            // v4's Zod refuses it inside the handler. Both are a 400 carrying
            // `Validation error`, so the handler is driven with a marker the
            // schema also refuses (`min1` fails on the empty string) and the
            // equality of the two 400s is what the row measures.
            let seed_arg: String = seed.clone().unwrap_or_default();
            let resp = rt.block_on(chat_impersonation_voice_preview(
                &db,
                Some(&driver),
                user,
                case.chat_id,
                case.participant_id,
                &seed_arg,
                case.connection_profile_id,
                case.system_prompt_id,
            ));
            let (status, body) = status_body(&resp);
            // [VALIDATION_DETAILS_GAP] v4's Zod 400 carries a `details` array
            // that v5's error envelope does not model — the standing, named
            // P4.6bb deferral. Asserted in BOTH directions (v4 HAS it, v5 has
            // NOT) and only then dropped, so the day either side moves this
            // family goes red rather than quietly agreeing.
            let mut want_body = want["body"].clone();
            if let Some(obj) = want_body.as_object_mut() {
                if obj.contains_key("details") {
                    assert_eq!(
                        obj.get("error").and_then(Value::as_str),
                        Some("Validation error"),
                        "[{}] only a Zod refusal carries `details`",
                        case.name
                    );
                    assert!(
                        body.get("details").is_none(),
                        "[{}] v5 has started carrying `details` — retire \
                         VALIDATION_DETAILS_GAP rather than dropping it",
                        case.name
                    );
                    obj.remove("details");
                }
            }
            let want = &json!({ "status": want["status"], "body": want_body, "canned": want["canned"], "tables": want["tables"] });
            let want_status = want["status"].as_u64().unwrap() as u16;
            if status != want_status {
                eprintln!("[{}] STATUS {status} != {want_status}", case.name);
                failed.push(format!("{}_status", case.name));
            }
            if norm(&body) != norm(&want["body"]) {
                eprintln!(
                    "[{}] BODY MISMATCH:\n{}",
                    case.name,
                    first_diff(&norm(&body), &norm(&want["body"]))
                );
                failed.push(format!("{}_body", case.name));
            } else {
                eprintln!("[{}] body OK.", case.name);
            }
        } else {
            let chat = db
                .read_main(|c| quilltap_core::db::chats_read::find_by_id(c, case.chat_id))
                .unwrap()
                .expect("fixture chat");
            let participant = chat["participants"]
                .as_array()
                .expect("participants")
                .iter()
                .find(|p| p["id"].as_str() == Some(case.participant_id))
                .cloned()
                .expect("fixture participant");
            let character_id = participant["characterId"].as_str().unwrap().to_string();
            let character = db
                .read_main(|main| {
                    db.read_mount_index(|mount| {
                        quilltap_core::db::characters_read::find_by_id(main, mount, &character_id)
                    })
                })
                .unwrap()
                .expect("fixture character");
            let profile = db
                .read_main(|main| {
                    quilltap_core::db::connection_profiles::find_by_id(
                        main,
                        case.service_profile_id,
                    )
                })
                .unwrap()
                .expect("fixture profile");
            let selected: Vec<String> = case
                .service_subprompt_ids
                .iter()
                .map(|s| s.to_string())
                .collect();
            let subprompts = db
                .read_main(|main| {
                    db.read_mount_index(|mount| {
                        Ok(
                            quilltap_core::subprompts::storage::resolve_selected_subprompts(
                                main,
                                mount,
                                &character_id,
                                &selected,
                            ),
                        )
                    })
                })
                .unwrap();

            let result = rt.block_on(generate_in_scene_voiced_line(
                &db,
                provider.as_ref(),
                &embedding,
                &executor,
                &InSceneVoicedLineParams {
                    chat: &chat,
                    participant: &participant,
                    character: &character,
                    profile: &profile,
                    seed_markdown: seed.as_deref().unwrap_or_default(),
                    system_prompt_id: case.service_system_prompt_id,
                    subprompts: Some(&subprompts),
                    user_id: &spec.user_id,
                    now_ms: NOW_MS as f64,
                },
            ));

            let got_result = json!({
                "success": result.success,
                "proposedMarkdown": result.proposed_markdown,
                "error": result.error,
            });
            // v4's result object omits `error` on success; compare over the union
            // so an absent key and an explicit null agree.
            let mut want_result = want["result"].clone();
            if want_result.get("error").is_none() {
                want_result
                    .as_object_mut()
                    .unwrap()
                    .insert("error".into(), Value::Null);
            }
            if norm(&got_result) != norm(&want_result) {
                eprintln!(
                    "[{}] RESULT MISMATCH:\n{}",
                    case.name,
                    first_diff(&norm(&got_result), &norm(&want_result))
                );
                failed.push(format!("{}_result", case.name));
            } else {
                eprintln!("[{}] result OK.", case.name);
            }
        }

        // The assembled prompt AND the ceiling — the substance of this tier.
        let got_calls = Value::Array(provider.calls.lock().unwrap().clone());
        if norm(&got_calls) != norm(&want["canned"]) {
            eprintln!(
                "[{} prompt] MISMATCH:\n{}",
                case.name,
                first_diff(&norm(&got_calls), &norm(&want["canned"]))
            );
            failed.push(format!("{}_prompt", case.name));
        } else {
            eprintln!("[{} prompt] OK.", case.name);
        }

        let got_tables = json!({ "memories": dump_memory_bumps(&db) });
        if norm(&got_tables) != norm(&want["tables"]) {
            eprintln!(
                "[{} tables] MISMATCH:\n{}",
                case.name,
                first_diff(&norm(&got_tables), &norm(&want["tables"]))
            );
            failed.push(format!("{}_tables", case.name));
        } else {
            eprintln!("[{} tables] OK.", case.name);
        }
    }

    let recorded: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = recorded.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&recorded).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case coverage drifted — oracle-only: {missing:?}, rust-only: {extra:?}"
    );
    eprintln!("in-scene-voiced: {} cases driven.", driven.len());

    assert!(failed.is_empty(), "in-scene-voiced FAILED: {failed:?}");
}

/// **The `Only a character seat can be spoken for.` refusal is unreachable
/// through v4's own write path** — measured, not assumed: v4's
/// `ChatParticipantSchema` types `type` as a one-member enum and `characterId`
/// as a required `UUIDSchema`, and the fixture builder was written with such a
/// seat and refused by `chats.create`. So no corpus row can drive it, and the
/// sentence is pinned here instead (the P4.D112 precedent for unreachable
/// boundary escapes).
#[test]
fn only_a_character_seat_refusal_is_unreachable_through_v4s_writer() {
    // The bytes v4 answers, so a rewording is caught even with no row to drive.
    assert_eq!(
        quilltap_core::api::chat_post_office::ONLY_A_CHARACTER_SEAT,
        "Only a character seat can be spoken for."
    );
}

/// **The `Failed to restate the line in character.` default is unreachable on
/// this path** — `executeVoiceRewrite` always returns a NON-empty `error`
/// (`llmResult.error || 'The LLM returned no content.'`), so v4's
/// `result.error || <default>` never takes its right-hand side. The corpus row
/// `route_failure_default_sentence` therefore measures the PROPAGATED sentence;
/// the default is pinned here.
#[test]
fn the_empty_error_default_sentence() {
    assert_eq!(
        quilltap_core::api::chat_post_office::FAILED_TO_RESTATE,
        "Failed to restate the line in character."
    );
}

// ===========================================================================
// Tier 2 — the log lines the differential cannot see
// ===========================================================================

/// A driver that answers without touching a provider, so the handler's own
/// logging can be observed on a SUCCESS path.
struct StubDriver(&'static str);
impl InSceneVoiceDriver for StubDriver {
    fn run(
        &self,
        _input: quilltap_core::api::chat_post_office::InSceneVoicedRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        quilltap_core::api::chat_post_office::InSceneVoicedOutcome,
                        String,
                    >,
                > + Send
                + '_,
        >,
    > {
        let proposed = self.0.to_string();
        Box::pin(async move {
            Ok(quilltap_core::api::chat_post_office::InSceneVoicedOutcome {
                success: true,
                proposed_markdown: proposed,
                error: None,
            })
        })
    }
}

/// **v4's `[Chats v1] Impersonation voice preview generated` info bag, key for
/// key.** The differential cannot see it — a log line changes no response byte
/// and no table — so it is pinned through a capturing layer
/// (`capture-layer-target-assert-is-a-prefix-match`: the assertions below match
/// on the SENTENCE, not the target, because the target is a prefix).
///
/// v4 `impersonation-voice-preview.ts:160-167`: `{chatId, participantId,
/// characterId, profileId, seedLength, proposedLength}` at INFO. `seedLength`
/// and `proposedLength` are JS `String.length` — UTF-16 units — which is why
/// the seed below is astral: a scalar-counting port would log 3, not 6.
#[test]
fn the_info_line_carries_v4s_bag() {
    let Some(_) = env_or_skip("QT_ORACLE_IN_SCENE_VOICED") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec")).expect("spec");
    let db = fresh_db(&spec, "infoline");
    let driver: Arc<dyn InSceneVoiceDriver> = Arc::new(StubDriver("ok."));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let lines = quilltap_core::test_support::captured(|| {
        let resp = rt.block_on(chat_impersonation_voice_preview(
            &db,
            Some(&driver),
            &spec.user_id,
            CHAT_STACK,
            P_VESPER,
            "\u{1F4DC}\u{1F4DC}\u{1F4DC}", // 3 scalars, SIX UTF-16 units
            None,
            None,
        ));
        assert_eq!(status_body(&resp).0, 200);
    });

    let info = lines
        .iter()
        .find(|l| l.contains("Impersonation voice preview generated"))
        .unwrap_or_else(|| panic!("no info line in {lines:#?}"));
    assert!(info.starts_with("INFO"), "v4 logs this at info: {info}");
    for field in [
        format!("chatId={CHAT_STACK}"),
        format!("participantId={P_VESPER}"),
        "characterId=a1000000-0000-4000-8000-000000000001".to_string(),
        "profileId=c0000000-0000-4000-8000-000000000001".to_string(),
        // UTF-16 units, not scalars — the whole reason the seed is astral.
        "seedLength=6".to_string(),
        "proposedLength=3".to_string(),
    ] {
        assert!(info.contains(&field), "{field} missing: {info}");
    }

    // The three debug lines v4 emits on the way, at v4's level.
    for sentence in [
        "Impersonation voice preview: seat verified",
        "Impersonation voice preview: profile resolved",
        "Impersonation voice preview: prompt resolved",
    ] {
        let l = lines
            .iter()
            .find(|l| l.contains(sentence))
            .unwrap_or_else(|| panic!("`{sentence}` missing from {lines:#?}"));
        assert!(l.starts_with("DEBUG"), "v4 logs this at debug: {l}");
    }
}

/// **The `%error` vs `?error` rendering check** (Tier 2 item 11,
/// `tracing-percent-field-renders-unquoted`), on the warn arm that actually
/// fires in production.
///
/// Every new log line in this lane spells its error field `error = %e` —
/// Display, so the sentence renders UNQUOTED, which is the house convention
/// every sibling ported warn uses (`chat_scenario.rs:321`,
/// `embedding_dimension_reconcile.rs:269`). A stray `?e` or a bare `e` would
/// render it Debug-QUOTED and silently diverge from its neighbours.
///
/// Driven through the **bystander-vault** warn, which the fixture's broken
/// CORMAC makes fire on every `CHAT_STACK` row — no surgery needed, and the
/// line under test is one production really emits.
///
/// ⚠ **The OTHER warn arm — `Failed to read Taboo settings` — is very nearly
/// unreachable, and this test was first written against it and fired nothing.**
/// `get_taboo_settings` folds BOTH a missing setting and an unparseable one
/// into `Ok(defaults)` (it warns `[InstanceSettings] taboo failed to parse —
/// using defaults` itself), so neither dropping `instance_settings` nor
/// renaming its `value` column reaches the service's own catch. Recorded rather
/// than faked: the arm is ported byte-for-byte against v4 and carries v4's bag,
/// but nothing in this tree can currently drive it.
#[test]
fn the_warn_lines_render_their_error_unquoted() {
    let Some(_) = env_or_skip("QT_ORACLE_IN_SCENE_VOICED") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec")).expect("spec");
    let db = fresh_db(&spec, "warnrender");
    let chat = db
        .read_main(|c| quilltap_core::db::chats_read::find_by_id(c, CHAT_STACK))
        .unwrap()
        .expect("fixture chat");
    let participant = chat["participants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"].as_str() == Some(P_VESPER))
        .cloned()
        .unwrap();
    let character_id = participant["characterId"].as_str().unwrap().to_string();
    let character = db
        .read_main(|m| {
            db.read_mount_index(|mo| {
                quilltap_core::db::characters_read::find_by_id(m, mo, &character_id)
            })
        })
        .unwrap()
        .expect("fixture character");
    let profile = db
        .read_main(|m| quilltap_core::db::connection_profiles::find_by_id(m, CONN_SEAT))
        .unwrap()
        .expect("fixture profile");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let completion = CannedCompletionProvider::new();
    let embedding = CannedEmbeddingProvider::new();
    let executor = CheapLlmTaskExecutor::new();
    let lines = quilltap_core::test_support::captured(|| {
        let _ = rt.block_on(generate_in_scene_voiced_line(
            &db,
            &completion,
            &embedding,
            &executor,
            &InSceneVoicedLineParams {
                chat: &chat,
                participant: &participant,
                character: &character,
                profile: &profile,
                seed_markdown: "The lamps are out.",
                system_prompt_id: Some(SP_DEFAULT),
                subprompts: None,
                user_id: &spec.user_id,
                now_ms: NOW_MS as f64,
            },
        ));
    });
    let warn = lines
        .iter()
        .find(|l| l.contains("Could not load a participant character for attribution"))
        .unwrap_or_else(|| panic!("the bystander warn did not fire: {lines:#?}"));
    assert!(warn.starts_with("WARN"), "v4 warns here: {warn}");
    // v4's bag keys, in v4's order (`in-scene-voiced.ts:258-262`).
    assert!(warn.contains(&format!("chatId={CHAT_STACK}")), "{warn}");
    assert!(
        warn.contains("characterId=a1000000-0000-4000-8000-000000000003"),
        "the BROKEN bystander, not the rehearsing seat: {warn}"
    );
    // Display, not Debug: `error=applyDocumentStoreOverlayOne: …`, never quoted.
    assert!(
        warn.contains("error=applyDocumentStoreOverlayOne") && !warn.contains("error=\""),
        "the error field must render UNQUOTED (`%e`, not `?e`): {warn}"
    );
    // And the debug bag the composer emits, with v4's twelve keys.
    let debug = lines
        .iter()
        .find(|l| l.contains("Composed rewrite request"))
        .unwrap_or_else(|| panic!("no composer debug line: {lines:#?}"));
    assert!(debug.starts_with("DEBUG"), "{debug}");
    for key in [
        "chatId=",
        "participantId=",
        "characterId=",
        "profileId=",
        "transcriptMessages=",
        "systemPromptLength=",
        "hasRecall=",
        "hasTemplate=",
        "tabooPhrases=",
        "usedPrecompiledStack=",
        "seedLength=",
        "maxTokens=",
    ] {
        assert!(debug.contains(key), "{key} missing: {debug}");
    }
}
