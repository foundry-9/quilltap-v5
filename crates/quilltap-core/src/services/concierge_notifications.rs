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
//! The three REFUSAL announcements (v4 `8bd080267`) carry `systemKind:
//! 'refusal'`; the auto-switch's `auto-flagged-refusals` bubble (v4 `49059fb14`)
//! is a manual kind and stays `danger`.
//!
//! Errors never propagate — the danger-classification job (and the manual flip)
//! must never fail because an announcement couldn't be written. A failure is
//! surfaced to the caller as `None` (v4 logs + returns null).
//!
//! These close the W4.2 `postConciergeDangerAnnouncement` /
//! `postConciergeManualAnnouncement` seams
//! (`services::dangerous_content::manual_flip::ConciergeAnnouncer` — since v4
//! `4d370a90f` the classifier's danger announcement is posted by the flip
//! too); the parent wires the seam impls. The category labels reuse
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

/// The Concierge's state-transition announcements (v4 `ConciergeManualKind`,
/// phase 3 — `4d370a90f`, #75): `set-moderated` / `set-unmoderated` /
/// `set-locked` / `auto-unmoderated`.
///
/// The SIX retired kinds of the four-state control (`manual-flagged`,
/// `manual-safe`, `manual-vouched`, `manual-resumed`, `manual-uncensored`,
/// `auto-flagged-refusals`) are still DECODED ([`Self::from_wire`]) and still
/// have their bodies: v4 — "Transcripts written before phase 3 carry bubbles of
/// the retired kinds … they are plain messages and render as they always did".
/// v5 must never EMIT one again: `apply_concierge_flip` names only the four
/// live wire strings, and `concierge_retired_kinds_census` pins that no
/// production file outside this one constructs a retired variant.
/// (`set-unmoderated`'s persona text is byte-identical to the retired
/// `manual-uncensored` — a corpus that diffs CONTENT cannot tell them apart;
/// diff the kind.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConciergeManualKind {
    /// → Moderated (the operator; resets the ledger).
    SetModerated,
    /// → Unmoderated (the operator opens the uncensored door themselves).
    SetUnmoderated,
    /// → Locked (the operator; ordinary providers only, refusals stand).
    SetLocked,
    /// Moderated → Unmoderated by the Concierge, after N stated moderation
    /// refusals (the refusal ledger's auto-switch).
    AutoUnmoderated,
    /// RETIRED at `4d370a90f` (read-only). → Flagged by the operator.
    ManualFlagged,
    /// RETIRED at `4d370a90f` (read-only). Flagged → Monitored.
    ManualSafe,
    /// RETIRED at `4d370a90f` (read-only). → Vouched Safe.
    ManualVouched,
    /// RETIRED at `4d370a90f` (read-only). Vouched/Uncensored → Monitored.
    ManualResumed,
    /// RETIRED at `4d370a90f` (read-only). → Uncensored.
    ManualUncensored,
    /// RETIRED at `4d370a90f` (read-only). Monitored → Flagged by the
    /// Concierge (`49059fb14`'s auto-switch, renamed `auto-unmoderated`).
    AutoFlaggedRefusals,
}

impl ConciergeManualKind {
    /// Parse the wire form — the four live kinds AND the six retired ones (an
    /// old transcript's kind must still decode). Unknown → `None`.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "set-moderated" => Some(Self::SetModerated),
            "set-unmoderated" => Some(Self::SetUnmoderated),
            "set-locked" => Some(Self::SetLocked),
            "auto-unmoderated" => Some(Self::AutoUnmoderated),
            "manual-flagged" => Some(Self::ManualFlagged),
            "manual-safe" => Some(Self::ManualSafe),
            "manual-vouched" => Some(Self::ManualVouched),
            "manual-resumed" => Some(Self::ManualResumed),
            "manual-uncensored" => Some(Self::ManualUncensored),
            "auto-flagged-refusals" => Some(Self::AutoFlaggedRefusals),
            _ => None,
        }
    }

    /// The wire form.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::SetModerated => "set-moderated",
            Self::SetUnmoderated => "set-unmoderated",
            Self::SetLocked => "set-locked",
            Self::AutoUnmoderated => "auto-unmoderated",
            Self::ManualFlagged => "manual-flagged",
            Self::ManualSafe => "manual-safe",
            Self::ManualVouched => "manual-vouched",
            Self::ManualResumed => "manual-resumed",
            Self::ManualUncensored => "manual-uncensored",
            Self::AutoFlaggedRefusals => "auto-flagged-refusals",
        }
    }

    /// Is this one of the six kinds phase 3 retired (decoded, never emitted)?
    pub fn is_retired(self) -> bool {
        !matches!(
            self,
            Self::SetModerated | Self::SetUnmoderated | Self::SetLocked | Self::AutoUnmoderated
        )
    }
}

