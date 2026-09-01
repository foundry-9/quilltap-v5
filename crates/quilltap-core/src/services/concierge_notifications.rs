//! Writer for Concierge chat notifications — v4
//! `lib/services/concierge-notifications/writer.ts`.
//!
//! When the gatekeeper classifies a chat as dangerous, this helper injects a
//! synthetic ASSISTANT-role chat message announcing the Concierge's quiet
//! intervention. Characters at the table see — through the avatar of the
//! Concierge, in discreet language — that the conversation has been marked for
//! handling by more appropriate providers. The manual variants speak to
//! operator-driven flips of the per-chat Concierge state.
//!
//! Both posts are tagged `systemSender: 'concierge'` / `systemKind: 'danger'`
//! (the manual variant reuses the same `systemKind` — see v4's object literals).
//!
//! Errors never propagate — the danger-classification job (and the manual flip)
//! must never fail because an announcement couldn't be written. A failure is
//! surfaced to the caller as `None` (v4 logs + returns null).
//!
//! These close the W4.2 `postConciergeDangerAnnouncement` /
//! `postConciergeManualAnnouncement` seams (`services::dangerous_content::{
//! gatekeeper_job::DangerAnnouncer, manual_flip::ConciergeAnnouncer}`); the
//! parent wires the seam impls. The category labels reuse
//! [`crate::services::dangerous_content::gatekeeper::category_label`] (v4's
//! `CATEGORY_LABELS`), and `.toFixed(2)` uses [`crate::jsnum::to_fixed`].

use serde_json::{json, Value};

use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::jsnum::to_fixed;
use crate::services::dangerous_content::gatekeeper::category_label;

/// One classifier category (v4 `ConciergeDangerDetails.categories` element). The
/// `label` is the provider-supplied label, used only when the category is absent
/// from `CATEGORY_LABELS`.
#[derive(Debug, Clone)]
pub struct ConciergeCategory {
    pub category: String,
    pub score: f64,
    pub label: Option<String>,
}

/// Classifier details attached to a Concierge danger announcement (v4
/// `ConciergeDangerDetails`). `source` is `"moderation"` (a dedicated moderation
/// provider) or `"llm"` (the cheap-LLM fallback).
#[derive(Debug, Clone)]
pub struct ConciergeDangerDetails {
    /// Overall danger score (0–1) returned by the classifier.
    pub score: f64,
    /// Threshold at which the classifier flips to "dangerous."
    pub threshold: f64,
    /// Per-category breakdown from the classifier.
    pub categories: Vec<ConciergeCategory>,
    /// `"moderation"` for a dedicated moderation provider; `"llm"` for the
    /// cheap-LLM fallback.
    pub source: Option<String>,
    /// Provider that performed the classification (e.g. `"OPENAI"`).
    pub provider_name: Option<String>,
}

/// A ranked category with its resolved display label (v4 `RankedCategory`).
struct RankedCategory {
    score: f64,
    label: String,
}

/// v4's label precedence: `CATEGORY_LABELS[category] || providedLabel || category`
/// (the `||` treats an empty string as absent). `category_label` returns the raw
/// category when it is not in `CATEGORY_LABELS`, so an unmapped category falls
/// through to the provider label, then the category itself.
fn resolve_label(category: &str, provided: Option<&str>) -> String {
    let mapped = category_label(category);
    if mapped != category {
        return mapped.to_string();
    }
    match provided.filter(|s| !s.is_empty()) {
        Some(l) => l.to_string(),
        None => category.to_string(),
    }
}

fn rank_categories(details: &ConciergeDangerDetails) -> Vec<RankedCategory> {
    let named: Vec<RankedCategory> = details
        .categories
        .iter()
        .map(|c| RankedCategory {
            score: c.score,
            label: resolve_label(&c.category, c.label.as_deref()),
        })
        .collect();

    let crossing: Vec<RankedCategory> = details
        .categories
        .iter()
        .zip(named.iter())
        .filter(|(c, _)| c.score >= details.threshold)
        .map(|(_, r)| RankedCategory {
            score: r.score,
            label: r.label.clone(),
        })
        .collect();

    let mut ranked = if crossing.is_empty() { named } else { crossing };
    // v4: `.sort((a, b) => b.score - a.score)` — stable descending. Rust's
    // `sort_by` is stable, so ties preserve input order.
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked.truncate(3);
    ranked
}

