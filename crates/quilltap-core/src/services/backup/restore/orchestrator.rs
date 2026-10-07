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
use crate::services::connection_profile_legacy_fields::seed_legacy_connection_profile_fields;
use crate::services::file_storage::PixelCodec;
use crate::services::profile_names::{make_unique_profile_name, normalize_profile_name};

/// v4 `RestoreOptions.mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreMode {
    Replace,
    NewAccount,
}

impl RestoreMode {
    /// v4's `mode` spelling (the census lines' `mode` field).
    pub fn as_str(self) -> &'static str {
        match self {
            RestoreMode::Replace => "replace",
            RestoreMode::NewAccount => "new-account",
        }
    }

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
/// v4 `RestoreOptions`' archive half (`lib/backup/types.ts`, new in
/// `d553f72a`). The rest of v4's options bag (mode, targetUserId) is already
/// this function's positional parameters.
#[derive(Debug, Clone, Copy, Default)]
pub struct RestoreOptions {
    /// Replace mode only: when true (**the default**), archived-character
    /// `.qtap` bundles (`files` rows of category `ARCHIVE` and their on-disk
    /// bytes) survive the pre-restore wipe as loose bundles — importable, not
    /// rehydratable, since the tombstone character rows are replaced like any
    /// others. Pass `Some(false)` to wipe them with everything else.
    pub keep_archived_character_bundles: Option<bool>,
}