/// What the Concierge says about the refusals that earned an auto-switch (v4
/// `ConciergeAutoFlagDetails`, `49059fb14`). `auto-unmoderated` only (and the
/// retired `auto-flagged-refusals` it renamed).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConciergeAutoFlagDetails {
    /// Refusals on the ledger when the switch fired.
    pub count: i64,
    /// Provider of the most recent refusal (e.g. `GOOGLE`); `""` when unknown.
    pub last_provider: String,
    pub last_model: Option<String>,
}

const TIMES_WORDS: [&str; 11] = [
    "Never",
    "Once",
    "Twice",
    "Three times",
    "Four times",
    "Five times",
    "Six times",
    "Seven times",
    "Eight times",
    "Nine times",
    "Ten times",
];
const COUNT_WORDS: [&str; 11] = [
    "No", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
];

/// v4 `refusalWho`: `None` when there is no (truthy) provider; else the
/// provider, and its model when that is truthy.
fn refusal_who(details: Option<&ConciergeAutoFlagDetails>) -> Option<String> {
    let d = details?;
    if d.last_provider.is_empty() {
        return None;
    }
    Some(match d.last_model.as_deref().filter(|m| !m.is_empty()) {
        Some(model) => format!("{} {model}", d.last_provider),
        None => d.last_provider.clone(),
    })
}

/// v4 `buildAutoFlagContent` (`49059fb14`). A threshold of one is stated
/// plainly; `TIMES_WORDS` only for 2..=10; everything else — INCLUDING a count
/// of 0 (`details?.count ?? 0`) — is "More than once now".
pub fn build_auto_flag_content(details: Option<&ConciergeAutoFlagDetails>) -> String {
    let count = details.map_or(0, |d| d.count);
    let who = refusal_who(details);
    let declined = if count == 1 {
        format!(
            "The house's regular staff have declined this conversation on grounds of propriety{}.",
            who.as_ref()
                .map(|w| format!(" — {w}, to be precise"))
                .unwrap_or_default()
        )
    } else {
        let tally = if (2..11).contains(&count) {
            format!("{} now", TIMES_WORDS[count as usize])
        } else {
            "More than once now".to_string()
        };
        format!(
            "{tally} the house's regular staff have declined this conversation on grounds of propriety{}.",
            who.as_ref().map(|w| format!(" — most recently {w}")).unwrap_or_default()
        )
    };
    format!(
        "{declined} The Concierge has taken the liberty of moving the whole affair to the uncensored desk; you may set it back to Moderated from the sidebar whenever you wish."
    )
}

/// v4 `buildAutoFlagOpaqueContent` (`49059fb14`): `COUNT_WORDS` for 1..=10,
/// else the number (so 0 reads `0`, not `No`); the noun singular only at 1.
pub fn build_auto_flag_opaque_content(details: Option<&ConciergeAutoFlagDetails>) -> String {
    let count = details.map_or(0, |d| d.count);
    let counted = if (1..11).contains(&count) {
        COUNT_WORDS[count as usize].to_string()
    } else {
        count.to_string()
    };
    let noun = if count == 1 {
        "moderation refusal"
    } else {
        "moderation refusals"
    };
    let last = refusal_who(details)
        .map(|w| format!(" (last: {w})"))
        .unwrap_or_default();
    format!(
        "{counted} {noun}{last}. The Concierge switched this chat to Unmoderated; change it in the sidebar."
    )
}

