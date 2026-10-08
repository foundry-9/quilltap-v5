//! Image failover — the one chokepoint every image call site sends its
//! provider call through (port of v4 `lib/services/dangerous-content/
//! image-failover.ts`, NEW at `8bd080267`, #73; the ledger at `49059fb14`,
//! #74).
//!
//! The Salon's `generate_image` tool, the Lantern's story backgrounds, Aurora's
//! avatar job and the legacy image dialog each used to carry their own
//! try/catch reroute with its own detection and its own gate. Now each owns
//! only what is profile-specific — building the parameters, LoRA trigger
//! phrases, the LLM-log line — inside an `attempt` closure, and this module
//! owns the rest:
//!
//!   1. ask the primary;
//!   2. on a failure, [`classify_refusal`]. Not a refusal → return the error
//!      untouched (a rate limit is not the Concierge's business);
//!   3. a refusal → record it on the trail, and on the chat's refusal ledger
//!      ([`record_moderation_refusal`]) once the outcome is known — whether or
//!      not the reroute later succeeds;
//!   4. the chat may not fail over (it is Locked — re-read NOW, v4
//!      `4d370a90f`) → announce `refusal-not-permitted` with `reason: locked`,
//!      fail; the Concierge is off duty (the policy's `failover_allowed || on_duty`,
//!      re-asked against the CURRENT switch — v4 `3b463d6b1`) → the ledger, no
//!      announcement, fail;
//!   5. ask the understudy resolver (excluding the primary). Nobody → announce
//!      `refusal-no-understudy`, fail (a caller that reports unresolved
//!      refusals itself — the Lantern — passes `announce_unresolved_refusal:
//!      false` and the chokepoint stays silent in steps 4 and 5; v4
//!      `ce2f1dabf`, #77);
//!   6. ask the understudy once. It answers → announce `refusal-rerouted` and
//!      return; it fails → record its own verdict and fail.
//!
//! Every failure after step 2 carries the trail ([`ImageFailoverError::trail`],
//! v4's `conciergeTrail` on the thrown error), so a caller that surfaces the
//! failure can still show what was tried. The pre-flight classifier reroutes
//! stay where they are; this is the post-hoc half, and it asks the same
//! resolver they do.
//!
//! **v4's `attachTrail` wrap arm has no analogue**: v4 sets `conciergeTrail` on
//! the thrown object and only wraps a frozen / non-object throw in a fresh
//! `Error(getErrorMessage(e))`. v5's error is a value that always carries its
//! trail, so the message is the original error's either way.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::llm_fallback::{classify_fallback_trigger, FallbackError, FallbackTrigger};
use crate::model::image::ImageGenError;
use crate::services::concierge_notifications::{
    post_concierge_refusal_announcement, ConciergeRefusalBar, ConciergeRefusalDetails,
    ConciergeRefusalKind, ConciergeRefusalPurpose,
};
use crate::services::route_trail::{
    truncate_detail, RouteAttempt, RouteAttemptOutcome, RouteAttemptVia, RouteProfileKind,
};

use super::chat_override::{concierge_state_may_fail_over, get_concierge_state};
use super::current_state::{read_current_concierge_on_duty, read_current_concierge_state};
use super::provider_routing::ApiKeyResolver;
use super::refusal::{classify_refusal, RefusalInput, RefusalVerdict};
use super::refusal_ledger::{
    record_moderation_refusal, RefusalKind, RefusalPurpose, RefusalRecord,
};
use super::resolver::ResolvedConciergePolicy;
use super::understudy::{resolve_image_understudy_on, ImageUnderstudyLookup};

const TARGET: &str = "quilltap::concierge_image_failover";

/// v4 `FailoverProfile` — the fields of a profile the trail and the
/// announcement read — plus the profile's whole stored `row`, which the call
/// site's `attempt` needs to build its parameters for whichever profile it is
/// handed (v4 passes the whole `ImageProfile` / `ConnectionProfile`).
#[derive(Clone, Debug, PartialEq)]
pub struct FailoverProfile {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_name: String,
    pub row: Value,
}

impl FailoverProfile {
    /// Read the four fields off a stored profile row.
    pub fn from_row(row: &Value) -> Self {
        let s = |k: &str| row.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        Self {
            id: s("id"),
            name: s("name"),
            provider: s("provider"),
            model_name: s("modelName"),
            row: row.clone(),
        }
    }
}

