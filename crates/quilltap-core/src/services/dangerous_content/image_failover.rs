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
//!   4. mode is not `AUTO_ROUTE` → announce `refusal-not-permitted`, fail;
//!   5. ask the understudy resolver (excluding the primary). Nobody → announce
//!      `refusal-no-understudy`, fail;
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

use crate::db::chat_settings::DangerousContentSettings;
use crate::db::runtime::Db;
use crate::llm_fallback::{classify_fallback_trigger, FallbackError, FallbackTrigger};
use crate::model::image::ImageGenError;
use crate::services::concierge_notifications::{
    post_concierge_refusal_announcement, ConciergeRefusalDetails, ConciergeRefusalKind,
    ConciergeRefusalPurpose,
};
use crate::services::route_trail::{
    truncate_detail, RouteAttempt, RouteAttemptOutcome, RouteAttemptVia, RouteProfileKind,
};

use super::provider_routing::ApiKeyResolver;
use super::refusal::{classify_refusal, RefusalInput, RefusalVerdict};
use super::refusal_ledger::{
    record_moderation_refusal, RefusalKind, RefusalPurpose, RefusalRecord,
};
use super::understudy::{resolve_uncensored_image_understudy, ImageUnderstudyLookup};

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
        }
    }

    fn ledger(self) -> RefusalPurpose {
        match self {
            ImagePurpose::Tool => RefusalPurpose::Tool,
            ImagePurpose::Lantern => RefusalPurpose::Lantern,
            ImagePurpose::Avatar => RefusalPurpose::Avatar,
            ImagePurpose::Dialog => RefusalPurpose::Dialog,
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
        let found = self
            .db
            .read_main(|conn| {
                Ok(resolve_uncensored_image_understudy(
                    conn,
                    self.api_keys,
                    ImageUnderstudyLookup {
                        user_id: self.user_id,
                        uncensored_image_profile_id: self.uncensored_image_profile_id,
                        exclude,
                    },
                ))
            })
            .ok()
            .flatten()?;
        Some((FailoverProfile::from_row(&found.row), found.api_key))
    }
}

/// v4 `ImageFailoverContext`.
pub struct ImageFailoverContext<'a, U: UnderstudySource> {
    pub db: &'a Db,
    /// Announcements and the ledger need it; the dialog may have none.
    pub chat_id: Option<&'a str>,
    pub purpose: ImagePurpose,
    /// Already resolved WITH the chat where there is one.
    pub settings: &'a DangerousContentSettings,
    pub understudy: &'a U,
    /// How the trail labels the profiles (v4's default `'image'`; the legacy
    /// dialog, which draws from connection profiles, says `Connection`).
    pub profile_kind: RouteProfileKind,
    /// How the primary came to be asked: `Concierge` when a pre-flight
    /// classifier reroute already swapped it in; v4's default `Primary`.
    pub primary_via: RouteAttemptVia,
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

/// v4 `announce(ctx, kind, refusing, answeringProfileName?)`.
async fn announce<U: UnderstudySource>(
    ctx: &ImageFailoverContext<'_, U>,
    kind: ConciergeRefusalKind,
    refusing: &FailoverProfile,
    answering_profile_name: Option<&str>,
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
        mode = ctx.settings.mode.as_str()
    );

    // 4. The caller's policy, stated here where a reader can see it: failover
    //    obeys Auto-Route in this phase.
    if ctx.settings.mode != "AUTO_ROUTE" {
        failover_line!(
            info,
            ctx,
            p,
            "Refusal not rerouted: the Concierge mode does not permit it",
            mode = ctx.settings.mode.as_str()
        );
        announce(ctx, ConciergeRefusalKind::RefusalNotPermitted, p, None).await;
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
        announce(ctx, ConciergeRefusalKind::RefusalNoUnderstudy, p, None).await;
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
