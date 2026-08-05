//! v4 `lib/backup/restore/restore.ts:35` — the restore orchestrator.
//!
//! (The file is `orchestrator.rs` rather than `restore.rs` so a `restore` module
//! does not nest inside a `restore` module; `restore()` is re-exported from the
//! parent, so the call path is unchanged.)
//!
//! Extract once, optionally wipe (`replace`) or remap (`new-account`), then
//! re-insert every entity in dependency order **with its backup id preserved**
//! (v4's `CreateOptions.id`), so cross-references need no fixing up. Per-row
//! failures become `warnings[]` entries rather than aborting the restore; the
//! phase numbering in the comments is v4's.
//!
//! ## What is NOT preserved, deliberately
//!
//! `createdAt` / `updatedAt`. v4 destructures both off every row and passes only
//! `{id}`, so each restored row is stamped with the write clock — the one
//! exception is `llmLogs`, which passes `{id, createdAt}` (`:333`). Ported
//! exactly; the differential normalizes the write clock and pins the llm-log
//! `createdAt` as data.
//!
//! ## Deliberate divergences, all named
//!
//! - **Phases 23 and 24 (npm plugins, theme bundles)** copy into the host's
//!   directories when it declares them ([`HostDirs`]) and are a documented no-op
//!   when it does not — v5's hosts do not ship an npm-plugin loader. The counters
//!   report what actually happened, so a no-op reads as `0`, never as silence.
//! - **The `finally`** that removes the extract directory is `Drop` on
//!   [`ExtractedBackup`], not a `finally` block.
//! - **`isLLMLogsDegraded()` / `isMountIndexDegraded()`** have no v5 analogue:
//!   a partition is either opened or absent. The absent case takes v4's degraded
//!   arm, warning string included.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;
use serde_json::Value;

use super::archive::{get_file_from_extracted_backup, parse_backup_zip, ExtractedBackup};
use super::rows::{b, de_or_default, embedding, n, ob, obj, on, os, s, sa};
use super::{ProfileCounts, RestoreSummary, TemplateCounts};
use crate::db::runtime::{Db, WriterSet};
use crate::db::DbError;
use crate::services::backup::{BackupHost, HostDirs};
use crate::services::file_storage::PixelCodec;
use crate::services::profile_names::{make_unique_profile_name, normalize_profile_name};

/// v4 `RestoreOptions.mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreMode {
    Replace,
    NewAccount,
}

impl RestoreMode {
    /// v4's route guard (`system/restore/route.ts:202`): anything other than the
    /// two spellings is a bad request.
    pub fn parse(mode: &str) -> Option<Self> {
        match mode {
            "replace" => Some(RestoreMode::Replace),
            "new-account" => Some(RestoreMode::NewAccount),
            _ => None,
        }
    }
}

/// v4 `restore(zipPath, {mode, targetUserId})`.
///
/// `replace` wipes first, through the already-landed
/// [`crate::services::delete_all::delete_user_data`]; `new-account` remaps every
/// UUID first (P4.9G6's `remap_backup_data` — see the ACTIVATE-AT-UNIFY marker
/// below).
pub async fn restore(
    db: &Db,
    host: &dyn BackupHost,
    zip_path: &Path,
    mode: RestoreMode,
    target_user_id: &str,
) -> Result<RestoreSummary, String> {
    let mut extracted = parse_backup_zip(zip_path, &host.temp_dir())?;

    if mode == RestoreMode::Replace {
        crate::services::delete_all::delete_user_data(db, target_user_id)
            .await
            .map_err(|e| e.to_string())?;
    }

    // v4 `:57-61`. P4.9G6's `remap_backup_data` is the whole of it: fresh ids for
    // every entity, every cross-reference rewritten to match, ownership moved to
    // the target user. It is differential-proven byte-for-byte over 19 cases, and
    // `p4_9g6_seam_contract` pins this call's signature at compile time.
    //
    // The REMAPPED collections drive every write; `extracted.data` keeps the
    // ORIGINAL rows, because the archive's on-disk file and blob names are keyed
    // by the original ids and do not move. Phase 5 and 22f pair the two by index,
    // exactly as v4 does (`:133-136`, `:505-508`).
    let remapped = if mode == RestoreMode::NewAccount {
        let mut remapper = crate::services::backup::uuid_remapper::UuidRemapper::new();
        Some(crate::services::backup::uuid_remap::remap_backup_data(
            &extracted.data,
            target_user_id,
            &mut remapper,
        ))
    } else {
        None
    };

    let codec = host.pixel_codec();
    let dirs = host.host_dirs();
    let user_id = target_user_id.to_string();
    // Step 25's reconcile takes the injected wall clock (its staleness window's
    // origin) — the host's, same as the boot caller.
    let now_ms = host.now_ms();
    db.write(move |ws| {
        Ok(restore_on_writer(
            ws,
            &mut extracted,
            remapped,
            &user_id,
            codec,
            dirs,
            now_ms,
        ))
    })
    .await
    .map_err(|e: DbError| e.to_string())
}

/// Every phase's `{id: row.id}`. v4 passes NO timestamps, so each restored row
/// is stamped with the write clock; every repository declares its own
/// `CreateOptions`, hence the module path.
macro_rules! copts {
    ($id:expr, $($m:tt)+) => {
        $($m)+ {
            id: $id,
            created_at: now(),
            updated_at: now(),
        }
    };
}

/// The per-row tolerance for the phases whose summary field is an INPUT length
/// rather than a counter (tags, the three profile families, characters,
/// memories): v4 still wraps each in a `try` that pushes one warning and keeps
/// going, it just never counts the successes.
macro_rules! warn_only {
    ($warnings:expr, $label:expr, $body:expr) => {
        if let Err(e) = $body {
            $warnings.push(format!("{}: {}", $label, e));
        }
    };
}

/// The per-row tolerance every phase shares: v4 wraps each entity in a `try`
/// that pushes one `warnings[]` line and keeps going.
macro_rules! warn_row {
    ($warnings:expr, $counter:expr, $label:expr, $body:expr) => {
        match $body {
            Ok(_) => $counter += 1,
            Err(e) => $warnings.push(format!("{}: {}", $label, e)),
        }
    };
}

