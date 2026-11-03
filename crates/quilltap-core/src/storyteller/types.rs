//! Storyteller mode — the shapes (v4 `lib/schemas/storyteller.types.ts` +
//! `lib/storyteller/types.ts`, P4.D343 at `236c04e35`).
//!
//! In a storyteller chat each operator post opens a **round**: every invited
//! LLM character is asked, in parallel, for a **bid** (a plan, not a turn), and
//! the operator picks which bids go ahead. The chosen characters then speak,
//! each limited to the **tier** it bid.
//!
//! A round lives in `storyteller_rounds` while it is open; its bids are cleared
//! when it closes (they stay in the LLM log). The bid a speaking turn carried
//! out is stored on that message as `storytellerBid`, which IS part of the
//! record.
//!
//! Serialisation follows v4's Zod output: keys in SCHEMA order; an
//! `.optional().nullable()` field keeps an explicit `null` and drops only an
//! ABSENT value (a tri-state `Option<Option<T>>`), because v4's repository
//! writes the Zod-parsed object — `pendingCard` sets every key, so a stored
//! card carries its `null`s.

use serde::{Deserialize, Serialize};

use crate::db::serde_tristate::double_option;
use crate::tools::effects::ToolEffect;

/// A chat's turn mode (v4 `ChatTurnModeSchema`). NULL in storage reads as
/// `rotation`.
pub const CHAT_TURN_MODES: [&str; 2] = ["rotation", "storyteller"];

/// The tiers a bid may claim, lowest first (v4 `StorytellerTierSchema`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorytellerTier {
    Simple,
    Lookup,
    Act,
    Destroy,
}

/// v4 `STORYTELLER_TIERS`.
pub const STORYTELLER_TIERS: [StorytellerTier; 4] = [
    StorytellerTier::Simple,
    StorytellerTier::Lookup,
    StorytellerTier::Act,
    StorytellerTier::Destroy,
];

impl StorytellerTier {
    pub fn as_str(self) -> &'static str {
        match self {
            StorytellerTier::Simple => "simple",
            StorytellerTier::Lookup => "lookup",
            StorytellerTier::Act => "act",
            StorytellerTier::Destroy => "destroy",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<StorytellerTier> {
        match s {
            "simple" => Some(StorytellerTier::Simple),
            "lookup" => Some(StorytellerTier::Lookup),
            "act" => Some(StorytellerTier::Act),
            "destroy" => Some(StorytellerTier::Destroy),
            _ => None,
        }
    }

    /// v4 `TIER_EFFECT_CEILING` — the effect ceiling each tier grants a
    /// speaking turn.
    pub fn effect_ceiling(self) -> ToolEffect {
        match self {
            StorytellerTier::Simple => ToolEffect::None,
            StorytellerTier::Lookup => ToolEffect::Read,
            StorytellerTier::Act => ToolEffect::Write,
            StorytellerTier::Destroy => ToolEffect::Destroy,
        }
    }

    /// v4 `EFFECT_TIER` — the tier a tool effect requires.
    pub fn for_effect(effect: ToolEffect) -> StorytellerTier {
        match effect {
            ToolEffect::None => StorytellerTier::Simple,
            ToolEffect::Read => StorytellerTier::Lookup,
            ToolEffect::Write => StorytellerTier::Act,
            ToolEffect::Destroy => StorytellerTier::Destroy,
        }
    }

    /// v4 `TIER_LABELS`.
    pub fn label(self) -> &'static str {
        match self {
            StorytellerTier::Simple => "Simple",
            StorytellerTier::Lookup => "Lookup",
            StorytellerTier::Act => "Act",
            StorytellerTier::Destroy => "Destroy",
        }
    }
}

