//! Storyteller mode (v4 `lib/storyteller/` + `lib/schemas/storyteller.types.ts`,
//! P4.D343 at `236c04e35`): the shapes and the pure modules — who is invited,
//! the bid prompt and its parser, the tier a bid earns, the board card, the
//! plan section a chosen turn carries, the concurrent-frame tag, the bid's
//! tool menu. The round lifecycle is `services::storyteller_rounds`; the
//! repository `db::storyteller_rounds`.
//!
//! v4's two `AsyncLocalStorage` twins are not ported: `round-context.ts`
//! becomes the explicit `LogContext::storyteller_round_id`, and `dry-scope.ts`
//! becomes the `ContextPurpose::Bid` gates plus the bid path's own writes-free
//! build (ruling R-1, `DRY_SCOPE_BY_GATES`).

pub mod bid_call;
pub mod bid_parse;
pub mod cards;
pub mod invite;
pub mod prompts;
pub mod tiers;
pub mod types;