pub async fn restore(
    db: &Db,
    host: &dyn BackupHost,
    zip_path: &Path,
    mode: RestoreMode,
    target_user_id: &str,
    options: RestoreOptions,
) -> Result<RestoreSummary, String> {
    // v4 `restore.ts:75` — before the archive is even extracted.
    tracing::info!(
        target: "quilltap::restore",
        mode = mode.as_str(),
        targetUserId = %target_user_id,
        "Starting restore operation"
    );
    let mut extracted = parse_backup_zip(zip_path, &host.temp_dir())?;

    if mode == RestoreMode::Replace {
        // Archived-character bundles survive the pre-restore wipe by default
        // (spec §4.7, v4 `d553f72a`) — the restore replaces the tombstone rows,
        // so what's kept is a loose, importable bundle.
        crate::services::delete_all::delete_user_data(
            db,
            target_user_id,
            crate::services::delete_all::DeleteUserDataOptions {
                keep_archived_character_bundles: Some(
                    options.keep_archived_character_bundles != Some(false),
                ),
            },
        )
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
    // v4's phase-14 gate is `isLLMLogsDegraded()` (`restore.ts:349`) — read
    // here, off the writer thread, from the open's recorded state (P4.159):
    // the writer set alone cannot tell a DEGRADED logs file from an ABSENT one.
    let llm_logs_degraded = db.partition_state(crate::db::table_shape::Partition::LlmLogs)
        == crate::db::runtime::PartitionState::Degraded;
    // Step 25's reconcile takes no clock since v4 `f7f3d7bf0` (its staleness
    // window is gone — P4.D235; the dead argument dropped at the `97b25fc53`
    // unification).
    db.write(move |ws| {
        Ok(restore_on_writer(
            ws,
            &mut extracted,
            remapped,
            &user_id,
            codec,
            dirs,
            llm_logs_degraded,
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
///
/// With a message and fields after the body, the catch also logs v4's
/// per-phase WARN (P4.158 R-G — `restore.ts`'s `moduleLogger.warn(<msg>,
/// {<id>, error})`, every phase's own; `error` last, bare, as v4's
/// `error.message`).
macro_rules! warn_only {
    ($warnings:expr, $label:expr, $body:expr) => {
        if let Err(e) = $body {
            $warnings.push(format!("{}: {}", $label, WarnText::warn_text(&e)));
        }
    };
    ($warnings:expr, $label:expr, $body:expr, $msg:literal $(, $k:ident = $v:expr)* $(,)?) => {
        if let Err(e) = $body {
            let error = WarnText::warn_text(&e);
            $warnings.push(format!("{}: {}", $label, error));
            tracing::warn!(target: "quilltap::restore", $($k = %$v,)* error = %error, $msg);
        }
    };
}

/// The per-row tolerance every phase shares: v4 wraps each entity in a `try`
/// that pushes one `warnings[]` line and keeps going.
///
/// The logging arm is [`warn_only!`]'s.
macro_rules! warn_row {
    ($warnings:expr, $counter:expr, $label:expr, $body:expr) => {
        match $body {
            Ok(_) => $counter += 1,
            Err(e) => $warnings.push(format!("{}: {}", $label, WarnText::warn_text(&e))),
        }
    };
    ($warnings:expr, $counter:expr, $label:expr, $body:expr, $msg:literal $(, $k:ident = $v:expr)* $(,)?) => {
        match $body {
            Ok(_) => $counter += 1,
            Err(e) => {
                let error = WarnText::warn_text(&e);
                $warnings.push(format!("{}: {}", $label, error));
                tracing::warn!(target: "quilltap::restore", $($k = %$v,)* error = %error, $msg);
            }
        }
    };
}

/// How a per-row failure renders in `summary.warnings`: v4's catches write
/// `error.message`, which for a SQLite failure is the driver's BARE sentence
/// (`table chats has no column named …`). `DbError`'s `Display` prefixes it
/// with `sqlite error: ` (a v5 convention for a propagated error), so every
/// warning goes through [`crate::db::fallback::error_text`] instead (P4.147
/// item 10(c) — the #137 / #140 class, proven on a real SQLite arm by
/// `system_restore_state`'s `restore_sqlite_tail_replace` plant).
trait WarnText {
    fn warn_text(&self) -> String;
}

impl WarnText for DbError {
    fn warn_text(&self) -> String {
        crate::db::fallback::error_text(self)
    }
}

impl WarnText for crate::db::document_store_overlay::OverlayError {
    fn warn_text(&self) -> String {
        match self {
            crate::db::document_store_overlay::OverlayError::Db(e) => e.warn_text(),
            other => other.to_string(),
        }
    }
}

/// A storage failure renders its bare message like every other phase; the two
/// wardrobe-specific refusals render v4's thrown message (P4.158 R-H — they
/// used to render their `Debug` form, `NoMount` / `Cycle("…")`). The restore
/// only CREATES, so the no-mount arm is v4's create sentence
/// (`wardrobe.repository.ts:345-348`, measured on `restore_phase_warns_replace`,
/// whose legacy preset names a character that did not restore); the cycle arm
/// already carries v4's own message (`wardrobe-writes.ts:136-139`).
impl WarnText for crate::db::vault_wardrobe_public::WardrobePublicError {
    fn warn_text(&self) -> String {
        use crate::db::vault_wardrobe_public::{WardrobePublicError, NO_MOUNT_MESSAGE};
        match self {
            WardrobePublicError::Db(e) => e.warn_text(),
            WardrobePublicError::NoMount => {
                format!("Cannot create wardrobe item: {NO_MOUNT_MESSAGE}")
            }
            WardrobePublicError::Cycle(message) => message.clone(),
        }
    }
}

impl WarnText for crate::db::text_replacement_rules::TrrError {
    fn warn_text(&self) -> String {
        match self {
            crate::db::text_replacement_rules::TrrError::Db(e) => e.warn_text(),
            other => other.to_string(),
        }
    }
}

impl WarnText for String {
    fn warn_text(&self) -> String {
        self.clone()
    }
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
    llm_logs_degraded: bool,
) -> RestoreSummary {
    let backup_format = extracted.backup_format();
    let root_path = extracted.root_path.clone();
    // v4's `data` (what gets written) vs `parsedData` (what the on-disk names are
    // keyed by). They are the same object in `replace` mode and diverge in
    // `new-account`, where only the former is remapped.
    let data: &crate::services::backup::BackupData = remapped.as_ref().unwrap_or(&extracted.data);
    // `remapped` is `Some` exactly in `new-account` mode.
    let replace_mode = remapped.is_none();

    let main = ws.main().connection();
    let mount = ws.mount_index().map(|w| w.connection());
    let llm = ws.llm_logs().map(|w| w.connection());

    let mut w: Vec<String> = Vec::new();
    let mut c = Counters::default();

    // ── 1. Tags (no dependencies) ────────────────────────────────────────────
    {
        let repo = crate::db::tags::TagsRepository::new(main);
        for tag in &data.tags {
            // v4 `:110-111`: `{ userId, createdAt, updatedAt, ...tagData }` →
            // `create({ ...tagData, nameLower: tagData.nameLower ||
            // tagData.name.toLowerCase() }, { id })`. The restorer's own
            // `toLowerCase()` throws a TypeError for a non-string name (no
            // repository line); `create` then derives again and `_create`
            // validates the WHOLE `TagSchema` (P4.161 Tier 2 — the arm used to
            // coerce every key). `repos.*` is `getUserRepositories(
            // targetUserId)`, whose `create` RE-OWNS the row to the target
            // user (`user-scoped.ts:84`).
            let label = format!(
                "Failed to restore tag \"{}\"",
                crate::services::quilltap_import::js_display_name(tag)
            );
            let created = (|| -> Result<(), String> {
                let mut item = tag.as_object().cloned().unwrap_or_default();
                for k in ["userId", "createdAt", "updatedAt"] {
                    item.remove(k);
                }
                if !crate::api::system_qtap::js_truthy(tag.get("nameLower")) {
                    let lower = match tag.get("name") {
                        Some(Value::String(n)) => n.to_lowercase(),
                        None => {
                            return Err(
                                "Cannot read properties of undefined (reading 'toLowerCase')"
                                    .into(),
                            )
                        }
                        Some(Value::Null) => {
                            return Err(
                                "Cannot read properties of null (reading 'toLowerCase')".into()
                            )
                        }
                        Some(_) => return Err("tagData.name.toLowerCase is not a function".into()),
                    };
                    item.insert("nameLower".into(), Value::String(lower));
                }
                item.insert("userId".into(), Value::String(target_user_id.to_string()));
                let claimed = id_of(tag);
                let create = crate::services::quilltap_import::parse_create_tag(
                    &Value::Object(item),
                    Some(claimed.as_str()).filter(|id| !id.is_empty()),
                )
                .map_err(|r| {
                    crate::services::quilltap_import::create_tag_refused(
                        target_user_id,
                        tag.get("name"),
                        &r,
                    );
                    r.text().to_string()
                })?;
                repo.create(&create, &copts!(claimed, crate::db::tags::CreateOptions))
                    .map_err(|e| e.warn_text())
            })();
            warn_only!(
                w,
                label,
                created,
                "Failed to restore tag",
                tagId = id_of(tag),
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
            // v4 `e000d6bfc` (bug 103), `restore.ts:97`: the columns the archive
            // PREDATES would otherwise be decided by the table DEFAULT rather
            // than by the profile's owner. Seeded BEFORE the unique-name pass,
            // exactly where v4 seeds them.
            let seeded = seed_legacy_connection_profile_fields(p);
            // v4 `:131-141`: logged when EITHER of the two 4.9 columns is
            // absent from the archived record (`=== undefined` — an explicit
            // `null` is present), with v4's four fields only (P4.158 R-G: v5
            // used to log on any seeded column, snake_case, with three more).
            let (absent_prefill, absent_image) = (
                p.get("multiCharacterPrefill").is_none(),
                p.get("supportsImageUpload").is_none(),
            );
            if absent_prefill || absent_image {
                tracing::debug!(
                    target: "quilltap::restore",
                    profileId = %id_of(p),
                    provider = %s(p, "provider"),
                    seededMultiCharacterPrefill = absent_prefill,
                    seededSupportsImageUpload = absent_image,
                    "Seeded connection-profile columns the archive predates"
                );
            }
            let unique = make_unique_profile_name(&original, &taken);
            if unique != original {
                tracing::debug!(
                    target: "quilltap::restore",
                    profileId = %id_of(p),
                    from = %original,
                    to = %unique,
                    "Renamed connection profile on restore to avoid name collision"
                );
            }
            taken.insert(normalize_profile_name(&unique));
            let create = crate::db::connection_profiles::CpCreate {
                user_id: target_user_id.to_string(),
                name: unique,
                provider: s(p, "provider"),
                transport: str_or(p, "transport", "api"),
                // ⚠ v4's Zod default here is `true`, not `false`
                // (`profile.types.ts:69`). v5 read it as `false` until P4.D126,
                // which was invisible because every committed archive carries
                // the key — the same shape bug 103 is about, one column over.
                // Caught by `restore-archive-legacy-profiles.zip`, whose
                // deliberately sparse records omit it.
                courier_delta_mode: b(p, "courierDeltaMode", true),
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
                // v4 `23af7146` + `e000d6bfc`: tri-state, and the seeded record
                // ALWAYS carries the key — a pre-4.9 archive lands an explicit
                // NULL ("never chosen") rather than the table default. On a
                // migrated instance those are different cells: `DEFAULT 1`
                // turned the `[Name]` prefill on for profiles nobody chose it
                // for, Anthropic included, and every multi-character turn then
                // 400s. That is bug 103.
                multi_character_prefill: Some(seeded.multi_character_prefill),
                model_class: os(p, "modelClass"),
                // v4 `65f5021c8`: carried through when the archive has them,
                // otherwise the neutral "no understudy / no tier pick". A
                // self-reference is dropped — see the seeder.
                fallback_profile_id: seeded.fallback_profile_id.clone(),
                allow_tier_fallback: seeded.allow_tier_fallback,
                max_context: on(p, "maxContext"),
                max_tokens: on(p, "maxTokens"),
                is_dangerous_compatible: b(p, "isDangerousCompatible", false),
                // Seeded from the frozen historic provider map when the archive
                // predates the per-profile flag (v4 `e000d6bfc`); a carried
                // value — a stored `false` included — is passed through.
                supports_image_upload: seeded.supports_image_upload,
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
                ),
                "Failed to restore connection profile",
                profileId = id_of(p),
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
                ),
                "Failed to restore image profile",
                profileId = id_of(p),
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
                ),
                "Failed to restore embedding profile",
                profileId = id_of(p),
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
    // built-ins, with their archive ids) and therefore also after projects and
    // groups (13/13a). This used to add "which is exactly what the surviving
    // `instance_settings` pointer expects" — true only when the target's
    // pointer already names the archive's Uploads store (your own backup onto
    // your own instance). On a fresh or re-minted target it does not (dogfood
    // #142), so 22a-ter now pre-applies the archive's built-in pointers before
    // this phase reads them — see that step. **No write changed, only when it happens** — and v4's own
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

    // ── The archived-store map (P4.147, dogfood #141) ────────────────────────
    //
    // ## ⚠ RULED DIVERGENCE (P4.147, 2026-10-05) — a restored entity keeps the
    // ## store the archive restores
    //
    // v4 restores every character (6), project (13) and group (13a) through its
    // CREATE path, which drops the archived `characterDocumentMountPointId` /
    // `officialMountPointId` and provisions a FRESH vault / official store
    // (`characters.repository.ts:262-296`, `restore.ts:315-345`); 22a then
    // restores the archive's real store under its archived id BESIDE it,
    // orphaned. Measured on the dogfood copy (#141): 144 stores from a 77-store
    // archive, Friday's pointer on a 12-file vault while her 805-link one sat
    // unreferenced. Under the standing backup/restore ruling (v5 fixes v4's bugs
    // on the READ side) a `replace` restore now PRESERVES the pointer whenever
    // the archive carries the pointed store — a character's must be a
    // `storeType: 'character'` vault — and mints nothing (Shape A,
    // preserve-at-create). Between here and 22a such a row carries an FK to a
    // store that does not exist yet; nothing in phases 7–22 reads a vault (22f-bis
    // runs after 22a, inside the mount family). Otherwise — no stores in the
    // archive, or a pointer naming a store it does not carry — the create path
    // runs unchanged and stays v4-convergent. `replace` only (ruling R-B): in
    // `new-account` mode the store pointers are never remapped, so preserving
    // would need its own translation and its own ruling.
    //
    // Keyed by the archive's RAW rows, exactly as 22a will create them.
    // `system_restore_state` pins it both ways (`FRESH_STORE_RESIDUAL`).
    //
    // ## One truth for a duplicated id (P4.158, ruling R-B)
    //
    // An archive can carry two `doc_mount_points` rows under one id. 22a
    // creates them in order and the primary key keeps the FIRST, refusing the
    // second (`UNIQUE constraint failed: doc_mount_points.id` — on v4 too). A
    // plain `collect()` kept the LAST, so the map could call a store by a type
    // 22a never restored it as (`restore-archive-dup-store-id.zip`: Lorian's
    // vault id duplicated as a `documents` row dropped her onto a fresh vault).
    // The map now keeps the first row per id — still keyed by the raw rows,
    // now agreeing with 22a on which one is real.
    let archived_stores: std::collections::HashMap<String, String> = if replace_mode {
        let mut map = std::collections::HashMap::new();
        for mp in &data.doc_mount_points {
            map.entry(id_of(mp))
                .or_insert_with(|| str_or(mp, "storeType", "documents"));
        }
        map
    } else {
        std::collections::HashMap::new()
    };
    // ## First claim wins (P4.158, ruling R-B)
    //
    // Nothing stops two archived entities naming ONE store (a hand-edited or
    // damaged archive). Preserving both would cross-link unrelated content —
    // the very thing v4's create refuses by always minting fresh
    // (`characters.repository.ts:253`). So the first entity to claim a store
    // keeps it — phase order (characters 6, projects 13, groups 13a), then the
    // archive's row order — and a later claimant takes the v4-convergent
    // fresh-store arm with a v5-only WARN (`system_restore_state`'s
    // `CLAIMED_STORE_WARNS`; v4 cannot reach the state, so it has no line).
    let mut claims = StoreClaims::default();
    // The entities the preserve arm kept on an archived store, for the
    // completeness pass after the mount family (P4.158 R-A).
    let mut preserved: Vec<PreservedStore> = Vec::new();

    // ── 6. Characters (vault-backed) ─────────────────────────────────────────
    //
    // The vault is preserved when the archive carries it (see the map above);
    // otherwise `create` provisions a fresh one, as v4 always does.
    if let Some(mount) = mount {
        for ch in &data.characters {
            let name = s(ch, "name");
            match restore_one_character(
                main,
                mount,
                target_user_id,
                ch,
                &archived_stores,
                &mut claims,
            ) {
                Ok(Some(vault)) => preserved.push(PreservedStore::new("character", ch, vault)),
                Ok(None) => {}
                Err(e) => {
                    let error = e.warn_text();
                    w.push(format!("Failed to restore character \"{name}\": {error}"));
                    tracing::warn!(
                        target: "quilltap::restore",
                        characterId = %id_of(ch),
                        error = %error,
                        "Failed to restore character"
                    );
                }
            }
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
            // v4 `4d370a90f` (#75), `restore.ts:207`: a backup from before the
            // three Concierge states carries only the legacy pair; derive the
            // state so the restored chat keeps its behaviour. The derive reads
            // only the Concierge keys and the bug-158 strip below only the
            // summary pair, so deriving on the archive's JSON before the typed
            // decode is v4's `withConciergeModeFromLegacy(stripScenarioSeeded
            // Summary(chat))` exactly.
            let chat_in =
                crate::services::dangerous_content::chat_override::with_concierge_mode_from_legacy(
                    chat.clone(),
                );
            // v4's `repos.chats.create` validates the three Concierge enums
            // (P4.124): an out-of-enum value skips the chat with the ZodError.
            // A refused create is v4's `validate` throw inside `_create`: the two
            // repository ERRORs, then the per-chat catch's warning + WARN
            // (`restore.ts:238-240` `Failed to restore chat {chatId, error}`).
            let skip = |w: &mut Vec<String>, error: &str| {
                crate::services::dangerous_content::chat_override::log_chat_create_validation_failure(
                    error,
                );
                w.push(format!("Failed to restore chat \"{title}\": {error}"));
                tracing::warn!(
                    target: "quilltap::restore",
                    chatId = %id,
                    error = %error,
                    "Failed to restore chat"
                );
            };
            if let Some(zod) =
                crate::services::dangerous_content::chat_override::concierge_columns_zod_error(
                    &chat_in,
                )
            {
                skip(&mut w, &zod);
                continue;
            }
            let mut create: crate::db::chats::ChatCreate = match serde_json::from_value(chat_in) {
                Ok(v) => v,
                Err(e) => {
                    skip(&mut w, &e.to_string());
                    continue;
                }
            };
            // The user-scoped `create` re-owns the chat (see phase 1).
            create.user_id = target_user_id.to_string();
            // P4.D208 OUT-OF-MANDATE — P4.D205 preserves. v4 bug 158
            // (`da9c4f34f`), `lib/backup/restore/restore.ts:199`; this call only.
            //
            // A backup taken before bug 158 holds the chat's scenario in
            // `contextSummary` as well as `scenarioText`. Restoring the instance
            // exactly would restore the defect with it, and the heal that
            // cleared those rows will not run again — so it is corrected on the
            // way in. The scenario itself is untouched.
            crate::services::scenario_seeded_summary::strip_scenario_seeded_summary(&mut create);
            // The DB-error arm is the same per-chat catch (v4 `:241-243`): the
            // warning AND the WARN, both with the bare message (P4.147 item
            // 10(a) — this arm logged nothing and rendered `sqlite error: …`).
            if let Err(e) = chats.create(
                &create,
                &copts!(id.clone(), crate::db::chats::CreateOptions),
            ) {
                // v4's two repository ERRORs beneath the catch (`_create`'s
                // rethrowing `safeQuery`, then `chats.repository.ts:280`'s own
                // wrap), each `{collection: chats, error: <bare>}` — the restore
                // runs OUTSIDE the strict scope on both sides, so no
                // `strictFailures` (measured on `restore_sqlite_tail_replace`).
                crate::db::fallback::log_create_failure("chats", &e);
                crate::db::fallback::log_chat_create_wrap_failure(&e);
                let error = e.warn_text();
                w.push(format!("Failed to restore chat \"{title}\": {error}"));
                tracing::warn!(
                    target: "quilltap::restore",
                    chatId = %id,
                    error = %error,
                    "Failed to restore chat"
                );
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
                        "Failed to restore message in chat \"{title}\": {}",
                        e.warn_text()
                    )),
                }
            }

            // `add_message` stamps `lastMessageAt` with the wall clock, so
            // replaying a transcript dates every restored chat to the instant of
            // the restore — which is the timestamp every list sorts and displays
            // by, so the whole history would land in one flat heap. Re-derive it
            // from the transcript we just wrote, under the one predicate that
            // defines it (`crate::chat_activity`). NULL when no character ever
            // posted, where readers fall back to `createdAt`. (v4 `735d9408c` —
            // its own try, post-write, `updatedAt` preserved by omission.)
            // The lookup is v4's `safeQuery(…, null)` in FALLBACK mode: a failed
            // READ logs and writes NULL with no warning pushed — only the
            // `repos.chats.update` can reach v4's catch (the unification
            // review's catch; nothing in a corpus reaches this arm).
            let last = crate::db::chats_messages_read::get_last_played_message_at(main, &id)
                .unwrap_or_else(|e| {
                    tracing::error!(
                        chat_id = %id,
                        error = %e,
                        "Failed to get last played message timestamp"
                    );
                    None
                });
            let restamp = chats
                .update(
                    &id,
                    &crate::db::chats::ChatUpdate {
                        last_message_at: Some(last),
                        ..Default::default()
                    },
                )
                .map(|_| ());
            if let Err(e) = restamp {
                w.push(format!(
                    "Failed to restore last-activity date for chat \"{title}\": {}",
                    e.warn_text()
                ));
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
            // v4 `:255-259`: `{ id, createdAt, updatedAt, ...memoryData }`, the
            // legacy `personaId` stripped, `create(clean, { id })`.
            let mut item = m.as_object().cloned().unwrap_or_default();
            for k in ["id", "createdAt", "updatedAt", "personaId"] {
                item.remove(k);
            }
            // The RULED divergence (`rows::decode_index_keyed_embedding`): a
            // full backup's `JSON.stringify(Float32Array)` embedding — which
            // v4's own restore refuses — is decoded so the memory restores.
            super::rows::decode_index_keyed_embedding(&mut item);
            let item = Value::Object(item);
            let character_id = s(m, "characterId");
            let claimed = id_of(m);
            // v4 restores through the USER-SCOPED memories repository, whose
            // `create` first looks the character up for the target user and
            // refuses a memory whose character is not there
            // (`user-scoped.ts:310-311`) — reached when the character itself
            // failed to restore (P4.158 R-G's plant measured it: v5 went
            // straight to the insert and failed on a different error). ONLY
            // THEN does the base `create` → `_create` validate the WHOLE
            // entity through `MemorySchema` (P4.161 — the restore used to
            // write any row as it came, and to default an absent `importance`
            // to 5.0, `reinforcementCount` / `reinforcedImportance` to 0 and
            // `source` to `AUTO` where the schema's defaults are 0.5 / 1 / 0.5
            // / `MANUAL`: R-C). A refusal logs v4's three repository ERRORs
            // (outside the strict scope — no `strictFailures`).
            let created = if !character_owned_by(main, &character_id, target_user_id) {
                Err("Character not found or access denied".to_string())
            } else {
                crate::db::memories::parse_create_memory(
                    &item,
                    Some(claimed.as_str()).filter(|id| !id.is_empty()),
                )
                .inspect_err(|zod| {
                    crate::db::fallback::log_refused_create("memories", zod);
                    crate::db::fallback::log_memory_create_failure(
                        &character_id,
                        &crate::db::DbError::Internal(zod.clone()),
                    );
                })
                .and_then(|create| {
                    repo.create(
                        &create,
                        &copts!(claimed.clone(), crate::db::memories::CreateOptions),
                    )
                    .map_err(|e| e.warn_text())
                })
            };
            warn_only!(
                w,
                "Failed to restore memory".to_string(),
                created,
                "Failed to restore memory",
                memoryId = id_of(m),
            );
        }
    }

    // ── 10. Prompt templates — id NOT preserved (v4 `:251`), userId retargeted ─
    {
        let repo = crate::db::prompt_templates::PromptTemplatesRepository::new(main);
        for t in &data.prompt_templates {
            // v4 `:270-276`: `{ id, userId, createdAt, updatedAt, ...templateData
            // }` → `create({ ...templateData, userId: targetUserId })`, the
            // WHOLE `PromptTemplateSchema` validated by `_create` before the
            // insert (P4.161 Tier 2 — the arm used to coerce every key). A
            // refusal logs v4's three repository ERRORs (no `strictFailures`).
            let mut item = t.as_object().cloned().unwrap_or_default();
            for k in ["id", "userId", "createdAt", "updatedAt"] {
                item.remove(k);
            }
            item.insert("userId".into(), Value::String(target_user_id.to_string()));
            let created = crate::services::quilltap_import::parse_create_prompt_template(
                &Value::Object(item),
            )
            .inspect_err(|zod| {
                crate::services::quilltap_import::log_refused_prompt_template(
                    target_user_id,
                    t.get("name").and_then(Value::as_str),
                    zod,
                )
            })
            .and_then(|create| {
                repo.create(
                    &create,
                    &copts!(new_id(), crate::db::prompt_templates::CreateOptions),
                )
                .map_err(|e| e.warn_text())
            });
            warn_row!(
                w,
                c.prompt_templates,
                format!("Failed to restore prompt template \"{}\"", s(t, "name")),
                created,
                "Failed to restore prompt template",
                templateId = s(t, "id"),
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
                ),
                "Failed to restore roleplay template",
                templateId = s(t, "id"),
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
                repo.upsert_model(&create),
                "Failed to restore provider model",
                modelId = s(m, "modelId"),
            );
        }
    }

    // ── 13 / 13a. Projects and groups (store-backed) ─────────────────────────
    //
    // **The preserve arm (P4.147 — see the archived-store map above):** when the
    // archived `officialMountPointId` names a store the archive carries, the slim
    // row is written with that pointer and NOTHING is provisioned — the store,
    // its `properties.json` / `description.md` / … and its
    // `project_/group_doc_mount_links` row all restore under their archived ids
    // at 22a–22h.
    //
    // **Otherwise (v4-convergent):** `create` provisions a fresh store and writes
    // the hydrated description/instructions/state and the WHOLE property bag into
    // it (v4 `:315-345` hands `create` the whole row; P4.147 item 8 — the old
    // six-key copy dropped ten project values, and a group's `null` colour landed
    // absent). The bag is parsed BEFORE any row is written, so a value v4's
    // schema would refuse skips the entity with no slim row left behind. In the
    // differential the minted store ids are labelled by ORIGIN (an id the archive
    // does not carry), not by column.
    if let Some(mount) = mount {
        use crate::db::document_store_overlay::StoreEntity;
        use crate::db::groups::GroupEntity;
        use crate::db::projects::ProjectEntity;
        // A project's / group's official store is a `storeType: 'documents'`
        // store, as a character's is a `'character'` vault (phase 6) — a pointer
        // at any other kind takes the fallback arm, as does one an earlier
        // entity already claimed (R-B).
        let mut archived_store = |row: &Value, entity: &'static str| {
            os(row, "officialMountPointId")
                .filter(|id| archived_stores.get(id).map(String::as_str) == Some("documents"))
                .filter(|store| claims.claim(store, entity, &id_of(row)))
        };

        let projects = crate::db::projects::ProjectsRepository::new(main, mount);
        let slim_projects =
            crate::db::store_backed::StoreBackedRepository::<ProjectEntity>::new(main, mount);
        for p in &data.projects {
            let label = format!("Failed to restore project \"{}\"", s(p, "name"));
            // v4's `_create` validates the WHOLE entity on both of v5's arms
            // (`store-backed.repository.ts:142-144`): the row keys (`name`
            // 1–100 code points, `description` ≤ 2000, `instructions` ≤ 10000,
            // `state` a record, the claimed `id`) THEN the bag after its
            // create seed — so a row v4's schema refuses skips the project on
            // the preserve arm too, with nothing written: no slim row, no
            // store claim, no backfill entry (P4.161, P4.155's R-B — the arm
            // used to validate the BAG alone, so a 101-code-point name
            // restored). A refusal logs v4's three repository ERRORs (no
            // `strictFailures` — outside the strict scope).
            let entity = crate::services::quilltap_import::store_create_payload(p, None);
            let claimed = id_of(p);
            if let Err(e) = crate::db::projects::parse_create_entity(
                &entity,
                Some(claimed.as_str()).filter(|id| !id.is_empty()),
            ) {
                crate::db::document_store_overlay::log_refused_store_create(
                    crate::db::document_store_overlay::StoreKind::Project,
                    entity.get("name"),
                    &e,
                );
                w.push(format!("{label}: {e}"));
                project_warn(p, &e);
                continue;
            }
            let properties = crate::db::document_store_overlay::fold_properties(
                p,
                ProjectEntity::property_keys(),
            );
            if let Some(store) = archived_store(p, "project") {
                match slim_projects.create_slim_linked(&s(p, "name"), &store_opts(id_of(p)), &store)
                {
                    Ok(_) => {
                        c.projects += 1;
                        preserved.push(PreservedStore::new("project", p, store));
                    }
                    Err(e) => {
                        let error = e.warn_text();
                        w.push(format!("{label}: {error}"));
                        project_warn(p, &error);
                    }
                }
                continue;
            }
            let input = crate::db::projects::ProjectCreateInput {
                name: s(p, "name"),
                description: os(p, "description"),
                instructions: os(p, "instructions"),
                state: obj(p, "state", serde_json::json!({})),
                properties,
            };
            warn_row!(
                w,
                c.projects,
                label,
                projects.create(&input, &store_opts(id_of(p))),
                "Failed to restore project",
                projectId = id_of(p),
            );
        }

        let groups = crate::db::groups::GroupsRepository::new(main, mount);
        let slim_groups =
            crate::db::store_backed::StoreBackedRepository::<GroupEntity>::new(main, mount);
        for g in &data.groups {
            let label = format!("Failed to restore group \"{}\"", s(g, "name"));
            // Validated WHOLE on both arms, as the projects above (v4's groups
            // have no create seed; P4.161 — the bag alone before).
            let entity = crate::services::quilltap_import::store_create_payload(g, None);
            let claimed = id_of(g);
            let properties = match crate::db::groups::parse_create_entity(
                &entity,
                Some(claimed.as_str()).filter(|id| !id.is_empty()),
            ) {
                Ok(bag) => bag,
                Err(e) => {
                    crate::db::document_store_overlay::log_refused_store_create(
                        crate::db::document_store_overlay::StoreKind::Group,
                        entity.get("name"),
                        &e,
                    );
                    w.push(format!("{label}: {e}"));
                    group_warn(g, &e);
                    continue;
                }
            };
            if let Some(store) = archived_store(g, "group") {
                match slim_groups.create_slim_linked(&s(g, "name"), &store_opts(id_of(g)), &store) {
                    Ok(_) => {
                        c.groups += 1;
                        preserved.push(PreservedStore::new("group", g, store));
                    }
                    Err(e) => {
                        let error = e.warn_text();
                        w.push(format!("{label}: {error}"));
                        group_warn(g, &error);
                    }
                }
                continue;
            }
            let input = crate::db::groups::GroupCreateInput {
                name: s(g, "name"),
                description: os(g, "description"),
                instructions: os(g, "instructions"),
                state: obj(g, "state", serde_json::json!({})),
            };
            warn_row!(
                w,
                c.groups,
                label,
                groups.create_with_properties(&input, &properties, &store_opts(id_of(g))),
                "Failed to restore group",
                groupId = id_of(g),
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
        // v4's `isLLMLogsDegraded()` arm (`restore.ts:349-351`), with its
        // warning verbatim: a DEGRADED logs file warns whether or not the
        // archive carries logs (P4.159's handoff, landed at the `94fbb1ae3`
        // boot-hardness unification). An ABSENT logs partition — a v5-only
        // state; v4 creates the file — keeps v5's older rule: warn only when
        // there are logs to lose.
        None if llm_logs_degraded || !data.llm_logs.is_empty() => {
            tracing::warn!(
                target: "quilltap::restore",
                "Skipping LLM logs restore — logs database is in degraded mode"
            );
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
                    connection_profile_id: os(log, "connectionProfileId"),
                    image_profile_id: os(log, "imageProfileId"),
                    request: match de_opt(log, "request") {
                        Some(r) => r,
                        None => {
                            let error = "request summary is missing or malformed";
                            w.push(format!("Failed to restore LLM log: {error}"));
                            tracing::warn!(target: "quilltap::restore", logId = %id_of(log), error = %error, "Failed to restore LLM log");
                            continue;
                        }
                    },
                    response: match de_opt(log, "response") {
                        Some(r) => r,
                        None => {
                            let error = "response summary is missing or malformed";
                            w.push(format!("Failed to restore LLM log: {error}"));
                            tracing::warn!(target: "quilltap::restore", logId = %id_of(log), error = %error, "Failed to restore LLM log");
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
                    repo.create_for_restore(
                        &create,
                        &crate::db::llm_logs::CreateOptions {
                            id: id_of(log),
                            created_at: created_at.clone(),
                            updated_at: now(),
                        },
                    ),
                    "Failed to restore LLM log",
                    logId = id_of(log),
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
                ),
                "Failed to restore plugin config",
                pluginName = s(cfg, "pluginName"),
            );
        }
    }

    // ── 16. Chat settings ────────────────────────────────────────────────────
    {
        let repo = crate::db::chat_settings::ChatSettingsRepository::new(main);
        // A pre-4.10 backup carries the retired Concierge settings and no
        // conciergeSettings; translate them before the schema strips the old
        // keys (v4 `3b463d6b1`, #76 — the flag reads the BACKUP's chats).
        let backup_has_unmoderated_chats =
            crate::services::backup::uuid_remap::backup_has_unmoderated_chats(&data.chats);
        for raw_row in &data.chat_settings {
            let translated =
                translate_restored_chat_settings(raw_row, backup_has_unmoderated_chats);
            let row = &translated;
            let create: crate::db::chat_settings::ChatSettingsCreate =
                match serde_json::from_value(row.clone()) {
                    Ok(v) => v,
                    Err(e) => {
                        w.push(format!("Failed to restore chat settings: {e}"));
                        tracing::warn!(
                            target: "quilltap::restore",
                            settingsId = %id_of(row),
                            error = %e,
                            "Failed to restore chat settings"
                        );
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
                ),
                "Failed to restore chat settings",
                settingsId = id_of(row),
            );
        }
    }

    // ── 17. Folders — userId retargeted (`:378`) ─────────────────────────────
    {
        let repo = crate::db::folders::FoldersRepository::new(main);
        for f in &data.folders {
            // v4 `:428-429`: `{ id, createdAt, updatedAt, ...folderData }` →
            // `create({ ...folderData, userId: targetUserId }, { id })`, the
            // WHOLE `FolderSchema` validated by `_create` before the insert
            // (P4.161 Tier 2 — the arm used to coerce every key).
            let mut item = f.as_object().cloned().unwrap_or_default();
            for k in ["id", "createdAt", "updatedAt"] {
                item.remove(k);
            }
            item.insert("userId".into(), Value::String(target_user_id.to_string()));
            let claimed = id_of(f);
            let create = match crate::services::quilltap_import::parse_create_folder(
                &Value::Object(item),
                Some(claimed.as_str()).filter(|id| !id.is_empty()),
            ) {
                Ok(create) => create,
                Err(zod) => {
                    crate::services::quilltap_import::log_refused_folder(
                        target_user_id,
                        f.get("path"),
                        &zod,
                    );
                    w.push(format!(
                        "Failed to restore folder \"{}\": {zod}",
                        // v4's template literal over the RAW `folder.name`.
                        crate::services::quilltap_import::js_display_name(f)
                    ));
                    tracing::warn!(
                        target: "quilltap::restore",
                        folderId = %id_of(f),
                        error = %zod,
                        "Failed to restore folder"
                    );
                    continue;
                }
            };
            // v4 `a5df98b3f` (bug 114). Restore KEEPS `create` — ids must be
            // preserved, which the `ensure_by_path` chokepoint cannot do — and
            // gains one arm ahead of the standard warning: a backup taken
            // before bug 114 was collapsed can carry many rows for one
            // (userId, projectId, path). The unique index rejects the extras;
            // the first one restored is the survivor and the rest are noise, so
            // they're dropped QUIETLY rather than filling the report with
            // warnings. No warning, no skipped counter, `foldersRestored`
            // simply not incremented — which is why `warn_row!` cannot express
            // it and this one site is written out.
            match repo.create(
                &create,
                &crate::db::folders::CreateOptions {
                    id: Some(id_of(f)),
                    created_at: Some(now()),
                    updated_at: Some(now()),
                },
            ) {
                Ok(_) => c.folders += 1,
                Err(e) if crate::db::sqlite_errors::is_unique_constraint_error(&e) => {
                    tracing::debug!(
                        target: "quilltap::restore",
                        folderId = %id_of(f),
                        path = %s(f, "path"),
                        "Skipped duplicate folder row during restore",
                    );
                    continue;
                }
                Err(e) => {
                    let error = e.warn_text();
                    w.push(format!(
                        "Failed to restore folder \"{}\": {error}",
                        // v4's template literal over the RAW `folder.name`.
                        crate::services::quilltap_import::js_display_name(f)
                    ));
                    tracing::warn!(
                        target: "quilltap::restore",
                        folderId = %id_of(f),
                        error = %error,
                        "Failed to restore folder"
                    );
                }
            }
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
                ),
                "Failed to restore character plugin data",
                cpdId = id_of(cpd),
                pluginName = s(cpd, "pluginName"),
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
                ),
                "Failed to restore conversation annotation",
                annotationId = id_of(a),
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
            replace_mode.then_some(&archived_stores),
            &mut c,
            &mut w,
        );
        // ── 22h-iii. The preserve arm's completeness pass (P4.158 R-A) ───────
        //
        // Every preserved store now holds whatever the archive restored into
        // it. One that LACKS a managed file is completed from the archived
        // row — see `backfill_preserved_store`.
        for p in &preserved {
            backfill_preserved_store(mount, p, &mut w);
        }
    } else {
        w.push(
            "Document stores were not restored — mount-index database is unavailable".to_string(),
        );
    }

    // ── 22a-ter — MOVED (P4.158 R-C). The archive's built-in pointers are now
    // pre-applied INSIDE `restore_mount_family`, right after 22a — see
    // `preapply_builtin_pointers` for the ruling and why the slot moved.

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
                    ),
                    "Failed to restore file",
                    fileId = id_of(file),
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
            // v4's catch WARNs as well as warning (`restore.ts:802-804`).
            let id = id_of(cd);
            match repo.create(
                &create,
                &copts!(id.clone(), crate::db::chat_documents::CreateOptions),
            ) {
                Ok(_) => c.chat_documents += 1,
                Err(e) => {
                    // v4's base `_create` rethrow line (the chat-documents
                    // repository has no wrap of its own — `create` is `_create`).
                    crate::db::fallback::log_create_failure("chat_documents", &e);
                    let error = e.warn_text();
                    w.push(format!("Failed to restore chat document: {error}"));
                    tracing::warn!(
                        target: "quilltap::restore",
                        chatDocumentId = %id,
                        error = %error,
                        "Failed to restore chat document"
                    );
                }
            }
        }
    }

    // ── 22i-ii. Inform rows (P4.D205, v4 `e7d77bb60`, `restore.ts:772-790`) ──
    //
    // Consumed rows come back too: the row a past turn consumed is what lets a
    // swipe of that turn re-apply the same passage. Must follow the chats
    // (13/14) and their replayed transcripts, since the row points at a seat, at
    // the Host record message and, once consumed, at the assistant message that
    // carried it.
    //
    // **The id is preserved; the timestamps are NOT.** v4 destructures
    // `{ id, createdAt, updatedAt, ...informData }` and passes only
    // `{ id: inform.id }` as its create options, so the restored row is minted
    // fresh clocks. (The IMPORT path is the opposite — it preserves nothing and
    // mints a new id as well.)
    //
    // v4 hands the row to `chatInforms.create`, whose `ChatInformSchema` parse
    // REFUSES a malformed `permanent` (`null`, `"true"`, `1` …): the per-row
    // catch warns with the ZodError's message and WARNs `Failed to restore chat
    // inform {informId, error}`, and the phase ends with a DEBUG summary. v5
    // converges on all three (P4.147 item 9, ruling R-D — v5 used to read any
    // non-boolean as a one-shot, silently demoting a standing inform).
    {
        let repo = crate::db::chat_informs::ChatInformsRepository::new(main);
        for inform in &data.chat_informs {
            // v4 `:812-813`: `{ id, createdAt, updatedAt, ...informData }`,
            // `chatInforms.create(informData, { id })` — a bare `_create`, so
            // the WHOLE `ChatInformSchema` validates the row (P4.161, §S.2 —
            // `restored_inform` used to check `permanent` alone). A refusal
            // logs `validate`'s ERROR then the base rethrow line (no
            // per-repository wrap; no `strictFailures` — the restore runs
            // outside the strict scope). Measured on `restore_informs_replace`
            // and `restore_inform_refusals_replace`.
            let mut item = inform.as_object().cloned().unwrap_or_default();
            for k in ["id", "createdAt", "updatedAt"] {
                item.remove(k);
            }
            let claimed = id_of(inform);
            let outcome =
                match crate::services::quilltap_import::reconcile::parse_create_chat_inform(
                    &Value::Object(item),
                    Some(claimed.as_str()).filter(|id| !id.is_empty()),
                ) {
                    Err(zod) => {
                        crate::db::fallback::log_refused_create("chat_informs", &zod);
                        Err(zod)
                    }
                    Ok(create) => repo.create(&create).map_err(|e| {
                        crate::db::fallback::log_create_failure("chat_informs", &e);
                        e.warn_text()
                    }),
                };
            match outcome {
                Ok(_) => c.chat_informs += 1,
                Err(error) => {
                    w.push(format!("Failed to restore inform: {error}"));
                    tracing::warn!(
                        target: "quilltap::restore",
                        informId = %id_of(inform),
                        error = %error,
                        "Failed to restore chat inform"
                    );
                }
            }
        }
        tracing::debug!(
            target: "quilltap::restore",
            total = data.chat_informs.len(),
            restored = c.chat_informs,
            "Restored chat informs"
        );
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
                repo.save_meta(&character_id, n(meta, "dimensions", 0.0)),
                "Failed to restore vector index meta",
                characterId = character_id,
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
                Err(e) => {
                    let error = e.warn_text();
                    w.push(format!("Failed to restore vector entries: {error}"));
                    tracing::warn!(
                        target: "quilltap::restore",
                        error = %error,
                        "Failed to restore vector entries batch"
                    );
                }
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
                ),
                "Failed to restore conversation chunk",
                chunkId = id_of(chunk),
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
                ),
                "Failed to restore tfidf vocabulary",
                vocabularyId = id_of(v),
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
                ),
                "Failed to restore embedding status",
                statusId = id_of(es),
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
                Err(crate::db::text_replacement_rules::TrrError::Conflict { .. }) => {
                    tracing::debug!(
                        target: "quilltap::restore",
                        fromText = %create.from_text,
                        caseSensitive = create.case_sensitive,
                        "Skipping duplicate text replacement rule on restore"
                    );
                }
                Err(e) => {
                    let error = e.warn_text();
                    w.push(format!("Failed to restore text replacement rule: {error}"));
                    tracing::warn!(
                        target: "quilltap::restore",
                        ruleId = %id_of(rule),
                        error = %error,
                        "Failed to restore text replacement rule"
                    );
                }
            }
        }
        if c.text_replacement_rules > 0 {
            tracing::debug!(
                target: "quilltap::restore",
                count = c.text_replacement_rules,
                "Restored text replacement rules"
            );
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
            Err(e) => {
                let error = DbError::from(e).warn_text();
                w.push(format!(
                    "Failed to restore instance setting \"{key}\": {error}"
                ));
                tracing::warn!(
                    target: "quilltap::restore",
                    key = %key,
                    error = %error,
                    "Failed to restore instance setting"
                );
            }
        }
    }

    // ── 23 / 24. npm plugins and theme bundles ───────────────────────────────
    c.npm_plugins = copy_host_subdirs(
        &root_path.join("plugins").join("npm"),
        &dirs.npm_plugins,
        &[],
        &mut w,
        HostCopyKind::NpmPlugin,
    );
    c.user_installed_themes = copy_host_subdirs(
        &root_path.join("themes"),
        &dirs.themes,
        &[".cache"],
        &mut w,
        HostCopyKind::ThemeBundle,
    );
    // v4 `:817-825` — `themes-index.json` rides along with the bundles.
    if let Some(themes_dir) = dirs.themes.as_ref() {
        let src = root_path.join("themes").join("themes-index.json");
        if src.exists() {
            let _ = std::fs::create_dir_all(themes_dir);
            if let Err(e) = std::fs::copy(&src, themes_dir.join("themes-index.json")) {
                // v4 logs and does NOT warn for this one file.
                tracing::warn!(
                    target: "quilltap::restore",
                    error = %e,
                    "Failed to restore themes-index.json"
                );
            }
        }
    }

    // v4 `:1037` — said whether or not anything failed.
    tracing::info!(
        target: "quilltap::restore",
        "All entities restored with preserved IDs - no reconciliation needed"
    );

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
            Ok(Some(profile_id)) => {
                tracing::debug!(
                    target: "quilltap::restore",
                    targetUserId = %target_user_id,
                    profileId = %profile_id,
                    "Queued full re-index for compact backup restore"
                );
                w.push(
                "This was a compact backup, so search indexes were rebuilt rather than restored — search will warm back up as re-indexing completes. Conversation and document chunks are rebuilt as those chats and stores are next touched."
                    .to_string(),
                )
            }
            Ok(None) => w.push(
                "This was a compact backup, but no default embedding profile is configured, so search cannot be rebuilt yet. Configure one and re-index from the Commonplace Book."
                    .to_string(),
            ),
            Err(e) => {
                let error = e.warn_text();
                w.push(format!(
                    "Failed to queue re-indexing after compact restore: {error}"
                ));
                tracing::warn!(
                    target: "quilltap::restore",
                    error = %error,
                    "Failed to enqueue reindex after compact restore"
                );
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
    let reconcile =
        crate::services::embedding_dimension_reconcile::reconcile_embedding_dimensions(main, mount);
    // v4 `:1088-1095`. `skippedReason` is v4's `null` when the pass ran;
    // `mismatched` an object, through the `…Json` convention (logged last).
    let mismatched = serde_json::json!({
        "memories": reconcile.mismatched.memories,
        "conversationChunks": reconcile.mismatched.conversation_chunks,
        "helpDocs": reconcile.mismatched.help_docs,
        "mountChunks": reconcile.mismatched.mount_chunks,
    })
    .to_string();
    tracing::debug!(
        target: "quilltap::restore",
        targetDimensions = %reconcile
            .target_dimensions
            .map_or_else(|| "null".to_string(), |d| d.to_string()),
        skippedReason = reconcile.skipped_reason.map_or("null", |r| r.as_str()),
        vectorEntriesDeleted = reconcile.vector_entries_deleted,
        vectorIndexMetaFixed = reconcile.vector_index_meta_fixed,
        reindexEnqueued = reconcile.reindex_enqueued,
        mismatchedJson = mismatched.as_str(),
        "Post-restore embedding reconcile complete"
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
    let summary = c.into_summary(data, w, embedding_reconcile);
    // v4 `:1165-1170`, in v4's key order `{targetUserId, mode, summary,
    // warningCount}` (P4.161 Tier 2 item 9 — the census compares order now).
    // `summary` is an object — the `…Json` convention; `mode` is v4's spelling.
    let summary_json = serde_json::to_string(&summary).unwrap_or_default();
    tracing::info!(
        target: "quilltap::restore",
        targetUserId = %target_user_id,
        mode = if replace_mode { "replace" } else { "new-account" },
        summaryJson = summary_json.as_str(),
        warningCount = summary.warnings.len(),
        "Restore operation completed"
    );
    summary
}

/// v4 24a's enqueue: `getDefaultEmbeddingProfile(targetUserId)` (strict
/// `findDefault` — no first-row fallback) then
/// `enqueueEmbeddingReindexAll(userId, {profileId, scope: 'all'})` — plain
/// enqueue at priority -1, maxAttempts 3, payload key order `{profileId,
/// scope}`. `Ok(Some(profile id))` = enqueued, `Ok(None)` = no default profile.
fn enqueue_compact_reindex(main: &Connection, user_id: &str) -> Result<Option<String>, DbError> {
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
        return Ok(None);
    };
    let now = now();
    crate::db::background_jobs::BackgroundJobsRepository::new(main).create(
        &crate::db::background_jobs::BjCreate {
            user_id: user_id.to_string(),
            job_type: "EMBEDDING_REINDEX_ALL".to_string(),
            status: Some("PENDING".to_string()),
            payload: serde_json::json!({ "profileId": &profile_id, "scope": "all" }),
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
    Ok(Some(profile_id))
}

// ─────────────────────────────────────────────────────────────────────────────
// The mount-index family (22a-22e, 22f, 22g, 22h, 22h-i, 22h-ii, 22i)
// ─────────────────────────────────────────────────────────────────────────────

/// The ids one collection carries, for the referential pre-check below.
fn id_set(rows: &[Value]) -> std::collections::HashSet<String> {
    rows.iter().map(id_of).collect()
}

#[allow(clippy::too_many_arguments)]
fn restore_mount_family(
    main: &Connection,
    mount: &Connection,
    data: &crate::services::backup::BackupData,
    original: &crate::services::backup::BackupData,
    root_path: &Path,
    archived_stores: Option<&std::collections::HashMap<String, String>>,
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
                ),
                "Failed to restore doc mount point",
                mountPointId = id_of(mp),
            );
        }
    }

    // 22a-ter. The archive's built-in pointers — `replace` only (`None` in
    // `new-account`, ruling R-B of P4.147).
    if let Some(archived_stores) = archived_stores {
        preapply_builtin_pointers(main, mount, data, archived_stores);
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
                ),
                "Failed to restore doc mount folder",
                folderId = id_of(f),
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
                ),
                "Failed to restore doc mount file",
                fileId = id_of(f),
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
                repo.create_from_row(&coerced, &id_of(link), &now()),
                "Failed to restore doc mount file link",
                linkId = id_of(link),
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
                ),
                "Failed to restore doc mount document",
                documentId = id_of(d),
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
                        Err(e) => blob_failed(w, &id, &DbError::from(e).warn_text()),
                    }
                }
                Err(e) => blob_failed(w, &id, &e.to_string()),
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
                Err(e) => {
                    let error = e.warn_text();
                    w.push(format!(
                        "Failed to restore wardrobe item \"{title}\": {error}"
                    ));
                    tracing::warn!(
                        target: "quilltap::restore",
                        wardrobeItemId = %id_of(item),
                        error = %error,
                        "Failed to restore wardrobe item"
                    );
                }
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
                ),
                "Failed to restore doc mount chunk",
                chunkId = id_of(chunk),
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
                ),
                "Failed to restore project doc mount link",
                linkId = id_of(link),
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
                ),
                "Failed to restore group doc mount link",
                linkId = id_of(link),
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
                ),
                "Failed to restore group character member",
                memberId = id_of(m),
            );
        }
    }
}