/// The purposes an image call serves (v4's `'tool' | 'lantern' | 'avatar' |
/// 'dialog'`), each with its announcement and its ledger spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImagePurpose {
    Tool,
    Lantern,
    Avatar,
    Dialog,
    /// v4 `'wardrobe'` (`image-failover.ts:68`, `7c8572869`) — the wardrobe
    /// item picture. Its generation passes no chat, so its announcement and
    /// ledger spellings are unreachable through v4's real code (P4.D263 R-B).
    Wardrobe,
}

impl ImagePurpose {
    pub fn as_str(self) -> &'static str {
        self.announcement().as_wire()
    }

    fn announcement(self) -> ConciergeRefusalPurpose {
        match self {
            ImagePurpose::Tool => ConciergeRefusalPurpose::Tool,
            ImagePurpose::Lantern => ConciergeRefusalPurpose::Lantern,
            ImagePurpose::Avatar => ConciergeRefusalPurpose::Avatar,
            ImagePurpose::Dialog => ConciergeRefusalPurpose::Dialog,
            ImagePurpose::Wardrobe => ConciergeRefusalPurpose::Wardrobe,
        }
    }

    fn ledger(self) -> RefusalPurpose {
        match self {
            ImagePurpose::Tool => RefusalPurpose::Tool,
            ImagePurpose::Lantern => RefusalPurpose::Lantern,
            ImagePurpose::Avatar => RefusalPurpose::Avatar,
            ImagePurpose::Dialog => RefusalPurpose::Dialog,
            ImagePurpose::Wardrobe => RefusalPurpose::Wardrobe,
        }
    }
}

/// v4 `resolveUnderstudy` — who could stand in, excluding the given ids.
pub trait UnderstudySource {
    fn resolve(
        &self,
        exclude: &[String],
    ) -> impl std::future::Future<Output = Option<(FailoverProfile, String)>>;
}

/// v4's default understudy: `resolveUncensoredImageUnderstudy({ userId,
/// settings, exclude })` over the user's IMAGE profiles.
pub struct ImageUnderstudySource<'a, A: ApiKeyResolver> {
    pub db: &'a Db,
    pub api_keys: &'a A,
    pub user_id: &'a str,
    pub uncensored_image_profile_id: Option<&'a str>,
}

impl<A: ApiKeyResolver> UnderstudySource for ImageUnderstudySource<'_, A> {
    async fn resolve(&self, exclude: &[String]) -> Option<(FailoverProfile, String)> {
        // A pool failure logs v4's catch line (P4.124 — this site had dropped it).
        let found = resolve_image_understudy_on(
            self.db,
            self.api_keys,
            ImageUnderstudyLookup {
                user_id: self.user_id,
                uncensored_image_profile_id: self.uncensored_image_profile_id,
                exclude,
            },
        )?;
        Some((FailoverProfile::from_row(&found.row), found.api_key))
    }
}

/// v4 `ImageFailoverContext`.
pub struct ImageFailoverContext<'a, U: UnderstudySource> {
    pub db: &'a Db,
    /// Announcements and the ledger need it; the dialog may have none.
    pub chat_id: Option<&'a str>,
    pub purpose: ImagePurpose,
    /// v4 `ctx.userId` — whose Concierge switch the refusal-time on-duty
    /// re-read asks (`3b463d6b1`, #76).
    pub user_id: &'a str,
    /// The Concierge policy, already resolved WITH the chat where there is
    /// one (v4 `3b463d6b1`, #76 — replaces the retired settings bag).
    pub concierge_policy: &'a ResolvedConciergePolicy,
    /// The chat's Concierge state when the call began, where there is a chat
    /// (v4 `4d370a90f`, #75). A Locked chat never fails over, whatever the
    /// mode says. At refusal time the chokepoint re-reads the chat (by
    /// `chat_id`) and uses this only if that read fails. `None` (the dialog)
    /// reads as Moderated.
    pub chat: Option<&'a Value>,
    pub understudy: &'a U,
    /// How the trail labels the profiles (v4's default `'image'`; the legacy
    /// dialog, which draws from connection profiles, says `Connection`).
    pub profile_kind: RouteProfileKind,
    /// How the primary came to be asked: `Concierge` when a pre-flight
    /// classifier reroute already swapped it in; v4's default `Primary`.
    pub primary_via: RouteAttemptVia,
    /// v4 `announceUnresolvedRefusal` (NEW at `ce2f1dabf`, #77; default
    /// `true`): whether the Concierge announces a refusal he could not get
    /// past (`refusal-no-understudy`, `refusal-not-permitted`). The Lantern
    /// passes `false` — its own `background-refused` bubble reports the
    /// refusal and carries the retry, one bubble per refusal. The ledger and
    /// the rethrow are UNGATED, and so is the success exit's
    /// `refusal-rerouted`.
    pub announce_unresolved_refusal: bool,
}

