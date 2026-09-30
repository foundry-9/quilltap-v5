//! The uncensored understudies — the ONE order the Concierge uses to find an
//! uncensored stand-in for a text or an image call (port of v4
//! `lib/services/dangerous-content/understudy.ts`, NEW at `8bd080267`, #73;
//! unchanged by `49059fb14`).
//!
//! The order, for both kinds: the configured uncensored profile first (when it
//! is not excluded, is the user's own, and — for text — is eligible), then any
//! `isDangerousCompatible` profile in `findAll()` order. The caller's own ids
//! are EXCLUDED, so a refused profile is never asked twice; text candidates on
//! the Courier transport are skipped (a clipboard hand-off cannot stand in for
//! a refused call); a caller's `filter` applies to the explicit pick as well as
//! to the scan.
//!
//! **These resolvers never read the policy's gates** — only its desk (v4
//! `3b463d6b1`, #76). The policy lives with the callers: the two pre-flight
//! wrappers in [`super::provider_routing`] keep v4's `route_direct ||
//! failover_allowed` gate, the image failover chokepoint and the text failover
//! gate their own. A lookup that fails is swallowed (logged, `None`) — v4's catch.
//!
//! **Every read here is v4's FALLBACK repository read (P4.124).** v4's
//! `findById`/`findAll`/`findApiKeyByIdAndUserId` are `safeQuery(…, fallback)`
//! calls: a failed read logs the repository's ERROR (`Error finding entity by
//! ID` / `Error finding all entities` / `Error finding API key by ID and user
//! ID`, with `collection` first) and answers `null`/`[]` — it never throws. So a
//! database error walks v4's not-found arms (the explicit pick WARNs "missing",
//! the scan finds nothing) and never reaches the resolver's own catch. v5 read
//! through `?` and took the catch instead, logging `… understudy lookup failed`
//! where v4 logs the repository line; the reads below are now the fallback
//! shape. The catch's line survives for the one failure v4 has no counterpart
//! for — the read POOL failing to hand out a connection at all — through
//! [`resolve_text_understudy_on`] / [`resolve_image_understudy_on`], which every
//! production site calls (a `read_main(…).ok().flatten()` had dropped it
//! silently at four sites).
//!
//! Deltas from v5's pre-#73 inline order (`provider_routing.rs`, recorded for
//! the review): the original profile is now EXCLUDED (the old "rerouted to the
//! same id" outcome is gone); couriers are skipped; an excluded / courier /
//! keyless / filtered explicit pick logs and then scans; the deprioritising
//! line moved here WITHOUT its `[DangerousContent]` prefix; a keyless or
//! not-owned explicit IMAGE pick now WARNs (the old image code fell through
//! silently).

use serde_json::Value;

use super::provider_routing::{route_profile_from_value, ApiKeyResolver, RouteProfile};
use crate::db::runtime::Db;
use crate::db::{connection_profiles, image_profiles};

// v4's fallback reads (`_findById` / `_findAll` under `safeQuery`) — ONE home,
// `crate::db::fallback`; the resolvers below read through it.
pub(crate) use crate::db::fallback::{find_all_or_empty, find_by_id_or_none};

/// `repos.connections.findAll()` — v4's fallback read (`connection_profiles`).
pub(crate) fn connection_profiles_find_all_or_empty(conn: &rusqlite::Connection) -> Vec<Value> {
    find_all_or_empty("connection_profiles", || {
        connection_profiles::find_all(conn)
    })
}

/// A resolved understudy: the chosen profile's identity, its raw row (callers
/// re-run the attachment decision against it — v4's result carries the whole
/// profile), and its decrypted key.
#[derive(Clone, Debug, PartialEq)]
pub struct Understudy {
    pub profile: RouteProfile,
    pub row: Value,
    pub api_key: String,
}

/// v4 `TextUnderstudyLookup`. `uncensored_text_profile_id` is the one settings
/// field the text resolver reads (v4 hands it the whole settings object).
#[derive(Clone, Copy)]
pub struct TextUnderstudyLookup<'a> {
    pub user_id: &'a str,
    pub uncensored_text_profile_id: Option<&'a str>,
    /// Ids that must not be chosen on this call — the refused profile, and
    /// whatever the caller has already tried.
    pub exclude: &'a [String],
    /// MIME types riding in this turn's message array (bug 106): a preference,
    /// not a filter — carriers are tried first.
    pub turn_attachment_mime_types: &'a [String],
    /// A caller's own eligibility test (the legacy image dialog restricts the
    /// connection-profile understudy to providers that can generate images).
    pub filter: Option<&'a dyn Fn(&Value) -> bool>,
}