fn format_category_phrase(categories: &[RankedCategory], score_word: &str) -> String {
    if categories.is_empty() {
        return String::new();
    }
    if categories.len() == 1 {
        let c = &categories[0];
        return format!("{} ({} {})", c.label, score_word, to_fixed(c.score, 2));
    }
    let head = categories[..categories.len() - 1]
        .iter()
        .map(|c| format!("{} ({} {})", c.label, score_word, to_fixed(c.score, 2)))
        .collect::<Vec<_>>()
        .join(", ");
    let tail = &categories[categories.len() - 1];
    format!(
        "{head} and {} ({} {})",
        tail.label,
        score_word,
        to_fixed(tail.score, 2)
    )
}

/// Bare noun phrase naming the classifier that rendered the verdict, e.g.
/// "the house's OPENAI moderation assayer" or "the cheap-LLM assayer, OPENAI".
/// `""` when no provider is known (v4 `assayerNounPhrase`).
fn assayer_noun_phrase(details: &ConciergeDangerDetails) -> String {
    let Some(provider) = details.provider_name.as_deref() else {
        return String::new();
    };
    if details.source.as_deref() == Some("moderation") {
        format!("the house's {provider} moderation assayer")
    } else {
        format!("the cheap-LLM assayer, {provider}")
    }
}

fn format_assayer(details: &ConciergeDangerDetails) -> String {
    let noun = assayer_noun_phrase(details);
    if noun.is_empty() {
        String::new()
    } else {
        format!(" (per {noun})")
    }
}

/// Persona-voiced danger announcement body (v4 `buildDangerContent`).
pub fn build_danger_content(details: Option<&ConciergeDangerDetails>) -> String {
    let opener = "The Concierge, with his customary discretion, has stepped quietly to the table.";
    let closer =
        "He has arranged for the present conversation — and any adjunct errands it may occasion — \
         to be entrusted to a desk better appointed to subjects of its particular character. \
         No interruption is required; pray continue at your leisure.";

    let Some(details) = details else {
        return format!("{opener} {closer}");
    };

    let ranked = rank_categories(details);
    let phrase = format_category_phrase(&ranked, "severity");
    let overall = to_fixed(details.score, 2);
    let threshold = to_fixed(details.threshold, 2);
    let assayer = format_assayer(details);
    let crossed_threshold = details.score >= details.threshold;

    let specifics = if crossed_threshold {
        if phrase.is_empty() {
            format!(
                "The matter, on close inspection, registered {overall} against the present threshold of {threshold}{assayer}."
            )
        } else {
            format!(
                "The matter that drew his eye: {phrase} — together registering {overall} against the present threshold of {threshold}{assayer}."
            )
        }
    } else {
        let verdict_by = {
            let noun = assayer_noun_phrase(details);
            if noun.is_empty() {
                "the house assayer".to_string()
            } else {
                noun
            }
        };
        if phrase.is_empty() {
            format!(
                "The matter was marked by the direct verdict of {verdict_by}, the severities notwithstanding — they stayed shy of the present threshold of {threshold} (registering {overall})."
            )
        } else {
            format!(
                "The matter was marked by the direct verdict of {verdict_by}: {phrase}. The severities themselves stayed shy of the present threshold of {threshold} (the highest reading {overall}) — it was the assayer's own judgement, not the tally, that drew his eye."
            )
        }
    };

    format!("{opener} {specifics} {closer}")
}

