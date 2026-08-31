//! The DB-backed [`FallbackRepos`] the four fallback-chain call sites share
//! (v4 `65f5021c8`).
//!
//! v4 hands the engine `repos` — its ambient repository factory — and every
//! chain read goes through it. v5's reads are closures over a borrowed
//! `&Connection`, so the seam is a small struct holding a [`Db`] and doing its
//! own `read_main` per question. That is what lets the chain be built from an
//! `async` service (a borrowed connection cannot be held across an await) while
//! the engine itself stays synchronous and driveable from an in-memory `Vec` in
//! the differential.
//!
//! [`FallbackChainRepos`] adds the one read the engine does not need but a
//! *walk* does: an understudy's API key has to be resolved before its call goes
//! out, and v4 records a resolution failure as an `auth` attempt rather than
//! silently skipping the candidate.
//!
//! Every read here is fail-soft in the same direction v4's are: a read error
//! reads as "no such profile" / "no usable key", which drops the candidate. A
//! chain is a recovery path — refusing to fail over because the profile table
//! was briefly unreadable would turn one failure into two.

use crate::db::runtime::Db;
use crate::llm_fallback::{FallbackProfile, FallbackRepos};

use super::api_key_service::{
    resolve_connection_profile_api_key, ProfileApiKeyFailure, ProfileApiKeyResolution,
};

/// The reads a chain WALK needs, on top of the engine's own surface.
pub trait FallbackChainRepos: FallbackRepos {
    /// v4 `resolveConnectionProfileApiKey(repos, understudy)`. `Err` carries the
    /// reason v4 records as the attempt's error text.
    fn resolve_api_key(&self, profile: &FallbackProfile) -> Result<String, ProfileApiKeyFailure>;
}

/// [`FallbackRepos`] + [`FallbackChainRepos`] over a live instance.
pub struct DbFallbackRepos<'a> {
    db: &'a Db,
}

impl<'a> DbFallbackRepos<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }
}

impl FallbackRepos for DbFallbackRepos<'_> {
    fn find_by_id(&self, id: &str) -> Option<FallbackProfile> {
        let owned = id.to_string();
        self.db
            .read_main(move |conn| crate::db::connection_profiles::find_by_id(conn, &owned))
            .ok()
            .flatten()
            .as_ref()
            .and_then(FallbackProfile::from_value)
    }

    fn find_by_user_id(&self, user_id: &str) -> Vec<FallbackProfile> {
        let owned = user_id.to_string();
        self.db
            .read_main(move |conn| crate::db::connection_profiles::find_by_user_id(conn, &owned))
            .unwrap_or_default()
            .iter()
            .filter_map(FallbackProfile::from_value)
            .collect()
    }
}

impl FallbackChainRepos for DbFallbackRepos<'_> {
    fn resolve_api_key(&self, profile: &FallbackProfile) -> Result<String, ProfileApiKeyFailure> {
        let provider = profile.provider.clone();
        let api_key_id = profile.api_key_id.clone();
        let resolution = self.db.read_main(move |conn| {
            Ok(resolve_connection_profile_api_key(
                conn,
                &provider,
                api_key_id.as_deref(),
            ))
        });
        match resolution {
            Ok(ProfileApiKeyResolution::Ok(key)) => Ok(key),
            Ok(ProfileApiKeyResolution::Failed(reason)) => Err(reason),
            // A read that could not run at all is the same answer the resolver
            // gives for a key row it cannot find: this candidate cannot
            // authenticate, move on.
            Err(_) => Err(ProfileApiKeyFailure::ApiKeyNotFound),
        }
    }
}