/// Persona-voiced manual-transition body (v4 `buildManualContent`) — the
/// no-details form; `auto-flagged-refusals` without details reads v4's
/// `details === undefined` sentence.
pub fn build_manual_content(kind: ConciergeManualKind) -> String {
    build_manual_content_with_details(kind, None)
}

/// v4 `buildManualContent(kind, details?)` (`49059fb14`).
pub fn build_manual_content_with_details(
    kind: ConciergeManualKind,
    details: Option<&ConciergeAutoFlagDetails>,
) -> String {
    match kind {
        ConciergeManualKind::SetModerated =>
            "By the operator's own hand, the conversation is Moderated once more. The house's usual providers are asked first; should one of them decline a matter on grounds of propriety, the Concierge will quietly take it across the street. His ledger of refusals is wiped clean.",
        ConciergeManualKind::SetUnmoderated =>
            "By the operator's own hand, the Concierge has been sent away and the uncensored door stands open. Nothing is to be examined, nothing softened; the conversation and its errands go henceforth to the frank desk, entirely on the operator's own recognizance.",
        ConciergeManualKind::SetLocked =>
            "The operator has locked the present company to the house's usual desks. Should one of them decline a matter, the refusal stands: the Concierge will take nothing elsewhere, nor move the conversation of his own accord.",
        ConciergeManualKind::AutoUnmoderated => return build_auto_flag_content(details),
        // The retired kinds keep their pre-phase-3 bodies (read-only).
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
        ConciergeManualKind::AutoFlaggedRefusals => return build_auto_flag_content(details),
    }
    .to_string()
}

/// Opaque-audience manual-transition advisory (v4 `buildManualOpaqueContent`)
/// — the no-details form.
pub fn build_manual_opaque_content(kind: ConciergeManualKind) -> String {
    build_manual_opaque_content_with_details(kind, None)
}

/// v4 `buildManualOpaqueContent(kind, details?)` (`49059fb14`).
pub fn build_manual_opaque_content_with_details(
    kind: ConciergeManualKind,
    details: Option<&ConciergeAutoFlagDetails>,
) -> String {
    match kind {
        ConciergeManualKind::SetModerated =>
            "Operator advisory: this conversation is Moderated. Ordinary providers are asked first; a content refusal is retried on an uncensored provider.",
        ConciergeManualKind::SetUnmoderated =>
            "Operator advisory: this conversation has been manually set to Unmoderated. It goes to the uncensored providers only; no classification or scanning will run, and prompts go out unaltered.",
        ConciergeManualKind::SetLocked =>
            "Operator advisory: this conversation is Locked to the ordinary providers. A content refusal stands; nothing is rerouted and the Concierge will not switch the chat.",
        ConciergeManualKind::AutoUnmoderated => {
            return build_auto_flag_opaque_content(details)
        }
        // The retired kinds keep their pre-phase-3 bodies (read-only).
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
        ConciergeManualKind::AutoFlaggedRefusals => {
            return build_auto_flag_opaque_content(details)
        }
    }
    .to_string()
}

/// The three refusal announcements (v4 `ConciergeRefusalKind`, `8bd080267`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConciergeRefusalKind {
    RefusalRerouted,
    RefusalNoUnderstudy,
    RefusalNotPermitted,
}

impl ConciergeRefusalKind {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::RefusalRerouted => "refusal-rerouted",
            Self::RefusalNoUnderstudy => "refusal-no-understudy",
            Self::RefusalNotPermitted => "refusal-not-permitted",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "refusal-rerouted" => Some(Self::RefusalRerouted),
            "refusal-no-understudy" => Some(Self::RefusalNoUnderstudy),
            "refusal-not-permitted" => Some(Self::RefusalNotPermitted),
            _ => None,
        }
    }
}

/// What was being made when the refusal happened (v4
/// `ConciergeRefusalPurpose`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConciergeRefusalPurpose {
    Tool,
    Lantern,
    Avatar,
    Dialog,
    Text,
}