/// v4 `ImageFailoverOutcome`.
#[derive(Debug)]
pub struct ImageFailoverOutcome<T> {
    pub result: T,
    /// The profile that answered.
    pub profile: FailoverProfile,
    pub api_key: String,
    pub rerouted: bool,
    /// Empty when the primary answered first time; otherwise ends with the
    /// answering row.
    pub trail: Vec<RouteAttempt>,
}

/// A failure out of the chokepoint: the error that ended it, and what was
/// tried (empty on the untouched not-a-refusal rethrow).
#[derive(Debug)]
pub struct ImageFailoverError {
    pub error: ImageGenError,
    pub trail: Vec<RouteAttempt>,
}

impl ImageFailoverError {
    /// v4 `getConciergeTrail(error)` — the trail, or `None` when it is empty.
    pub fn concierge_trail(&self) -> Option<&[RouteAttempt]> {
        (!self.trail.is_empty()).then_some(self.trail.as_slice())
    }
}

/// v4 `row(profile, profileKind, via, outcome, extra)`: `profileKind` only on
/// an image row, and the detail truncated.
fn row(
    profile: &FailoverProfile,
    profile_kind: RouteProfileKind,
    via: RouteAttemptVia,
    outcome: RouteAttemptOutcome,
    trigger: Option<FallbackTrigger>,
    verdict: Option<&RefusalVerdict>,
    detail: Option<&str>,
) -> RouteAttempt {
    RouteAttempt {
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        provider: profile.provider.clone(),
        model_name: profile.model_name.clone(),
        via,
        outcome,
        profile_kind: (profile_kind == RouteProfileKind::Image).then_some(profile_kind),
        trigger,
        evidence: verdict.and_then(|v| v.evidence),
        detail: truncate_detail(detail),
    }
}

fn classify(error: &ImageGenError) -> RefusalVerdict {
    let refusal = error.refusal_error();
    classify_refusal(RefusalInput {
        error: Some(&refusal),
        ..Default::default()
    })
}

macro_rules! failover_line {
    ($level:ident, $ctx:expr, $primary:expr, $msg:literal $(, $k:ident = $v:expr)*) => {
        tracing::$level!(
            target: TARGET,
            purpose = $ctx.purpose.as_str(),
            chat_id = $ctx.chat_id,
            primary_profile_id = %$primary.id,
            primary_provider = %$primary.provider,
            primary_model = %$primary.model_name,
            $($k = $v,)*
            $msg
        )
    };
}

/// v4 `announce(ctx, kind, refusing, answeringProfileName?, reason?)` — the
/// `reason` spread into the details only when given (`4d370a90f`).
async fn announce<U: UnderstudySource>(
    ctx: &ImageFailoverContext<'_, U>,
    kind: ConciergeRefusalKind,
    refusing: &FailoverProfile,
    answering_profile_name: Option<&str>,
    reason: Option<ConciergeRefusalBar>,
) {
    let Some(chat_id) = ctx.chat_id else {
        tracing::debug!(
            target: TARGET,
            kind = kind.as_wire(),
            purpose = ctx.purpose.as_str(),
            "No chat to announce the refusal in"
        );
        return;
    };
    post_concierge_refusal_announcement(
        ctx.db,
        chat_id,
        kind,
        &ConciergeRefusalDetails {
            refusing_provider: refusing.provider.clone(),
            refusing_model: refusing.model_name.clone(),
            answering_profile_name: answering_profile_name.map(str::to_string),
            purpose: ctx.purpose.announcement(),
            reason,
        },
    )
    .await;
}

