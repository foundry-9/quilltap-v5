//! Storyteller — turning one bid call's outcome into a board card (v4
//! `lib/storyteller/cards.ts`, P4.D343).
//!
//! Pure: a failed call is an `error` card, an unreadable reply or a declined
//! bid a `pass`, and a bid to speak a `bid` card whose tier the server computed
//! from the tools it named (`tiers`).

use super::bid_parse::parse_bid_reply;
use super::tiers::compute_bid_tier;
use super::types::{StorytellerBid, StorytellerBidCallOutcome, StorytellerBidStatus};

/// v4 `cardFromOutcome(base, outcome, answeredAt)`. `answered_at` is the
/// caller's clock reading (v4 defaults it to `new Date().toISOString()`).
pub fn card_from_outcome(
    base: &StorytellerBid,
    outcome: &StorytellerBidCallOutcome,
    answered_at: &str,
) -> StorytellerBid {
    let mut card = base.clone();
    card.answered_at = Some(Some(answered_at.to_string()));
    let text = match outcome {
        StorytellerBidCallOutcome::Err { reason } => {
            card.status = StorytellerBidStatus::Error;
            card.reason = Some(Some(reason.clone()));
            return card;
        }
        StorytellerBidCallOutcome::Ok { text, .. } => text,
    };
    let Some(parsed) = parse_bid_reply(text) else {
        card.status = StorytellerBidStatus::Pass;
        card.reason = Some(Some("No bid could be read from the reply".to_string()));
        return card;
    };
    if !parsed.speak {
        card.status = StorytellerBidStatus::Pass;
        card.reason = Some(if parsed.plan.is_some() {
            None
        } else {
            Some("Passed".to_string())
        });
        card.plan = Some(parsed.plan);
        return card;
    }
    let StorytellerBidCallOutcome::Ok { offered, .. } = outcome else {
        unreachable!("the error arm returned above");
    };
    let computed = compute_bid_tier(&parsed, offered);
    card.status = StorytellerBidStatus::Bid;
    card.stated_tier = Some(parsed.stated_tier);
    card.tier = Some(Some(computed.tier));
    card.tier_raised = computed.raised;
    card.tools = computed.tools;
    card.plan = Some(Some(
        parsed.plan.unwrap_or_else(|| "(no plan given)".to_string()),
    ));
    card.reason = Some(if computed.unknown_tools.is_empty() {
        None
    } else {
        Some(format!(
            "Named tools it does not have: {}",
            computed.unknown_tools.join(", ")
        ))
    });
    card
}