/// What became of one invited seat's bid (v4 `StorytellerBidStatusSchema`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StorytellerBidStatus {
    /// Asked; no answer yet.
    Pending,
    /// Wants to speak.
    Bid,
    /// Declined, or said nothing parseable.
    Pass,
    /// The call failed or was refused — shown as a pass with a reason.
    Error,
    /// Not invited this round (the talkativeness filter); the operator may ask anyway.
    KeptQuiet,
}

impl StorytellerBidStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            StorytellerBidStatus::Pending => "pending",
            StorytellerBidStatus::Bid => "bid",
            StorytellerBidStatus::Pass => "pass",
            StorytellerBidStatus::Error => "error",
            StorytellerBidStatus::KeptQuiet => "kept-quiet",
        }
    }
}

fn ser_js_number<S: serde::Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    crate::db::js_number_to_json(*v).serialize(s)
}

fn is_absent<T>(v: &Option<Option<T>>) -> bool {
    v.is_none()
}

/// One seat's card on the board (v4 `StorytellerBidSchema`, key order kept).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorytellerBid {
    pub participant_id: String,
    pub character_id: String,
    pub character_name: String,
    pub status: StorytellerBidStatus,
    /// The tier the character claimed.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "is_absent"
    )]
    pub stated_tier: Option<Option<StorytellerTier>>,
    /// The tier the server computed: the highest of the claim and the named
    /// tools' effects.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "is_absent"
    )]
    pub tier: Option<Option<StorytellerTier>>,
    /// The named tools put the bid above what the character claimed.
    #[serde(default)]
    pub tier_raised: bool,
    /// Tools the character named, as intent (`run_custom:<name>` for a custom tool).
    #[serde(default)]
    pub tools: Vec<String>,
    /// One or two sentences of what they mean to do. Private to the board.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "is_absent"
    )]
    pub plan: Option<Option<String>>,
    /// Why a pass or error is one.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "is_absent"
    )]
    pub reason: Option<Option<String>>,
    /// The talkativeness the seat was weighed with (serialised as a JS number:
    /// `1`, not `1.0`).
    #[serde(serialize_with = "ser_js_number")]
    pub talkativeness: f64,
    /// Silent seat: may only act or emote.
    #[serde(default)]
    pub silent: bool,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "is_absent"
    )]
    pub asked_at: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "is_absent"
    )]
    pub answered_at: Option<Option<String>>,
}

impl StorytellerBid {
    /// The computed tier, when one is set (v4's `bid.tier` truthiness).
    pub fn tier_value(&self) -> Option<StorytellerTier> {
        self.tier.flatten()
    }
    /// The plan, when one is set.
    pub fn plan_value(&self) -> Option<&str> {
        self.plan.as_ref().and_then(|p| p.as_deref())
    }
}

/// v4 `StorytellerOrderSchema`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorytellerOrder {
    Sequential,
    Simultaneous,
}

impl StorytellerOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            StorytellerOrder::Sequential => "sequential",
            StorytellerOrder::Simultaneous => "simultaneous",
        }
    }
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<StorytellerOrder> {
        match s {
            "sequential" => Some(StorytellerOrder::Sequential),
            "simultaneous" => Some(StorytellerOrder::Simultaneous),
            _ => None,
        }
    }
}

/// v4 `StorytellerSelectionSchema`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorytellerSelection {
    pub participant_ids: Vec<String>,
    pub order: Option<StorytellerOrder>,
    pub chosen_at: String,
    /// Display names in chosen order, kept for the one-line summary after the
    /// bids are cleared.
    #[serde(default)]
    pub names: Vec<String>,
}

/// v4 `StorytellerRoundStatusSchema`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorytellerRoundStatus {
    Open,
    Running,
    Closed,
}

impl StorytellerRoundStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            StorytellerRoundStatus::Open => "open",
            StorytellerRoundStatus::Running => "running",
            StorytellerRoundStatus::Closed => "closed",
        }
    }
}

/// Why a round closed (v4 `closedReason`): run, let lie, superseded by a new
/// post, or abandoned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StorytellerClosedReason {
    Ran,
    LetLie,
    Superseded,
    Abandoned,
}