/// v4 `UnderstudyLookup` for images.
#[derive(Clone, Copy)]
pub struct ImageUnderstudyLookup<'a> {
    pub user_id: &'a str,
    pub uncensored_image_profile_id: Option<&'a str>,
    pub exclude: &'a [String],
}

fn str_of<'v>(v: &'v Value, key: &str) -> &'v str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn user_id_of(v: &Value) -> Option<&str> {
    v.get("userId").and_then(Value::as_str)
}

fn is_dangerous_compatible(v: &Value) -> bool {
    v.get("isDangerousCompatible").and_then(Value::as_bool) == Some(true)
}

fn is_courier(v: &Value) -> bool {
    v.get("transport").and_then(Value::as_str) == Some("courier")
}

/// v4 `decryptKey`: no `apiKeyId` → `None`; the key's `key_value` when truthy;
/// a lookup that throws WARNs and answers `None`.
fn decrypt_key<A: ApiKeyResolver>(api_keys: &A, profile: &Value, user_id: &str) -> Option<String> {
    let api_key_id = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?;
    match api_keys.try_resolve(api_key_id, user_id) {
        Ok(key) => key.filter(|k| !k.is_empty()),
        Err(error) => {
            tracing::warn!(
                target: "quilltap::concierge_understudy",
                profile_id = %str_of(profile, "id"),
                error = %error,
                "Could not decrypt an understudy candidate's API key"
            );
            None
        }
    }
}

/// v4 `profileCanCarryTurn` (bug 106): every MIME type this turn carries can
/// be received by the profile (`[].every` is `true`).
fn profile_can_carry_turn(profile: &Value, mime_types: &[String]) -> bool {
    let view = crate::files::image_transport::AttachmentProfileView::from_json(profile);
    mime_types
        .iter()
        .all(|m| crate::files::image_transport::profile_can_receive_attachment(view, m))
}

fn found(profile: &Value, api_key: String) -> Understudy {
    Understudy {
        profile: route_profile_from_value(profile),
        row: profile.clone(),
        api_key,
    }
}