/// Opaque-audience danger advisory body (v4 `buildDangerOpaqueContent`).
pub fn build_danger_opaque_content(details: Option<&ConciergeDangerDetails>) -> String {
    let opener =
        "Content advisory: the present conversation — and any adjunct operations it occasions — \
         has been routed to a provider better suited to subjects of its particular character.";
    let closer = "No interruption is required; proceed at your leisure.";

    let Some(details) = details else {
        return format!("{opener} {closer}");
    };

    let ranked = rank_categories(details);
    let triggers = format_category_phrase(&ranked, "score");
    let overall = to_fixed(details.score, 2);
    let threshold = to_fixed(details.threshold, 2);
    let provider_via = match details.provider_name.as_deref() {
        Some(provider) => {
            let kind = if details.source.as_deref() == Some("moderation") {
                "moderation endpoint"
            } else {
                "cheap-LLM fallback"
            };
            format!("{provider} ({kind})")
        }
        None => "the classifier".to_string(),
    };
    let crossed_threshold = details.score >= details.threshold;

    let specifics = if crossed_threshold {
        let via = if details.provider_name.is_some() {
            format!(" Classified by {provider_via}.")
        } else {
            String::new()
        };
        if triggers.is_empty() {
            format!("Overall score {overall} against threshold {threshold}.{via}")
        } else {
            format!(
                "Triggers: {triggers}. Overall score {overall} against threshold {threshold}.{via}"
            )
        }
    } else if triggers.is_empty() {
        format!(
            "Flagged directly by {provider_via}, below the numeric threshold. Overall score {overall}, threshold {threshold} (not reached)."
        )
    } else {
        format!(
            "Flagged directly by {provider_via}, below the numeric threshold. Triggers: {triggers}. Highest score {overall}, threshold {threshold} (not reached)."
        )
    };

    format!("{opener} {specifics} {closer}")
}

/// The five operator-driven Concierge transitions (v4 `ConciergeManualKind`,
/// widened from four at v4 `60e3c4a0a` for the four-state control).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConciergeManualKind {
    /// → Flagged (the operator flipped the switch themselves).
    ManualFlagged,
    /// Flagged → Monitored (the operator says all clear).
    ManualSafe,
    /// anything → Vouched Safe (operator vouches; the Concierge stops watching).
    ManualVouched,
    /// Vouched/Uncensored → Monitored (operator calls the Concierge back).
    ManualResumed,
    /// anything → Uncensored (operator opens the uncensored door themselves).
    ManualUncensored,
}

impl ConciergeManualKind {
    /// Parse the wire form (`"manual-flagged"` / `"manual-safe"` /
    /// `"manual-vouched"` / `"manual-resumed"` / `"manual-uncensored"`) the
    /// `ConciergeAnnouncer` seam passes, for the parent's wiring. Unknown →
    /// `None`.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "manual-flagged" => Some(Self::ManualFlagged),
            "manual-safe" => Some(Self::ManualSafe),
            "manual-vouched" => Some(Self::ManualVouched),
            "manual-resumed" => Some(Self::ManualResumed),
            "manual-uncensored" => Some(Self::ManualUncensored),
            _ => None,
        }
    }

    /// The wire form.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::ManualFlagged => "manual-flagged",
            Self::ManualSafe => "manual-safe",
            Self::ManualVouched => "manual-vouched",
            Self::ManualResumed => "manual-resumed",
            Self::ManualUncensored => "manual-uncensored",
        }
    }
}

/// Persona-voiced manual-transition body (v4 `buildManualContent`).
pub fn build_manual_content(kind: ConciergeManualKind) -> String {
    match kind {
        ConciergeManualKind::ManualFlagged =>
            "By the operator's own hand, the Concierge has thrown the switch: the conversation is to be entrusted henceforth to a desk better appointed to subjects of its particular character. Pray continue at your leisure.",
        ConciergeManualKind::ManualSafe =>
            "By the operator's own hand, the Concierge stands down for the moment. Routine arrangements are restored; he shall, of course, return to his post should the matter again take a turn.",
        ConciergeManualKind::ManualVouched =>
            "The operator has vouched for the present company, and the Concierge, satisfied, takes the afternoon off. No moderation, no rerouting, no quiet interventions; the ordinary desks remain in service, on the operator's own recognizance.",
        ConciergeManualKind::ManualResumed =>
            "The Concierge returns to his post. Customary watch is resumed; the present arrangements are once again subject to his discreet attentions.",
        ConciergeManualKind::ManualUncensored =>
            "By the operator's own hand, the Concierge has been sent away and the uncensored door stands open. Nothing is to be examined, nothing softened; the conversation and its errands go henceforth to the frank desk, entirely on the operator's own recognizance.",
    }
    .to_string()
}

