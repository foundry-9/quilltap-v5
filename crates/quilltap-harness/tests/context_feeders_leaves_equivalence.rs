//! Tier-1 differential for the W4.6a context-feeder PURE leaves — the byte-exact
//! builders / formatters / config resolvers / parsers, diffed against v4's REAL
//! exports (`harness/oracle/cases/context-feeders-leaves.ts`).
//!
//! Covers: the off-scene content/opaque builders + `findIntroducedOffScene-
//! CharacterIds`, `resolveCoreWhisperConfig` + the three core-whisper content
//! builders, `renderRelevantConversationsBlock`, `buildSuparnaMailLLMContext`,
//! and `capClothingSummary`. The stateful reads (frozen-archive DB ranking, the
//! recap's vault-summary search, the off-scene SCAN, the core-whisper packet
//! assembly, the recap composition end-to-end) are exercised by
//! `build_context_tier3_equivalence`.
//!
//! Generate (Node 24, from the v4 checkout). `TZ=UTC` is REQUIRED — v4's
//! `formatLetterDate` renders in the machine's local zone (system-TZ
//! `toLocaleDateString`), while the Rust `format_time` is UTC-pinned (the
//! documented harness seam shared with `mail_carina_tools_equivalence`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx $V5/harness/oracle/cases/context-feeders-leaves.ts \
//!       > /tmp/oracle-context-feeders-leaves.ndjson
//! Run:
//!   QT_ORACLE_CONTEXT_FEEDERS_LEAVES=/tmp/oracle-context-feeders-leaves.ndjson \
//!     cargo test -p quilltap-harness --test context_feeders_leaves_equivalence

use serde_json::{json, Value};

use quilltap_core::services::core_whisper::{
    build_core_whisper_content, build_core_whisper_llm_context, build_core_whisper_opaque_content,
    resolve_core_whisper_config, CoreFile, CorePacket, GlobalCoreWhisperSettings,
};
use quilltap_core::services::off_scene::{
    build_off_scene_characters_content, build_off_scene_characters_opaque_content,
    find_introduced_off_scene_character_ids, OffSceneCharacterCard,
};
use quilltap_core::services::scene_state_tracking::cap_clothing_summary;
use quilltap_core::services::suparna_mail::build_suparna_mail_llm_context;

/// Recompute the Rust value for one `(kind, id)` oracle row.
fn rust_value(kind: &str, id: &str) -> Value {
    match kind {
        "off_scene_content" | "off_scene_opaque" => {
            let (chars, user) = off_scene_case(id);
            let s = if kind == "off_scene_content" {
                build_off_scene_characters_content(&chars, user.as_deref())
            } else {
                build_off_scene_characters_opaque_content(&chars, user.as_deref())
            };
            Value::String(s)
        }
        "introduced_ids" => {
            let messages = introduced_case(id);
            let mut ids: Vec<String> = find_introduced_off_scene_character_ids(&messages)
                .into_iter()
                .collect();
            ids.sort();
            json!(ids)
        }
        "core_config" => {
            let (chat_enabled, chat_interval, char_enabled, global) = core_config_case(id);
            let cfg =
                resolve_core_whisper_config(chat_enabled, chat_interval, char_enabled, &global);
            json!({
                "enabled": cfg.enabled,
                "interval": cfg.interval,
                "silenceThreshold": cfg.silence_threshold,
                "packetTokenBudget": cfg.packet_token_budget,
                "fireOnContextTransition": cfg.fire_on_context_transition,
            })
        }
        "core_persona" | "core_opaque" | "core_llm" => {
            let packet = core_packet_case(id);
            let s = match kind {
                "core_persona" => build_core_whisper_content(&packet),
                "core_opaque" => build_core_whisper_opaque_content(&packet),
                _ => build_core_whisper_llm_context(&packet),
            };
            Value::String(s)
        }
        "relevant_block" => Value::String(relevant_block_case(id)),
        "suparna_llm" => Value::String(suparna_case(id)),
        "clothing_cap" => Value::String(clothing_case(id)),
        other => panic!("unknown oracle kind {other}"),
    }
}