/// v4 `resolveUncensoredTextUnderstudy` — the uncensored connection profile that
/// could take a text call, or `None`.
pub fn resolve_uncensored_text_understudy<A: ApiKeyResolver>(
    conn: &rusqlite::Connection,
    api_keys: &A,
    lookup: TextUnderstudyLookup<'_>,
) -> Option<Understudy> {
    let excluded = |id: &str| lookup.exclude.iter().any(|e| e == id);
    let eligible = |p: &Value| {
        user_id_of(p) == Some(lookup.user_id)
            && !excluded(str_of(p, "id"))
            && !is_courier(p)
            && lookup.filter.is_none_or(|f| f(p))
    };

    // Infallible: every read is v4's fallback read (see the module doc).
    let attempt = || -> Option<Understudy> {
        // `explicitId && …` — an empty id is no pick at all.
        let explicit_id = lookup.uncensored_text_profile_id.filter(|s| !s.is_empty());
        if let Some(explicit_id) = explicit_id.filter(|id| !excluded(id)) {
            let explicit = find_by_id_or_none("connection_profiles", explicit_id, || {
                connection_profiles::find_by_id(conn, explicit_id)
            });
            match &explicit {
                Some(p) if eligible(p) => {
                    if let Some(api_key) = decrypt_key(api_keys, p, lookup.user_id) {
                        tracing::debug!(
                            target: "quilltap::concierge_understudy",
                            profile_id = %str_of(p, "id"),
                            profile_name = %str_of(p, "name"),
                            provider = %str_of(p, "provider"),
                            model = %str_of(p, "modelName"),
                            "Text understudy: the configured uncensored profile"
                        );
                        return Some(found(p, api_key));
                    }
                    tracing::warn!(
                        target: "quilltap::concierge_understudy",
                        profile_id = %explicit_id,
                        "Configured uncensored text profile has no usable API key; scanning instead"
                    );
                }
                _ => {
                    tracing::warn!(
                        target: "quilltap::concierge_understudy",
                        profile_id = %explicit_id,
                        found = explicit.is_some(),
                        courier = explicit.as_ref().is_some_and(is_courier),
                        "Configured uncensored text profile is missing, not owned, or not eligible; scanning instead"
                    );
                }
            }
        } else if let Some(explicit_id) = explicit_id {
            tracing::debug!(
                target: "quilltap::concierge_understudy",
                profile_id = %explicit_id,
                "Configured uncensored text profile is excluded on this call"
            );
        }

        // Ordered, not filtered: profiles that can carry this turn's
        // attachments first, the rest behind them.
        let compatible: Vec<Value> = connection_profiles_find_all_or_empty(conn)
            .into_iter()
            .filter(|p| is_dangerous_compatible(p) && eligible(p))
            .collect();
        let (can_carry, cannot_carry): (Vec<&Value>, Vec<&Value>) = compatible
            .iter()
            .partition(|p| profile_can_carry_turn(p, lookup.turn_attachment_mime_types));
        if !lookup.turn_attachment_mime_types.is_empty() && !cannot_carry.is_empty() {
            let names = |v: &[&Value]| -> Vec<String> {
                v.iter().map(|p| str_of(p, "name").to_string()).collect()
            };
            // v4 moved this line here from the wrapper WITHOUT its
            // `[DangerousContent]` prefix (`8bd080267`).
            tracing::info!(
                target: "quilltap::concierge_understudy",
                turn_attachment_mime_types = ?lookup.turn_attachment_mime_types,
                can_carry = ?names(&can_carry),
                cannot_carry = ?names(&cannot_carry),
                "Deprioritising uncensored candidates that cannot carry this turn"
            );
        }

        for p in can_carry.iter().chain(cannot_carry.iter()) {
            if let Some(api_key) = decrypt_key(api_keys, p, lookup.user_id) {
                tracing::debug!(
                    target: "quilltap::concierge_understudy",
                    profile_id = %str_of(p, "id"),
                    profile_name = %str_of(p, "name"),
                    provider = %str_of(p, "provider"),
                    model = %str_of(p, "modelName"),
                    "Text understudy: a discovered uncensored-compatible profile"
                );
                return Some(found(p, api_key));
            }
        }

        tracing::debug!(
            target: "quilltap::concierge_understudy",
            user_id = %lookup.user_id,
            excluded = ?lookup.exclude,
            candidates = compatible.len(),
            "No uncensored text understudy is available"
        );
        None
    };

    attempt()
}

/// v4 `resolveUncensoredImageUnderstudy` — the uncensored image profile that
/// could take an image call, or `None`. The explicit gate is ownership alone
/// (no courier, no filter: image profiles have neither).
pub fn resolve_uncensored_image_understudy<A: ApiKeyResolver>(
    conn: &rusqlite::Connection,
    api_keys: &A,
    lookup: ImageUnderstudyLookup<'_>,
) -> Option<Understudy> {
    let excluded = |id: &str| lookup.exclude.iter().any(|e| e == id);

    // Infallible: every read is v4's fallback read (see the module doc).
    let attempt = || -> Option<Understudy> {
        let explicit_id = lookup.uncensored_image_profile_id.filter(|s| !s.is_empty());
        if let Some(explicit_id) = explicit_id.filter(|id| !excluded(id)) {
            let explicit = image_profiles::find_by_id_or_none(conn, explicit_id);
            match &explicit {
                Some(p) if user_id_of(p) == Some(lookup.user_id) => {
                    if let Some(api_key) = decrypt_key(api_keys, p, lookup.user_id) {
                        tracing::debug!(
                            target: "quilltap::concierge_understudy",
                            profile_id = %str_of(p, "id"),
                            profile_name = %str_of(p, "name"),
                            provider = %str_of(p, "provider"),
                            model = %str_of(p, "modelName"),
                            "Image understudy: the configured uncensored profile"
                        );
                        return Some(found(p, api_key));
                    }
                    tracing::warn!(
                        target: "quilltap::concierge_understudy",
                        profile_id = %explicit_id,
                        "Configured uncensored image profile has no usable API key; scanning instead"
                    );
                }
                _ => {
                    tracing::warn!(
                        target: "quilltap::concierge_understudy",
                        profile_id = %explicit_id,
                        found = explicit.is_some(),
                        "Configured uncensored image profile is missing or not owned; scanning instead"
                    );
                }
            }
        } else if let Some(explicit_id) = explicit_id {
            tracing::debug!(
                target: "quilltap::concierge_understudy",
                profile_id = %explicit_id,
                "Configured uncensored image profile is excluded on this call"
            );
        }

        let compatible: Vec<Value> = image_profiles::find_all_or_empty(conn)
            .into_iter()
            .filter(|p| {
                user_id_of(p) == Some(lookup.user_id)
                    && is_dangerous_compatible(p)
                    && !excluded(str_of(p, "id"))
            })
            .collect();

        for p in &compatible {
            if let Some(api_key) = decrypt_key(api_keys, p, lookup.user_id) {
                tracing::debug!(
                    target: "quilltap::concierge_understudy",
                    profile_id = %str_of(p, "id"),
                    profile_name = %str_of(p, "name"),
                    provider = %str_of(p, "provider"),
                    model = %str_of(p, "modelName"),
                    "Image understudy: a discovered uncensored-compatible profile"
                );
                return Some(found(p, api_key));
            }
        }

        tracing::debug!(
            target: "quilltap::concierge_understudy",
            user_id = %lookup.user_id,
            excluded = ?lookup.exclude,
            candidates = compatible.len(),
            "No uncensored image understudy is available"
        );
        None
    };

    attempt()
}