/// ## 22a-ter — the archive's built-in pointers, pre-applied (P4.147, #142)
///
/// ### ⚠ RULED DIVERGENCE (P4.147, 2026-10-05) — Uploads resolves to the
/// ### ARCHIVE's store before the files phase reads it
///
/// The files phase resolves Quilltap Uploads through
/// `instance_settings.userUploadsMountPointId` — the TARGET's, which the
/// `replace` wipe deliberately leaves alone and 22o only overwrites LAST. On a
/// freshly provisioned target (the disaster-recovery case), or one whose
/// Uploads store was re-minted, that pointer names the target's own,
/// now-wiped store, so every project-less file the restore must replay failed
/// `Quilltap Uploads mount has not been provisioned` and 22o then pointed at
/// the restored store, which never got the bytes (dogfood #142 — 11 files on
/// the copy; v4 shares it, `restore.ts:508-512,569-576`).
///
/// So each built-in pointer the archive carries is written NOW, with 22o's
/// exact upsert, when its RAW value names a store 22a just restored (ruling
/// R-C). A JSON-quoted value — 15 of the 19 committed archives carry the
/// General/Lantern pointers that way — names no store and is left for 22o, as
/// is any pointer to a store the archive lacks (re-provisioning a missing
/// built-in is a named follow-up). 22o rewrites the same value afterwards and
/// still counts it once. A failed write here is left for 22o, which retries
/// it with the same statement and reports it. `replace` only (ruling R-B).
/// `system_restore_state` pins it both ways (`FRESH_TARGET_UPLOADS`).
///
/// ### ⚠ RULED DIVERGENCE (P4.158 R-C, 2026-10-06) — the slot moved to right
/// ### after 22a
///
/// It used to run after the WHOLE mount family, which left 22f-bis reading the
/// TARGET's General pointer: a SHARED legacy wardrobe item (`characterId:
/// null`) was filed — with a `Wardrobe` folder — in the target's own,
/// now-wiped General store, while 22a-ter and 22o then pointed the instance at
/// the archive's General, so the item was unreachable. v4 shares it (its 22o
/// runs last; measured on v4's own dump of
/// `restore_general_pointer_fresh_replace`). Nothing between 22a and 22f-bis
/// reads a built-in pointer, so the only change the move makes is that 22f-bis
/// resolves the General the instance will come out pointing at.
/// `system_restore_state` pins it both ways (`GENERAL_POINTER_PREAPPLY`).
fn preapply_builtin_pointers(
    main: &Connection,
    mount: &Connection,
    data: &crate::services::backup::BackupData,
    archived_stores: &std::collections::HashMap<String, String>,
) {
    let points = crate::db::doc_mount_points::DocMountPointsRepository::new(mount);
    for key in crate::services::backup::uuid_remap::MOUNT_POINT_SETTING_KEYS {
        let Some(row) = data.instance_settings.iter().find(|r| s(r, "key") == key) else {
            continue;
        };
        let value = s(row, "value");
        if !archived_stores.contains_key(&value) || !points.exists(&value).unwrap_or(false) {
            continue;
        }
        let _ = main.execute(
            "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES (?1, ?2) \
             ON CONFLICT(\"key\") DO UPDATE SET \"value\" = excluded.\"value\"",
            rusqlite::params![key, value],
        );
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
    /// The archived blob's own hash (bug 117, v4 `0b0617fee`). `None` when the
    /// restored blob row carries no hash — v4's `carriedSha256 ? … : {}` arm,
    /// where the ARCHIVE's `files.sha256` stands.
    sha256: Option<String>,
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
    //
    // `sha256` rides the SAME read (bug 117, v4 `0b0617fee`). This branch skips
    // the replay, so it never sees a bridge and cannot take a hash from one — and
    // the archive's own `files.sha256` may be the pre-transcode lie a pre-4.9.0
    // source instance wrote. v4 indexes `data.docMountBlobs` by id and reads the
    // hash from there; v5 reads the row 22f has already restored, which is the
    // same array written to disk — INSIDE the ruled divergence above, which is
    // precisely the difference: v5 consults the restored store, v4 the parsed
    // archive. A NULL/empty hash reads as v4's falsy `carriedSha256`.
    let (stored_mime_type, size_bytes, sha256) = mount
        .query_row(
            "SELECT storedMimeType, sizeBytes, sha256 FROM doc_mount_blobs WHERE id = ?1",
            rusqlite::params![blob_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
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
        // v4's `carriedSha256 ? {sha256} : {}` is a JS truthiness test, so an
        // empty string is as absent as a NULL.
        sha256: sha256.filter(|h| !h.is_empty()),
    })
}

/// Phase 5's body for one file (v4 `:136-187`).
///
/// Project-bound files land in the project's document store; project-less ones in
/// the Quilltap Uploads store under `restored/` — **not** the catch-all
/// `_general/`. The bridges may transcode bitmaps, so the row records the
/// POST-bridge mime, size AND sha256 rather than what the backup claimed
/// (v4 `:552-572`); re-writing the backup's claim would re-introduce the
/// "media_type X but bytes are Y" error a pre-fix backup carries, and (for
/// `sha256`) a FileEntry that cannot be joined to the mount blob it points at
/// (bug 117, v4 `0b0617fee`).
///
/// `carried` short-circuits the ingest — see [`carried_store_rows`] for the
/// ruling and the predicate. The row it writes keeps the same shape: the mime,
/// size and hash still describe what is actually in the store, read off the
/// archive's own blob rather than off a bridge that has just re-written it.
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
    // Bug 117 (v4 `0b0617fee`): `sha256` joins `mimeType`/`size` in describing the
    // bytes actually stored. The replay arm takes the bridge's answer; the carried
    // arm takes the archived blob's own hash, falling back to the archive's
    // `files.sha256` when the blob row carries none (v4's `carriedSha256 ? … : {}`).
    let (storage_key, stored_mime_type, size_bytes, sha256) = match (&stored, carried) {
        (Some(s), _) => (
            s.storage_key(),
            s.stored_mime_type.clone(),
            s.size_bytes as f64,
            s.sha256.clone(),
        ),
        (None, Some(c)) => (
            c.storage_key.clone(),
            c.stored_mime_type.clone(),
            c.size_bytes,
            c.sha256.clone().unwrap_or_else(|| s(file, "sha256")),
        ),
        // Unreachable: `stored` is `None` only when `carried` is `Some`.
        (None, None) => unreachable!("the file phase either ingests or reuses"),
    };

    let create = crate::db::files::FileCreate {
        user_id: target_user_id.to_string(),
        sha256,
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
        // The avatar cache key travels AS-IS: never remapped, never derived
        // (v4 `7fbf8a55b` — the vendored `qtap-export.schema.json` says so in
        // as many words, and nothing on a receiving instance recomputes one).
        generation_key: os(file, "generationKey"),
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

/// Phase 6's body for one character (v4 `:200`).
///
/// **The preserve arm (P4.147, a ruled divergence):** when the archived
/// `characterDocumentMountPointId` names a `storeType: 'character'` store the
/// archive carries (`archived_stores`, `replace` mode only), the slim row is
/// written WITH that pointer and no vault is provisioned — the archive's own
/// vault, its files and their projected managed fields restore under their
/// archived ids at 22a–22g.
///
/// **Otherwise (v4-convergent):** `create_character_with_options` inserts the
/// slim row, provisions a FRESH vault (dropping any incoming pointer, as v4's
/// `repos.characters.create` does), and projects the managed fields into it.
///
/// The vault fields are decoded on BOTH arms, so a row v4's schema would refuse
/// is refused on either. Answers the vault id when the preserve arm kept it
/// (for the R-A completeness pass), `None` on the fresh arm.
fn restore_one_character(
    main: &Connection,
    mount: &Connection,
    target_user_id: &str,
    ch: &Value,
    archived_stores: &std::collections::HashMap<String, String>,
    claims: &mut StoreClaims,
) -> Result<Option<String>, DbError> {
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
            .map_err(|e| DbError::Internal(format!("character vault fields: {e}")))?;
    let opts = crate::db::characters::CreateOptions {
        id: id_of(ch),
        created_at: now(),
        updated_at: now(),
    };
    if let Some(vault_id) = os(ch, "characterDocumentMountPointId")
        .filter(|id| archived_stores.get(id).map(String::as_str) == Some("character"))
        .filter(|vault| claims.claim(vault, "character", &opts.id))
    {
        let slim = crate::db::characters::CharacterCreate {
            character_document_mount_point_id: Some(vault_id.clone()),
            ..slim
        };
        crate::db::characters::CharactersRepository::new(main).create(&slim, &opts)?;
        return Ok(Some(vault_id));
    }
    crate::db::character_vault::create_character_with_options(main, mount, &slim, &vault, &opts)
        .map(|_| None)
}

/// Which of phases 23 / 24 a [`copy_host_subdirs`] call is: it names the
/// warning's noun and picks the phase's log lines (v4 `restore.ts:956-1035` —
/// per bundle copied (debug) or refused (WARN, beside the warning), the
/// phase's summary (info, only when one landed), and the debug a backup without
/// the directory gets). `tracing` takes a literal message, hence the branches.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HostCopyKind {
    NpmPlugin,
    ThemeBundle,
}

impl HostCopyKind {
    fn label(self) -> &'static str {
        match self {
            HostCopyKind::NpmPlugin => "npm plugin",
            HostCopyKind::ThemeBundle => "theme bundle",
        }
    }

    fn log_missing(self) {
        match self {
            HostCopyKind::NpmPlugin => {
                tracing::debug!(target: "quilltap::restore", "No npm plugins directory in backup")
            }
            HostCopyKind::ThemeBundle => {
                tracing::debug!(target: "quilltap::restore", "No themes directory in backup")
            }
        }
    }

    fn log_copied(self, name: &str) {
        match self {
            HostCopyKind::NpmPlugin => {
                tracing::debug!(target: "quilltap::restore", pluginName = %name, "Restored npm plugin")
            }
            HostCopyKind::ThemeBundle => {
                tracing::debug!(target: "quilltap::restore", themeId = %name, "Restored theme bundle")
            }
        }
    }

    fn log_failed(self, name: &str, error: &str) {
        match self {
            HostCopyKind::NpmPlugin => tracing::warn!(
                target: "quilltap::restore",
                pluginName = %name,
                error = %error,
                "Failed to restore npm plugin"
            ),
            HostCopyKind::ThemeBundle => tracing::warn!(
                target: "quilltap::restore",
                themeId = %name,
                error = %error,
                "Failed to restore theme bundle"
            ),
        }
    }

    /// `listed` is every directory entry, copied or not (v4's npm `plugins`).
    fn log_summary(self, count: usize, listed: &[String]) {
        match self {
            HostCopyKind::NpmPlugin => {
                // `plugins` is an array: the `…Json` file-layer convention.
                let plugins = serde_json::to_string(listed).unwrap_or_default();
                tracing::info!(
                    target: "quilltap::restore",
                    count,
                    pluginsJson = plugins.as_str(),
                    "Restored npm plugins"
                )
            }
            HostCopyKind::ThemeBundle => tracing::info!(
                target: "quilltap::restore",
                count,
                "Restored user-installed theme bundles"
            ),
        }
    }
}

