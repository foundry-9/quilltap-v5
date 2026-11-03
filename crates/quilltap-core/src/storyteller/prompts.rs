//! Storyteller mode — the two trailing prompt sections (v4
//! `lib/storyteller/prompts.ts`, P4.D343).
//!
//! - The **bid** section asks a character whether it wants to speak and what it
//!   plans to do, in the labelled format `bid_parse` reads.
//! - The **plan** section rides on the speaking turn the operator chose: what
//!   the character planned, who has spoken since, that the plan is private, and
//!   which tools this turn allows.
//!
//! Both are trailing sections — appended after recall, progressions and the
//! inform block, never in the system prefix — so a bid shares the character's
//! cached prefix with the speaking turn it may become. Both are ephemeral: they
//! ride in the outgoing request only and are never persisted.

use super::tiers::CUSTOM_TOOL_PREFIX;
use super::types::{StorytellerBidOffer, StorytellerTier};
use crate::skip_signal::NOTHING_TO_ADD_SENTINEL;
use crate::tools::effects::ToolEffect;

const NOTE_HEADER: &str = "[Turn note from the Salon — not spoken by any character]";

/// Talkativeness (0.1–1.0) in plain words (v4 `describeDisposition`).
pub fn describe_disposition(talkativeness: f64) -> &'static str {
    if talkativeness < 0.3 {
        return "You are reserved: you speak when you have something real to add, and are content to let others carry a moment.";
    }
    if talkativeness < 0.6 {
        return "You are even-tempered: you speak when the moment invites you, and let it pass when it does not.";
    }
    if talkativeness < 0.85 {
        return "You are forthcoming: you are usually glad to take part.";
    }
    "You are very talkative: you rarely let a moment pass without a word."
}

/// v4 `namesWithEffect` — the menu's tool names with that effect, sorted by
/// UTF-16 code unit (JS `Array.prototype.sort` with no comparator).
fn names_with_effect(offer: &StorytellerBidOffer, effect: ToolEffect) -> Vec<String> {
    let mut names: Vec<String> = offer
        .tools
        .iter()
        .filter(|(_, e)| *e == effect)
        .map(|(n, _)| n.clone())
        .collect();
    names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    names
}

/// The bid section — the last thing a bid call's model reads (v4
/// `buildBidSection`).
pub fn build_bid_section(
    character_name: &str,
    talkativeness: f64,
    offer: &StorytellerBidOffer,
) -> String {
    let read_tools = names_with_effect(offer, ToolEffect::Read);
    // `localeCompare` — the ICU collator (en-US), stable.
    let mut custom_entries = offer.custom_tools.clone();
    custom_entries.sort_by(|(a, _), (b, _)| crate::collation::locale_compare(a, b));

    let mut tiers: Vec<String> = vec![
        "- simple: just speak or act in character, using no tools.".to_string(),
        if read_tools.is_empty() {
            "- lookup: look things up first with read-only tools, then speak.".to_string()
        } else {
            format!(
                "- lookup: look things up first with read-only tools, then speak. Read-only tools: {}.",
                read_tools.join(", ")
            )
        },
        "- act: do something that changes the world (write a document, change clothes, move an object, send a letter, run a tool with effects), then speak.".to_string(),
    ];
    if offer.destroy_offered {
        tiers.push(format!(
            "- destroy: delete something ({}). Only when deletion is truly the point.",
            names_with_effect(offer, ToolEffect::Destroy).join(", ")
        ));
    }

    let mut lines: Vec<String> = vec![
        NOTE_HEADER.to_string(),
        "This is not your turn yet. The storyteller is asking each character whether they want to respond to what just happened, and will choose who goes ahead. Do not write your reply now — answer only with your bid.".to_string(),
        String::new(),
        describe_disposition(talkativeness).to_string(),
    ];
    if offer.silent {
        lines.push(
            "You are silent in this scene: if you go ahead, you may only act or emote, not speak aloud."
                .to_string(),
        );
    }
    lines.push(String::new());
    lines.push("How much do you mean to do? Pick one tier:".to_string());
    lines.extend(tiers);
    if !custom_entries.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Custom tools you can run (name them as {CUSTOM_TOOL_PREFIX}<name>), with what each does to the world:"
        ));
        for (name, effect) in &custom_entries {
            lines.push(format!(
                "- {name}: {}",
                if *effect == ToolEffect::Read {
                    "read (lookup tier)"
                } else {
                    "write (act tier)"
                }
            ));
        }
    }
    lines.push(String::new());
    lines.push("Reply with exactly these four lines and nothing else:".to_string());
    lines.push("BID: speak or pass".to_string());
    lines.push(format!(
        "TIER: {}",
        if offer.destroy_offered {
            "simple, lookup, act or destroy"
        } else {
            "simple, lookup or act"
        }
    ));
    lines.push("TOOLS: the tools you plan to use, comma-separated, or none".to_string());
    lines.push(format!(
        "PLAN: one or two sentences, in {character_name}'s own words, of what you mean to say or do"
    ));
    lines.push(String::new());
    lines.push("Pass if you have nothing worth adding. Do not call any tool now.".to_string());
    lines.join("\n")
}

