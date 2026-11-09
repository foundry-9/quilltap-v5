//! P4.D374 — inline Brahma connection-profile resolution (v4 `c539c82e0`,
//! `lib/services/brahma-console/inline-profile.ts`).
//!
//! Decides which connection profile (model) answers an inline `@Brahma:` /
//! `@Brahma?` consultation from inside a Salon chat. This is the one place that
//! order is spelt out:
//!
//!  1. the chat's project's `brahmaConnectionProfileId` (project properties.json),
//!  2. the global `brahmaConsole.inlineConnectionProfileId` instance setting,
//!  3. the user's default connection profile.
//!
//! A configured id that no longer names a profile the user owns falls through to
//! the next step with a warning — a deleted profile never leaves Brahma mute.
//!
//! The standalone Brahma Console does not come through here: it keeps its own
//! per-chat picker ([`super::resolve_brahma_connection_profile`]).
//!
//! ## Where a read failure lands (ruling L-6, MEASURED at the pin)
//!
//! v4 try-wraps steps 1–2 and the settings read; the global step's
//! `findConfiguredProfile` and the default step are not. But every read they
//! make is itself a fallback `safeQuery` at the pin: `connections.findById` is
//! the base `_findById` (`base.repository.ts:247-258` — ERROR `Error finding
//! entity by ID` → `null`) and `connections.findDefault` wraps
//! `findOneByFilter` (`connection-profiles.repository.ts:69-85`, whose inner
//! `safeQuery` logs `Error finding entity by filter` → `null`). So nothing
//! propagates out of `resolveInlineBrahmaConnectionProfile`: a failed
//! configured-profile read is the stale-id WARN and the next step, a failed
//! default read is the `profileId: null` DEBUG and `no-profile`. This port
//! takes exactly those fallbacks ([`fallback::find_by_id_or_none`],
//! [`fallback::find_one_by_filter_or_none`]), so the function answers an
//! `Option` and `run_brahma_query`'s `no-profile` arm is the only landing.
//! The same holds for step 1's chat read (`_findById` again — its WARN is
//! reachable only through a pool failure here) and the settings read
//! (`readJsonSetting` never throws); step 2's project read DOES throw in v4
//! (`applyOverlayOne` on an unusable store) and is the one reachable WARN arm.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{chats_read, connection_profiles, fallback, instance_settings, projects};

/// v4 `createServiceLogger('BrahmaInlineProfile')` — the child bindings ride first.
macro_rules! inline_profile_line {
    ($level:ident, $($rest:tt)*) => {
        tracing::$level!(
            service = "BrahmaInlineProfile",
            module = "service",
            $($rest)*
        )
    };
}

/// A string field of a profile / chat / project object.
fn str_field<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str)
}

/// v4 `findConfiguredProfile(repos, userId, profileId, source, context)`: the
/// profile when it exists AND belongs to `user_id`; otherwise the WARN
/// `{source, profileId, ...context}` and `None`.
fn find_configured_profile(
    db: &Db,
    user_id: &str,
    profile_id: &str,
    source: &'static str,
    chat_id: &str,
    project_id: Option<&str>,
) -> Option<Value> {
    let profile = fallback::find_by_id_or_none("connection_profiles", profile_id, || {
        db.read_main(|c| connection_profiles::find_by_id(c, profile_id))
    });
    if let Some(profile) = profile {
        if str_field(&profile, "userId") == Some(user_id) {
            return Some(profile);
        }
    }
    match project_id {
        Some(project_id) => inline_profile_line!(
            warn,
            source = source,
            profileId = %profile_id,
            chatId = %chat_id,
            projectId = %project_id,
            "Inline Brahma connection profile no longer exists; falling through"
        ),
        None => inline_profile_line!(
            warn,
            source = source,
            profileId = %profile_id,
            chatId = %chat_id,
            "Inline Brahma connection profile no longer exists; falling through"
        ),
    }
    None
}

