//! Tier-1 differential (P4.D343): Storyteller mode's pure modules —
//! `quilltap_core::storyteller::{bid_parse, tiers, cards, invite, prompts,
//! sse_tag, bid_call}` vs v4's REAL `lib/storyteller/*.ts` at the pin
//! `236c04e35` (v4 `b15fe9404`).
//!
//! Covers `parseBidReply` over 50 replies (v4 `bids.test.ts`'s five, fenced /
//! emphasised / reordered / `TOOL:` / `=` / en- and em-dash separators, the
//! synonyms, `run_custom:` + `()` tools, CRLF, a lone CR, U+00A0 / U+2028, a
//! quoted label), `computeBidTier` (the six + destroy claims + unknown tools),
//! `cardFromOutcome` with a fixed `answeredAt`, `inviteProbability`,
//! `biddableSeats` + `chooseInvitees` with a SCRIPTED random sequence (the
//! draw count compared), `describeDisposition` at the boundaries, every byte
//! of `buildBidSection` / `buildPlanSection` (both forms; `spokenSince` 0–3;
//! `toolsNamed` on / off) and the four tier allowances, `tagSseChunk` over
//! multi-frame chunks (v5: `tag_frame` per `data:` line), and `buildBidOffer`.
//! The custom-tool list sorts by `localeCompare` (ICU4X en-US); the read-tool
//! list by code unit.
//!
//! Inputs: the committed `harness/oracle/fixtures/storyteller-pure.json`.
//! Generate the oracle output (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/storyteller-pure.ts \
//!     > /tmp/oracle-storyteller-pure.ndjson
//! Run:
//!   QT_ORACLE_STORYTELLER_PURE=/tmp/oracle-storyteller-pure.ndjson \
//!     cargo test -p quilltap-harness --test storyteller_pure_equivalence -- --nocapture

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use quilltap_core::chat_predicates::ParticipantStatus;
use quilltap_core::select_speaker::SpeakerParticipant;
use quilltap_core::storyteller::bid_call::build_bid_offer;
use quilltap_core::storyteller::bid_parse::parse_bid_reply;
use quilltap_core::storyteller::cards::card_from_outcome;
use quilltap_core::storyteller::invite::{
    biddable_seats, choose_invitees, invite_probability, SeatCharacter,
};
use quilltap_core::storyteller::prompts::{
    build_bid_section, build_plan_section, describe_disposition,
};
use quilltap_core::storyteller::sse_tag::tag_frame;
use quilltap_core::storyteller::tiers::compute_bid_tier;
use quilltap_core::storyteller::types::{
    ParsedBid, StorytellerBid, StorytellerBidCallOutcome, StorytellerBidOffer, StorytellerTier,
};
use quilltap_core::tools::effects::ToolEffect;
use serde_json::{json, Map, Value};

fn spec() -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/storyteller-pure.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("spec")).expect("spec json")
}

fn effect(v: &Value) -> ToolEffect {
    ToolEffect::from_str(v.as_str().unwrap()).unwrap()
}

fn pairs(v: &Value) -> Vec<(String, ToolEffect)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_str().unwrap().to_string(), effect(&p[1])))
        .collect()
}

fn offer_of(v: &Value) -> StorytellerBidOffer {
    StorytellerBidOffer {
        tools: pairs(&v["tools"]),
        custom_tools: pairs(&v["customTools"]),
        destroy_offered: v["destroyOffered"].as_bool().unwrap(),
        silent: v["silent"].as_bool().unwrap(),
    }
}

fn tier_json(t: Option<StorytellerTier>) -> Value {
    t.map_or(Value::Null, |t| json!(t.as_str()))
}

fn parsed_json(p: &ParsedBid) -> Value {
    let mut m = Map::new();
    m.insert("speak".into(), json!(p.speak));
    m.insert("statedTier".into(), tier_json(p.stated_tier));
    m.insert("tools".into(), json!(p.tools));
    m.insert("plan".into(), json!(p.plan));
    Value::Object(m)
}