impl ConciergeRefusalPurpose {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Lantern => "lantern",
            Self::Avatar => "avatar",
            Self::Dialog => "dialog",
            Self::Text => "text",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "tool" => Some(Self::Tool),
            "lantern" => Some(Self::Lantern),
            "avatar" => Some(Self::Avatar),
            "dialog" => Some(Self::Dialog),
            "text" => Some(Self::Text),
            _ => None,
        }
    }

    /// v4 `refusalCommission(purpose)` → `(voiced, plain)`.
    fn commission(self) -> (&'static str, &'static str) {
        match self {
            Self::Tool | Self::Dialog => ("the commission for a picture", "an image request"),
            Self::Lantern => ("the commission for a new backdrop", "a story background"),
            Self::Avatar => ("the commission for a new portrait", "a character portrait"),
            Self::Text => ("the request for a reply", "this turn"),
        }
    }
}

/// v4 `ConciergeRefusalDetails`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConciergeRefusalDetails {
    /// Provider of the profile that refused (e.g. `OPENAI`).
    pub refusing_provider: String,
    /// Model of the profile that refused.
    pub refusing_model: String,
    /// Name the user gave the answering profile — `refusal-rerouted` only.
    pub answering_profile_name: Option<String>,
    pub purpose: ConciergeRefusalPurpose,
    /// `refusal-not-permitted` only: what barred the reroute (v4 `4d370a90f`,
    /// #75) — since `3b463d6b1` (#76) only ever Locked, and the sentences no
    /// longer read it (an off-duty Concierge announces nothing at all).
    pub reason: Option<ConciergeRefusalBar>,
}

/// What barred a reroute (v4 `ConciergeRefusalDetails.reason?: 'locked'`).
/// #75 (`4d370a90f`) had a second value, `'mode'`, with its own two sentences;
/// #76 (`3b463d6b1`) narrowed the domain to `'locked'` and DELETED them — the
/// Concierge's mode is gone, and off duty he announces nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConciergeRefusalBar {
    /// The chat is Locked.
    Locked,
}

impl ConciergeRefusalBar {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Locked => "locked",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "locked" => Some(Self::Locked),
            _ => None,
        }
    }
}

/// v4 `buildRefusalContent` (`8bd080267`; the rerouted sentence LOST its last
/// sentence "The result is attached above." at `49059fb14` — the PR #73 review
/// fix carried into phase 2).
pub fn build_refusal_content(
    kind: ConciergeRefusalKind,
    details: &ConciergeRefusalDetails,
) -> String {
    let painter = format!("{} {}", details.refusing_provider, details.refusing_model);
    let (voiced, _) = details.purpose.commission();
    let house = if details.purpose == ConciergeRefusalPurpose::Text {
        "the house's usual correspondent"
    } else {
        "the house's usual painter"
    };
    match kind {
        ConciergeRefusalKind::RefusalRerouted => format!(
            "The Concierge regrets to report that {house} ({painter}) declined {voiced} on grounds of propriety; he has taken it across the street to {}, who were happy to oblige.",
            details
                .answering_profile_name
                .as_deref()
                .unwrap_or("a more obliging studio")
        ),
        ConciergeRefusalKind::RefusalNoUnderstudy => format!(
            "The Concierge regrets to report that {house} ({painter}) declined {voiced} on grounds of propriety, and he knows of no more obliging establishment to take it to. Should you care to name one, tick \"Uncensored-compatible\" on a suitable profile, or choose one in the Concierge's settings."
        ),
        // v4 `3b463d6b1` (#76): the one remaining bar is a Locked chat.
        ConciergeRefusalKind::RefusalNotPermitted => format!(
            "The Concierge observes that {house} ({painter}) declined {voiced} on grounds of propriety. This conversation is Locked to the usual desks, so the refusal stands; set it to Moderated should you wish him to take such things elsewhere."
        ),
    }
}