/// v4 `repos.projects.findById(projectId)` (the overlay read — it surfaces the
/// vault key) as an `error.message`-bearing `Result`.
fn read_project(db: &Db, project_id: &str) -> Result<Option<Value>, String> {
    let read = db.read_main(|main| {
        db.read_mount_index(|mount| {
            Ok(projects::ProjectsRepository::new(main, mount).find_by_id(project_id))
        })
    });
    match read {
        Ok(Ok(project)) => Ok(project),
        Ok(Err(crate::db::document_store_overlay::OverlayError::Db(e))) | Err(e) => {
            Err(fallback::error_text(&e))
        }
        Ok(Err(other)) => Err(other.to_string()),
    }
}

/// v4 `resolveInlineBrahmaConnectionProfile(repos, userId, chatId)` — the
/// connection profile an inline Brahma query in `chat_id` should use; `None`
/// only when no step resolves (the user has no default profile).
pub fn resolve_inline_brahma_connection_profile(
    db: &Db,
    user_id: &str,
    chat_id: &str,
) -> Option<Value> {
    // 1. Project override.
    let mut project_id: Option<String> = None;
    match db.read_main(|c| Ok(chats_read::find_by_id_or_none(c, chat_id))) {
        Ok(chat) => {
            project_id = chat
                .as_ref()
                .and_then(|c| str_field(c, "projectId"))
                .map(str::to_string);
        }
        Err(e) => inline_profile_line!(
            warn,
            chatId = %chat_id,
            error = %fallback::error_text(&e),
            "Could not read chat for inline Brahma profile resolution; skipping project override"
        ),
    }

    // `if (projectId)` — an empty string is falsy.
    if let Some(project_id) = project_id.as_deref().filter(|p| !p.is_empty()) {
        match read_project(db, project_id) {
            Ok(project) => {
                let project_profile_id = project
                    .as_ref()
                    .and_then(|p| str_field(p, "brahmaConnectionProfileId"))
                    .filter(|p| !p.is_empty());
                if let Some(project_profile_id) = project_profile_id {
                    if let Some(profile) = find_configured_profile(
                        db,
                        user_id,
                        project_profile_id,
                        "project",
                        chat_id,
                        Some(project_id),
                    ) {
                        inline_profile_line!(
                            debug,
                            chatId = %chat_id,
                            projectId = %project_id,
                            profileId = %str_field(&profile, "id").unwrap_or_default(),
                            "Inline Brahma using project connection profile"
                        );
                        return Some(profile);
                    }
                }
            }
            Err(error) => inline_profile_line!(
                warn,
                chatId = %chat_id,
                projectId = %project_id,
                error = %error,
                "Could not read project for inline Brahma profile resolution; skipping project override"
            ),
        }
    }

    // 2. Global instance setting.
    let mut global_profile_id: Option<String> = None;
    match db.read_main(instance_settings::read_brahma_console_settings) {
        Ok(settings) => global_profile_id = settings.inline_connection_profile_id.flatten(),
        Err(e) => inline_profile_line!(
            warn,
            chatId = %chat_id,
            error = %fallback::error_text(&e),
            "Could not read Brahma Console settings; skipping global inline profile"
        ),
    }
    if let Some(global_profile_id) = global_profile_id.as_deref().filter(|g| !g.is_empty()) {
        if let Some(profile) =
            find_configured_profile(db, user_id, global_profile_id, "global", chat_id, None)
        {
            inline_profile_line!(
                debug,
                chatId = %chat_id,
                profileId = %str_field(&profile, "id").unwrap_or_default(),
                "Inline Brahma using global connection profile"
            );
            return Some(profile);
        }
    }

    // 3. The user's default profile.
    let fallback_profile = fallback::find_one_by_filter_or_none("connection_profiles", || {
        db.read_main(|c| connection_profiles::find_default(c, user_id))
    });
    // `profileId: fallback?.id ?? null` — logged even when nothing resolves.
    inline_profile_line!(
        debug,
        chatId = %chat_id,
        profileId = %fallback_profile
            .as_ref()
            .and_then(|p| str_field(p, "id"))
            .unwrap_or("null"),
        "Inline Brahma using default connection profile"
    );
    fallback_profile
}