/// The plan section — rides on the speaking turn the operator chose (v4
/// `buildPlanSection`). `reroll`: a swipe re-rolls a line that was already
/// chosen and spoken — it runs with no tools and may not pass, so neither is
/// offered.
pub fn build_plan_section(
    plan: &str,
    tier: StorytellerTier,
    tools_named: &[String],
    spoken_since: &[String],
    allowed_tools: &[String],
    reroll: bool,
) -> String {
    if reroll {
        return [
            NOTE_HEADER.to_string(),
            "The storyteller chose you to go ahead. When you were asked, you planned this:".to_string(),
            String::new(),
            format!("\"{plan}\""),
            String::new(),
            "The plan is private: do not announce it, quote it, or mention that you made one. You may adapt it to the moment.".to_string(),
        ]
        .join("\n");
    }
    let mut lines: Vec<String> = vec![
        NOTE_HEADER.to_string(),
        "The storyteller chose you to go ahead. When you were asked, you planned this:".to_string(),
        String::new(),
        format!("\"{plan}\""),
        String::new(),
        if spoken_since.is_empty() {
            "Nobody has spoken since you planned it.".to_string()
        } else {
            format!(
                "Since you planned it, {} {} spoken. Read what they said before you answer.",
                join_names(spoken_since),
                if spoken_since.len() == 1 { "has" } else { "have" }
            )
        },
        "The plan is private: do not announce it, quote it, or mention that you made one.".to_string(),
        "You may adapt it to the moment. If the moment for it has gone, reply with exactly this single line and nothing else:".to_string(),
        String::new(),
        NOTHING_TO_ADD_SENTINEL.to_string(),
        String::new(),
        tier_allowance(tier, allowed_tools),
    ];
    if !tools_named.is_empty() {
        lines.push(format!("You mentioned: {}.", tools_named.join(", ")));
    }
    lines.join("\n")
}

/// v4 `tierAllowance`.
pub fn tier_allowance(tier: StorytellerTier, allowed_tools: &[String]) -> String {
    let label = tier.label();
    if tier == StorytellerTier::Simple {
        return format!("Your bid was {label}: this turn allows no tools. Just speak or act.");
    }
    let list = if allowed_tools.is_empty() {
        String::new()
    } else {
        format!(" Tools available: {}.", allowed_tools.join(", "))
    };
    match tier {
        StorytellerTier::Lookup => format!(
            "Your bid was {label}: this turn allows read-only tools; anything that changes the world will be refused.{list}"
        ),
        StorytellerTier::Act => format!(
            "Your bid was {label}: this turn allows tools that look things up or change the world, but not deletion.{list}"
        ),
        _ => format!("Your bid was {label}: this turn allows every tool, deletion included.{list}"),
    }
}

/// v4 `joinNames`: one name as is; else `a, b and c`.
fn join_names(names: &[String]) -> String {
    if names.len() <= 1 {
        return names.join("");
    }
    format!(
        "{} and {}",
        names[..names.len() - 1].join(", "),
        names[names.len() - 1]
    )
}