fn parsed_of(v: &Value) -> ParsedBid {
    ParsedBid {
        speak: v["speak"].as_bool().unwrap(),
        stated_tier: v["statedTier"].as_str().and_then(StorytellerTier::from_str),
        tools: serde_json::from_value(v["tools"].clone()).unwrap(),
        plan: v["plan"].as_str().map(String::from),
    }
}

fn status_of(s: &str) -> ParticipantStatus {
    quilltap_core::chat_predicates::participant_status_from_str(Some(s))
}

/// JS `JSON.stringify` writes an integral number without a fraction (`1`, not
/// serde's `1.0`); normalise v5's floats so the byte compare sees numbers the
/// way v4 wrote them.
fn js_numbers(v: Value) -> Value {
    match v {
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 9.0e15 => json!(f as i64),
            _ => Value::Number(n),
        },
        Value::Array(a) => Value::Array(a.into_iter().map(js_numbers).collect()),
        Value::Object(m) => Value::Object(m.into_iter().map(|(k, v)| (k, js_numbers(v))).collect()),
        other => other,
    }
}

#[test]
fn storyteller_pure_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_STORYTELLER_PURE") else {
        eprintln!("SKIP: QT_ORACLE_STORYTELLER_PURE not set");
        return;
    };
    let spec = spec();
    let offers: BTreeMap<String, StorytellerBidOffer> = spec["offers"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), offer_of(v)))
        .collect();
    let participants: Vec<SpeakerParticipant> = spec["seats"]["participants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| SpeakerParticipant {
            id: p["id"].as_str().unwrap().into(),
            participant_type: p["type"].as_str().unwrap().into(),
            status: status_of(p["status"].as_str().unwrap()),
            character_id: p["characterId"].as_str().map(String::from),
            controlled_by: p["controlledBy"].as_str().unwrap().into(),
            talkativeness: p["talkativeness"].as_f64(),
        })
        .collect();
    let characters: HashMap<String, SeatCharacter> = spec["seats"]["characters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c[0].as_str().unwrap().to_string(),
                SeatCharacter {
                    name: c[1]["name"].as_str().unwrap().into(),
                    talkativeness: c[1]["talkativeness"].as_f64(),
                    archived: !c[1]["archivedAt"].is_null(),
                },
            )
        })
        .collect();
    let impersonating: Vec<String> =
        serde_json::from_value(spec["seats"]["impersonating"].clone()).unwrap();
    let seats = biddable_seats(&participants, &characters, Some(&impersonating));

    let text = std::fs::read_to_string(&path).expect("oracle");
    let mut failures = Vec::new();
    let mut kinds = BTreeMap::<String, usize>::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap().to_string();
        let id = row["id"].as_str().unwrap().to_string();
        let got: Value = match kind.as_str() {
            "parse" => parse_bid_reply(row["raw"].as_str().unwrap())
                .map_or(Value::Null, |p| parsed_json(&p)),
            "tier" => {
                let c = compute_bid_tier(
                    &parsed_of(&row["parsed"]),
                    &offers[row["offer"].as_str().unwrap()],
                );
                json!({"tier": c.tier.as_str(), "raised": c.raised, "tools": c.tools, "unknownTools": c.unknown_tools})
            }
            "card" => {
                let base: StorytellerBid =
                    serde_json::from_value(spec["cardBase"].clone()).unwrap();
                let o = &row["outcome"];
                let outcome = if o["ok"].as_bool().unwrap() {
                    StorytellerBidCallOutcome::Ok {
                        text: o["text"].as_str().unwrap().into(),
                        discarded_tool_calls: Vec::new(),
                        offered: offers[row["offer"].as_str().unwrap()].clone(),
                    }
                } else {
                    StorytellerBidCallOutcome::Err {
                        reason: o["reason"].as_str().unwrap().into(),
                    }
                };
                serde_json::to_value(card_from_outcome(
                    &base,
                    &outcome,
                    spec["cardAnsweredAt"].as_str().unwrap(),
                ))
                .unwrap()
            }
            "inviteProbability" => {
                json!(invite_probability(row["talkativeness"].as_f64().unwrap()))
            }
            "seats" => Value::Array(
                seats
                    .iter()
                    .map(|s| {
                        json!({"participantId": s.participant.id, "characterName": s.character_name, "talkativeness": s.talkativeness, "silent": s.silent})
                    })
                    .collect(),
            ),
            "invite" => {
                let draws: Vec<f64> = serde_json::from_value(row["draws"].clone()).unwrap();
                let mut i = 0;
                let mut random = || {
                    i += 1;
                    draws[i - 1]
                };
                let (inv, quiet) = choose_invitees(
                    seats.clone(),
                    row["filter"].as_bool().unwrap(),
                    &mut random,
                );
                json!({
                    "invited": inv.iter().map(|s| s.participant.id.clone()).collect::<Vec<_>>(),
                    "keptQuiet": quiet.iter().map(|s| s.participant.id.clone()).collect::<Vec<_>>(),
                    "consumed": i,
                })
            }
            "disposition" => json!(describe_disposition(row["talkativeness"].as_f64().unwrap())),
            "bidSection" => json!(build_bid_section(
                row["characterName"].as_str().unwrap(),
                row["talkativeness"].as_f64().unwrap(),
                &offers[row["offer"].as_str().unwrap()],
            )),
            "planSection" => {
                let p = &row["params"];
                let v = |k: &str| -> Vec<String> { serde_json::from_value(p[k].clone()).unwrap() };
                json!(build_plan_section(
                    p["plan"].as_str().unwrap(),
                    StorytellerTier::from_str(p["tier"].as_str().unwrap()).unwrap(),
                    &v("toolsNamed"),
                    &v("spokenSince"),
                    &v("allowedTools"),
                    p["reroll"].as_bool().unwrap(),
                ))
            }
            "allowance" => {
                let tools: Vec<String> = serde_json::from_value(row["tools"].clone()).unwrap();
                let section = build_plan_section(
                    "p",
                    StorytellerTier::from_str(row["tier"].as_str().unwrap()).unwrap(),
                    &[],
                    &[],
                    &tools,
                    false,
                );
                json!(section.rsplit('\n').next().unwrap())
            }
            "sse" => {
                // v4 `tagSseChunk` re-encodes every `data: <json>` line whose
                // JSON is a non-array object; v5 tags the frame VALUE
                // (`tag_frame`). Lines are split on `\n` (no other terminator
                // in the corpus).
                let chunk = row["chunk"].as_str().unwrap();
                let tagged: Vec<String> = chunk
                    .split('\n')
                    .map(|l| match l.strip_prefix("data: ") {
                        Some(j) => match serde_json::from_str::<Value>(j) {
                            Ok(v @ Value::Object(_)) => format!(
                                "data: {}",
                                serde_json::to_string(&tag_frame(v, "p-tag")).unwrap()
                            ),
                            _ => l.to_string(),
                        },
                        None => l.to_string(),
                    })
                    .collect();
                json!(tagged.join("\n"))
            }
            "offer" => {
                let s = spec["slates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["id"] == row["id"])
                    .unwrap();
                let tools: Vec<Value> = s["tools"].as_array().unwrap().clone();
                let custom: BTreeMap<String, ToolEffect> =
                    pairs(&s["customToolEffects"]).into_iter().collect();
                let plugin: BTreeMap<String, ToolEffect> =
                    pairs(&s["pluginToolEffects"]).into_iter().collect();
                let o = build_bid_offer(&tools, &custom, &plugin, s["silent"].as_bool().unwrap());
                let p = |v: &[(String, ToolEffect)]| {
                    v.iter().map(|(n, e)| json!([n, e.as_str()])).collect::<Vec<_>>()
                };
                json!({"tools": p(&o.tools), "customTools": p(&o.custom_tools), "destroyOffered": o.destroy_offered, "silent": o.silent})
            }
            other => panic!("unknown kind {other}"),
        };
        let got = js_numbers(got);
        *kinds.entry(kind.clone()).or_default() += 1;
        let (g, w) = (
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&row["out"]).unwrap(),
        );
        if g != w {
            failures.push(format!("{kind}/{id}:\n  v5: {g}\n  v4: {w}"));
        }
    }
    eprintln!("storyteller_pure_equivalence: {kinds:?}");
    assert!(
        kinds.get("parse").copied().unwrap_or(0) >= 40,
        "stale oracle"
    );
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