impl StorytellerClosedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            StorytellerClosedReason::Ran => "ran",
            StorytellerClosedReason::LetLie => "let-lie",
            StorytellerClosedReason::Superseded => "superseded",
            StorytellerClosedReason::Abandoned => "abandoned",
        }
    }
}

/// v4 `StorytellerRoundSchema` (key order kept).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorytellerRound {
    pub id: String,
    pub chat_id: String,
    /// The operator post that opened the round.
    pub trigger_message_id: Option<String>,
    pub status: StorytellerRoundStatus,
    #[serde(default)]
    pub bids: Vec<StorytellerBid>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "is_absent"
    )]
    pub selection: Option<Option<StorytellerSelection>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "is_absent"
    )]
    pub closed_reason: Option<Option<StorytellerClosedReason>>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "is_absent"
    )]
    pub closed_at: Option<Option<String>>,
}

/// The bid a speaking turn carried out, stored on its message
/// (`chat_messages.storytellerBid`; v4 `StorytellerBidRecordSchema`). A swipe
/// or regenerate re-applies it, so a re-roll runs under the same plan. Never
/// model-facing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorytellerBidRecord {
    pub round_id: String,
    pub tier: StorytellerTier,
    pub plan: String,
    #[serde(default)]
    pub tools_named: Vec<String>,
}

/// What the turn path needs to run one storyteller speaking turn (v4
/// `StorytellerTurnContext`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorytellerTurnContext {
    /// The bid this turn carries out; stored on the message.
    pub bid_record: StorytellerBidRecord,
    /// The tier's ceiling.
    pub effect_ceiling: ToolEffect,
    /// Display names of everyone who has spoken since this bid was made
    /// (sequential order).
    pub spoken_since: Vec<String>,
    /// A concurrent turn other than the group's first: it skips the
    /// once-per-cadence public Prospero context announcement, which every turn
    /// of the group would otherwise post from the same message count.
    pub concurrent_follower: bool,
}

/// Asks the turn path to build a bid instead of a turn (v4
/// `StorytellerBidRequest`; dry — no stream, no save).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorytellerBidRequest {
    pub round_id: String,
}

/// The menu a bid prompt offered one character (v4 `StorytellerBidOffer`).
/// v4's two `Record`s keep INSERTION order (the slate's order), which the
/// prompt's sorts make unobservable except through `Object.entries` — kept as
/// ordered vectors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorytellerBidOffer {
    /// Built-in / plugin tool name → highest possible effect.
    pub tools: Vec<(String, ToolEffect)>,
    /// Custom tool name → derived effect.
    pub custom_tools: Vec<(String, ToolEffect)>,
    /// Whether any `destroy` tool was on the menu (the Destroy tier is offered
    /// only then).
    pub destroy_offered: bool,
    pub silent: bool,
}

impl StorytellerBidOffer {
    pub fn tool(&self, name: &str) -> Option<ToolEffect> {
        self.tools.iter().find(|(n, _)| n == name).map(|(_, e)| *e)
    }
    pub fn custom_tool(&self, name: &str) -> Option<ToolEffect> {
        self.custom_tools
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, e)| *e)
    }
}

/// The raw outcome of one bid call, before it becomes a board card (v4
/// `StorytellerBidCallOutcome`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorytellerBidCallOutcome {
    Ok {
        /// The model's text, as returned.
        text: String,
        /// Tool names the provider returned calls for anyway (discarded; v4
        /// never fills it).
        discarded_tool_calls: Vec<String>,
        /// What the bid prompt offered, for computing the tier.
        offered: StorytellerBidOffer,
    },
    Err {
        reason: String,
    },
}

/// A parsed bid reply (v4 `ParsedBid`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBid {
    pub speak: bool,
    pub stated_tier: Option<StorytellerTier>,
    pub tools: Vec<String>,
    pub plan: Option<String>,
}