// ---- case data (mirrors the oracle corpus) ----

fn card(id: &str, name: &str) -> OffSceneCharacterCard {
    OffSceneCharacterCard {
        id: id.into(),
        name: name.into(),
        aliases: vec![],
        pronouns: None,
        identity: None,
        description: None,
    }
}

fn off_scene_case(id: &str) -> (Vec<OffSceneCharacterCard>, Option<String>) {
    match id {
        "single-identity" => {
            let mut c = card("c1", "Ada");
            c.identity = Some("A brilliant analyst named {{char}}.".into());
            (vec![c], Some("Charlie".into()))
        }
        "single-description-fallback" => {
            let mut c = card("c1", "Bea");
            c.description = Some("A gentle beekeeper.".into());
            c.identity = Some("".into());
            (vec![c], None)
        }
        "plural-aliases-pronouns-sorted" => {
            let mut zed = card("c2", "Zed");
            zed.aliases = vec!["Z".into(), "Zee".into()];
            zed.pronouns = Some(("they".into(), "them".into(), "theirs".into()));
            zed.identity = Some("The last one.".into());
            let mut ada = card("c1", "Ada");
            ada.identity = Some("The first one, {{user}} knows her.".into());
            (vec![zed, ada], Some("Charlie".into()))
        }
        "accented-name-sort" => {
            let mut zoe = card("c2", "Zoë");
            zoe.identity = Some("z".into());
            let mut aegis = card("c1", "Ægis");
            aegis.identity = Some("a".into());
            let mut ada = card("c3", "ada");
            ada.identity = Some("lowercase ada".into());
            (vec![zoe, aegis, ada], None)
        }
        other => panic!("unknown off_scene case {other}"),
    }
}

fn introduced_case(id: &str) -> Vec<Value> {
    match id {
        "mixed-host-and-others" => vec![
            json!({"type":"message","systemSender":"host","systemKind":"off-scene-characters","hostEvent":{"introducedCharacterIds":["c1","c2"]}}),
            json!({"type":"message","systemSender":"aurora","systemKind":"core-whisper"}),
            json!({"type":"message","role":"USER","content":"hi"}),
            json!({"type":"message","systemSender":"host","systemKind":"off-scene-characters","hostEvent":{"introducedCharacterIds":["c2","c3"]}}),
            json!({"type":"message","systemSender":"host","systemKind":"add","hostEvent":{"introducedCharacterIds":["c9"]}}),
        ],
        "empty" => vec![],
        other => panic!("unknown introduced case {other}"),
    }
}

#[allow(clippy::type_complexity)]
fn core_config_case(
    id: &str,
) -> (
    Option<bool>,
    Option<i64>,
    Option<bool>,
    GlobalCoreWhisperSettings,
) {
    match id {
        "all-defaults" => (None, None, None, GlobalCoreWhisperSettings::default()),
        "chat-disables" => (
            Some(false),
            Some(7),
            Some(true),
            GlobalCoreWhisperSettings::default(),
        ),
        "character-disables" => (
            None,
            None,
            Some(false),
            GlobalCoreWhisperSettings::default(),
        ),
        "global-overrides" => (
            None,
            None,
            None,
            GlobalCoreWhisperSettings {
                enabled: Some(false),
                interval: Some(20),
                silence_threshold: Some(5),
                packet_token_budget: Some(2048),
                fire_on_context_transition: Some(false),
            },
        ),
        "chat-null-falls-to-character" => (
            None,
            None,
            Some(true),
            GlobalCoreWhisperSettings {
                enabled: Some(false),
                ..Default::default()
            },
        ),
        other => panic!("unknown core_config case {other}"),
    }
}