/// The whole restore, on the writer thread — one place holding all three
/// partitions' connections, which is what lets the cross-partition phases
/// (doc-store rows in the mount index, llm logs in their own file) run in the
/// same pass v4 runs them in.
#[allow(clippy::too_many_arguments)]
fn restore_on_writer(
    ws: &mut WriterSet,
    extracted: &mut ExtractedBackup,
    remapped: Option<crate::services::backup::BackupData>,
    target_user_id: &str,
    codec: Arc<dyn PixelCodec>,
    dirs: HostDirs,
    now_ms: i64,
) -> RestoreSummary {
    let backup_format = extracted.backup_format();
    let root_path = extracted.root_path.clone();
    // v4's `data` (what gets written) vs `parsedData` (what the on-disk names are
    // keyed by). They are the same object in `replace` mode and diverge in
    // `new-account`, where only the former is remapped.
    let data: &crate::services::backup::BackupData = remapped.as_ref().unwrap_or(&extracted.data);

    let main = ws.main().connection();
    let mount = ws.mount_index().map(|w| w.connection());
    let llm = ws.llm_logs().map(|w| w.connection());

    let mut w: Vec<String> = Vec::new();
    let mut c = Counters::default();

    // ── 1. Tags (no dependencies) ────────────────────────────────────────────
    {
        let repo = crate::db::tags::TagsRepository::new(main);
        for tag in &data.tags {
            let name = s(tag, "name");
            let lower = os(tag, "nameLower").filter(|l| !l.is_empty());
            let create = crate::db::tags::TagCreate {
                // `repos.*` is `getUserRepositories(targetUserId)`, whose `create`
                // spreads `{...data, userId: this.userId}` (`user-scoped.ts:84`) —
                // so every user-scoped phase RE-OWNS the row to the target user,
                // discarding the archive's `userId`. Only the `globalRepos.*`
                // phases keep what the archive said.
                user_id: target_user_id.to_string(),
                name: name.clone(),
                // v4 `nameLower: tagData.nameLower || tagData.name.toLowerCase()`.
                name_lower: Some(lower.unwrap_or_else(|| name.to_lowercase())),
                quick_hide: ob(tag, "quickHide"),
                visual_style: de_opt(tag, "visualStyle"),
            };
            warn_only!(
                w,
                format!("Failed to restore tag \"{name}\""),
                repo.create(&create, &copts!(id_of(tag), crate::db::tags::CreateOptions))
            );
        }
    }

    // ── 2. Connection profiles — rename on name collision ────────────────────
    {
        let repo = crate::db::connection_profiles::ConnectionProfilesRepository::new(main);
        // v4 `repos.connections.findAll()` (`:84`) to seed the taken-name set. On
        // an instance that never provisioned the table v4's `safeQuery` yields
        // `[]`; v5 must not raise `no such table` (the unit-1/2 `if_table` rule —
        // this is the read the order singles out).
        let mut taken: HashSet<String> = HashSet::new();
        if table_exists(main, "connection_profiles") {
            if let Ok(existing) = crate::db::connection_profiles::find_all(main) {
                for p in &existing {
                    taken.insert(normalize_profile_name(&s(p, "name")));
                }
            }
        }
        for p in &data.connection_profiles {
            let original = s(p, "name");
            let unique = make_unique_profile_name(&original, &taken);
            taken.insert(normalize_profile_name(&unique));
            let create = crate::db::connection_profiles::CpCreate {
                user_id: target_user_id.to_string(),
                name: unique,
                provider: s(p, "provider"),
                transport: str_or(p, "transport", "api"),
                courier_delta_mode: b(p, "courierDeltaMode", false),
                // v4 `:89`: apiKeyId is deliberately NOT restored — keys are
                // encrypted with a pepper the archive does not carry.
                api_key_id: None,
                base_url: os(p, "baseUrl"),
                model_name: s(p, "modelName"),
                parameters: obj(p, "parameters", serde_json::json!({})),
                is_default: b(p, "isDefault", false),
                is_cheap: b(p, "isCheap", false),
                allow_web_search: b(p, "allowWebSearch", false),
                use_native_web_search: b(p, "useNativeWebSearch", false),
                allow_tool_use: b(p, "allowToolUse", true),
                pseudo_tool_mode: str_or(p, "pseudoToolMode", "auto"),
                model_class: os(p, "modelClass"),
                max_context: on(p, "maxContext"),
                max_tokens: on(p, "maxTokens"),
                is_dangerous_compatible: b(p, "isDangerousCompatible", false),
                supports_image_upload: b(p, "supportsImageUpload", false),
                tags: sa(p, "tags"),
                sort_index: n(p, "sortIndex", 0.0),
                total_tokens: n(p, "totalTokens", 0.0),
                total_prompt_tokens: n(p, "totalPromptTokens", 0.0),
                total_completion_tokens: n(p, "totalCompletionTokens", 0.0),
                message_count: n(p, "messageCount", 0.0),
            };
            warn_only!(
                w,
                format!("Failed to restore connection profile \"{original}\""),
                repo.create(
                    &create,
                    &copts!(id_of(p), crate::db::connection_profiles::CreateOptions)
                )
            );
        }
    }

    // ── 3. Image profiles ────────────────────────────────────────────────────
    {
        let repo = crate::db::image_profiles::ImageProfilesRepository::new(main);
        for p in &data.image_profiles {
            let create = crate::db::image_profiles::IpCreate {
                user_id: target_user_id.to_string(),
                name: s(p, "name"),
                provider: s(p, "provider"),
                api_key_id: None,
                base_url: os(p, "baseUrl"),
                model_name: s(p, "modelName"),
                parameters: obj(p, "parameters", serde_json::json!({})),
                is_default: b(p, "isDefault", false),
                is_dangerous_compatible: b(p, "isDangerousCompatible", false),
                tags: sa(p, "tags"),
            };
            warn_only!(
                w,
                format!("Failed to restore image profile \"{}\"", s(p, "name")),
                repo.create(
                    &create,
                    &copts!(id_of(p), crate::db::image_profiles::CreateOptions)
                )
            );
        }
    }

    // ── 4. Embedding profiles ────────────────────────────────────────────────
    {
        let repo = crate::db::embedding_profiles::EmbeddingProfilesRepository::new(main);
        for p in &data.embedding_profiles {
            let create = crate::db::embedding_profiles::EpCreate {
                user_id: target_user_id.to_string(),
                name: s(p, "name"),
                provider: s(p, "provider"),
                api_key_id: None,
                base_url: os(p, "baseUrl"),
                model_name: s(p, "modelName"),
                dimensions: on(p, "dimensions"),
                truncate_to_dimensions: on(p, "truncateToDimensions"),
                normalize_l2: b(p, "normalizeL2", true),
                is_default: b(p, "isDefault", false),
                tags: sa(p, "tags"),
            };
            warn_only!(
                w,
                format!("Failed to restore embedding profile \"{}\"", s(p, "name")),
                repo.create(
                    &create,
                    &copts!(id_of(p), crate::db::embedding_profiles::CreateOptions)
                )
            );
        }
    }

    // ── 5. Files — MOVED. See "phase 5 runs late" below. ─────────────────────
    //
    // ## ⚠ RULED DIVERGENCE (2026-07-25) — phase 5 runs AFTER the doc-store family
    //
    // v4 runs files fifth, and it cannot work there. Both bridges the phase
    // writes through resolve a document store that does not exist yet:
    //
    // - A project-less file goes to `writeUserUploadToMountStore`, which calls
    //   `getUserUploadsStore()` → `docMountPoints.findById(...)` and **throws
    //   `Quilltap Uploads mount has not been provisioned`** when it misses
    //   (`user-uploads-bridge.ts:98`). In `replace` mode `deleteUserData` has
    //   just run `DELETE FROM doc_mount_points` (`delete-service.ts:72`), so it
    //   always misses. (`instance_settings` is deliberately NOT cleared, so the
    //   pointer survives and dangles.)
    // - A project-bound file goes to `writeProjectFileToMountStore`, which
    //   throws `Project <id> has no linked database-backed document store`
    //   (`project-store-bridge.ts:131`) — and projects do not restore until
    //   phase 13, eight phases later, in EITHER mode.
    //
    // So on any fresh or wiped target v4 restores **no user file at all**, in
    // either mode, and reports each one as a per-file warning. That is the same
    // family as the two findings the 2026-07-25 ruling covers, found while
    // implementing it, and the ruling's principle decides it: restore should
    // restore.
    //
    // The fix is the smallest one that satisfies the real dependency: files run
    // after the doc-store family (22a restores the mount points, including the
    // built-ins, with their archive ids — which is exactly what the surviving
    // `instance_settings` pointer expects) and therefore also after projects and
    // groups (13/13a). **No write changed, only when it happens** — and v4's own
    // comment calls this list "dependency order" (`restore.ts:65`), so this is
    // v4's stated intent, applied.
    //
    // Everything else keeps v4's order exactly. `system_restore_state` asserts
    // the divergence in both directions (v4 restores zero files, v5 restores
    // them).
    //
    // ## ⚠ RULED DIVERGENCE (2026-07-26) — the slot is KEPT, and it earns its keep
    //
    // v4's `c1507f47` moved ITS files phase to `22a-bis` (after 22a, before 22b).
    // v5 was asked to follow and was overruled: **v5 keeps this later slot.** The
    // reason is not that one slot's hazard is milder — both slots have one — but
    // that only this slot makes v4's own named repair WRITABLE. At `22a-bis` the
    // archived link and blob rows have not been restored yet, so a replay has
    // nothing to consult; here it does. See `carried_store_rows` below for the
    // check that repair became, and `status-log.md` → "Ruling — the restore
    // file-replay dedupe". Aligning the two phase orders fails
    // `system_restore_state` deliberately.

    // ── 6. Characters (vault-backed) ─────────────────────────────────────────
    if let Some(mount) = mount {
        for ch in &data.characters {
            let name = s(ch, "name");
            warn_only!(
                w,
                format!("Failed to restore character \"{name}\""),
                restore_one_character(main, mount, target_user_id, ch)
            );
        }
    } else if !data.characters.is_empty() {
        w.push("Characters were not restored — mount-index database is unavailable".to_string());
    }

    // ── 7. Chats (with messages) ─────────────────────────────────────────────
    {
        let chats = crate::db::chats::ChatsRepository::new(main);
        let messages = crate::db::chats_messages::ChatMessagesRepository::new(main);
        for chat in &data.chats {
            let title = s(chat, "title");
            let id = id_of(chat);
            let mut create: crate::db::chats::ChatCreate =
                match serde_json::from_value(chat.clone()) {
                    Ok(v) => v,
                    Err(e) => {
                        w.push(format!("Failed to restore chat \"{title}\": {e}"));
                        continue;
                    }
                };
            // The user-scoped `create` re-owns the chat (see phase 1).
            create.user_id = target_user_id.to_string();
            if let Err(e) = chats.create(
                &create,
                &copts!(id.clone(), crate::db::chats::CreateOptions),
            ) {
                w.push(format!("Failed to restore chat \"{title}\": {e}"));
                continue;
            }
            for message in chat
                .get("messages")
                .and_then(Value::as_array)
                .unwrap_or(&vec![])
            {
                let event: crate::db::chats_messages::ChatEventInput =
                    match serde_json::from_value(message.clone()) {
                        Ok(v) => v,
                        Err(e) => {
                            w.push(format!(
                                "Failed to restore message in chat \"{title}\": {e}"
                            ));
                            continue;
                        }
                    };
                match messages.add_message(&id, &event) {
                    Ok(()) => c.messages += 1,
                    Err(e) => w.push(format!(
                        "Failed to restore message in chat \"{title}\": {e}"
                    )),
                }
            }
        }
    }

    // ── 9. Memories — ids ARE preserved (v4 `restore.ts:189`), as they are for
    //    every other entity. `characterId` / `aboutCharacterId` already point at
    //    preserved character ids, so nothing dangles there either. Legacy
    //    `personaId` is stripped.
    //
    //    The memory's OWN id is the load-bearing one: memories reference each
    //    other through `relatedMemoryIds`, which uuid-remap rewrites in lockstep
    //    with `id` for new-account restores (`uuid_remap.rs:159-173` — one shared
    //    memo, so an edge and its target resolve to the same new value). Minting
    //    a fresh id here would leave every one of those edges pointing at a
    //    memory that no longer exists, quietly flattening the Commonplace Book's
    //    graph on restore.
    //
    //    ⚠ This was v4's own bug until `4ac66c29` (2026-07-30) — memories were
    //    the ONLY entity restored without their backed-up id, because
    //    `UserScopedMemoriesRepository` scopes by character rather than userId,
    //    so it is hand-written and never forwarded the `CreateOptions` the
    //    generic wrapper passes through. v5 had ported the bug faithfully. v5
    //    needs no repository change to match the fix: `db::memories::
    //    MemoriesRepository::create` has always taken `&CreateOptions`.
    {
        let repo = crate::db::memories::MemoriesRepository::new(main);
        for m in &data.memories {
            let create = crate::db::memories::MemCreate {
                character_id: s(m, "characterId"),
                about_character_id: os(m, "aboutCharacterId"),
                chat_id: os(m, "chatId"),
                project_id: os(m, "projectId"),
                content: s(m, "content"),
                summary: s(m, "summary"),
                keywords: sa(m, "keywords"),
                tags: sa(m, "tags"),
                importance: n(m, "importance", 5.0),
                embedding: embedding(m, "embedding"),
                source: str_or(m, "source", "AUTO"),
                witnessed_context: os(m, "witnessedContext"),
                occurred_at: os(m, "occurredAt"),
                narrative_time: os(m, "narrativeTime"),
                entities: sa(m, "entities"),
                kind: str_or(m, "kind", "semantic"),
                source_message_id: os(m, "sourceMessageId"),
                last_accessed_at: os(m, "lastAccessedAt"),
                reinforcement_count: n(m, "reinforcementCount", 0.0),
                last_reinforced_at: os(m, "lastReinforcedAt"),
                related_memory_ids: sa(m, "relatedMemoryIds"),
                reinforced_importance: n(m, "reinforcedImportance", 0.0),
            };
            warn_only!(
                w,
                "Failed to restore memory".to_string(),
                repo.create(
                    &create,
                    &copts!(id_of(m), crate::db::memories::CreateOptions)
                )
            );
        }
    }

    // ── 10. Prompt templates — id NOT preserved (v4 `:251`), userId retargeted ─
    {
        let repo = crate::db::prompt_templates::PromptTemplatesRepository::new(main);
        for t in &data.prompt_templates {
            let create = crate::db::prompt_templates::PtCreate {
                user_id: Some(target_user_id.to_string()),
                name: s(t, "name"),
                content: s(t, "content"),
                description: os(t, "description"),
                is_built_in: b(t, "isBuiltIn", false),
                category: os(t, "category"),
                model_hint: os(t, "modelHint"),
                tags: sa(t, "tags"),
            };
            warn_row!(
                w,
                c.prompt_templates,
                format!("Failed to restore prompt template \"{}\"", s(t, "name")),
                repo.create(
                    &create,
                    &copts!(new_id(), crate::db::prompt_templates::CreateOptions)
                )
            );
        }
    }

    // ── 11. Roleplay templates — same shape ──────────────────────────────────
    {
        let repo = crate::db::roleplay_templates::RoleplayTemplatesRepository::new(main);
        for t in &data.roleplay_templates {
            let create = crate::db::roleplay_templates::RtCreate {
                user_id: Some(target_user_id.to_string()),
                name: s(t, "name"),
                description: os(t, "description"),
                system_prompt: s(t, "systemPrompt"),
                is_built_in: b(t, "isBuiltIn", false),
                tags: sa(t, "tags"),
                delimiters: de_or_default(t, "delimiters"),
                rendering_patterns: de_or_default(t, "renderingPatterns"),
                dialogue_detection: de_opt(t, "dialogueDetection"),
                narration_delimiters: de_opt(t, "narrationDelimiters")
                    // v4's Zod default for `narrationDelimiters` is `"*"`.
                    .unwrap_or_else(|| {
                        crate::db::roleplay_templates::StringOrPair::Single("*".to_string())
                    }),
            };
            warn_row!(
                w,
                c.roleplay_templates,
                format!("Failed to restore roleplay template \"{}\"", s(t, "name")),
                repo.create(
                    &create,
                    &copts!(new_id(), crate::db::roleplay_templates::CreateOptions)
                )
            );
        }
    }

    // ── 12. Provider models (global cache) — UPSERT, id not preserved ────────
    {
        let repo = crate::db::provider_models::ProviderModelsRepository::new(main);
        for m in &data.provider_models {
            let create = crate::db::provider_models::PmCreate {
                provider: s(m, "provider"),
                model_id: s(m, "modelId"),
                model_type: str_or(m, "modelType", "chat"),
                display_name: s(m, "displayName"),
                base_url: os(m, "baseUrl"),
                context_window: on(m, "contextWindow"),
                max_output_tokens: on(m, "maxOutputTokens"),
                deprecated: b(m, "deprecated", false),
                experimental: b(m, "experimental", false),
            };
            warn_row!(
                w,
                c.provider_models,
                format!("Failed to restore provider model \"{}\"", s(m, "modelId")),
                repo.upsert_model(&create)
            );
        }
    }

    // ── 13 / 13a. Projects and groups (store-backed) ─────────────────────────
    //
    // Both `create`s DISCARD the incoming `officialMountPointId` and provision a
    // fresh store, writing the hydrated description/instructions/state/properties
    // into it (v4 `:306-311`). The archive's own store rows still restore at 22a
    // with their original ids; the pointer is the thing that moves, which is why
    // the differential normalizes exactly those two columns and nothing else.
    if let Some(mount) = mount {
        let projects = crate::db::projects::ProjectsRepository::new(main, mount);
        for p in &data.projects {
            let input = crate::db::projects::ProjectCreateInput {
                name: s(p, "name"),
                description: os(p, "description"),
                instructions: os(p, "instructions"),
                state: obj(p, "state", serde_json::json!({})),
                properties: project_properties(p),
            };
            warn_row!(
                w,
                c.projects,
                format!("Failed to restore project \"{}\"", s(p, "name")),
                projects.create(&input, &store_opts(id_of(p)))
            );
        }

        let groups = crate::db::groups::GroupsRepository::new(main, mount);
        for g in &data.groups {
            let input = crate::db::groups::GroupCreateInput {
                name: s(g, "name"),
                description: os(g, "description"),
                instructions: os(g, "instructions"),
                state: obj(g, "state", serde_json::json!({})),
                color: os(g, "color"),
                icon: os(g, "icon"),
            };
            warn_row!(
                w,
                c.groups,
                format!("Failed to restore group \"{}\"", s(g, "name")),
                groups.create(&input, &store_opts(id_of(g)))
            );
        }
    } else {
        if !data.projects.is_empty() {
            w.push("Projects were not restored — mount-index database is unavailable".to_string());
        }
        if !data.groups.is_empty() {
            w.push("Groups were not restored — mount-index database is unavailable".to_string());
        }
    }

    // ── 14. LLM logs — the ONLY phase that preserves `createdAt` (`:333`) ─────
    match llm {
        // v4's `isLLMLogsDegraded()` arm (`:326`), with its warning verbatim.
        None if !data.llm_logs.is_empty() => {
            w.push(
                "LLM logs were not restored because the logs database is in degraded mode"
                    .to_string(),
            );
        }
        None => {}
        Some(llm) => {
            let repo = crate::db::llm_logs::LLMLogsRepository::new(llm);
            for log in &data.llm_logs {
                let create = crate::db::llm_logs::LlCreate {
                    user_id: target_user_id.to_string(),
                    log_type: s(log, "type"),
                    message_id: os(log, "messageId"),
                    chat_id: os(log, "chatId"),
                    character_id: os(log, "characterId"),
                    autonomous_run_id: os(log, "autonomousRunId"),
                    provider: s(log, "provider"),
                    model_name: s(log, "modelName"),
                    request: match de_opt(log, "request") {
                        Some(r) => r,
                        None => {
                            w.push("Failed to restore LLM log: request summary is missing or malformed".to_string());
                            continue;
                        }
                    },
                    response: match de_opt(log, "response") {
                        Some(r) => r,
                        None => {
                            w.push("Failed to restore LLM log: response summary is missing or malformed".to_string());
                            continue;
                        }
                    },
                    usage: de_opt(log, "usage"),
                    cache_usage: de_opt(log, "cacheUsage"),
                    raw_provider_usage: log.get("rawProviderUsage").cloned(),
                    request_hashes: de_opt(log, "requestHashes"),
                    duration_ms: on(log, "durationMs"),
                };
                let created_at = s(log, "createdAt");
                warn_row!(
                    w,
                    c.llm_logs,
                    "Failed to restore LLM log".to_string(),
                    repo.create(
                        &create,
                        &crate::db::llm_logs::CreateOptions {
                            id: id_of(log),
                            created_at: created_at.clone(),
                            updated_at: now(),
                        },
                    )
                );
            }
        }
    }

    // ── 15. Plugin configs — UPSERT by (user, plugin) ────────────────────────
    {
        let repo = crate::db::plugin_config::PluginConfigRepository::new(main);
        for cfg in &data.plugin_configs {
            warn_row!(
                w,
                c.plugin_configs,
                format!(
                    "Failed to restore plugin config for \"{}\"",
                    s(cfg, "pluginName")
                ),
                repo.upsert_for_user_plugin(
                    target_user_id,
                    &s(cfg, "pluginName"),
                    &obj(cfg, "config", serde_json::json!({})),
                    // v4 `restore.ts:301-306` (`7189a968`): the archived
                    // `enabled` rides through, tri-state — an absent flag
                    // leaves the stored one untouched, so a plugin the user
                    // had switched off doesn't come back on.
                    cfg.get("enabled").and_then(serde_json::Value::as_bool),
                )
            );
        }
    }

    // ── 16. Chat settings ────────────────────────────────────────────────────
    {
        let repo = crate::db::chat_settings::ChatSettingsRepository::new(main);
        for row in &data.chat_settings {
            let create: crate::db::chat_settings::ChatSettingsCreate =
                match serde_json::from_value(row.clone()) {
                    Ok(v) => v,
                    Err(e) => {
                        w.push(format!("Failed to restore chat settings: {e}"));
                        continue;
                    }
                };
            warn_row!(
                w,
                c.chat_settings,
                "Failed to restore chat settings".to_string(),
                repo.create(
                    &create,
                    &crate::db::chat_settings::CreateOptions {
                        id: id_of(row),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }

    // ── 17. Folders — userId retargeted (`:378`) ─────────────────────────────
    {
        let repo = crate::db::folders::FoldersRepository::new(main);
        for f in &data.folders {
            let create = crate::db::folders::FolderCreate {
                user_id: target_user_id.to_string(),
                path: s(f, "path"),
                name: s(f, "name"),
                parent_folder_id: os(f, "parentFolderId"),
                project_id: os(f, "projectId"),
            };
            warn_row!(
                w,
                c.folders,
                format!("Failed to restore folder \"{}\"", s(f, "name")),
                repo.create(
                    &create,
                    &crate::db::folders::CreateOptions {
                        id: Some(id_of(f)),
                        created_at: Some(now()),
                        updated_at: Some(now()),
                    },
                )
            );
        }
    }

    // 19. Wardrobe items — DEFERRED to 22f-bis (the vaults don't exist yet).
    // 20. Outfit presets — REMOVED (folded into wardrobeItems at parse time).

    // ── 21. Character plugin data ────────────────────────────────────────────
    {
        let repo = crate::db::character_plugin_data::CharacterPluginDataRepository::new(main);
        for cpd in &data.character_plugin_data {
            let create = crate::db::character_plugin_data::CpdCreate {
                character_id: s(cpd, "characterId"),
                plugin_name: s(cpd, "pluginName"),
                data: parse_cpd_data(cpd),
            };
            warn_row!(
                w,
                c.character_plugin_data,
                format!(
                    "Failed to restore character plugin data for plugin \"{}\"",
                    s(cpd, "pluginName")
                ),
                repo.create(
                    &create,
                    &crate::db::character_plugin_data::CreateOptions {
                        id: id_of(cpd),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }

    // ── 22. Conversation annotations ─────────────────────────────────────────
    {
        let repo =
            crate::db::conversation_annotations::ConversationAnnotationsRepository::new(main);
        for a in &data.conversation_annotations {
            let create = crate::db::conversation_annotations::CaCreate {
                chat_id: s(a, "chatId"),
                message_index: n(a, "messageIndex", 0.0),
                source_message_id: os(a, "sourceMessageId"),
                character_name: s(a, "characterName"),
                content: s(a, "content"),
            };
            warn_row!(
                w,
                c.conversation_annotations,
                "Failed to restore conversation annotation".to_string(),
                repo.create(
                    &create,
                    &crate::db::conversation_annotations::CreateOptions {
                        id: id_of(a),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }

    // ── 22a-22h. The format-3 doc-store family (mount-index partition) ───────
    if let Some(mount) = mount {
        restore_mount_family(
            main,
            mount,
            data,
            &extracted.data,
            &root_path,
            &mut c,
            &mut w,
        );
    } else {
        w.push(
            "Document stores were not restored — mount-index database is unavailable".to_string(),
        );
    }

    // ── 5 (moved). Files — bytes from the extracted tree into the mount stores ─
    //
    // Runs here rather than fifth; the reasoning is at the phase-5 marker above.
    // The mount points (22a), projects and groups now all exist, so both bridges
    // resolve.
    //
    // v4 reads the bytes with `parsedData.files` (ORIGINAL ids, which is what the
    // on-disk names use) and writes the row from `data.files` (remapped in
    // new-account mode). `replace` mode makes the two identical; the pairing is
    // explicit because new-account needs it.
    if let Some(mount) = mount {
        for (i, file) in data.files.iter().enumerate() {
            let name = s(file, "originalFilename");
            // The on-disk lookup MUST use the original row: new-account remaps
            // the id, the archive's file names do not move (v4 `:133-136`).
            let on_disk = extracted.data.files.get(i).unwrap_or(file);
            // ⚠ RULED DIVERGENCE (2026-07-26) — see `carried_store_rows`.
            let carried = carried_store_rows(mount, data, &extracted.data, on_disk);
            match get_file_from_extracted_backup(&root_path, on_disk, backup_format) {
                Some(bytes) => warn_row!(
                    w,
                    c.files,
                    format!("Failed to restore file \"{name}\""),
                    restore_one_file(
                        main,
                        mount,
                        codec.as_ref(),
                        target_user_id,
                        file,
                        &bytes,
                        carried.as_ref()
                    )
                ),
                None => w.push(format!("File not found in backup: {name}")),
            }
        }
    } else if !data.files.is_empty() {
        w.push("Files were not restored — mount-index database is unavailable".to_string());
    }

    // ── 22f-bis — MOVED (P4.6BK). v4 runs the legacy-wardrobe-item restore
    // BETWEEN 22f (blobs) and 22g (archive chunk rows) — `restore.ts:543` vs
    // `:562` — and now that `wardrobe.create` chunks on write (chunk-on-write),
    // the insertion order is observable in `doc_mount_chunks` rowids. The block
    // lives inside `restore_mount_family` at v4's exact position.

    // ── 22i. Chat documents (Document Mode pane state per chat) ──────────────
    {
        let repo = crate::db::chat_documents::ChatDocumentsRepository::new(main);
        for cd in &data.chat_documents {
            let create = crate::db::chat_documents::CdCreate {
                chat_id: s(cd, "chatId"),
                file_path: s(cd, "filePath"),
                scope: str_or(cd, "scope", "project"),
                mount_point: os(cd, "mountPoint"),
                display_title: os(cd, "displayTitle"),
                is_active: b(cd, "isActive", true),
            };
            warn_row!(
                w,
                c.chat_documents,
                "Failed to restore chat document".to_string(),
                repo.create(
                    &create,
                    &copts!(id_of(cd), crate::db::chat_documents::CreateOptions),
                )
            );
        }
    }

    // ── 22j. Vector index metas + entries (main partition) ───────────────────
    {
        let repo = crate::db::vector_indices::VectorIndicesRepository::new(main);
        for meta in &data.vector_index_metas {
            let character_id = s(meta, "characterId");
            warn_row!(
                w,
                c.vector_index_metas,
                format!("Failed to restore vector index meta for character {character_id}"),
                repo.save_meta(&character_id, n(meta, "dimensions", 0.0))
            );
        }
        if !data.vector_entries.is_empty() {
            let entries: Vec<crate::db::vector_indices::VectorEntryInput> = data
                .vector_entries
                .iter()
                .map(|e| crate::db::vector_indices::VectorEntryInput {
                    id: id_of(e),
                    character_id: s(e, "characterId"),
                    embedding: embedding(e, "embedding"),
                })
                .collect();
            match repo.add_entries(&entries) {
                Ok(()) => c.vector_entries = entries.len(),
                Err(e) => w.push(format!("Failed to restore vector entries: {e}")),
            }
        }
    }

    // ── 22k. Conversation chunks ─────────────────────────────────────────────
    {
        let repo = crate::db::conversation_chunks::ConversationChunksRepository::new(main);
        for chunk in &data.conversation_chunks {
            let create = crate::db::conversation_chunks::CcCreate {
                chat_id: s(chunk, "chatId"),
                interchange_index: n(chunk, "interchangeIndex", 0.0),
                content: s(chunk, "content"),
                participant_names: sa(chunk, "participantNames"),
                message_ids: sa(chunk, "messageIds"),
                embedding: embedding(chunk, "embedding"),
            };
            warn_row!(
                w,
                c.conversation_chunks,
                "Failed to restore conversation chunk".to_string(),
                repo.create(
                    &create,
                    &crate::db::conversation_chunks::CreateOptions {
                        id: id_of(chunk),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }

    // ── 22l. TF-IDF vocabularies — userId retargeted (`:685`) ────────────────
    {
        let repo = crate::db::tfidf_vocabulary::TfidfVocabularyRepository::new(main);
        for v in &data.tfidf_vocabularies {
            let create = crate::db::tfidf_vocabulary::TvCreate {
                profile_id: s(v, "profileId"),
                user_id: target_user_id.to_string(),
                vocabulary: json_text_of(v, "vocabulary"),
                idf: json_text_of(v, "idf"),
                avg_doc_length: n(v, "avgDocLength", 0.0),
                vocabulary_size: n(v, "vocabularySize", 0.0),
                include_bigrams: b(v, "includeBigrams", false),
                fitted_at: s(v, "fittedAt"),
            };
            warn_row!(
                w,
                c.tfidf_vocabularies,
                "Failed to restore TF-IDF vocabulary".to_string(),
                repo.create(
                    &create,
                    &crate::db::tfidf_vocabulary::CreateOptions {
                        id: id_of(v),
                        created_at: now(),
                    },
                )
            );
        }
    }

    // ── 22m. Embedding status — userId retargeted (`:701`) ───────────────────
    {
        let repo = crate::db::embedding_status::EmbeddingStatusRepository::new(main);
        for es in &data.embedding_status {
            let create = crate::db::embedding_status::EsCreate {
                user_id: target_user_id.to_string(),
                entity_type: s(es, "entityType"),
                entity_id: s(es, "entityId"),
                profile_id: s(es, "profileId"),
                status: s(es, "status"),
                embedded_at: os(es, "embeddedAt"),
                error: os(es, "error"),
            };
            warn_row!(
                w,
                c.embedding_status,
                "Failed to restore embedding status".to_string(),
                repo.create(
                    &create,
                    &crate::db::embedding_status::CreateOptions {
                        id: id_of(es),
                        created_at: now(),
                    },
                )
            );
        }
    }

    // ── 22n. Text replacement rules — a CONFLICT is skipped SILENTLY ─────────
    {
        let repo = crate::db::text_replacement_rules::TextReplacementRulesRepository::new(main);
        for rule in &data.text_replacement_rules {
            let create = crate::db::text_replacement_rules::TrrCreate {
                from_text: s(rule, "fromText"),
                to_text: s(rule, "toText"),
                case_sensitive: b(rule, "caseSensitive", false),
                enabled: b(rule, "enabled", true),
                sort_order: n(rule, "sortOrder", 0.0) as i64,
            };
            match repo.create(
                &create,
                &crate::db::text_replacement_rules::CreateOptions {
                    id: id_of(rule),
                    created_at: now(),
                    updated_at: now(),
                },
            ) {
                Ok(()) => c.text_replacement_rules += 1,
                // v4 `:723` — `TextReplacementRuleConflictError` is `continue`d
                // with a debug log and NO warning; every other error warns.
                Err(crate::db::text_replacement_rules::TrrError::Conflict { .. }) => {}
                Err(e) => w.push(format!("Failed to restore text replacement rule: {e}")),
            }
        }
    }

    // ── 22o. Instance settings — LAST, because the mount-point keys point at
    //    the doc_mount_points restored above. Raw upsert by key (`:746`).
    for row in &data.instance_settings {
        let key = s(row, "key");
        match main.execute(
            "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES (?1, ?2) \
             ON CONFLICT(\"key\") DO UPDATE SET \"value\" = excluded.\"value\"",
            rusqlite::params![key, s(row, "value")],
        ) {
            Ok(_) => c.instance_settings += 1,
            Err(e) => w.push(format!("Failed to restore instance setting \"{key}\": {e}")),
        }
    }

    // ── 23 / 24. npm plugins and theme bundles ───────────────────────────────
    c.npm_plugins = copy_host_subdirs(
        &root_path.join("plugins").join("npm"),
        &dirs.npm_plugins,
        &[],
        &mut w,
        "npm plugin",
    );
    c.user_installed_themes = copy_host_subdirs(
        &root_path.join("themes"),
        &dirs.themes,
        &[".cache"],
        &mut w,
        "theme bundle",
    );
    // v4 `:817-825` — `themes-index.json` rides along with the bundles.
    if let Some(themes_dir) = dirs.themes.as_ref() {
        let src = root_path.join("themes").join("themes-index.json");
        if src.exists() {
            let _ = std::fs::create_dir_all(themes_dir);
            if let Err(e) = std::fs::copy(&src, themes_dir.join("themes-index.json")) {
                // v4 logs and does NOT warn for this one file.
                tracing::warn!(error = %e, "Failed to restore themes-index.json");
            }
        }
    }

    // ── 24a. Compact archives arrive with no vectors at all (v4 `7189a968`,
    //    `restore.ts:873-907`): memory embeddings are NULL and every derived
    //    collection was omitted at backup time. The reconcile below
    //    deliberately ignores *absent* chunk rows — it only repairs
    //    non-conforming ones — so without this the instance would come back
    //    with search quietly cold. Enqueued BEFORE the reconcile so the
    //    reconcile's own dedupe sees this job and doesn't stack a second one.
    if extracted
        .manifest
        .get("compact")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        match enqueue_compact_reindex(main, target_user_id) {
            Ok(true) => w.push(
                "This was a compact backup, so search indexes were rebuilt rather than restored — search will warm back up as re-indexing completes. Conversation and document chunks are rebuilt as those chats and stores are next touched."
                    .to_string(),
            ),
            Ok(false) => w.push(
                "This was a compact backup, but no default embedding profile is configured, so search cannot be rebuilt yet. Configure one and re-index from the Commonplace Book."
                    .to_string(),
            ),
            Err(e) => {
                w.push(format!(
                    "Failed to queue re-indexing after compact restore: {e}"
                ));
                tracing::warn!(error = %e, "Failed to enqueue reindex after compact restore");
            }
        }
    }

    // ── 25. Embedding reconcile (v4 `restore.ts:910-938`). Restore is the one
    //    moment a corpus can arrive whose vectors were produced under a
    //    different embedding standard than this instance's default profile —
    //    new-account mode, or simply a machine configured differently from the
    //    one the backup came off. Until now the only repair was the *next
    //    boot's* sweep, which is fine for an in-place restore and wrong for
    //    everything else. The reconcile never throws (it catches into an
    //    all-zero result), resolves the default profile itself and dedupes its
    //    own reindex enqueue — so in the ordinary conforming case this is a
    //    cheap no-op.
    let reconcile = crate::services::embedding_dimension_reconcile::reconcile_embedding_dimensions(
        main, mount, now_ms,
    );
    if let Some(reason) = reconcile.skipped_reason {
        w.push(format!(
            "Embedding reconcile was skipped after restore ({}); semantic search will be repaired on the next startup.",
            reason.as_str()
        ));
    } else if reconcile.reindex_enqueued {
        w.push(
            "Some restored embeddings did not match this instance's embedding profile; re-indexing has been queued and search will warm back up as it completes."
                .to_string(),
        );
    }
    let embedding_reconcile = crate::services::backup::restore::EmbeddingReconcileSummary {
        target_dimensions: reconcile.target_dimensions,
        skipped_reason: reconcile.skipped_reason.map(|r| r.as_str()),
        vector_entries_deleted: reconcile.vector_entries_deleted,
        vector_index_meta_fixed: reconcile.vector_index_meta_fixed,
        reindex_enqueued: reconcile.reindex_enqueued,
    };

    // v4's `finally cleanupDir(extractDir)` (`:899`) is `Drop` on
    // [`ExtractedBackup`]. It is borrowed here rather than owned (the caller keeps
    // it so the remap can read the ORIGINAL rows alongside the remapped ones), so
    // the cleanup fires when `restore` returns — one stack frame later, on every
    // path including a panic. `system_restore_state` asserts the scratch root is
    // empty after every case, which is what actually proves it.
    c.into_summary(data, w, embedding_reconcile)
}

/// v4 24a's enqueue: `getDefaultEmbeddingProfile(targetUserId)` (strict
/// `findDefault` — no first-row fallback) then
/// `enqueueEmbeddingReindexAll(userId, {profileId, scope: 'all'})` — plain
/// enqueue at priority -1, maxAttempts 3, payload key order `{profileId,
/// scope}`. `Ok(true)` = enqueued, `Ok(false)` = no default profile.
fn enqueue_compact_reindex(main: &Connection, user_id: &str) -> Result<bool, DbError> {
    let profile_id: Option<String> = main
        .query_row(
            "SELECT id FROM embedding_profiles WHERE userId = ?1 AND isDefault = 1 LIMIT 1",
            rusqlite::params![user_id],
            |r| r.get::<_, String>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    let Some(profile_id) = profile_id else {
        return Ok(false);
    };
    let now = now();
    crate::db::background_jobs::BackgroundJobsRepository::new(main).create(
        &crate::db::background_jobs::BjCreate {
            user_id: user_id.to_string(),
            job_type: "EMBEDDING_REINDEX_ALL".to_string(),
            status: Some("PENDING".to_string()),
            payload: serde_json::json!({ "profileId": profile_id, "scope": "all" }),
            priority: -1.0,
            attempts: 0.0,
            max_attempts: 3.0,
            last_error: None,
            scheduled_at: now.clone(),
            started_at: None,
            completed_at: None,
        },
        &crate::db::background_jobs::CreateOptions {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: now.clone(),
            updated_at: now,
        },
    )?;
    Ok(true)
}

// ─────────────────────────────────────────────────────────────────────────────
// The mount-index family (22a-22e, 22f, 22g, 22h, 22h-i, 22h-ii, 22i)
// ─────────────────────────────────────────────────────────────────────────────

/// The ids one collection carries, for the referential pre-check below.
fn id_set(rows: &[Value]) -> std::collections::HashSet<String> {
    rows.iter().map(id_of).collect()
}

fn restore_mount_family(
    main: &Connection,
    mount: &Connection,
    data: &crate::services::backup::BackupData,
    original: &crate::services::backup::BackupData,
    root_path: &Path,
    c: &mut Counters,
    w: &mut Vec<String>,
) {
    // ## ⚠ DELIBERATE DIVERGENCE (dogfood #58, 2026-08-03) — an archive can be
    // ## referentially broken, and a restore must say so in words
    //
    // The backup's doc-store collections are raw `SELECT *` dumps (v4
    // `dumpMountIndexTable`, ported in `collect.rs`), so whatever the mount
    // index holds is what the archive carries — **including rows whose parent
    // is gone**. That is not hypothetical: measured on the real dogfood
    // instance, 2026-08-03, `doc_mount_file_links` held **43** rows whose
    // `mountPointId` matched no `doc_mount_points` row and `doc_mount_folders`
    // held **118**, left behind by document stores deleted without them. Nothing
    // notices while they sit there — read connections do not enable foreign
    // keys, and a `generateDDL` link table declares none at all.
    //
    // Restore is where it lands. v5 opens writable connections with
    // `PRAGMA foreign_keys = ON`, and a **migration-vintage** `doc_mount_file_links`
    // carries `fileId` → `doc_mount_files` and `mountPointId` →
    // `doc_mount_points` (v4 `migrations/scripts/add-doc-mount-file-links.ts:190`),
    // so each orphan failed with a bare `FOREIGN KEY constraint failed` — 50-odd
    // of them on the 2026-08-03 walk, naming a filename and no cause.
    //
    // Under the standing 2026-08-03 backup/restore ruling (v5 FIXES v4's bugs in
    // this family) each phase below now checks that a row's parent is IN THE
    // ARCHIVE before attempting the insert, and skips it with a sentence naming
    // what is missing. Three things about the shape, all deliberate:
    //
    // 1. The check is against what the ARCHIVE CONTAINS, not against what the
    //    preceding phase managed to write. A parent that failed to restore for
    //    some other reason already produced its own warning, and conflating the
    //    two would turn one fault into two — and would make this check's
    //    behaviour depend on unrelated failures.
    // 2. It is **reader-side only**. `collect.rs` still dumps every row, so v5's
    //    archives stay byte-identical and readable by v4 (the standing shape for
    //    this family's divergences), and — the point — an archive the user
    //    ALREADY HAS becomes restorable. A collect-side filter would help nobody
    //    who has been taking backups.
    // 3. It is invisible on every committed archive but
    //    `restore-archive-orphan-links.zip`, because all the others were built
    //    from healthy instances. `restore_vintage_state` is what proves it.
    //
    // The ROOT cause — a `doc_mount_points` delete that does not take its links
    // and folders with it — belongs to the mount-index delete path, which this
    // lane does not own. Recorded as a handoff in the lane record.
    let archived_points = id_set(&data.doc_mount_points);
    let archived_files = id_set(&data.doc_mount_files);
    let mut usable_links: std::collections::HashSet<String> = id_set(&data.doc_mount_file_links);

    // 22a. Mount points. The archive carries these rows as the raw `SELECT *`
    // gave them up — pattern arrays as JSON text, `enabled` as INTEGER 0/1 — so
    // coerce back to domain shape (v4 `mount-index-coercion.ts`, applied at v4's
    // own call site). Without it the patterns read as `[]` and a disabled store
    // comes back enabled; see that module's header.
    {
        let repo = crate::db::doc_mount_points::DocMountPointsRepository::new(mount);
        for raw in &data.doc_mount_points {
            let mp = &super::mount_index_coercion::coerce_doc_mount_point_row(raw);
            let create = crate::db::doc_mount_points::DmpCreate {
                name: s(mp, "name"),
                base_path: s(mp, "basePath"),
                mount_type: s(mp, "mountType"),
                store_type: str_or(mp, "storeType", "documents"),
                include_patterns: sa(mp, "includePatterns"),
                exclude_patterns: sa(mp, "excludePatterns"),
                enabled: b(mp, "enabled", true),
                last_scanned_at: os(mp, "lastScannedAt"),
                scan_status: str_or(mp, "scanStatus", "idle"),
                last_scan_error: os(mp, "lastScanError"),
                conversion_status: str_or(mp, "conversionStatus", "idle"),
                conversion_error: os(mp, "conversionError"),
                file_count: n(mp, "fileCount", 0.0),
                chunk_count: n(mp, "chunkCount", 0.0),
                total_size_bytes: n(mp, "totalSizeBytes", 0.0),
            };
            warn_row!(
                w,
                c.doc_mount_points,
                format!("Failed to restore document store \"{}\"", s(mp, "name")),
                repo.create(
                    &create,
                    &crate::db::doc_mount_points::CreateOptions {
                        id: id_of(mp),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }

    // 22b. Folders — sorted by path length so parents precede children (`:446`).
    {
        let repo = crate::db::doc_mount_folders::DocMountFoldersRepository::new(mount);
        let mut sorted: Vec<&Value> = data.doc_mount_folders.iter().collect();
        sorted.sort_by_key(|f| s(f, "path").len());
        for f in sorted {
            if !archived_points.contains(&s(f, "mountPointId")) {
                w.push(format!(
                    "Skipped doc-store folder \"{}\": its document store is not in the backup",
                    s(f, "name")
                ));
                continue;
            }
            let create = crate::db::doc_mount_folders::DmfCreate {
                mount_point_id: s(f, "mountPointId"),
                parent_id: os(f, "parentId"),
                name: s(f, "name"),
                path: s(f, "path"),
            };
            warn_row!(
                w,
                c.doc_mount_folders,
                format!("Failed to restore doc-store folder \"{}\"", s(f, "name")),
                repo.create(
                    &create,
                    &crate::db::doc_mount_folders::CreateOptions {
                        id: id_of(f),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }

    // 22c. File content rows.
    {
        let repo = crate::db::doc_mount_files::DocMountFilesRepository::new(mount);
        for f in &data.doc_mount_files {
            let create = crate::db::doc_mount_files::DmfCreate {
                sha256: s(f, "sha256"),
                file_size_bytes: n(f, "fileSizeBytes", 0.0),
                file_type: s(f, "fileType"),
                source: str_or(f, "source", "filesystem"),
            };
            warn_row!(
                w,
                c.doc_mount_files,
                "Failed to restore doc-store file row".to_string(),
                repo.create(
                    &create,
                    &crate::db::doc_mount_files::CreateOptions {
                        id: id_of(f),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }

    // 22d. File links. Same storage-type coercion as 22a — the three policy
    // flags arrive as INTEGER 0/1 and the reader wants booleans.
    {
        let repo = crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(mount);
        for link in &data.doc_mount_file_links {
            let missing = if !archived_points.contains(&s(link, "mountPointId")) {
                Some("its document store is not in the backup")
            } else if !archived_files.contains(&s(link, "fileId")) {
                Some("its file content row is not in the backup")
            } else {
                None
            };
            if let Some(why) = missing {
                usable_links.remove(&id_of(link));
                w.push(format!(
                    "Skipped doc-store file link \"{}\": {why}",
                    s(link, "relativePath")
                ));
                continue;
            }
            let coerced = super::mount_index_coercion::coerce_doc_mount_file_link_row(link);
            warn_row!(
                w,
                c.doc_mount_file_links,
                format!(
                    "Failed to restore doc-store file link \"{}\"",
                    s(link, "relativePath")
                ),
                repo.create_from_row(&coerced, &id_of(link), &now())
            );
        }
    }

    // 22e. Text documents.
    {
        let repo = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
        for d in &data.doc_mount_documents {
            if !archived_files.contains(&s(d, "fileId")) {
                w.push(
                    "Skipped doc-store document: its file content row is not in the backup"
                        .to_string(),
                );
                continue;
            }
            let create = crate::db::doc_mount_documents::DmdCreate {
                file_id: s(d, "fileId"),
                content: s(d, "content"),
                content_sha256: s(d, "contentSha256"),
                plain_text_length: n(d, "plainTextLength", 0.0),
            };
            warn_row!(
                w,
                c.doc_mount_documents,
                "Failed to restore doc-store document".to_string(),
                repo.create(
                    &create,
                    &crate::db::doc_mount_documents::CreateOptions {
                        id: id_of(d),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }

    // 22f. Binary blobs — metadata row + bytes, written RAW so the original blob
    // id survives (it is a UNIQUE column on fileId). v4 `:514`.
    if !data.doc_mount_blobs.is_empty() {
        let blobs_dir = root_path.join("mount-blobs");
        for (i, blob) in data.doc_mount_blobs.iter().enumerate() {
            let id = id_of(blob);
            if !archived_files.contains(&s(blob, "fileId")) {
                w.push(format!(
                    "Skipped doc-store blob {id}: its file content row is not in the backup"
                ));
                continue;
            }
            // v4 `:505-508`: in new-account mode the metadata id is remapped but
            // the bytes on disk are still keyed by the ORIGINAL id, so the two are
            // paired by index — the same trick phase 5 uses for user files.
            let disk_id = original
                .doc_mount_blobs
                .get(i)
                .map(id_of)
                .unwrap_or_else(|| id.clone());
            match std::fs::read(blobs_dir.join(&disk_id)) {
                Ok(bytes) => {
                    let res = mount.execute(
                        "INSERT INTO \"doc_mount_blobs\" \
                         (id, fileId, sha256, sizeBytes, storedMimeType, data, createdAt, updatedAt) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        rusqlite::params![
                            id,
                            s(blob, "fileId"),
                            s(blob, "sha256"),
                            n(blob, "sizeBytes", 0.0),
                            s(blob, "storedMimeType"),
                            bytes,
                            s(blob, "createdAt"),
                            s(blob, "updatedAt"),
                        ],
                    );
                    match res {
                        Ok(_) => c.doc_mount_blobs += 1,
                        Err(e) => w.push(format!("Failed to restore doc-store blob {id}: {e}")),
                    }
                }
                Err(e) => w.push(format!("Failed to restore doc-store blob {id}: {e}")),
            }
        }
    }

    // 22f-bis. LEGACY wardrobe items (deferred from step 19; post-cutover
    // backups carry none). v4 `restore.ts:543` — runs BETWEEN the blobs (22f)
    // and the archive chunk rows (22g); `wardrobe.create` writes the item into
    // the character's vault and (since P4.6BK) chunks it on write, so this
    // position is observable in `doc_mount_chunks` insertion order.
    {
        let links = crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(mount);
        let docs = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
        for item in &data.wardrobe_items {
            let title = s(item, "title");
            let stored = crate::vault_overlay::WardrobeItem {
                id: id_of(item),
                character_id: Some(os(item, "characterId")),
                title: title.clone(),
                description: Some(os(item, "description")),
                image_prompt: Some(os(item, "imagePrompt")),
                types: sa(item, "types"),
                component_item_ids: sa(item, "componentItemIds"),
                appropriateness: Some(os(item, "appropriateness")),
                is_default: b(item, "isDefault", false),
                replace: b(item, "replace", false),
                migrated_from_clothing_record_id: Some(os(item, "migratedFromClothingRecordId")),
                archived_at: Some(os(item, "archivedAt")),
                created_at: now(),
                updated_at: now(),
            };
            match crate::db::vault_wardrobe_public::create_vault_wardrobe_item(
                main, &links, &docs, &stored,
            ) {
                Ok(_) => c.wardrobe_items += 1,
                Err(e) => w.push(format!(
                    "Failed to restore wardrobe item \"{title}\": {e:?}"
                )),
            }
        }
    }

    // 22g. Embedded chunks.
    {
        let repo = crate::db::doc_mount_chunks::DocMountChunksRepository::new(mount);
        for chunk in &data.doc_mount_chunks {
            // `usable_links` starts as the archive's link ids and loses the ones
            // 22d skipped, so a chunk whose link never landed goes with it.
            if !usable_links.contains(&s(chunk, "linkId")) {
                w.push("Skipped doc-store chunk: its file link is not in the backup".to_string());
                continue;
            }
            let create = crate::db::doc_mount_chunks::DmcCreate {
                link_id: s(chunk, "linkId"),
                mount_point_id: s(chunk, "mountPointId"),
                chunk_index: n(chunk, "chunkIndex", 0.0),
                content: s(chunk, "content"),
                token_count: n(chunk, "tokenCount", 0.0),
                heading_context: os(chunk, "headingContext"),
                embedding: embedding(chunk, "embedding"),
            };
            warn_row!(
                w,
                c.doc_mount_chunks,
                "Failed to restore doc-store chunk".to_string(),
                repo.create(
                    &create,
                    &crate::db::doc_mount_chunks::CreateOptions {
                        id: id_of(chunk),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }

    // 22h / 22h-i / 22h-ii. The three link tables.
    {
        let repo = crate::db::project_doc_mount_links::ProjectDocMountLinksRepository::new(mount);
        for link in &data.project_doc_mount_links {
            if !archived_points.contains(&s(link, "mountPointId")) {
                w.push(
                    "Skipped project↔store link: its document store is not in the backup"
                        .to_string(),
                );
                continue;
            }
            let create = crate::db::project_doc_mount_links::PdmlCreate {
                project_id: s(link, "projectId"),
                mount_point_id: s(link, "mountPointId"),
            };
            warn_row!(
                w,
                c.project_doc_mount_links,
                "Failed to restore project↔store link".to_string(),
                repo.create(
                    &create,
                    &crate::db::project_doc_mount_links::CreateOptions {
                        id: id_of(link),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }
    {
        let repo = crate::db::group_doc_mount_links::GroupDocMountLinksRepository::new(mount);
        for link in &data.group_doc_mount_links {
            if !archived_points.contains(&s(link, "mountPointId")) {
                w.push(
                    "Skipped group↔store link: its document store is not in the backup".to_string(),
                );
                continue;
            }
            let create = crate::db::group_doc_mount_links::GdmlCreate {
                group_id: s(link, "groupId"),
                mount_point_id: s(link, "mountPointId"),
            };
            warn_row!(
                w,
                c.group_doc_mount_links,
                "Failed to restore group↔store link".to_string(),
                repo.create(
                    &create,
                    &crate::db::group_doc_mount_links::CreateOptions {
                        id: id_of(link),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }
    {
        let repo = crate::db::group_character_members::GroupCharacterMembersRepository::new(mount);
        for m in &data.group_character_members {
            let create = crate::db::group_character_members::GcmCreate {
                group_id: s(m, "groupId"),
                character_id: s(m, "characterId"),
            };
            warn_row!(
                w,
                c.group_character_members,
                "Failed to restore group membership".to_string(),
                repo.create(
                    &create,
                    &crate::db::group_character_members::CreateOptions {
                        id: id_of(m),
                        created_at: now(),
                        updated_at: now(),
                    },
                )
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-phase helpers
// ─────────────────────────────────────────────────────────────────────────────

/// What the archive already holds for one user file: the document-store blob its
/// `storageKey` points at, as that blob now stands in the restored database.
struct CarriedBlob {
    /// `mount-blob:<mp>:<blob>` in the RESTORED id space.
    storage_key: String,
    stored_mime_type: String,
    size_bytes: f64,
}

/// ## ⚠ RULED DIVERGENCE (2026-07-26) — the archive's own store rows win
///
/// v4 re-ingests every user file unconditionally: the bytes go back through a
/// bridge, which mints a fresh link (and, at v4's `22a-bis` slot, a fresh content
/// row and blob) beside the ones the archive already carries. v4 names the proper
/// repair itself and puts it out of scope (`found-bugs.md:400-402`) — *"teach the
/// replay to recognise that the archive already carries the store rows for a file
/// and skip re-ingesting it, rather than reshuffling phase order"*. **The human
/// ruling of 2026-07-26 keeps v5's later files-phase slot precisely so that this
/// check can exist**, because at `22a-bis` the archived rows have not been
/// restored yet and there is nothing to consult. `status-log.md` → "Ruling — the
/// restore file-replay dedupe"; `system_restore_state` asserts it in both
/// directions.
///
/// ## The predicate, and why it is exact rather than heuristic
///
/// A file is "already carried by the archive's document store" when all three
/// hold:
///
/// 1. its ARCHIVED `storageKey` parses as `mount-blob:<mp>:<blob>` — a legacy
///    disk key (`<userId>/portrait.png`) names no store row and is re-ingested
///    exactly as before;
/// 2. the archive's `doc_mount_blobs` collection carries a row with that blob id
///    — this is what "the archive carries the store rows" means literally;
/// 3. that blob is PRESENT in the mount-index database right now, i.e. phase 22f
///    actually restored it. If 22f warned, the bytes are not in the store and the
///    file is re-ingested so the user still gets it back.
///
/// The handle is the storage key, not the `files` row's id. P4.d22 reasoned that
/// "the archived doc-store rows key on that same id" — **that is wrong, and this
/// lane's first job was to check it**: `doc_mount_blobs.fileId` references
/// `doc_mount_files.id` (a content row, content-addressed by sha and shared
/// between mounts), an id space entirely disjoint from `files.id`. The storage
/// key is the only exact handle, and it is the very pointer the `files` row uses
/// to find its bytes — so a match means "this row's bytes are already in the
/// store", which is exactly the question.
///
/// ## new-account
///
/// `uuid_remap` has no rule for `storageKey`, so the remapped row still names the
/// ARCHIVE's blob and mount. Resolution therefore runs in the archive's id space
/// and the key is rebuilt in the restored one, pairing original to remapped BY
/// INDEX — the same trick 22f and the files loop already use for on-disk names.
///
/// Fails soft in every direction: any miss returns `None` and the file is
/// re-ingested, which is the safe answer. Silently dropping a file the user
/// expected back is the worst failure this surface has.
fn carried_store_rows(
    mount: &Connection,
    data: &crate::services::backup::BackupData,
    original: &crate::services::backup::BackupData,
    on_disk: &Value,
) -> Option<CarriedBlob> {
    let key = os(on_disk, "storageKey")?;
    let (archive_mp, archive_blob) =
        crate::services::file_storage::parse_mount_blob_storage_key(&key)?;

    let i = original
        .doc_mount_blobs
        .iter()
        .position(|b| id_of(b) == archive_blob)?;
    let blob_id = data
        .doc_mount_blobs
        .get(i)
        .map(id_of)
        .unwrap_or(archive_blob);

    // Present, or 22f never got it in. A missing table reads the same as a
    // missing row: re-ingest.
    let (stored_mime_type, size_bytes) = mount
        .query_row(
            "SELECT storedMimeType, sizeBytes FROM doc_mount_blobs WHERE id = ?1",
            rusqlite::params![blob_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)),
        )
        .ok()?;

    // The mount half of the key is cosmetic on the read path (every reader
    // resolves by blob id), but a restored row should not name a mount that no
    // longer exists. An unremapped mount falls back to the archive's own id.
    let mount_point_id = original
        .doc_mount_points
        .iter()
        .position(|m| id_of(m) == archive_mp)
        .and_then(|j| data.doc_mount_points.get(j))
        .map(id_of)
        .unwrap_or(archive_mp);

    Some(CarriedBlob {
        storage_key: crate::services::file_storage::build_mount_blob_storage_key(
            &mount_point_id,
            &blob_id,
        ),
        stored_mime_type,
        size_bytes,
    })
}

/// Phase 5's body for one file (v4 `:136-187`).
///
/// Project-bound files land in the project's document store; project-less ones in
/// the Quilltap Uploads store under `restored/` — **not** the catch-all
/// `_general/`. The bridges may transcode bitmaps, so the row records the
/// POST-bridge mime and size rather than what the backup claimed (`:167-172`);
/// re-writing the backup's claim would re-introduce the "media_type X but bytes
/// are Y" error a pre-fix backup carries.
///
/// `carried` short-circuits the ingest — see [`carried_store_rows`] for the
/// ruling and the predicate. The row it writes keeps the same shape: the mime and
/// size still describe what is actually in the store, read off the archive's own
/// blob rather than off a bridge that has just re-written it.
fn restore_one_file(
    main: &Connection,
    mount: &Connection,
    codec: &dyn PixelCodec,
    target_user_id: &str,
    file: &Value,
    bytes: &[u8],
    carried: Option<&CarriedBlob>,
) -> Result<(), DbError> {
    let filename = s(file, "originalFilename");
    let mime = s(file, "mimeType");
    let stored = match carried {
        Some(_) => None,
        None => Some(match os(file, "projectId") {
            Some(project_id) => crate::services::file_storage::write_project_file_to_mount_store(
                mount,
                codec,
                &project_id,
                &filename,
                bytes,
                &mime,
                os(file, "folderPath").as_deref().or(Some("/")),
                None,
            )?,
            None => crate::services::file_storage::write_user_upload_to_mount_store(
                main, mount, codec, &filename, bytes, &mime, "restored", None,
            )?,
        }),
    };
    let (storage_key, stored_mime_type, size_bytes) = match (&stored, carried) {
        (Some(s), _) => (
            s.storage_key(),
            s.stored_mime_type.clone(),
            s.size_bytes as f64,
        ),
        (None, Some(c)) => (
            c.storage_key.clone(),
            c.stored_mime_type.clone(),
            c.size_bytes,
        ),
        // Unreachable: `stored` is `None` only when `carried` is `Some`.
        (None, None) => unreachable!("the file phase either ingests or reuses"),
    };

    let create = crate::db::files::FileCreate {
        user_id: target_user_id.to_string(),
        sha256: s(file, "sha256"),
        original_filename: filename,
        mime_type: stored_mime_type,
        size: size_bytes,
        width: on(file, "width"),
        height: on(file, "height"),
        is_plain_text: ob(file, "isPlainText"),
        linked_to: sa(file, "linkedTo"),
        source: str_or(file, "source", "UPLOADED"),
        category: str_or(file, "category", "DOCUMENT"),
        generation_prompt: os(file, "generationPrompt"),
        generation_model: os(file, "generationModel"),
        generation_revised_prompt: os(file, "generationRevisedPrompt"),
        description: os(file, "description"),
        tags: sa(file, "tags"),
        project_id: os(file, "projectId"),
        folder_path: os(file, "folderPath"),
        storage_key: Some(storage_key),
        file_status: str_or(file, "fileStatus", "ok"),
    };
    crate::db::files::FilesRepository::new(main).create(
        &create,
        &crate::db::files::CreateOptions {
            id: id_of(file),
            created_at: now(),
            updated_at: now(),
        },
    )
}

/// Phase 6's body for one character (v4 `:200`). `repos.characters.create`
/// inserts the slim row, provisions a FRESH vault (dropping any incoming
/// `characterDocumentMountPointId`), and projects the managed fields into it.
fn restore_one_character(
    main: &Connection,
    mount: &Connection,
    target_user_id: &str,
    ch: &Value,
) -> Result<(), DbError> {
    let slim = crate::db::characters::CharacterCreate {
        user_id: target_user_id.to_string(),
        name: s(ch, "name"),
        default_image_id: os(ch, "defaultImageId"),
        default_connection_profile_id: os(ch, "defaultConnectionProfileId"),
        default_partner_id: os(ch, "defaultPartnerId"),
        default_roleplay_template_id: os(ch, "defaultRoleplayTemplateId"),
        default_image_profile_id: os(ch, "defaultImageProfileId"),
        silly_tavern_data: ch.get("sillyTavernData").cloned().filter(|v| !v.is_null()),
        is_favorite: b(ch, "isFavorite", false),
        npc: b(ch, "npc", false),
        controlled_by: str_or(ch, "controlledBy", "llm"),
        default_agent_mode_enabled: ob(ch, "defaultAgentModeEnabled"),
        default_help_tools_enabled: ob(ch, "defaultHelpToolsEnabled"),
        default_timestamp_config: de_opt(ch, "defaultTimestampConfig"),
        default_scenario_id: os(ch, "defaultScenarioId"),
        default_system_prompt_id: os(ch, "defaultSystemPromptId"),
        character_document_mount_point_id: None,
        can_dress_themselves: ob(ch, "canDressThemselves"),
        can_create_outfits: ob(ch, "canCreateOutfits"),
        system_transparency: ob(ch, "systemTransparency"),
        core_whisper_enabled: ob(ch, "coreWhisperEnabled"),
        can_be_carina: ob(ch, "canBeCarina"),
        partner_links: de_or_default(ch, "partnerLinks"),
        tags: sa(ch, "tags"),
        avatar_overrides: de_or_default(ch, "avatarOverrides"),
    };
    let vault: crate::db::vault_character_write::CharacterVaultWriteInput =
        serde_json::from_value(ch.clone())
            .map_err(|e| DbError::Key(format!("character vault fields: {e}")))?;
    crate::db::character_vault::create_character_with_options(
        main,
        mount,
        &slim,
        &vault,
        &crate::db::characters::CreateOptions {
            id: id_of(ch),
            created_at: now(),
            updated_at: now(),
        },
    )
    .map(|_| ())
}

/// Phases 23 and 24 — recursive copy of every subdirectory of `src` into `dest`,
/// skipping `skip` names. Returns how many landed; `None` for `dest` (a host with
/// no such directory) is a documented no-op that reports 0.
fn copy_host_subdirs(
    src: &Path,
    dest: &Option<std::path::PathBuf>,
    skip: &[&str],
    w: &mut Vec<String>,
    label: &str,
) -> usize {
    let Some(dest) = dest else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(src) else {
        // No such directory in the backup — v4 debug-logs and moves on (`:789`).
        return 0;
    };
    let _ = std::fs::create_dir_all(dest);
    let mut n0 = 0usize;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.path().is_dir() || skip.contains(&name.as_str()) {
            continue;
        }
        match copy_dir_recursive(&entry.path(), &dest.join(&name)) {
            Ok(()) => n0 += 1,
            Err(e) => w.push(format!("Failed to restore {label} \"{name}\": {e}")),
        }
    }
    n0
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Small shared helpers
// ─────────────────────────────────────────────────────────────────────────────

fn store_opts(id: String) -> crate::db::store_backed::StoreCreateOptions {
    crate::db::store_backed::StoreCreateOptions {
        id: Some(id),
        created_at: None,
        updated_at: None,
    }
}

fn id_of(row: &Value) -> String {
    row.get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(new_id)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now() -> String {
    crate::clock::now_iso()
}

/// A required-with-default enum column.
fn str_or(v: &Value, k: &str, default: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

/// A nested typed column that is genuinely optional (absent → SQL NULL).
fn de_opt<T: serde::de::DeserializeOwned>(v: &Value, k: &str) -> Option<T> {
    v.get(k)
        .filter(|x| !x.is_null())
        .cloned()
        .and_then(|x| serde_json::from_value(x).ok())
}

/// `character_plugin_data.data` is a JSON **string** in the backup (v4's repo
/// declares no JSON columns, so `findByCharacterId` hands the raw column text
/// through — see `collect.rs`'s header). v5's create re-serializes a `Value`, so
/// the string is parsed back first.
fn parse_cpd_data(row: &Value) -> Value {
    match row.get("data") {
        Some(Value::String(text)) => {
            serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        Some(other) => other.clone(),
        None => serde_json::json!({}),
    }
}

/// `tfidf_vocabularies.vocabulary` / `.idf` are stored as raw JSON TEXT and must
/// go back as the same bytes.
fn json_text_of(row: &Value, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => "[]".to_string(),
    }
}

/// v4's project row carries its non-managed properties inline; the store-backed
/// create takes them as one `properties` object.
fn project_properties(p: &Value) -> Value {
    let mut m = serde_json::Map::new();
    for k in [
        "allowAnyCharacter",
        "characterRoster",
        "color",
        "defaultDisabledTools",
        "defaultDisabledToolGroups",
        "backgroundDisplayMode",
    ] {
        if let Some(v) = p.get(k) {
            m.insert(k.to_string(), v.clone());
        }
    }
    Value::Object(m)
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

/// The actually-restored counters. v4's summary mixes these with plain input
/// lengths — see [`Counters::into_summary`].
#[derive(Default)]
struct Counters {
    messages: usize,
    files: usize,
    prompt_templates: usize,
    roleplay_templates: usize,
    provider_models: usize,
    projects: usize,
    groups: usize,
    llm_logs: usize,
    plugin_configs: usize,
    chat_settings: usize,
    folders: usize,
    wardrobe_items: usize,
    npm_plugins: usize,
    character_plugin_data: usize,
    conversation_annotations: usize,
    user_installed_themes: usize,
    chat_documents: usize,
    instance_settings: usize,
    embedding_status: usize,
    conversation_chunks: usize,
    tfidf_vocabularies: usize,
    vector_index_metas: usize,
    vector_entries: usize,
    doc_mount_points: usize,
    doc_mount_folders: usize,
    doc_mount_files: usize,
    doc_mount_file_links: usize,
    doc_mount_chunks: usize,
    doc_mount_documents: usize,
    doc_mount_blobs: usize,
    project_doc_mount_links: usize,
    group_doc_mount_links: usize,
    group_character_members: usize,
    text_replacement_rules: usize,
}

impl Counters {
    /// v4 `restore.ts:840-887`, field by field. The mix is deliberate:
    /// `characters` / `chats` / `tags` / `memories` and the three `profiles`
    /// report the archive's INPUT lengths even when a row failed, while
    /// everything else reports what actually landed.
    fn into_summary(
        self,
        data: &crate::services::backup::collect::BackupData,
        warnings: Vec<String>,
        embedding_reconcile: crate::services::backup::restore::EmbeddingReconcileSummary,
    ) -> RestoreSummary {
        RestoreSummary {
            characters: data.characters.len(),
            chats: data.chats.len(),
            messages: self.messages,
            tags: data.tags.len(),
            files: self.files,
            memories: data.memories.len(),
            profiles: ProfileCounts {
                connection: data.connection_profiles.len(),
                image: data.image_profiles.len(),
                embedding: data.embedding_profiles.len(),
            },
            templates: TemplateCounts {
                prompt: self.prompt_templates,
                roleplay: self.roleplay_templates,
            },
            provider_models: self.provider_models,
            projects: self.projects,
            groups: self.groups,
            llm_logs: self.llm_logs,
            plugin_configs: self.plugin_configs,
            chat_settings: self.chat_settings,
            folders: self.folders,
            wardrobe_items: self.wardrobe_items,
            npm_plugins: self.npm_plugins,
            character_plugin_data: self.character_plugin_data,
            conversation_annotations: self.conversation_annotations,
            user_installed_themes: self.user_installed_themes,
            chat_documents: self.chat_documents,
            instance_settings: self.instance_settings,
            embedding_status: self.embedding_status,
            conversation_chunks: self.conversation_chunks,
            tfidf_vocabularies: self.tfidf_vocabularies,
            vector_index_metas: self.vector_index_metas,
            vector_entries: self.vector_entries,
            doc_mount_points: self.doc_mount_points,
            doc_mount_folders: self.doc_mount_folders,
            doc_mount_files: self.doc_mount_files,
            doc_mount_file_links: self.doc_mount_file_links,
            doc_mount_chunks: self.doc_mount_chunks,
            doc_mount_documents: self.doc_mount_documents,
            doc_mount_blobs: self.doc_mount_blobs,
            project_doc_mount_links: self.project_doc_mount_links,
            group_doc_mount_links: self.group_doc_mount_links,
            group_character_members: self.group_character_members,
            text_replacement_rules: self.text_replacement_rules,
            embedding_reconcile: Some(embedding_reconcile),
            warnings,
        }
    }
}