/// v4 `buildRefusalOpaqueContent` (`8bd080267`).
pub fn build_refusal_opaque_content(
    kind: ConciergeRefusalKind,
    details: &ConciergeRefusalDetails,
) -> String {
    let who = format!("{} {}", details.refusing_provider, details.refusing_model);
    let (_, plain) = details.purpose.commission();
    match kind {
        ConciergeRefusalKind::RefusalRerouted => format!(
            "Provider {who} refused {plain} on content grounds. The Concierge rerouted it to {}.",
            details
                .answering_profile_name
                .as_deref()
                .unwrap_or("an uncensored profile")
        ),
        ConciergeRefusalKind::RefusalNoUnderstudy => format!(
            "Provider {who} refused {plain} on content grounds. No uncensored profile is available to retry it; mark a profile \"Uncensored-compatible\" or choose one in the Concierge settings."
        ),
        ConciergeRefusalKind::RefusalNotPermitted => format!(
            "Provider {who} refused {plain} on content grounds. This chat is Locked, so it was not rerouted; set it to Moderated to allow an uncensored retry."
        ),
    }
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
    post_concierge_message_kind(db, chat_id, content, opaque_content, "danger")
        .await
        .ok()
        .flatten()
}

/// The shared post primitive with the `systemKind` named — `danger` for the
/// classifier verdict and the transitions, `refusal` for the three refusal
/// announcements (v4 `8bd080267`). `Ok(None)` = the chat is missing; `Err` =
/// the read or the write failed (the message, for the caller's ERROR line).
async fn post_concierge_message_kind(
    db: &Db,
    chat_id: &str,
    content: String,
    opaque_content: String,
    system_kind: &str,
) -> Result<Option<Value>, String> {
    // v4: `if (!chat) return null;`
    let cid = chat_id.to_string();
    let exists = db
        .read_main(move |conn| crate::db::chats_read::find_by_id(conn, &cid))
        .map_err(|e| e.to_string())?
        .is_some();
    if !exists {
        return Ok(None);
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
        "systemKind": system_kind,
    });

    let event: ChatEventInput =
        serde_json::from_value(message.clone()).map_err(|e| e.to_string())?;
    let cid = chat_id.to_string();
    db.write(move |writers| writers.main().chat_messages().add_message(&cid, &event))
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(message))
}

/// Post a Concierge manual-transition announcement (v4
/// `postConciergeManualAnnouncement`). Returns the posted message (for callers
/// that splice it into the current turn) or `None` on failure.
pub async fn post_concierge_manual_announcement(
    db: &Db,
    chat_id: &str,
    kind: ConciergeManualKind,
) -> Option<Value> {
    post_concierge_manual_announcement_with_details(db, chat_id, kind, None).await
}

/// v4 `postConciergeManualAnnouncement({ chatId, kind, details })`
/// (`49059fb14`) — `details` threads the auto-switch's tally into the
/// `auto-flagged-refusals` bubble; every other kind ignores it.
pub async fn post_concierge_manual_announcement_with_details(
    db: &Db,
    chat_id: &str,
    kind: ConciergeManualKind,
    details: Option<&ConciergeAutoFlagDetails>,
) -> Option<Value> {
    let content = build_manual_content_with_details(kind, details);
    let opaque_content = build_manual_opaque_content_with_details(kind, details);
    // v4's two lines here (`Manual transition announced` / `Failed to post
    // manual announcement`) predate `49059fb14` and were never ported; the
    // image-failover family's auto-switch arm surfaced the absence. A missing
    // chat stays silent, as v4's early `return null` is.
    match post_concierge_message_kind(db, chat_id, content, opaque_content, "danger").await {
        Ok(None) => None,
        Ok(Some(message)) => {
            let message_id = message.get("id").and_then(Value::as_str).unwrap_or("");
            tracing::info!(
                target: "quilltap::concierge_notification",
                context = "concierge-notifications",
                chat_id = %chat_id,
                message_id = %message_id,
                kind = kind.as_wire(),
                "[ConciergeNotification] Manual transition announced"
            );
            Some(message)
        }
        Err(error) => {
            tracing::error!(
                target: "quilltap::concierge_notification",
                context = "concierge-notifications",
                chat_id = %chat_id,
                kind = kind.as_wire(),
                error = %error,
                "[ConciergeNotification] Failed to post manual announcement"
            );
            None
        }
    }
}