/// v4's resolver catch, `logger.error('Text understudy lookup failed', { error
/// })`, for the one failure v5 can meet before the resolver runs: the read pool
/// cannot hand out a connection. Answers `None` — the "nobody" v4's catch
/// answers — so no caller reads a pool failure as anything else.
pub fn resolve_text_understudy_on<A: ApiKeyResolver>(
    db: &Db,
    api_keys: &A,
    lookup: TextUnderstudyLookup<'_>,
) -> Option<Understudy> {
    db.read_main(|conn| Ok(resolve_uncensored_text_understudy(conn, api_keys, lookup)))
        .unwrap_or_else(|error| {
            tracing::error!(
                target: "quilltap::concierge_understudy",
                error = %error,
                "Text understudy lookup failed"
            );
            None
        })
}

/// The image twin of [`resolve_text_understudy_on`] — v4's `Image understudy
/// lookup failed` catch.
pub fn resolve_image_understudy_on<A: ApiKeyResolver>(
    db: &Db,
    api_keys: &A,
    lookup: ImageUnderstudyLookup<'_>,
) -> Option<Understudy> {
    db.read_main(|conn| Ok(resolve_uncensored_image_understudy(conn, api_keys, lookup)))
        .unwrap_or_else(|error| {
            tracing::error!(
                target: "quilltap::concierge_understudy",
                error = %error,
                "Image understudy lookup failed"
            );
            None
        })
}