/// Phases 23 and 24 — recursive copy of every subdirectory of `src` into `dest`,
/// skipping `skip` names. Returns how many landed; `None` for `dest` (a host with
/// no such directory) is a documented no-op that reports 0.
///
/// v4 reads the backup's directory FIRST and logs the "no directory" debug when
/// it is absent (`:988-991`, `:1032-1035`) — so does v5, before the host check
/// (P4.158 R-G: the line fires on every archive without bundles).
fn copy_host_subdirs(
    src: &Path,
    dest: &Option<std::path::PathBuf>,
    skip: &[&str],
    w: &mut Vec<String>,
    kind: HostCopyKind,
) -> usize {
    let Ok(entries) = std::fs::read_dir(src) else {
        kind.log_missing();
        return 0;
    };
    let Some(dest) = dest else {
        return 0;
    };
    let entries: Vec<std::fs::DirEntry> = entries.flatten().collect();
    let _ = std::fs::create_dir_all(dest);
    let mut n0 = 0usize;
    for entry in &entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.path().is_dir() || skip.contains(&name.as_str()) {
            continue;
        }
        match copy_dir_recursive(&entry.path(), &dest.join(&name)) {
            Ok(()) => {
                n0 += 1;
                kind.log_copied(&name);
            }
            Err(e) => {
                w.push(format!(
                    "Failed to restore {} \"{name}\": {e}",
                    kind.label()
                ));
                kind.log_failed(&name, &e.to_string());
            }
        }
    }
    if n0 > 0 {
        let listed: Vec<String> = entries
            .iter()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        kind.log_summary(n0, &listed);
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

/// v4's per-project / per-group catch WARN (`restore.ts:325`, `:343`) for the
/// arms that refuse before `create` (the bag parse, the preserve arm's slim
/// write) — the fallback arm's `create` logs it through [`warn_row!`].
fn project_warn(row: &Value, error: &str) {
    tracing::warn!(
        target: "quilltap::restore",
        projectId = %id_of(row),
        error = %error,
        "Failed to restore project"
    );
}

fn group_warn(row: &Value, error: &str) {
    tracing::warn!(
        target: "quilltap::restore",
        groupId = %id_of(row),
        error = %error,
        "Failed to restore group"
    );
}

/// 22f's per-blob catch: the warning and v4's WARN (`restore.ts:714-715`), for
/// both the bytes read and the raw insert.
fn blob_failed(w: &mut Vec<String>, id: &str, error: &str) {
    w.push(format!("Failed to restore doc-store blob {id}: {error}"));
    tracing::warn!(
        target: "quilltap::restore",
        blobId = %id,
        error = %error,
        "Failed to restore doc mount blob"
    );
}

/// One entity the preserve arm kept on an archived store (P4.158 R-A).
struct PreservedStore {
    /// `character` / `project` / `group` — the WARN's `entity` field.
    entity: &'static str,
    entity_id: String,
    store: String,
    /// The archived row, the backfill's source.
    row: Value,
}

impl PreservedStore {
    fn new(entity: &'static str, row: &Value, store: String) -> Self {
        Self {
            entity,
            entity_id: id_of(row),
            store,
            row: row.clone(),
        }
    }
}

/// ## ⚠ RULED DIVERGENCE (P4.158 R-A, 2026-10-06) — a preserved store is
/// ## COMPLETED from the archived row
///
/// The preserve arm (P4.147) keeps an entity on the store the archive carries
/// and projects nothing, trusting the archive to restore the store whole. A
/// damaged archive can carry a store that LACKS a managed file; left alone, the
/// restored entity reads that field blank, where v4 — which never preserves,
/// it projects every managed field from the archived row into a FRESH store
/// (`characters.repository.ts:262-296`, `store-backed.repository.ts:156`) —
/// reads the row's value. Refusing the preserve instead would lose the store's
/// OTHER files (the reason #141 exists), so each MISSING managed file is
/// written from the row, through the same write path the projection uses (so
/// it is chunked on write like the projection's), and a file the store
/// carries is never touched.
///
/// The managed sets are the projections': a vault's is
/// [`crate::db::character_vault::backfill_character_vault_managed_files`]'s; a
/// project's / group's official store holds `properties.json` (the bag through
/// the create-time parse, the fallback arm's bytes), `description.md`,
/// `instructions.md` (`""` when absent) and `state.json` (v4
/// `writeManagedFields`). Each file written logs a v5-only WARN (v4 cannot
/// reach the state); a write that fails is one warning and the entity keeps
/// what the archive gave it. `system_restore_state` pins it both ways
/// (`PRESERVE_BACKFILL`, `BACKFILL_WARNS`).
fn backfill_preserved_store(mount: &Connection, p: &PreservedStore, w: &mut Vec<String>) {
    let written = match p.entity {
        "character" => serde_json::from_value::<
            crate::db::vault_character_write::CharacterVaultWriteInput,
        >(p.row.clone())
        .map_err(|e| DbError::Internal(format!("character vault fields: {e}")))
        .and_then(|vault| {
            crate::db::character_vault::backfill_character_vault_managed_files(
                mount, &p.store, &vault,
            )
        }),
        _ => backfill_official_store(mount, p),
    };
    match written {
        Ok(paths) => {
            for path in paths {
                tracing::warn!(
                    target: "quilltap::restore",
                    entity = p.entity,
                    entityId = %p.entity_id,
                    mountPointId = %p.store,
                    relativePath = %path,
                    "Backfilled a managed file the archived store was missing"
                );
            }
        }
        Err(e) => w.push(format!(
            "Failed to complete the archived store for {} \"{}\": {}",
            p.entity,
            s(&p.row, "name"),
            e.warn_text()
        )),
    }
}

/// [`backfill_preserved_store`]'s project / group half: the four files v4's
/// `writeManagedFields` projects, each only where the store lacks it.
fn backfill_official_store(mount: &Connection, p: &PreservedStore) -> Result<Vec<String>, DbError> {
    use crate::db::document_store_overlay::{fold_properties, StoreEntity};
    let internal = |e: String| DbError::Internal(e);
    let pretty = |v: serde_json::Result<String>| v.map_err(|e| internal(e.to_string()));
    let properties = match p.entity {
        "project" => pretty(serde_json::to_string_pretty(
            &crate::db::projects::parse_create_properties(&fold_properties(
                &p.row,
                crate::db::projects::ProjectEntity::property_keys(),
            ))
            .map_err(internal)?,
        ))?,
        _ => pretty(serde_json::to_string_pretty(
            &crate::db::groups::GroupEntity::parse_properties(&fold_properties(
                &p.row,
                crate::db::groups::GroupEntity::property_keys(),
            ))
            .map_err(internal)?,
        ))?,
    };
    let state = obj(&p.row, "state", serde_json::json!({}));
    let state = if state.is_null() {
        "{}".to_string()
    } else {
        pretty(serde_json::to_string_pretty(&state))?
    };
    let files = [
        ("properties.json", properties),
        (
            "description.md",
            os(&p.row, "description").unwrap_or_default(),
        ),
        (
            "instructions.md",
            os(&p.row, "instructions").unwrap_or_default(),
        ),
        ("state.json", state),
    ];
    let links = crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(mount);
    let mut written = Vec::new();
    for (path, content) in files {
        if links
            .find_by_mount_point_and_path(&p.store, path)?
            .is_some()
        {
            continue;
        }
        links.write_database_document(&p.store, path, &content)?;
        written.push(path.to_string());
    }
    Ok(written)
}

#[cfg(test)]
mod backfill_official_store_tests {
    //! P4.161 Tier 2 item 10: the official store's managed files and the
    //! backfill's failure warning — v5-alone pins (v4 never preserves an
    //! archived store, so it cannot reach either state), over a provisioned
    //! mount-index in a temp dir.
    use super::*;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const STORE: &str = "mp-preserved-store";

    fn mount() -> (tempfile::TempDir, crate::db::Writer) {
        let dir = tempfile::tempdir().unwrap();
        crate::services::provisioning::provision_fresh_instance(dir.path(), PEPPER).unwrap();
        let w =
            crate::db::Writer::open_writable(&dir.path().join("quilltap-mount-index.db"), PEPPER)
                .unwrap();
        (dir, w)
    }

    fn project(row: Value) -> PreservedStore {
        PreservedStore::new("project", &row, STORE.to_string())
    }

    fn read(m: &crate::db::Writer, path: &str) -> Option<String> {
        crate::db::database_store::read_database_document(m.connection(), STORE, path)
            .ok()
            .map(|d| d.content)
    }

    /// All four of `writeManagedFields`' files land in an EMPTY store, from
    /// the row: `properties.json` (the bag through the create-time parse),
    /// `description.md`, `instructions.md` and `state.json`.
    #[test]
    fn every_official_store_file_is_backfilled_from_the_row() {
        let (_d, m) = mount();
        let p = project(serde_json::json!({
            "id": "a3000000-0000-4000-8000-000000000001",
            "name": "The Voyage",
            "description": "A fixture project.",
            "instructions": "Mind the gap.",
            "state": {"leg": 2},
            "color": "#334455"
        }));
        let mut written = backfill_official_store(m.connection(), &p).unwrap();
        written.sort();
        assert_eq!(
            written,
            [
                "description.md",
                "instructions.md",
                "properties.json",
                "state.json"
            ]
        );
        assert_eq!(
            read(&m, "description.md").as_deref(),
            Some("A fixture project.")
        );
        assert_eq!(
            read(&m, "instructions.md").as_deref(),
            Some("Mind the gap.")
        );
        assert_eq!(
            read(&m, "state.json").as_deref(),
            Some("{\n  \"leg\": 2\n}")
        );
        let props: Value = serde_json::from_str(&read(&m, "properties.json").unwrap()).unwrap();
        assert_eq!(props["color"], "#334455");
        // The create-time seed (`allowAnyCharacter ?? true`).
        assert_eq!(props["allowAnyCharacter"], true);
    }

    /// An absent `instructions` writes `""` and an absent `state` writes
    /// `{}`; a file the store carries is never touched.
    #[test]
    fn absent_fields_write_defaults_and_present_files_stay() {
        let (_d, m) = mount();
        let links =
            crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(m.connection());
        links
            .write_database_document(STORE, "properties.json", "{}")
            .unwrap();
        links
            .write_database_document(STORE, "description.md", "Theirs.")
            .unwrap();
        let p = project(serde_json::json!({ "name": "Bare", "description": "Ours." }));
        let mut written = backfill_official_store(m.connection(), &p).unwrap();
        written.sort();
        assert_eq!(written, ["instructions.md", "state.json"]);
        assert_eq!(read(&m, "properties.json").as_deref(), Some("{}"));
        assert_eq!(read(&m, "description.md").as_deref(), Some("Theirs."));
        assert_eq!(read(&m, "instructions.md").as_deref(), Some(""));
        assert_eq!(read(&m, "state.json").as_deref(), Some("{}"));
    }

    /// The failure arm: a write the store refuses (a planted trigger) is ONE
    /// warning — `Failed to complete the archived store for <entity> "<name>":
    /// <bare error>` — and no WARN line for a file that was not written.
    #[test]
    fn a_failed_backfill_is_one_warning() {
        let (_d, m) = mount();
        m.connection()
            .execute_batch(
                "CREATE TRIGGER planted_refusal BEFORE INSERT ON doc_mount_file_links \
                 BEGIN SELECT RAISE(ABORT, 'planted: store refuses writes'); END",
            )
            .unwrap();
        let p = project(serde_json::json!({ "name": "The Voyage", "description": "d" }));
        let mut w = Vec::new();
        let ((), lines) = crate::test_support::captured_with(|| {
            backfill_preserved_store(m.connection(), &p, &mut w)
        });
        assert_eq!(
            w,
            vec![
                "Failed to complete the archived store for project \"The Voyage\": \
                 planted: store refuses writes"
                    .to_string()
            ]
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Backfilled a managed file")),
            "{lines:?}"
        );
    }
}

/// The archived stores the preserve arm has handed out so far (P4.158, ruling
/// R-B): store id → the entity that claimed it first.
#[derive(Default)]
struct StoreClaims(std::collections::HashMap<String, String>);

impl StoreClaims {
    /// Claim `store` for `entity_id`; `true` when it was free. A taken store
    /// logs the v5-only WARN naming both claimants and answers `false`, so the
    /// caller takes the fresh-store arm.
    fn claim(&mut self, store: &str, entity: &'static str, entity_id: &str) -> bool {
        match self.0.get(store) {
            None => {
                self.0.insert(store.to_string(), entity_id.to_string());
                true
            }
            Some(first) => {
                tracing::warn!(
                    target: "quilltap::restore",
                    entity,
                    entityId = %entity_id,
                    mountPointId = %store,
                    claimedBy = %first,
                    "Archived store already claimed by an earlier entity; falling back to a fresh store"
                );
                false
            }
        }
    }
}

fn store_opts(id: String) -> crate::db::store_backed::StoreCreateOptions {
    crate::db::store_backed::StoreCreateOptions {
        id: Some(id),
        created_at: None,
        updated_at: None,
    }
}

/// One archived `chat_informs` row as v4's restore writes it: the id kept,
/// the clocks minted (`now`), and — P4.D249, v4 `52d6e7ecd` — the standing
/// flag carried. v4 spreads the archived row into `create`, where
/// The two legacy translations v4's restore chains over a `chat_settings`
/// row before `create` (`restore.ts:393-413`): the Concierge one first
/// (`3b463d6b1`, #76), THEN — on its output — the retired impersonated-line
/// voice toggle (`07b8f0209`, P4.D251). Each logs its DEBUG only when it
/// changed the record (v4 compares references; v5 compares the value / the
/// `Cow` variant), with `settingsId` read off the RAW row both times.
fn translate_restored_chat_settings(raw_row: &Value, backup_has_unmoderated_chats: bool) -> Value {
    let concierge_translated =
        crate::services::dangerous_content::legacy_concierge_settings::with_concierge_settings_from_legacy(
            raw_row,
            backup_has_unmoderated_chats,
        );
    // v4 `conciergeTranslated !== rawSettings` — the translation returns the
    // record untouched when it already carried a truthy object.
    if &concierge_translated != raw_row {
        tracing::debug!(
            target: "quilltap::restore",
            settingsId = %id_of(raw_row),
            backupHasUnmoderatedChats = backup_has_unmoderated_chats,
            "Translated pre-4.10 Concierge settings for restore"
        );
    }
    // A 4.10-dev backup carries the retired on/off `impersonationVoiceRewrite`
    // — and so does a v5 backup taken before this round, since the backup
    // carries `find_by_user_id`'s whole row. Translate it the way v4 does
    // (`true`/`1` → `'ask'`, an explicit mode kept), or `ChatSettingsCreate`
    // would silently drop the unknown key and land `'off'`.
    match crate::services::impersonation_voice_legacy::with_impersonation_voice_mode_from_legacy(
        &concierge_translated,
    ) {
        std::borrow::Cow::Borrowed(_) => concierge_translated,
        std::borrow::Cow::Owned(settings) => {
            // Read outside the macro: `tracing` shadows `Value` inside it.
            let mode = settings
                .get("impersonationVoiceMode")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            tracing::debug!(
                target: "quilltap::restore",
                settingsId = %id_of(raw_row),
                impersonationVoiceMode = %mode,
                "Translated the retired impersonated-line voice toggle for restore"
            );
            settings
        }
    }
}

#[cfg(test)]
mod restored_chat_settings_translation_tests {
    //! P4.D251: the voice-toggle translation's DEBUG line is CAPTURE-PINNED —
    //! its bytes on a legacy record, its silence on a current one (the
    //! `round-52d6e7ecd-unification` lesson: a literal in a differential is
    //! not a capture pin). The `system_restore_state` family proves the
    //! restored ROWS against v4 over `restore-archive-voice-legacy.zip`.
    use super::*;
    use serde_json::json;

    /// A current record (no legacy Concierge keys, a truthy `conciergeSettings`,
    /// no retired voice key) → NO line from either translation.
    fn current() -> Value {
        json!({
            "id": "ab000000-0000-4000-8000-000000000001",
            "userId": "u1",
            "conciergeSettings": { "enabled": true },
            "impersonationVoiceMode": "ask",
            "createdAt": "2026-01-01T00:00:00.000Z"
        })
    }

    #[test]
    fn a_legacy_toggle_is_translated_with_v4s_debug_line_in_v4s_field_order() {
        let mut row = current();
        let obj = row.as_object_mut().unwrap();
        obj.remove("impersonationVoiceMode");
        obj.insert("impersonationVoiceRewrite".into(), json!(true));
        let (out, lines) =
            crate::test_support::captured_with(|| translate_restored_chat_settings(&row, false));
        assert_eq!(out["impersonationVoiceMode"], json!("ask"));
        assert!(out.get("impersonationVoiceRewrite").is_none());
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::restore Translated the retired impersonated-line voice toggle for restore \
                 settingsId=ab000000-0000-4000-8000-000000000001 impersonationVoiceMode=ask"
                    .to_string()
            ],
            "{lines:#?}"
        );
    }

    #[test]
    fn an_explicit_mode_beside_the_toggle_wins_and_the_line_reports_it() {
        let mut row = current();
        row.as_object_mut()
            .unwrap()
            .insert("impersonationVoiceRewrite".into(), json!(1));
        row["impersonationVoiceMode"] = json!("always");
        let (out, lines) =
            crate::test_support::captured_with(|| translate_restored_chat_settings(&row, false));
        assert_eq!(out["impersonationVoiceMode"], json!("always"));
        assert_eq!(lines.len(), 1, "{lines:#?}");
        assert!(
            lines[0].ends_with("impersonationVoiceMode=always"),
            "{}",
            lines[0]
        );
    }

    #[test]
    fn a_current_record_is_returned_untouched_in_silence() {
        let row = current();
        let (out, lines) =
            crate::test_support::captured_with(|| translate_restored_chat_settings(&row, false));
        assert_eq!(out, row);
        assert!(lines.is_empty(), "{lines:#?}");
    }

    /// The order of the chain: a pre-4.10 record (legacy Concierge keys AND the
    /// retired toggle) logs the Concierge line FIRST, then the voice line.
    #[test]
    fn the_concierge_translation_runs_first() {
        let row = json!({
            "id": "ab000000-0000-4000-8000-000000000002",
            "userId": "u1",
            "dangerousContentSettings": { "mode": "OFF" },
            "impersonationVoiceRewrite": false,
            "createdAt": "2026-01-01T00:00:00.000Z"
        });
        let (out, lines) =
            crate::test_support::captured_with(|| translate_restored_chat_settings(&row, false));
        assert!(out.get("conciergeSettings").is_some());
        assert_eq!(out["impersonationVoiceMode"], json!("off"));
        assert_eq!(lines.len(), 2, "{lines:#?}");
        assert!(lines[0].starts_with(
            "DEBUG quilltap::restore Translated pre-4.10 Concierge settings for restore"
        ));
        assert!(lines[1].starts_with("DEBUG quilltap::restore Translated the retired impersonated-line voice toggle for restore"));
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

/// v4's user-scoped `charactersRepo.findById(characterId)` as the memories
/// create uses it: the character exists and belongs to `user_id`. A failed
/// read answers `false` (v4's `findById` is a fallback read — it logs and
/// answers `null`).
fn character_owned_by(main: &Connection, character_id: &str, user_id: &str) -> bool {
    main.query_row(
        "SELECT 1 FROM characters WHERE id = ?1 AND userId = ?2",
        rusqlite::params![character_id, user_id],
        |_| Ok(()),
    )
    .is_ok()
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
    // === P4.D205 ===
    chat_informs: usize,
    // === end P4.D205 ===
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
            // === P4.D205 ===
            chat_informs: self.chat_informs,
            // === end P4.D205 ===
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

#[cfg(test)]
mod host_copy_log_tests {
    //! P4.158 R-G: phases 23/24's lines are unreachable in the restore family
    //! (no archive carries a bundle, and its host declares no plugin/theme
    //! directory), so they are capture-pinned here — v4's messages and fields
    //! (`restore.ts:974-990`, `:1011-1034`), with the silence of a host
    //! without the directory.
    use super::*;

    fn tree() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("qtap-plugin-a")).unwrap();
        std::fs::write(src.join("qtap-plugin-a").join("index.js"), "x").unwrap();
        std::fs::write(src.join("README.txt"), "not a bundle").unwrap();
        let dest = dir.path().join("dest");
        (dir, src, dest)
    }

    #[test]
    fn a_copied_npm_plugin_logs_v4s_debug_and_summary() {
        let (_dir, src, dest) = tree();
        let mut w = Vec::new();
        let (n, lines) = crate::test_support::captured_with(|| {
            copy_host_subdirs(
                &src,
                &Some(dest.clone()),
                &[],
                &mut w,
                HostCopyKind::NpmPlugin,
            )
        });
        assert_eq!(n, 1);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::restore Restored npm plugin pluginName=qtap-plugin-a".to_string(),
                "INFO quilltap::restore Restored npm plugins count=1 pluginsJson=[\"qtap-plugin-a\"]"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn a_copied_theme_bundle_logs_v4s_debug_and_summary() {
        let (_dir, src, dest) = tree();
        let mut w = Vec::new();
        let (_, lines) = crate::test_support::captured_with(|| {
            copy_host_subdirs(
                &src,
                &Some(dest.clone()),
                &[],
                &mut w,
                HostCopyKind::ThemeBundle,
            )
        });
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::restore Restored theme bundle themeId=qtap-plugin-a".to_string(),
                "INFO quilltap::restore Restored user-installed theme bundles count=1".to_string(),
            ]
        );
    }

    /// No directory in the backup → v4's debug, read BEFORE the host check;
    /// a host without the directory and a backup WITH one → silence (v5's
    /// documented no-op).
    #[test]
    fn a_missing_directory_logs_and_a_hostless_copy_is_silent() {
        let (dir, src, _) = tree();
        let mut w = Vec::new();
        let (_, lines) = crate::test_support::captured_with(|| {
            copy_host_subdirs(
                &dir.path().join("absent"),
                &None,
                &[],
                &mut w,
                HostCopyKind::NpmPlugin,
            );
            copy_host_subdirs(
                &dir.path().join("absent"),
                &None,
                &[],
                &mut w,
                HostCopyKind::ThemeBundle,
            );
            copy_host_subdirs(&src, &None, &[], &mut w, HostCopyKind::NpmPlugin);
        });
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::restore No npm plugins directory in backup".to_string(),
                "DEBUG quilltap::restore No themes directory in backup".to_string(),
            ]
        );
        assert!(w.is_empty());
    }
}