/// Post a Concierge refusal announcement (v4
/// `postConciergeRefusalAnnouncement`, `8bd080267`): `systemKind: 'refusal'`,
/// the three log lines, NO dedupe (every refusal is its own bubble). Never
/// fails the caller: a missing chat is a DEBUG and `None`, a failed write an
/// ERROR and `None`.
pub async fn post_concierge_refusal_announcement(
    db: &Db,
    chat_id: &str,
    kind: ConciergeRefusalKind,
    details: &ConciergeRefusalDetails,
) -> Option<Value> {
    let content = build_refusal_content(kind, details);
    let opaque_content = build_refusal_opaque_content(kind, details);
    match post_concierge_message_kind(db, chat_id, content, opaque_content, "refusal").await {
        Ok(None) => {
            tracing::debug!(
                target: "quilltap::concierge_notification",
                context = "concierge-notifications",
                chat_id = %chat_id,
                kind = kind.as_wire(),
                "[ConciergeNotification] Refusal announcement skipped: chat not found"
            );
            None
        }
        Ok(Some(message)) => {
            let message_id = message
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            tracing::info!(
                target: "quilltap::concierge_notification",
                context = "concierge-notifications",
                chat_id = %chat_id,
                message_id = %message_id,
                kind = kind.as_wire(),
                purpose = details.purpose.as_wire(),
                refusing_provider = %details.refusing_provider,
                refusing_model = %details.refusing_model,
                answering_profile_name = details.answering_profile_name.as_deref(),
                reason = details.reason.map(ConciergeRefusalBar::as_wire),
                "[ConciergeNotification] Refusal announced"
            );
            Some(message)
        }
        Err(error) => {
            tracing::error!(
                target: "quilltap::concierge_notification",
                context = "concierge-notifications",
                chat_id = %chat_id,
                kind = kind.as_wire(),
                error = %error,
                "[ConciergeNotification] Failed to post refusal announcement"
            );
            None
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::runtime::DbPaths;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c1000000-0000-4000-8000-0000000000e7";
    const NOW: &str = "2026-09-25T00:00:00.000Z";

    fn provisioned() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("rs");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    async fn seed_chat(db: &Db) {
        let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
            "userId": crate::api::SINGLE_USER_ID,
            "title": "The Refusal Room",
            "participants": [],
        }))
        .expect("a ChatCreate");
        let opts = crate::db::chats::CreateOptions {
            id: CHAT.to_string(),
            created_at: NOW.to_string(),
            updated_at: NOW.to_string(),
        };
        db.write(move |w| w.main().chats().create(&create, &opts).map(|_| ()))
            .await
            .unwrap();
    }

    fn details() -> ConciergeRefusalDetails {
        ConciergeRefusalDetails {
            refusing_provider: "OPENAI".into(),
            refusing_model: "gpt-image-2".into(),
            answering_profile_name: Some("Frank Desk".into()),
            purpose: ConciergeRefusalPurpose::Tool,
            reason: None,
        }
    }

    fn run<T>(f: impl std::future::Future<Output = T>) -> (T, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        crate::test_support::captured_with(|| rt.block_on(f))
    }

    /// P4.D226 Tier 2 item 13 (v4 `4d370a90f`: "Transcripts written before
    /// phase 3 carry bubbles of the retired kinds … they are plain messages and
    /// render as they always did"). A transcript holding one bubble of each of
    /// the six retired kinds — written exactly as the pre-phase-3 writer wrote
    /// them — reads back through the transcript read every consumer shares
    /// (`get_messages`, under the chat GET and the export) in order, with
    /// content, opaque content, sender and kind unchanged.
    #[test]
    fn the_six_retired_kinds_read_back_unchanged() {
        let (_dir, db) = provisioned();
        let retired = [
            ConciergeManualKind::ManualFlagged,
            ConciergeManualKind::ManualSafe,
            ConciergeManualKind::ManualVouched,
            ConciergeManualKind::ManualResumed,
            ConciergeManualKind::ManualUncensored,
            ConciergeManualKind::AutoFlaggedRefusals,
        ];
        assert!(retired.iter().all(|k| k.is_retired()));
        let (posted, _) = run(async {
            seed_chat(&db).await;
            let mut posted = Vec::new();
            for kind in retired {
                posted.push(
                    post_concierge_manual_announcement(&db, CHAT, kind)
                        .await
                        .expect("a retired bubble posts (a pre-phase-3 transcript)"),
                );
            }
            posted
        });
        let read = db
            .read_main(|c| crate::db::chats_messages_read::get_messages(c, CHAT))
            .unwrap();
        let got: Vec<&Value> = read
            .iter()
            .filter(|m| m["systemSender"] == "concierge")
            .collect();
        assert_eq!(got.len(), 6, "every retired bubble reads back: {read:#?}");
        for ((kind, want), got) in retired.iter().zip(&posted).zip(got) {
            assert_eq!(got["id"], want["id"], "{kind:?}: order");
            assert_eq!(got["content"], build_manual_content(*kind), "{kind:?}");
            assert_eq!(
                got["opaqueContent"],
                build_manual_opaque_content(*kind),
                "{kind:?}"
            );
            assert_eq!(got["systemKind"], "danger", "{kind:?}");
            assert_eq!(got["role"], "ASSISTANT", "{kind:?}");
        }
    }

    /// v4 `postConciergeRefusalAnnouncement` (`8bd080267`): the message literal
    /// IN v4's ORDER with `systemKind: 'refusal'`, persisted and read back, and
    /// the ONE INFO line with v4's bag (an absent answerer is an absent field).
    #[test]
    fn a_refusal_announcement_posts_the_refusal_literal_and_logs_once() {
        let (_dir, db) = provisioned();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(seed_chat(&db));
        drop(rt);

        let (message, lines) = run(post_concierge_refusal_announcement(
            &db,
            CHAT,
            ConciergeRefusalKind::RefusalRerouted,
            &details(),
        ));
        let message = message.expect("posted");
        let keys: Vec<&str> = message
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "type",
                "id",
                "role",
                "content",
                "opaqueContent",
                "attachments",
                "createdAt",
                "participantId",
                "systemSender",
                "systemKind"
            ]
        );
        assert_eq!(message["systemKind"], "refusal");
        assert_eq!(message["systemSender"], "concierge");
        assert_eq!(
            message["content"],
            build_refusal_content(ConciergeRefusalKind::RefusalRerouted, &details())
        );
        let id = message["id"].as_str().unwrap().to_string();
        assert_eq!(
            lines,
            vec![format!(
                "INFO quilltap::concierge_notification [ConciergeNotification] Refusal announced context=concierge-notifications chat_id={CHAT} message_id={id} kind=refusal-rerouted purpose=tool refusing_provider=OPENAI refusing_model=gpt-image-2 answering_profile_name=Frank Desk"
            )]
        );

        let stored = db
            .read_main(|conn| crate::db::chats_messages_read::get_messages(conn, CHAT))
            .unwrap();
        let row = stored
            .iter()
            .find(|m| m["id"] == id.as_str())
            .expect("persisted");
        assert_eq!(row["systemKind"], "refusal");
        assert_eq!(row["opaqueContent"], message["opaqueContent"]);

        // NO dedupe: the same refusal twice is two bubbles.
        let (again, _) = run(post_concierge_refusal_announcement(
            &db,
            CHAT,
            ConciergeRefusalKind::RefusalRerouted,
            &details(),
        ));
        assert!(again.is_some());
        let n = db
            .read_main(|conn| crate::db::chats_messages_read::get_messages(conn, CHAT))
            .unwrap()
            .iter()
            .filter(|m| m["systemKind"] == "refusal")
            .count();
        assert_eq!(n, 2);
    }

    /// The two other exits: a missing chat is ONE DEBUG and `None`; a failed
    /// write is ONE ERROR and `None` (the table broken — the
    /// `force-a-swallowed-catch-by-breaking-the-table` idiom). Neither says
    /// "Refusal announced".
    #[test]
    fn a_missing_chat_debugs_and_a_failed_write_errors() {
        let (_dir, db) = provisioned();
        let d = ConciergeRefusalDetails {
            answering_profile_name: None,
            ..details()
        };
        let (none, lines) = run(post_concierge_refusal_announcement(
            &db,
            CHAT,
            ConciergeRefusalKind::RefusalNotPermitted,
            &d,
        ));
        assert!(none.is_none());
        assert_eq!(
            lines,
            vec![format!(
                "DEBUG quilltap::concierge_notification [ConciergeNotification] Refusal announcement skipped: chat not found context=concierge-notifications chat_id={CHAT} kind=refusal-not-permitted"
            )]
        );

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(seed_chat(&db));
        rt.block_on(db.write(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE chat_messages")?;
            Ok(())
        }))
        .unwrap();
        drop(rt);
        let (none, lines) = run(post_concierge_refusal_announcement(
            &db,
            CHAT,
            ConciergeRefusalKind::RefusalNoUnderstudy,
            &d,
        ));
        assert!(none.is_none());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with(
            "ERROR quilltap::concierge_notification [ConciergeNotification] Failed to post refusal announcement context=concierge-notifications"
        ));
        assert!(lines[0].contains(" kind=refusal-no-understudy error="));
    }

    /// The manual announcement's three exits (v4's two lines, ported late —
    /// the image-failover family surfaced their absence): a missing chat is
    /// SILENT (v4's early `return null`), a post is ONE INFO with the minted id,
    /// a failed write is ONE ERROR.
    #[test]
    fn a_manual_announcement_logs_v4s_two_lines() {
        let (_dir, db) = provisioned();
        let (none, lines) = run(post_concierge_manual_announcement(
            &db,
            CHAT,
            ConciergeManualKind::ManualVouched,
        ));
        assert!(none.is_none());
        assert!(lines.is_empty(), "a missing chat is silent: {lines:?}");

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(seed_chat(&db));
        drop(rt);
        let (posted, lines) = run(post_concierge_manual_announcement(
            &db,
            CHAT,
            ConciergeManualKind::ManualVouched,
        ));
        let id = posted.expect("posted")["id"].as_str().unwrap().to_string();
        assert_eq!(
            lines,
            vec![format!(
                "INFO quilltap::concierge_notification [ConciergeNotification] Manual transition announced context=concierge-notifications chat_id={CHAT} message_id={id} kind=manual-vouched"
            )]
        );

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(db.write(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE chat_messages")?;
            Ok(())
        }))
        .unwrap();
        drop(rt);
        let (none, lines) = run(post_concierge_manual_announcement(
            &db,
            CHAT,
            ConciergeManualKind::ManualResumed,
        ));
        assert!(none.is_none());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with(
            "ERROR quilltap::concierge_notification [ConciergeNotification] Failed to post manual announcement context=concierge-notifications"
        ));
        assert!(lines[0].contains(" kind=manual-resumed error="));
    }

    /// The auto-switch bubble is a MANUAL kind (`systemKind: 'danger'`), and its
    /// details reach the post (E.7).
    #[test]
    fn the_auto_flag_bubble_is_a_danger_row_carrying_its_tally() {
        let (_dir, db) = provisioned();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(seed_chat(&db));
        let tally = ConciergeAutoFlagDetails {
            count: 2,
            last_provider: "GOOGLE".into(),
            last_model: Some("gemini-3-pro".into()),
        };
        let posted = rt
            .block_on(post_concierge_manual_announcement_with_details(
                &db,
                CHAT,
                ConciergeManualKind::AutoFlaggedRefusals,
                Some(&tally),
            ))
            .expect("posted");
        assert_eq!(posted["systemKind"], "danger");
        assert_eq!(posted["content"], build_auto_flag_content(Some(&tally)));
        assert!(posted["content"]
            .as_str()
            .unwrap()
            .starts_with("Twice now the house's regular staff"));
    }
}