fn core_packet_case(id: &str) -> CorePacket {
    match id {
        "own-and-shared" => CorePacket {
            files: vec![
                CoreFile {
                    path: "Core/manifesto.md".into(),
                    body: "I build things.".into(),
                    source_label: None,
                },
                CoreFile {
                    path: "Core/values.md".into(),
                    body: "Kindness above all.".into(),
                    source_label: Some("Household".into()),
                },
            ],
            approx_tokens: 0,
        },
        "single-own" => CorePacket {
            files: vec![CoreFile {
                path: "Core/a.md".into(),
                body: "One file.".into(),
                source_label: None,
            }],
            approx_tokens: 0,
        },
        other => panic!("unknown core_packet case {other}"),
    }
}

fn relevant_block_case(id: &str) -> String {
    // The REAL renderer over the oracle's matches (P4.d13 made it pub —
    // previously this test mirrored the two-line format string inline).
    use quilltap_core::services::memory_recap::{
        render_relevant_conversations_block, VaultConversationMatch,
    };
    let mk =
        |cid: &str, title: &str, rel: &str, score: f64, first: Option<&str>, last: Option<&str>| {
            VaultConversationMatch {
                conversation_id: cid.to_string(),
                conversation_title: title.to_string(),
                relative_path: rel.to_string(),
                score,
                first_message_at: first.map(str::to_string),
                last_message_at: last.map(str::to_string),
            }
        };
    let matches: Vec<VaultConversationMatch> = match id {
        "empty" => vec![],
        "two-entries" => vec![
            mk("cid-1", "A Talk", "x", 0.9, None, None),
            mk("cid-2", "Another", "y", 0.8, None, None),
        ],
        "dated-and-undated" => vec![
            mk(
                "cid-3",
                "The Harbor Visit",
                "z",
                0.9,
                Some("2026-07-14T10:00:00.000Z"),
                Some("2026-07-14T12:00:00.000Z"),
            ),
            mk("cid-4", "Undated", "w", 0.5, None, None),
        ],
        other => panic!("unknown relevant_block case {other}"),
    };
    render_relevant_conversations_block(&matches)
}

fn suparna_case(id: &str) -> String {
    use quilltap_core::post_office::mailbox::DeliveredLetterSummary;
    let letter = |path: &str, from: &str, sent: &str, body: &str| DeliveredLetterSummary {
        path: path.into(),
        from: from.into(),
        sent_at: sent.into(),
        body: body.into(),
        alerted: false,
        in_reply_to: None,
    };
    let letters = match id {
        "empty" => vec![],
        "single" => vec![letter(
            "Mail/friday-2026-02-01.md",
            "Friday",
            "2026-02-01T09:05:00.000Z",
            "  Hello there.  ",
        )],
        "plural-with-blank" => vec![
            letter("Mail/a.md", "Ada", "2026-02-02T09:00:00.000Z", "First."),
            letter("Mail/b.md", "Bob", "2026-02-01T09:00:00.000Z", "   "),
        ],
        other => panic!("unknown suparna case {other}"),
    };
    build_suparna_mail_llm_context(&letters)
}

fn clothing_case(id: &str) -> String {
    match id {
        "trimmed" => cap_clothing_summary(Some(&json!("  a shirt  "))),
        "null" => cap_clothing_summary(Some(&Value::Null)),
        "number" => cap_clothing_summary(Some(&json!(42))),
        "over-200" => cap_clothing_summary(Some(&json!("x".repeat(250)))),
        "exactly-200" => cap_clothing_summary(Some(&json!("y".repeat(200)))),
        other => panic!("unknown clothing case {other}"),
    }
}

#[test]
fn context_feeders_leaves_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CONTEXT_FEEDERS_LEAVES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("QT_ORACLE_CONTEXT_FEEDERS_LEAVES unset; skipping");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let kind = row["kind"].as_str().unwrap();
        let id = row["id"].as_str().unwrap();
        let want = &row["value"];
        let got = rust_value(kind, id);
        assert_eq!(&got, want, "context feeder leaf {kind}/{id} diverges");
        count += 1;
    }
    assert!(count > 0, "oracle had no rows");
    eprintln!("context-feeders-leaves: {count} rows matched");
}
