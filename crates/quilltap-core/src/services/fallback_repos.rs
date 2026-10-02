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
        // The resolver reads over the POOL (`Db` is `MainReads`), so a read
        // that could not run at all lands inside its `Error finding API key
        // by ID` wrap and is `api-key-not-found` — this candidate cannot
        // authenticate, move on. P4.139: the `read_main(|c| Ok(…))` wrapper
        // had folded that pool failure with no line (v4's
        // `provider-failover.service.ts:28` logs the repository's).
        match resolve_connection_profile_api_key(
            self.db,
            &profile.provider,
            profile.api_key_id.as_deref(),
        ) {
            ProfileApiKeyResolution::Ok(key) => Ok(key),
            ProfileApiKeyResolution::Failed(reason) => Err(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    //! P4.139: the failover chain's key resolution reads over the POOL, so a
    //! checkout failure is the composite's `Error finding API key by ID` line
    //! and `api-key-not-found` (v4 `provider-failover.service.ts:28` →
    //! `resolveConnectionProfileApiKey` → the fallback `findApiKeyById`). ONE
    //! line — v4's `Failed to get API keys collection` before it is the
    //! `MainReads` divergence.
    use super::*;
    use crate::services::api_key_service::test_instance::{db_lines, db_with_a_failing_read_pool};

    #[test]
    fn a_failed_pool_is_the_composites_line_and_api_key_not_found() {
        let (dir, db) = db_with_a_failing_read_pool();
        let profile = FallbackProfile::from_value(&serde_json::json!({
            "id": "cp-1", "userId": "u-1", "name": "Understudy", "provider": "ANTHROPIC",
            "modelName": "m", "apiKeyId": "k-1",
        }))
        .expect("a FallbackProfile");
        let (got, lines) = crate::test_support::captured_with(|| {
            DbFallbackRepos::new(&db).resolve_api_key(&profile)
        });
        assert_eq!(got, Err(ProfileApiKeyFailure::ApiKeyNotFound));
        // Measured: the read pool's checkout of the unlinked file answers
        // SQLite's open failure, the path included, rendered bare (the home's
        // `error_text`).
        assert_eq!(
            db_lines(&lines),
            vec![format!(
                "ERROR quilltap::db Error finding API key by ID collection=connection_profiles keyId=k-1 error=unable to open database file: {}",
                dir.path().join("main.db").display()
            )]
        );
    }
}