/// v4 `ledger(ctx, refusing, verdict, rerouted)` — the PRIMARY's refusal on the
/// chat's ledger; a chatless call records nothing. Never fails.
async fn ledger<U: UnderstudySource>(
    ctx: &ImageFailoverContext<'_, U>,
    refusing: &FailoverProfile,
    verdict: &RefusalVerdict,
    rerouted: bool,
) {
    let Some(chat_id) = ctx.chat_id else {
        return;
    };
    record_moderation_refusal(
        ctx.db,
        &RefusalRecord {
            chat_id: chat_id.to_string(),
            kind: RefusalKind::Image,
            purpose: ctx.purpose.ledger(),
            refused_profile_id: refusing.id.clone(),
            refused_profile_name: refusing.name.clone(),
            provider: refusing.provider.clone(),
            model_name: Some(refusing.model_name.clone()),
            evidence: verdict.evidence,
            rerouted,
        },
    )
    .await;
}

/// v4 `generateImageWithConciergeFailover` — run an image call, failing over
/// once to an uncensored understudy when the provider refuses on content
/// grounds. `attempt` builds the call for whichever profile it is handed.
///
/// `attempt` takes its profile and key BY VALUE: an `async` closure over
/// borrowed arguments cannot be proven `Send` for every lifetime (the
/// higher-ranked limitation), and the image jobs run on the job runner, whose
/// futures must be `Send`.
pub async fn generate_image_with_concierge_failover<T, U, F, Fut>(
    primary: (FailoverProfile, String),
    mut attempt: F,
    ctx: &ImageFailoverContext<'_, U>,
) -> Result<ImageFailoverOutcome<T>, ImageFailoverError>
where
    U: UnderstudySource,
    F: FnMut(FailoverProfile, String) -> Fut,
    Fut: std::future::Future<Output = Result<T, ImageGenError>>,
{
    let (primary_profile, primary_key) = primary;
    let p = &primary_profile;

    // 1. The primary.
    let primary_error = match attempt(p.clone(), primary_key.clone()).await {
        Ok(result) => {
            failover_line!(debug, ctx, p, "Image call answered first time");
            return Ok(ImageFailoverOutcome {
                result,
                profile: primary_profile,
                api_key: primary_key,
                rerouted: false,
                trail: Vec::new(),
            });
        }
        Err(e) => e,
    };

    // 2. Is it the Concierge's business?
    let verdict = classify(&primary_error);
    if !verdict.refused {
        failover_line!(
            debug,
            ctx,
            p,
            "Image call failed for a reason that is not a refusal; rethrowing untouched",
            error = primary_error.message.as_str()
        );
        return Err(ImageFailoverError {
            error: primary_error,
            trail: Vec::new(),
        });
    }

    // 3. A refusal: the trail's first row.
    let mut trail = vec![row(
        p,
        ctx.profile_kind,
        ctx.primary_via,
        RouteAttemptOutcome::Refused,
        Some(FallbackTrigger::ModerationRefusal),
        Some(&verdict),
        verdict.detail.as_deref(),
    )];
    failover_line!(
        info,
        ctx,
        p,
        "Image provider refused on content grounds",
        evidence = verdict.evidence.map(|e| e.as_str()),
        detail = verdict.detail.as_deref(),
        concierge_source = ctx.concierge_policy.source.as_str(),
        concierge_state = ctx.concierge_policy.state.as_str()
    );

    // 4. The caller's policy, stated here where a reader can see it: a Locked
    //    chat never fails over, and otherwise failover needs the Concierge on
    //    duty (`failover_allowed` is exactly "on duty and not Locked"). The
    //    state is read now, not when the call began: the operator may have
    //    locked the chat while the provider was thinking (v4 `4d370a90f`).
    let concierge_state =
        read_current_concierge_state(ctx.db, ctx.chat_id, Some(get_concierge_state(ctx.chat)));
    if !concierge_state_may_fail_over(concierge_state) {
        failover_line!(
            info,
            ctx,
            p,
            "Refusal not rerouted: the chat is Locked",
            concierge_state = concierge_state.as_str()
        );
        if ctx.announce_unresolved_refusal {
            announce(
                ctx,
                ConciergeRefusalKind::RefusalNotPermitted,
                p,
                None,
                Some(ConciergeRefusalBar::Locked),
            )
            .await;
        }
        ledger(ctx, p, &verdict, false).await;
        return Err(ImageFailoverError {
            error: primary_error,
            trail,
        });
    }
    // The Locked half of `failover_allowed` was just re-asked against the
    // current state, so what remains of it is "is the Concierge on duty?" — a
    // chat unlocked mid-call may fail over even though its snapshot policy,
    // resolved while Locked, said no. The global switch is re-asked too: the
    // operator may have sent the Concierge off duty while the provider was
    // thinking (v4 `3b463d6b1`, #76).
    let failover_allowed = (ctx.concierge_policy.failover_allowed || ctx.concierge_policy.on_duty)
        && read_current_concierge_on_duty(ctx.db, Some(ctx.user_id), ctx.concierge_policy.on_duty);
    if !failover_allowed {
        // Off duty (or an exempt chat type): the Concierge does nothing at
        // all, announcements included — the refusal still reaches the ledger.
        failover_line!(
            info,
            ctx,
            p,
            "Refusal not rerouted: the Concierge is off duty",
            concierge_source = ctx.concierge_policy.source.as_str()
        );
        ledger(ctx, p, &verdict, false).await;
        return Err(ImageFailoverError {
            error: primary_error,
            trail,
        });
    }

    // 5. Who could stand in?
    let exclude = vec![p.id.clone()];
    let Some((understudy, understudy_key)) = ctx.understudy.resolve(&exclude).await else {
        failover_line!(
            warn,
            ctx,
            p,
            "Refusal not rerouted: no uncensored understudy is available"
        );
        if ctx.announce_unresolved_refusal {
            announce(
                ctx,
                ConciergeRefusalKind::RefusalNoUnderstudy,
                p,
                None,
                None,
            )
            .await;
        }
        ledger(ctx, p, &verdict, false).await;
        return Err(ImageFailoverError {
            error: primary_error,
            trail,
        });
    };

    // 6. One more try.
    failover_line!(
        info,
        ctx,
        p,
        "Rerouting a refused image call to an uncensored understudy",
        understudy_profile_id = understudy.id.as_str(),
        understudy_name = understudy.name.as_str(),
        understudy_provider = understudy.provider.as_str(),
        understudy_model = understudy.model_name.as_str()
    );
    match attempt(understudy.clone(), understudy_key.clone()).await {
        Ok(result) => {
            trail.push(row(
                &understudy,
                ctx.profile_kind,
                RouteAttemptVia::Concierge,
                RouteAttemptOutcome::Answered,
                None,
                None,
                None,
            ));
            failover_line!(
                info,
                ctx,
                p,
                "Uncensored understudy answered a refused image call",
                understudy_profile_id = understudy.id.as_str(),
                understudy_name = understudy.name.as_str()
            );
            announce(
                ctx,
                ConciergeRefusalKind::RefusalRerouted,
                p,
                Some(&understudy.name),
                None,
            )
            .await;
            ledger(ctx, p, &verdict, true).await;
            Ok(ImageFailoverOutcome {
                result,
                profile: understudy,
                api_key: understudy_key,
                rerouted: true,
                trail,
            })
        }
        Err(understudy_error) => {
            let understudy_verdict = classify(&understudy_error);
            trail.push(if understudy_verdict.refused {
                row(
                    &understudy,
                    ctx.profile_kind,
                    RouteAttemptVia::Concierge,
                    RouteAttemptOutcome::Refused,
                    Some(FallbackTrigger::ModerationRefusal),
                    Some(&understudy_verdict),
                    understudy_verdict.detail.as_deref(),
                )
            } else {
                let refusal = understudy_error.refusal_error();
                let trigger = classify_fallback_trigger(FallbackError {
                    kind: None,
                    name: refusal.name.as_deref(),
                    message: &understudy_error.message,
                    refusal: understudy_error.refusal.as_deref(),
                })
                .unwrap_or(FallbackTrigger::ProviderError);
                row(
                    &understudy,
                    ctx.profile_kind,
                    RouteAttemptVia::Concierge,
                    RouteAttemptOutcome::Failed,
                    Some(trigger),
                    None,
                    Some(&understudy_error.message),
                )
            });
            failover_line!(
                error,
                ctx,
                p,
                "Uncensored understudy also failed a refused image call",
                understudy_profile_id = understudy.id.as_str(),
                understudy_name = understudy.name.as_str(),
                understudy_refused = understudy_verdict.refused,
                error = understudy_error.message.as_str()
            );
            // The PRIMARY's verdict: the ledger counts the refusal that started
            // this, not the understudy's. No announcement on this exit.
            ledger(ctx, p, &verdict, false).await;
            Err(ImageFailoverError {
                error: understudy_error,
                trail,
            })
        }
    }
}
