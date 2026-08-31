//! Fallback chains for cheap-LLM tasks (v4 `65f5021c8`,
//! `lib/memory/cheap-llm-tasks/fallback.ts`).
//!
//! The cheap path speaks a different currency from the Salon: a
//! [`CheapLlmSelection`] (provider + model + baseUrl + an *optional*
//! `connectionProfileId`), not a connection profile. The chain logic itself is
//! identical, so this module converts between the two rather than growing a
//! second engine — the two paths drifting apart is the trap this feature was
//! warned about.
//!
//! Two shapes of selection, two answers:
//!
//! - **Backed by a profile** — walk that profile's chain exactly as the Salon
//!   does (`fallbackProfileId`, then an optional tier pick).
//! - **Not backed by a profile** — a pure-local Ollama pick or a
//!   provider-cheapest synthesis has nothing to hang a chain on. Those are
//!   governed by the one off-profile switch,
//!   `cheapLLMSettings.allowCheapFallback`, and draw a stand-in from the user's
//!   `isCheap` profiles.
//!
//! Everything here is a read, which is what lets it run wherever the cheap path
//! does.

use crate::cheap_llm::{selection_from_profile, CheapLlmSelection};
use crate::db::runtime::Db;
use crate::llm_fallback::{
    build_fallback_chain, pick_tier_candidate, FallbackContext, FallbackProfile, FallbackPurpose,
};
use crate::tools::generate_image::cheap_llm_profile_from_value;

use super::fallback_repos::DbFallbackRepos;

/// What the caller knows about the failed cheap call.
pub struct CheapFallbackRequest<'a> {
    pub selection: &'a CheapLlmSelection,
    pub user_id: &'a str,
    /// True when the failed task was itself an uncensored reroute, or the chat
    /// is dangerous — a stand-in must then be cleared for the content.
    pub dangerous: bool,
    /// Profile ids already spent on this task (the primary, any uncensored
    /// retry).
    pub already_tried: Vec<String>,
    pub task_type: Option<&'a str>,
}