/// Opaque-audience manual-transition advisory (v4 `buildManualOpaqueContent`).
pub fn build_manual_opaque_content(kind: ConciergeManualKind) -> String {
    match kind {
        ConciergeManualKind::ManualFlagged =>
            "Operator advisory: this conversation has been manually marked for handling by an uncensored provider. Subsequent traffic may be routed accordingly.",
        ConciergeManualKind::ManualSafe =>
            "Operator advisory: the prior dangerous-content mark has been manually cleared. Standard routing is restored.",
        ConciergeManualKind::ManualVouched =>
            "Operator advisory: moderation is disabled for this conversation. No classification, scanning, or provider rerouting will run on the operator’s behalf. Ordinary providers still apply.",
        ConciergeManualKind::ManualResumed =>
            "Operator advisory: standard moderation is restored for this conversation.",
        ConciergeManualKind::ManualUncensored =>
            "Operator advisory: this conversation has been manually routed to the uncensored providers. No classification or scanning will run; prompts go out unaltered.",
    }
    .to_string()
}

/// Shared post primitive: read the chat (missing → `None`), mint `id` +
/// `createdAt`, assemble the exact v4 `MessageEvent` object literal
/// (`systemSender: 'concierge'` / `systemKind: 'danger'`, ASSISTANT role,
/// `participantId: null`, non-opaque honoring an explicit `opaqueContent`), and
/// insert it through the writer. Returns the built message (`Some`) or `None` on
/// any failure (v4 catches, logs, returns null).
async fn post_concierge_message(
    db: &Db,
    chat_id: &str,
    content: String,
    opaque_content: String,
) -> Option<Value> {
    // v4: `if (!chat) return null;`
    let cid = chat_id.to_string();
    let exists = db
        .read_main(move |conn| crate::db::chats_read::find_by_id(conn, &cid))
        .ok()?
        .is_some();
    if !exists {
        return None;
    }

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content,
        "opaqueContent": opaque_content,
        "attachments": [],
        "createdAt": now,
        "participantId": Value::Null,
        "systemSender": "concierge",
        "systemKind": "danger",
    });

    let event: ChatEventInput = serde_json::from_value(message.clone()).ok()?;
    let cid = chat_id.to_string();
    match db
        .write(move |writers| writers.main().chat_messages().add_message(&cid, &event))
        .await
    {
        Ok(()) => Some(message),
        Err(_) => None,
    }
}

/// Post a Concierge manual-transition announcement (v4
/// `postConciergeManualAnnouncement`). Returns the posted message (for callers
/// that splice it into the current turn) or `None` on failure.
pub async fn post_concierge_manual_announcement(
    db: &Db,
    chat_id: &str,
    kind: ConciergeManualKind,
) -> Option<Value> {
    let content = build_manual_content(kind);
    let opaque_content = build_manual_opaque_content(kind);
    post_concierge_message(db, chat_id, content, opaque_content).await
}

/// Post a Concierge danger announcement (v4 `postConciergeDangerAnnouncement`).
/// Returns the posted message or `None` on failure.
pub async fn post_concierge_danger_announcement(
    db: &Db,
    chat_id: &str,
    details: Option<&ConciergeDangerDetails>,
) -> Option<Value> {
    let content = build_danger_content(details);
    let opaque_content = build_danger_opaque_content(details);
    post_concierge_message(db, chat_id, content, opaque_content).await
}