/// P4.124 (P4.D225 NIT (b)): the fallback reads' repository lines, the pool
/// failure's catch line at EACH production site (a `read_main(…).ok()
/// .flatten()` had dropped it at four), and the silence legs.
#[cfg(test)]
mod pool_failure_tests {
    use super::*;
    use crate::services::dangerous_content::image_failover::{
        ImageUnderstudySource, UnderstudySource,
    };
    use crate::services::dangerous_content::provider_routing::{
        ConnApiKeys, DangerContentRouter, DbApiKeys, NoApiKeys,
    };
    use crate::services::dangerous_content::retry_uncensored::{
        resolve_image_retry_understudy, resolve_text_retry_understudy, AnsweredBy,
        RetryUncensoredRefusal,
    };
    use crate::services::provider_failover::DangerousContentRouter;
    use crate::test_support::captured_with;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A `Db` whose read pool cannot hand out a connection: the file is
    /// unlinked after the open (the writer keeps its handle; a fresh read-only
    /// open of the path fails). Asserted, so the plant cannot silently pass.
    fn db_with_a_failing_read_pool() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.db");
        drop(crate::db::Writer::open_writable(&path, PEPPER).unwrap());
        let db = Db::open_main(&path, PEPPER).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(
            db.read_main(|_| Ok(())).is_err(),
            "the plant must fail the pool"
        );
        (dir, db)
    }

    /// A healthy `Db` with no tables — every read fails INSIDE the pool.
    fn db_with_no_tables() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.db");
        drop(crate::db::Writer::open_writable(&path, PEPPER).unwrap());
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn with<'a>(lines: &'a [String], needle: &str) -> Vec<&'a String> {
        lines.iter().filter(|l| l.contains(needle)).collect()
    }

    fn text_lookup<'a>(explicit: Option<&'a str>) -> TextUnderstudyLookup<'a> {
        TextUnderstudyLookup {
            user_id: "u-1",
            uncensored_text_profile_id: explicit,
            exclude: &[],
            turn_attachment_mime_types: &[],
            filter: None,
        }
    }

    fn image_lookup<'a>(explicit: Option<&'a str>) -> ImageUnderstudyLookup<'a> {
        ImageUnderstudyLookup {
            user_id: "u-1",
            uncensored_image_profile_id: explicit,
            exclude: &[],
        }
    }

    /// A failed read takes v4's not-found arms — the repository ERRORs, the
    /// explicit pick's "missing" WARN — and never the resolver's catch.
    #[test]
    fn a_failed_read_walks_v4s_not_found_arms_not_the_catch() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let (found, lines) = captured_with(|| {
            resolve_uncensored_text_understudy(&conn, &NoApiKeys, text_lookup(Some("cp-x")))
        });
        assert!(found.is_none());
        let by_id = with(&lines, "Error finding entity by ID");
        assert_eq!(by_id.len(), 1, "{lines:?}");
        assert!(
            by_id[0].starts_with("ERROR ")
                && by_id[0].contains("collection=connection_profiles")
                && by_id[0].contains("id=cp-x")
                && by_id[0].contains("error="),
            "{}",
            by_id[0]
        );
        assert_eq!(
            with(&lines, "is missing, not owned, or not eligible").len(),
            1,
            "{lines:?}"
        );
        let all = with(&lines, "Error finding all entities");
        assert_eq!(all.len(), 1, "{lines:?}");
        assert!(all[0].starts_with("ERROR ") && all[0].contains("collection=connection_profiles"));
        assert!(
            with(&lines, "understudy lookup failed").is_empty(),
            "{lines:?}"
        );

        let (found, lines) = captured_with(|| {
            resolve_uncensored_image_understudy(&conn, &NoApiKeys, image_lookup(Some("ip-x")))
        });
        assert!(found.is_none());
        let by_id = with(&lines, "Error finding entity by ID");
        assert_eq!(by_id.len(), 1, "{lines:?}");
        assert!(
            by_id[0].contains("collection=image_profiles") && by_id[0].contains("id=ip-x"),
            "{}",
            by_id[0]
        );
        assert_eq!(
            with(&lines, "is missing or not owned").len(),
            1,
            "{lines:?}"
        );
        let all = with(&lines, "Error finding all entities");
        assert_eq!(all.len(), 1, "{lines:?}");
        assert!(all[0].contains("collection=image_profiles"));
        assert!(
            with(&lines, "understudy lookup failed").is_empty(),
            "{lines:?}"
        );
    }

    /// The real API-key resolvers are v4's fallback read too — the
    /// repository's ERROR and "no key", never `decryptKey`'s WARN.
    #[test]
    fn a_failed_api_key_read_logs_the_repository_line() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let (key, lines) = captured_with(|| ConnApiKeys::new(&conn).try_resolve("k-1", "u-1"));
        assert_eq!(key, Ok(None));
        let hit = with(&lines, "Error finding API key by ID and user ID");
        assert_eq!(hit.len(), 1, "{lines:?}");
        assert!(
            hit[0].starts_with("ERROR ")
                && hit[0].contains("collection=connection_profiles")
                && hit[0].contains("keyId=k-1")
                && hit[0].contains("userId=u-1"),
            "{}",
            hit[0]
        );
        // The pooled form, over a pool that cannot open: the same line.
        let (_dir, db) = db_with_a_failing_read_pool();
        let keys = DbApiKeys(db);
        let (key, lines) = captured_with(|| keys.resolve("k-2", "u-1"));
        assert_eq!(key, None);
        assert_eq!(
            with(&lines, "Error finding API key by ID and user ID").len(),
            1,
            "{lines:?}"
        );
        // A key found is silent.
        conn.execute_batch(
            "CREATE TABLE api_keys (id TEXT, provider TEXT, label TEXT, key_value TEXT, \
             isActive INTEGER, lastUsed TEXT, userId TEXT, createdAt TEXT, updatedAt TEXT)",
        )
        .unwrap();
        let (_, lines) = captured_with(|| ConnApiKeys::new(&conn).resolve("k-1", "u-1"));
        assert!(
            with(&lines, "Error finding API key").is_empty(),
            "{lines:?}"
        );
    }

    /// The pool arm, through both wrappers, and its silence leg (a pool that
    /// opens is silent here even when every read inside it fails).
    #[test]
    fn a_pool_failure_logs_the_catch_line_and_answers_nobody() {
        let (_dir, db) = db_with_a_failing_read_pool();
        let (found, lines) =
            captured_with(|| resolve_text_understudy_on(&db, &NoApiKeys, text_lookup(None)));
        assert!(found.is_none());
        let hit = with(&lines, "Text understudy lookup failed");
        assert_eq!(hit.len(), 1, "{lines:?}");
        assert!(
            hit[0].starts_with("ERROR quilltap::concierge_understudy ")
                && hit[0].contains("error="),
            "{}",
            hit[0]
        );
        let (found, lines) =
            captured_with(|| resolve_image_understudy_on(&db, &NoApiKeys, image_lookup(None)));
        assert!(found.is_none());
        assert_eq!(
            with(&lines, "Image understudy lookup failed").len(),
            1,
            "{lines:?}"
        );

        let (_dir2, healthy) = db_with_no_tables();
        let (_, lines) =
            captured_with(|| resolve_text_understudy_on(&healthy, &NoApiKeys, text_lookup(None)));
        assert!(
            with(&lines, "understudy lookup failed").is_empty(),
            "{lines:?}"
        );
        let (_, lines) =
            captured_with(|| resolve_image_understudy_on(&healthy, &NoApiKeys, image_lookup(None)));
        assert!(
            with(&lines, "understudy lookup failed").is_empty(),
            "{lines:?}"
        );
    }

    /// Each of the four production sites routes its pool failure through the
    /// wrapper — one pin per site, so restoring a silent `.ok().flatten()` at
    /// any ONE of them reddens exactly its row.
    #[test]
    fn every_production_site_logs_its_pool_failure() {
        let (_dir, db) = db_with_a_failing_read_pool();
        let rt = rt();
        let policy = crate::services::dangerous_content::resolver::test_policy("AUTO_ROUTE", None);
        let chat = json!({ "id": "chat-1", "participants": [] });
        let message = json!({ "id": "m-1" });

        // 1. The text failover's router.
        let router = DangerContentRouter::new(db.clone(), NoApiKeys);
        let (found, lines) =
            captured_with(|| rt.block_on(router.resolve_understudy("u-1", &policy, &[], &[])));
        assert!(found.is_none());
        assert_eq!(
            with(&lines, "Text understudy lookup failed").len(),
            1,
            "router: {lines:?}"
        );

        // 2. The image failover chokepoint's default source.
        let source = ImageUnderstudySource {
            db: &db,
            api_keys: &NoApiKeys,
            user_id: "u-1",
            uncensored_image_profile_id: None,
        };
        let (found, lines) = captured_with(|| rt.block_on(source.resolve(&[])));
        assert!(found.is_none());
        assert_eq!(
            with(&lines, "Image understudy lookup failed").len(),
            1,
            "image source: {lines:?}"
        );

        // 3. The text retry — "nobody", i.e. v4's 409 `no-understudy`.
        let (found, lines) = captured_with(|| {
            rt.block_on(resolve_text_retry_understudy(
                &db, &NoApiKeys, "u-1", &chat, None, &message,
            ))
        });
        assert!(matches!(found, Err(RetryUncensoredRefusal::NoUnderstudy)));
        assert_eq!(
            with(&lines, "Text understudy lookup failed").len(),
            1,
            "text retry: {lines:?}"
        );

        // 4. The image retry.
        let (found, lines) = captured_with(|| {
            rt.block_on(resolve_image_retry_understudy(
                &db,
                &NoApiKeys,
                "u-1",
                &chat,
                None,
                &[],
                None,
                AnsweredBy {
                    provider: None,
                    model_name: None,
                },
            ))
        });
        assert!(matches!(found, Err(RetryUncensoredRefusal::NoUnderstudy)));
        assert_eq!(
            with(&lines, "Image understudy lookup failed").len(),
            1,
            "image retry: {lines:?}"
        );
    }
}