/// Build the ordered list of stand-in selections for a failed cheap-LLM call.
///
/// Returns an EMPTY vec when the route has no chain — no profile behind it and
/// `allowCheapFallback` off, or a profile that named no understudy and declined
/// a tier pick. Callers treat that as "fail as we always have".
pub fn build_cheap_fallback_selections(
    db: &Db,
    req: CheapFallbackRequest<'_>,
) -> Vec<CheapLlmSelection> {
    let repos = DbFallbackRepos::new(db);
    let context = FallbackContext {
        user_id: req.user_id.to_string(),
        purpose: FallbackPurpose::Cheap,
        dangerous: req.dangerous,
        // Cheap tasks are text in, text out. None of them attach an image or
        // send tools, so a stand-in needs neither capability.
        needs_vision: false,
        needs_tools: false,
        already_tried: req.already_tried,
    };

    // --- Selection backed by a connection profile: walk its chain. ---
    if let Some(profile_id) = req
        .selection
        .connection_profile_id
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        let Some(primary) = read_profile(db, profile_id) else {
            tracing::debug!(
                target: "quilltap::cheap_llm",
                task_type = req.task_type.unwrap_or(""),
                connection_profile_id = profile_id,
                "[CheapLLM] Fallback skipped: selection names a profile that no longer exists"
            );
            return Vec::new();
        };
        let chain = build_fallback_chain(&primary.0, &repos, &context);
        // The chain leads with the primary itself; it is the one route we
        // already know does not work right now.
        return chain
            .into_iter()
            .filter(|c| c.profile.id != primary.0.id)
            .filter_map(|c| read_profile(db, &c.profile.id))
            .map(|(_, cheap)| selection_from_profile(&cheap))
            .collect();
    }

    // --- No profile behind the selection. ---
    //
    // Read the switch here rather than threading it down through every cheap-task
    // caller: it is a single instance setting, this is the only place that wants
    // it, and `chatSettings` is a plain read.
    let uid = req.user_id.to_string();
    let allow = db
        .read_main(move |conn| crate::db::chat_settings::find_by_user_id(conn, &uid))
        .ok()
        .flatten()
        .and_then(|row| {
            row.get("cheapLLMSettings")
                .and_then(|c| c.get("allowCheapFallback"))
                .and_then(serde_json::Value::as_bool)
        })
        == Some(true);

    if !allow {
        tracing::debug!(
            target: "quilltap::cheap_llm",
            task_type = req.task_type.unwrap_or(""),
            provider = %req.selection.provider,
            model = %req.selection.model_name,
            "[CheapLLM] Fallback skipped: profile-less selection and allowCheapFallback is off"
        );
        return Vec::new();
    }

    let all: Vec<(FallbackProfile, crate::cheap_llm::CheapLlmProfile)> =
        read_user_profiles(db, req.user_id);
    let cheap: Vec<(FallbackProfile, crate::cheap_llm::CheapLlmProfile)> =
        all.into_iter().filter(|(f, _)| f.is_cheap).collect();

    if cheap.is_empty() {
        tracing::debug!(
            target: "quilltap::cheap_llm",
            task_type = req.task_type.unwrap_or(""),
            provider = %req.selection.provider,
            "[CheapLLM] Fallback skipped: no isCheap profiles to draft from"
        );
        return Vec::new();
    }

    // There is no failed *profile* to rank against, so stand in a synthetic one
    // carrying the selection's provider and no model class. Unknown-vs-unknown
    // matches and unknown-vs-known does not, which is exactly the right rule
    // here: a profile-less route has never been classified, so a classified
    // profile is not a like-for-like replacement for it.
    let synthetic = FallbackProfile {
        id: String::new(),
        user_id: req.user_id.to_string(),
        name: String::new(),
        provider: req.selection.provider.clone(),
        model_name: req.selection.model_name.clone(),
        base_url: None,
        api_key_id: None,
        transport: "api".to_string(),
        is_cheap: false,
        is_dangerous_compatible: false,
        supports_image_upload: false,
        // v4 casts a five-key literal through `as ConnectionProfile`, so every
        // other field is `undefined` — and `allowToolUse` is only read when
        // `needsTools`, which a cheap task never sets.
        allow_tool_use: false,
        model_class: None,
        sort_index: 0.0,
        fallback_profile_id: None,
        allow_tier_fallback: false,
        parameters: None,
    };

    let candidates: Vec<FallbackProfile> = cheap.iter().map(|(f, _)| f.clone()).collect();
    let Some(pick) = pick_tier_candidate(&synthetic, &candidates, &context) else {
        return Vec::new();
    };
    let picked = cheap
        .iter()
        .find(|(f, _)| f.id == pick.id)
        .map(|(_, c)| c.clone());
    let Some(picked) = picked else {
        return Vec::new();
    };

    tracing::info!(
        target: "quilltap::cheap_llm",
        task_type = req.task_type.unwrap_or(""),
        failed_provider = %req.selection.provider,
        failed_model = %req.selection.model_name,
        picked_profile_id = %picked.id,
        picked_provider = %picked.provider,
        picked_model = %picked.model_name,
        "[CheapLLM] Drafted a stand-in for a profile-less cheap route"
    );

    vec![selection_from_profile(&picked)]
}

/// One row, read once, in both projections the chain needs: the engine's
/// [`FallbackProfile`] and the cheap path's `CheapLlmProfile` (which carries the
/// `maxContext` / `maxTokens` the selection's `profileParams` reads).
fn read_profile(db: &Db, id: &str) -> Option<(FallbackProfile, crate::cheap_llm::CheapLlmProfile)> {
    let owned = id.to_string();
    let row = db
        .read_main(move |conn| crate::db::connection_profiles::find_by_id(conn, &owned))
        .ok()
        .flatten()?;
    let fallback = FallbackProfile::from_value(&row)?;
    let cheap = cheap_llm_profile_from_value(&row);
    Some((fallback, cheap))
}

fn read_user_profiles(
    db: &Db,
    user_id: &str,
) -> Vec<(FallbackProfile, crate::cheap_llm::CheapLlmProfile)> {
    let owned = user_id.to_string();
    db.read_main(move |conn| crate::db::connection_profiles::find_by_user_id(conn, &owned))
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            FallbackProfile::from_value(row).map(|f| (f, cheap_llm_profile_from_value(row)))
        })
        .collect()
}
