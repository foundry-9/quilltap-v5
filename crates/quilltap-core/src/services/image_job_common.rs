//! Shared helpers for the avatar + story-background job handlers: building the
//! cheap-LLM selection from the user's profiles, decoding the provider's base64
//! bytes, and the generate-with-post-hoc-reroute flow the two handlers share
//! (parameterized by the failure-message prefix + orientation).
//!
//! **The params are no longer built here.** Until v4 `84f33ce94` these two job
//! paths assembled the request inline as `{ prompt, model, n: 1, ...resolved,
//! quality, style: 'natural' }`, reading exactly ONE key off the profile — so a
//! `negativePrompt`, `seed`, `guidanceScale`, `steps` or (later) `loras`
//! configured on a profile worked in the Salon and vanished for avatars and
//! story backgrounds. v5 inherited that drift verbatim (the deleted
//! `build_job_gen_params` hard-coded it and `quality_from_parameters` WAS the
//! "reads only quality" bug). Both attempts now go through the one shared
//! [`build_image_gen_params`], so these paths GAIN every one of those fields.

use rusqlite::Connection;
use serde_json::Value;

use crate::cheap_llm::{
    get_cheap_llm_provider, CheapLlmConfig, CheapLlmProfile, CheapLlmSelection,
};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::image_gen::image_attempt::{
    make_logged_image_attempt, ImageAttemptParams, LoggedImageAttemptOptions, PrebuiltImageParams,
};
use crate::image_gen::params_builder::{
    build_image_gen_params, ImageDeclarations, ImageGenOverrides, ImageParamsLogContext,
    ImageProfileLike,
};
use crate::image_gen::Orientation;
use crate::model::image::{ImageGenResponse, ImageProvider};
use crate::services::dangerous_content::image_failover::{
    generate_image_with_concierge_failover, FailoverProfile, ImageFailoverContext,
    ImageFailoverError, ImageFailoverOutcome, ImagePurpose, ImageUnderstudySource,
};
use crate::services::dangerous_content::provider_routing::ApiKeyResolver;
use crate::services::dangerous_content::resolver::ResolvedConciergePolicy;
use crate::services::route_trail::{RouteAttemptVia, RouteProfileKind};

/// The plugin-registry declaration seam (v4's `getImageGenerationModels` +
/// `getImageProviderConstraints`, per provider). `84f33ce94` widened it from
/// the orientation half to v4's whole declaration set — see
/// [`ImageDeclarations`].
pub type ImageDeclarationsFn = dyn Fn(&str) -> ImageDeclarations + Send + Sync;

/// The job handlers' fixed overrides — v4's `{ n: 1, style: 'natural' }` on
/// BOTH job paths and BOTH attempts (natural reads better for an avatar and for
/// an ambient background alike).
fn job_overrides() -> ImageGenOverrides {
    ImageGenOverrides {
        n: Some(1.0),
        style: Some("natural".to_string()),
        ..Default::default()
    }
}

/// v4 `buildImageGenParams`'s `fallbackModel` default.
const DEFAULT_IMAGE_MODEL: &str = "dall-e-3";

/// v4 `cheapLLMProfile` field extraction from a connection-profile `Value`.
pub(crate) fn cheap_llm_profile_from_value(v: &Value) -> CheapLlmProfile {
    CheapLlmProfile {
        id: v
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        provider: v
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        model_name: v
            .get("modelName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        base_url: v.get("baseUrl").and_then(Value::as_str).map(str::to_string),
        is_cheap: v.get("isCheap").and_then(Value::as_bool) == Some(true),
        is_dangerous_compatible: v.get("isDangerousCompatible").and_then(Value::as_bool)
            == Some(true),
        parameters: v.get("parameters").cloned(),
        max_tokens: v.get("maxTokens").and_then(Value::as_f64),
        max_context: v.get("maxContext").and_then(Value::as_f64),
        model_class: v
            .get("modelClass")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// v4's `CheapLLMConfig` from a chat-settings `cheapLLMSettings` sub-object (or the
/// `DEFAULT_CHEAP_LLM_CONFIG` defaults when absent).
pub(crate) fn cheap_llm_config_from_settings(cheap: Option<&Value>) -> CheapLlmConfig {
    match cheap {
        Some(c) => CheapLlmConfig {
            strategy: c
                .get("strategy")
                .and_then(Value::as_str)
                .unwrap_or("PROVIDER_CHEAPEST")
                .to_string(),
            user_defined_profile_id: c
                .get("userDefinedProfileId")
                .and_then(Value::as_str)
                .map(str::to_string),
            default_cheap_profile_id: c
                .get("defaultCheapProfileId")
                .and_then(Value::as_str)
                .map(str::to_string),
            fallback_to_local: c
                .get("fallbackToLocal")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        None => CheapLlmConfig {
            strategy: "PROVIDER_CHEAPEST".to_string(),
            user_defined_profile_id: None,
            default_cheap_profile_id: None,
            fallback_to_local: false,
        },
    }
}

/// v4's repeated selection block: build the cheap-LLM selection from `allProfiles`
/// (default profile → `getCheapLLMProvider`), `None` when there are no profiles.
pub(crate) fn build_cheap_llm_selection(
    all_profiles: &[Value],
    cheap_settings: Option<&Value>,
) -> Option<CheapLlmSelection> {
    let profiles: Vec<CheapLlmProfile> = all_profiles
        .iter()
        .map(cheap_llm_profile_from_value)
        .collect();
    let default_index = all_profiles
        .iter()
        .position(|v| v.get("isDefault").and_then(Value::as_bool) == Some(true))
        .unwrap_or(0);
    let default_profile = profiles.get(default_index)?;
    let config = cheap_llm_config_from_settings(cheap_settings);
    Some(get_cheap_llm_provider(
        default_profile,
        &config,
        &profiles,
        false,
        None,
    ))
}

/// Node `Buffer.from(s, 'base64')`: decode standard/URL-safe base64, ignoring any
/// non-alphabet character (whitespace) and tolerating missing padding.
pub(crate) fn decode_base64_node(s: &str) -> Vec<u8> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    }
    let symbols: Vec<u8> = s.bytes().filter_map(val).collect();
    let mut out = Vec::with_capacity(symbols.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &sym in &symbols {
        acc = (acc << 6) | sym as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// Load an image profile's `parameters` object by id (v4 reads
/// `reroute.profile.parameters`; the `RouteProfile` doesn't carry them). The ONE
/// home — the image job and the generate-image tool both call it (P4.124: the
/// tool had a second, sync copy of the same body).
///
/// v4 has no direct counterpart: its attempt takes the whole `ImageProfile`,
/// which the understudy read (`repos.imageProfiles.findById`, a FALLBACK
/// `_findById`) already fetched. So a failed read logs that read's repository
/// ERROR (`Error finding entity by ID`, `collection: image_profiles`) and
/// answers `null` parameters — where v5 answered `Null` silently.
///
/// The whole checkout runs under the one fallback home (`db::fallback`): the
/// pool checkout sits inside v4's same `safeQuery` (`getCollection()` runs
/// under it), so a pool failure and a read failure log the same line, once.
pub(crate) fn load_profile_parameters(db: &Db, profile_id: &str) -> Value {
    crate::db::fallback::find_by_id_or_none("image_profiles", profile_id, || {
        db.read_main(|conn| crate::db::image_profiles::find_by_id(conn, profile_id))
    })
    .and_then(|p| p.get("parameters").cloned())
    .unwrap_or(Value::Null)
}

/// The outcome of an image-failover generation: the images + the profile that
/// actually produced them (for the file `generationModel`).
pub(crate) struct GenOutcome {
    pub images: Vec<crate::model::image::GeneratedImageData>,
    #[allow(dead_code)]
    pub active_provider: String,
    pub active_model: String,
}

/// v4 `buildImageGenParams` as the two image JOBS call it: `n: 1`, `style:
/// 'natural'`, the handler's orientation, and the profile's LoRAs + residual
/// options.
///
/// Extracted from the jobs' generation flow for P4.D184: the avatar handler must
/// derive its configuration cache key from the very params object that reaches
/// the wire, and must do it BEFORE the Concierge classification. Building the
/// object twice would key one and send the other — and would fire the
/// `[Image LoRA]` lines twice where v4 fires them once. So the caller builds,
/// keys, and hands the same object back in.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_job_image_params(
    provider: &str,
    model_name: &str,
    parameters: &Value,
    final_prompt: &str,
    orientation: Orientation,
    declarations_for: &ImageDeclarationsFn,
    log_context: &'static str,
    chat_id: Option<&str>,
    job_id: Option<&str>,
    profile_id: &str,
) -> crate::model::image::ImageGenParams {
    build_image_gen_params(
        ImageProfileLike {
            provider,
            model_name: Some(model_name),
            parameters: Some(parameters),
        },
        final_prompt,
        &job_overrides(),
        Some(orientation),
        DEFAULT_IMAGE_MODEL,
        &declarations_for(provider),
        &ImageParamsLogContext {
            context: log_context,
            chat_id: chat_id.map(str::to_string),
            job_id: job_id.map(str::to_string),
            profile_id: Some(profile_id.to_string()),
            rerouted: None,
        },
    )
    .params
}

/// The two image JOBS' generation through the Concierge's failover chokepoint
/// (v4 `8bd080267`: `attemptPortrait` / `attemptBackground` handed to
/// [`generate_image_with_concierge_failover`]). Since v4 `9ecebf986` the
/// attempt closure is the shared `makeLoggedImageAttempt`
/// ([`crate::image_gen::image_attempt::make_logged_image_attempt`]): the params
/// for whichever profile the chokepoint hands over (the PREBUILT ones — the
/// avatar's cache-keyed object — only for `requested_profile_id`; every other
/// profile rebuilds under the ONE `log_context`, its line carrying `profileId`
/// and `rerouted`), and the `IMAGE_GENERATION` `llm_logs` row per attempt, success
/// or failure, whose `(Concierge reroute)` suffix keys on `≠
/// unsuffixed_profile_id` (v4's `primaryProfileId` — the avatar's EFFECTIVE
/// primary after a pre-flight reroute, the story's requested profile). The
/// handlers log their own failure and rerouted lines.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_job_image<I: ImageProvider, A: ApiKeyResolver>(
    db: &Db,
    image_provider: &I,
    api_keys: &A,
    primary: FailoverProfile,
    primary_key: String,
    requested_profile_id: &str,
    prebuilt_params: Option<crate::model::image::ImageGenParams>,
    unsuffixed_profile_id: &str,
    final_prompt: &str,
    orientation: Orientation,
    declarations_for: &ImageDeclarationsFn,
    log_context: &'static str,
    concierge_policy: &ResolvedConciergePolicy,
    user_id: &str,
    chat_id: Option<&str>,
    // v4 `4d370a90f`: the chat, for the chokepoint's Concierge-state snapshot.
    chat: Option<&Value>,
    character_id: Option<&str>,
    job_id: Option<&str>,
    purpose: ImagePurpose,
    primary_via: RouteAttemptVia,
    // v4 `announceUnresolvedRefusal` (`ce2f1dabf`, #77): the Lantern passes
    // `false` (its own refusal bubble reports it); every other job `true`.
    announce_unresolved_refusal: bool,
) -> Result<ImageFailoverOutcome<ImageGenResponse>, ImageFailoverError> {
    let mut attempt = make_logged_image_attempt(
        db,
        image_provider,
        LoggedImageAttemptOptions {
            user_id,
            log_type: crate::services::llm_logging::log_type::IMAGE_GENERATION,
            prompt: final_prompt,
            primary_profile_id: unsuffixed_profile_id,
            chat_id,
            character_id,
            params: ImageAttemptParams {
                overrides: job_overrides(),
                orientation: Some(orientation),
                log_context: ImageParamsLogContext {
                    context: log_context,
                    chat_id: chat_id.map(str::to_string),
                    job_id: job_id.map(str::to_string),
                    profile_id: None,
                    rerouted: None,
                },
                fallback_model: DEFAULT_IMAGE_MODEL,
                declarations_for,
            },
            prebuilt: prebuilt_params.map(|params| PrebuiltImageParams {
                profile_id: requested_profile_id.to_string(),
                params,
            }),
        },
    );
    let understudy = ImageUnderstudySource {
        db,
        api_keys,
        user_id,
        uncensored_image_profile_id: concierge_policy.desk.image_profile_id.as_deref(),
    };
    generate_image_with_concierge_failover(
        (primary, primary_key),
        |profile, key| attempt.call(profile, key),
        &ImageFailoverContext {
            db,
            user_id,
            chat_id,
            chat,
            purpose,
            concierge_policy,
            understudy: &understudy,
            profile_kind: RouteProfileKind::Image,
            primary_via,
            announce_unresolved_refusal,
        },
    )
    .await
}

/// v4's handlers map a failed call's trail to `{ profileName, outcome }` pairs
/// for their ERROR bag (`conciergeTrail`), as JSON for the file layer's
/// `…Json` convention; `None` when there is no trail (v4's `trail?.map` →
/// `undefined`, an absent key).
pub(crate) fn concierge_trail_log_json(error: &ImageFailoverError) -> Option<String> {
    error.concierge_trail().map(|trail| {
        Value::Array(
            trail
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "profileName": a.profile_name,
                        "outcome": a.outcome.as_str(),
                    })
                })
                .collect(),
        )
        .to_string()
    })
}

/// v4's handler error: `trail && trail.length > 1 ? `${prefix} after Concierge
/// reroute: ${msg}` : `${prefix}: ${msg}``.
pub(crate) fn job_failure_message(prefix: &str, error: &ImageFailoverError) -> String {
    match error.concierge_trail() {
        Some(trail) if trail.len() > 1 => {
            format!("{prefix} after Concierge reroute: {}", error.error.message)
        }
        _ => format!("{prefix}: {}", error.error.message),
    }
}

/// The project-store `fileStorageManager.uploadFile` seam (the host FsSeam). The
/// handlers land project-scoped images through this; the corpus keeps the
/// database-backed (vault / Lantern) branches primary with a recorded project
/// case. Async — the real upload is a host call. `folder_path` is `/character-
/// avatars/` (avatar) or `/story-backgrounds/` (story). `Err` is v4
/// `uploadFile`'s throw — the handlers propagate it and the job FAILS before
/// the `files` row / avatar update / Lantern announcement (dogfood finding
/// #16: the old infallible shape buried the error in a sentinel storageKey
/// and let the job "succeed" with an unservable file).
pub trait ProjectImageUpload {
    fn upload(
        &self,
        filename: &str,
        content: &[u8],
        content_type: &str,
        project_id: &str,
        folder_path: &str,
    ) -> impl std::future::Future<Output = Result<ProjectUploadResult, String>> + Send;
}

/// The upload seam's result (v4 `uploadFile`'s `{ storageKey, storedMimeType,
/// sizeBytes }`).
#[derive(Clone, Debug)]
pub struct ProjectUploadResult {
    pub storage_key: String,
    pub stored_mime_type: String,
    pub size_bytes: usize,
}

/// The off-path default upload seam (no project-store host wired). Returns a
/// deterministic placeholder — only reached on the project branch, which the
/// primary corpus does not exercise.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoProjectImageUpload;
impl ProjectImageUpload for NoProjectImageUpload {
    async fn upload(
        &self,
        _filename: &str,
        content: &[u8],
        content_type: &str,
        _project_id: &str,
        _folder_path: &str,
    ) -> Result<ProjectUploadResult, String> {
        Ok(ProjectUploadResult {
            storage_key: "fs-seam:unwired".to_string(),
            stored_mime_type: content_type.to_string(),
            size_bytes: content.len(),
        })
    }
}

/// Read a JSON string field, `None` when absent/null/non-string.
pub(crate) fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// Read a JSON string field into an owned `String` when present & non-empty.
pub(crate) fn owned_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Read a Connection instance for both DBs and run a closure needing both (via a
/// serialized write on the writer thread — the established dual-read pattern).
///
/// P4.166 (dogfood #156): a DEGRADED (or absent) mount index refuses with the
/// neutral [`DbError::PartitionUnavailable`]`(MountIndex)` — whose text through
/// [`crate::db::fallback::error_text`] is v4's guard sentence `Mount index
/// database is in degraded mode` (`mount-index-guard.ts:20`), the error every
/// v4 mount write throws — and the api edge answers it with v4's middleware
/// 500 `Internal server error` ([`crate::api::types::db_error_response`]). It
/// had refused with an `Internal` naming the image job, which every one of the
/// 88 callers then answered as a 500 carrying that sentence. An entity-first
/// route uses [`with_both_conns_for`] instead.
///
/// [`DbError::PartitionUnavailable`]: crate::db::DbError::PartitionUnavailable
pub(crate) async fn with_both_conns<T, F>(db: &Db, f: F) -> Result<T, DbError>
where
    T: Send + 'static,
    F: FnOnce(&Connection, &Connection) -> Result<T, DbError> + Send + 'static,
{
    db.write(move |writers| {
        let mount = writers
            .mount_index()
            .ok_or(DbError::PartitionUnavailable(
                crate::write_partition::WriteDbTarget::MountIndex,
            ))?
            .connection();
        let main = writers.main().connection();
        f(main, mount)
    })
    .await
}

/// [`with_both_conns`] for a caller whose ONLY mount-index use is a FAIL-SOFT
/// read (P4.166, survey class C2 — the equip / rollback routes' wearable pool):
/// `f` gets `None` on a degraded partition and answers as v4's fallback reads
/// would (`crate::wearable_pool::load_wearable_pool_degraded`), instead of the
/// neutral refusal.
pub(crate) async fn with_main_and_mount_if_open<T, F>(db: &Db, f: F) -> Result<T, DbError>
where
    T: Send + 'static,
    F: FnOnce(&Connection, Option<&Connection>) -> Result<T, DbError> + Send + 'static,
{
    db.write(move |writers| {
        let mount = writers.mount_index().map(|w| w.connection());
        f(writers.main().connection(), mount)
    })
    .await
}

/// The store-backed entity an entity-first route loads before anything else
/// (P4.166, dogfood #156) — the id is the PATH id, read from main's slim row.
#[derive(Debug, Clone)]
pub(crate) enum EntityRef {
    Project(String),
    Group(String),
    Character(String),
}

impl EntityRef {
    fn label(&self) -> &'static str {
        match self {
            EntityRef::Project(_) => "project",
            EntityRef::Group(_) => "group",
            EntityRef::Character(_) => "character",
        }
    }

    fn id(&self) -> &str {
        match self {
            EntityRef::Project(id) | EntityRef::Group(id) | EntityRef::Character(id) => id,
        }
    }

    /// The slim row's store pointer: `Some(pointer)` when the row exists
    /// (`pointer` its `officialMountPointId` / `characterDocumentMountPointId`,
    /// `None` when NULL), `None` when it does not.
    fn store_pointer(&self, main: &Connection) -> Result<Option<Option<String>>, DbError> {
        let sql = match self {
            EntityRef::Project(_) => {
                r#"SELECT "officialMountPointId" FROM "projects" WHERE "id" = ?1"#
            }
            EntityRef::Group(_) => r#"SELECT "officialMountPointId" FROM "groups" WHERE "id" = ?1"#,
            EntityRef::Character(_) => {
                r#"SELECT "characterDocumentMountPointId" FROM "characters" WHERE "id" = ?1"#
            }
        };
        match main.query_row(sql, [self.id()], |r| r.get::<_, Option<String>>(0)) {
            // An empty pointer names no store (v4's `linkedMountId` reads it
            // falsy).
            Ok(pointer) => Ok(Some(pointer.filter(|p| !p.is_empty()))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// How many `doc_mount_documents` reads v4's overlay makes for this entity
    /// before it sees the keystone missing — each one `withRawDb`'s quiet
    /// DEBUG on a degraded partition (measured through v4's REAL routes:
    /// `loadStoreFiles` reads the four overlay files; the vault overlay's
    /// `loadVaultFileMaps` the nine single files + the Prompts / Scenarios
    /// folders).
    fn overlay_reads(&self) -> usize {
        match self {
            EntityRef::Project(_) | EntityRef::Group(_) => {
                crate::db::document_store_overlay::ALL_OVERLAY_PATHS.len()
            }
            EntityRef::Character(_) => {
                crate::db::vault_read_overlay::SINGLE_FILE_OVERLAY_PATHS.len() + 2
            }
        }
    }
}

/// The tier factories' entity: a project / group tier resolves its owner
/// through the overlay first (`findById` — v4's 503 on a degraded mount, as
/// measured on the project wardrobe list / inventory create and the group
/// wardrobe create); the character tier resolves its vault from the RAW row
/// and General reads `instance_settings`, so neither has an entity gate (a
/// mount write there is the neutral refusal — v4's 500, measured on the
/// character wardrobe create).
pub(crate) fn tier_entity(project_or_group: Option<bool>, id: Option<&str>) -> Option<EntityRef> {
    let id = id?.to_string();
    match project_or_group? {
        true => Some(EntityRef::Project(id)),
        false => Some(EntityRef::Group(id)),
    }
}

/// [`with_both_conns_for`] when the route has an entity gate, else
/// [`with_both_conns`] (the tier factories, [`tier_entity`]).
pub(crate) async fn with_both_conns_maybe_for<T, F>(
    db: &Db,
    entity: Option<EntityRef>,
    f: F,
) -> Result<T, DbError>
where
    T: Send + 'static,
    F: FnOnce(&Connection, &Connection) -> Result<T, DbError> + Send + 'static,
{
    match entity {
        Some(entity) => with_both_conns_for(db, entity, f).await,
        None => with_both_conns(db, f).await,
    }
}

/// [`with_both_conns`] for an ENTITY-FIRST route (P4.166, dogfood #156 —
/// survey class C1): the route's first act in v4 is `repos.<entity>.findById`,
/// whose overlay reads are FALLBACK reads, so on a degraded mount index the
/// keystone reads as missing and the route answers v4's contextful 503
/// (`Project` / `Group document store unavailable`, `Character vault
/// unavailable` — `lib/api/middleware/context.ts:176-205`) instead of reaching
/// any mount write. With the mount writer present this IS `with_both_conns`.
/// Degraded:
///
/// - a store-backed entity (a project / group row — v4's overlay refuses even
///   a NULL pointer — or a character whose raw row names a vault) →
///   [`DbError::StoreUnavailable`] after v4's per-read DEBUG
///   ([`crate::db::fallback::log_mount_index_unavailable`], the chat PUT
///   gate's shape in `api/salon.rs`);
/// - an ABSENT row, or a VAULT-LESS character (ruling R-B: only a raw row with
///   `characterDocumentMountPointId` raises `CharacterVaultUnavailableError`)
///   → `f` runs over an EMPTY in-memory connection standing in for the
///   degraded partition: every swapped site reads main's row first, so an
///   absent row answers its own not-found and a vault-less character proceeds
///   as v4's does (measured: a vault-less favorite is v4's 200). A statement
///   that does reach the stand-in finds no table — v4's mount guard throw —
///   and answers the neutral refusal, as [`with_both_conns`] does.
pub(crate) async fn with_both_conns_for<T, F>(
    db: &Db,
    entity: EntityRef,
    f: F,
) -> Result<T, DbError>
where
    T: Send + 'static,
    F: FnOnce(&Connection, &Connection) -> Result<T, DbError> + Send + 'static,
{
    db.write(move |writers| {
        let main = writers.main().connection();
        if let Some(mount) = writers.mount_index() {
            return f(main, mount.connection());
        }
        let degraded =
            DbError::PartitionUnavailable(crate::write_partition::WriteDbTarget::MountIndex);
        let pointer = entity.store_pointer(main)?;
        let store_backed = match (&entity, &pointer) {
            (_, None) => false,
            (EntityRef::Character(_), Some(vault)) => vault.is_some(),
            (_, Some(_)) => true,
        };
        if store_backed {
            let mount_point_id = pointer.flatten();
            // v4's overlay reads its files only when the row names a store;
            // a NULL pointer refuses before any read.
            if mount_point_id.is_some() {
                for _ in 0..entity.overlay_reads() {
                    crate::db::fallback::log_mount_index_unavailable(
                        "doc_mount_documents",
                        &degraded,
                    );
                }
            }
            let detail = match (&entity, &mount_point_id) {
                (_, Some(_)) => "properties.json missing".to_string(),
                (EntityRef::Character(_), None) => {
                    "characterDocumentMountPointId is null".to_string()
                }
                (_, None) => "officialMountPointId is null".to_string(),
            };
            return Err(
                crate::db::document_store_overlay::OverlayError::Unavailable {
                    entity_label: entity.label(),
                    id: entity.id().to_string(),
                    mount_point_id,
                    detail,
                }
                .into_db(),
            );
        }
        let stand_in = Connection::open_in_memory()?;
        f(main, &stand_in).map_err(|e| match e {
            DbError::Sqlite(ref s) if s.to_string().contains("no such table") => degraded,
            other => other,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::db::Writer;
    use crate::image_gen::Orientation;
    use crate::model::image::{GeneratedImageData, ImageGenError};
    use crate::services::dangerous_content::provider_routing::NoApiKeys;

    // === The f7f1a956-round §3 finding: `durationMs` must be a MEASURED span ===
    //
    // The differentials cannot see this — `normalize_duration_ms` collapses
    // every non-NULL duration to a placeholder, which is how a hardcoded
    // `Some(0.0)` survived differential-verified for a whole phase. Since v4
    // `0cde7fbc` (the Almanack) `durationMs` feeds real latency figures
    // (`durationMs > 0` filters, averages, medians), so these pins assert the
    // two properties no oracle diff can: the row's duration is PRESENT, and the
    // span BRACKETS the provider call (a provider that takes a real ~30 ms must
    // produce a duration in that ballpark, not 0).
    //
    // P4.D225: the bug-133 reroute tests that lived here pinned a gate and
    // three log lines v4 `8bd080267` RETIRED with the per-handler reroute; the
    // chokepoint's own lines are pinned by `image_failover_tier3_equivalence`,
    // the handlers' by their tier-3 families.

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A provider that takes a real ~30 ms and then answers, or fails — with a
    /// NON-refusal error, or (for the second call) a typed refusal first.
    struct SlowImageProvider {
        fail: bool,
        refuse_first: bool,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl SlowImageProvider {
        fn new(fail: bool, refuse_first: bool) -> Self {
            Self {
                fail,
                refuse_first,
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    impl ImageProvider for SlowImageProvider {
        fn generate_image(
            &self,
            _provider: &str,
            _api_key: &str,
            _params: &crate::model::image::ImageGenParams,
        ) -> impl std::future::Future<Output = Result<ImageGenResponse, ImageGenError>> + Send
        {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let fail = self.fail;
            let refuse = self.refuse_first && n == 0;
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                if refuse {
                    return Err(ImageGenError::moderation("blocked", Some(400), None));
                }
                if fail {
                    return Err(ImageGenError::new("provider exploded"));
                }
                Ok(ImageGenResponse {
                    images: vec![GeneratedImageData {
                        data: Some("aGk=".to_string()),
                        url: None,
                        mime_type: Some("image/png".to_string()),
                        revised_prompt: None,
                    }],
                })
            }
        }
    }

    fn open_db(dir: &std::path::Path) -> Db {
        let main_path = dir.join("main.db");
        let ll_path = dir.join("llm-logs.db");
        {
            let w = Writer::open_writable(&main_path, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE image_profiles (\
                       id TEXT PRIMARY KEY, userId TEXT, name TEXT, provider TEXT, \
                       apiKeyId TEXT, baseUrl TEXT, modelName TEXT, parameters TEXT, \
                       isDefault INTEGER, isDangerousCompatible INTEGER, tags TEXT, \
                       createdAt TEXT, updatedAt TEXT);\
                     INSERT INTO image_profiles VALUES ('understudy-1', 'user-1', \
                       'Night Desk', 'GROK', 'key-1', NULL, 'grok-2-image', '{}', 0, 1, \
                       '[]', '2026-01-01', '2026-01-01');",
                )
                .unwrap();
        }
        {
            let w = Writer::open_writable(&ll_path, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE llm_logs (\
                       id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
                       chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
                       modelName TEXT, connectionProfileId TEXT, imageProfileId TEXT, \
                       request TEXT, response TEXT, usage TEXT, \
                       cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
                       durationMs REAL, createdAt TEXT, updatedAt TEXT);",
                )
                .unwrap();
        }
        Db::open(
            DbPaths {
                main: main_path,
                mount_index: None,
                llm_logs: Some(ll_path),
            },
            PEPPER,
        )
        .unwrap()
    }

    async fn logged_rows(db: &Db) -> Vec<(Option<f64>, String)> {
        db.read_llm_logs(|conn| {
            let mut stmt =
                conn.prepare("SELECT durationMs, response FROM llm_logs ORDER BY rowid")?;
            let out = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, Option<f64>>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(out)
        })
        .unwrap()
    }

    struct AlwaysApiKey;
    impl ApiKeyResolver for AlwaysApiKey {
        fn resolve(&self, _api_key_id: &str, _user_id: &str) -> Option<String> {
            Some("sk-understudy".to_string())
        }
    }

    async fn run_gen<A: ApiKeyResolver>(
        db: &Db,
        provider: &SlowImageProvider,
        api_keys: &A,
        mode: &str,
    ) -> Result<ImageFailoverOutcome<ImageGenResponse>, ImageFailoverError> {
        let declarations: Box<ImageDeclarationsFn> =
            Box::new(|_p: &str| ImageDeclarations::default());
        // The retired mode, mapped as v4's migration maps it: `OFF` goes off
        // duty (no failover), anything else is on duty (failover allowed).
        let settings =
            crate::services::dangerous_content::resolver::resolve_stored_concierge_settings(
                Some(&serde_json::json!({ "enabled": mode != "OFF" })),
                None,
            );
        generate_job_image(
            db,
            provider,
            api_keys,
            FailoverProfile {
                id: "profile-1".into(),
                name: "Day Desk".into(),
                provider: "OPENAI".into(),
                model_name: "gpt-image-1".into(),
                row: Value::Null,
            },
            "sk-test".into(),
            "profile-1",
            None,
            "profile-1",
            "a prompt",
            Orientation::Square,
            &declarations,
            "test.image-job",
            &settings,
            "user-1",
            None,
            None,
            None,
            None,
            ImagePurpose::Lantern,
            RouteAttemptVia::Primary,
            true,
        )
        .await
    }

    #[tokio::test]
    async fn success_row_duration_brackets_the_provider_call() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let out = run_gen(
            &db,
            &SlowImageProvider::new(false, false),
            &NoApiKeys,
            "OFF",
        )
        .await;
        assert!(out.is_ok());
        let rows = logged_rows(&db).await;
        assert_eq!(rows.len(), 1, "exactly one llm_logs row written");
        let d = rows[0].0.expect("durationMs must be present");
        assert!(
            d >= 20.0,
            "durationMs must bracket the ~30 ms provider call, got {d}"
        );
    }

    #[tokio::test]
    async fn failure_row_duration_brackets_the_provider_call() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let out = run_gen(&db, &SlowImageProvider::new(true, false), &NoApiKeys, "OFF").await;
        let err = out.expect_err("a non-refusal failure comes back untouched");
        assert!(
            err.concierge_trail().is_none(),
            "no trail on the untouched arm"
        );
        let rows = logged_rows(&db).await;
        assert_eq!(rows.len(), 1, "exactly one llm_logs row written");
        let d = rows[0].0.expect("durationMs must be present");
        assert!(
            d >= 20.0,
            "durationMs must bracket the ~30 ms provider call, got {d}"
        );
    }

    /// The rerouted run logs one row PER ATTEMPT, each with its own span, and
    /// only the understudy's success content carries v4's `(Concierge
    /// reroute)` suffix (the refused primary's row is an error row).
    #[tokio::test]
    async fn a_rerouted_run_logs_both_attempts_and_suffixes_the_understudy() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let out = run_gen(
            &db,
            &SlowImageProvider::new(false, true),
            &AlwaysApiKey,
            "AUTO_ROUTE",
        )
        .await
        .expect("the understudy answers");
        assert!(out.rerouted);
        assert_eq!(out.profile.id, "understudy-1");
        assert_eq!(out.api_key, "sk-understudy");
        let rows = logged_rows(&db).await;
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(rows.iter().all(|(d, _)| d.is_some_and(|d| d >= 20.0)));
        assert!(rows[0].1.contains("\"error\":\"blocked\""), "{}", rows[0].1);
        assert!(
            rows[1]
                .1
                .contains("Generated 1 image(s) (Concierge reroute)"),
            "{}",
            rows[1].1
        );
    }

    /// v4's handler error: the "after Concierge reroute" prefix only when the
    /// trail has more than the refused primary on it; the ERROR bag's
    /// `conciergeTrail` pairs.
    #[tokio::test]
    async fn the_handler_error_names_a_reroute_only_when_one_was_tried() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let err = run_gen(
            &db,
            &SlowImageProvider::new(false, true),
            &NoApiKeys,
            "DETECT_ONLY",
        )
        .await
        .expect_err("not permitted");
        assert_eq!(
            job_failure_message("Image generation failed", &err),
            "Image generation failed: blocked"
        );
        assert_eq!(
            concierge_trail_log_json(&err).as_deref(),
            Some(r#"[{"profileName":"Day Desk","outcome":"refused"}]"#)
        );

        let err = run_gen(
            &db,
            &SlowImageProvider::new(true, true),
            &AlwaysApiKey,
            "AUTO_ROUTE",
        )
        .await
        .expect_err("the understudy fails too");
        assert_eq!(
            job_failure_message("Image generation failed", &err),
            "Image generation failed after Concierge reroute: provider exploded"
        );
        assert_eq!(
            concierge_trail_log_json(&err).as_deref(),
            Some(
                r#"[{"profileName":"Day Desk","outcome":"refused"},{"profileName":"Night Desk","outcome":"failed"}]"#
            )
        );

        let err = run_gen(
            &db,
            &SlowImageProvider::new(true, false),
            &NoApiKeys,
            "AUTO_ROUTE",
        )
        .await
        .expect_err("untouched");
        assert_eq!(concierge_trail_log_json(&err), None, "no trail → no key");
    }

    /// P4.124 (P4.D225 NIT (a)): a failed read logs v4's repository ERROR and
    /// answers `null` parameters; a found row is silent.
    #[test]
    fn load_profile_parameters_logs_a_failed_read() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let (params, lines) =
            crate::test_support::captured_with(|| load_profile_parameters(&db, "understudy-1"));
        assert_eq!(params, serde_json::json!({}));
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Error finding entity by ID")),
            "{lines:?}"
        );

        db.write_blocking(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE image_profiles;")?;
            Ok(())
        })
        .unwrap();
        let (params, lines) =
            crate::test_support::captured_with(|| load_profile_parameters(&db, "understudy-1"));
        assert_eq!(params, Value::Null);
        let hit: Vec<_> = lines
            .iter()
            .filter(|l| l.contains("Error finding entity by ID"))
            .collect();
        assert_eq!(hit.len(), 1, "{lines:?}");
        assert!(
            hit[0].starts_with("ERROR quilltap::db ")
                && hit[0].contains("collection=image_profiles")
                && hit[0].contains("id=understudy-1")
                && hit[0].contains("error="),
            "{}",
            hit[0]
        );
    }
}

/// P4.166 (dogfood #156) — the degraded-mount gates. The route-level answers
/// are differential (`degraded_mount_routes_equivalence`); these pin the
/// gates' own arms over a main partition with no mount-index writer.
#[cfg(test)]
mod degraded_gate_tests {
    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::db::Writer;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A main partition holding one row per arm, and NO mount index.
    fn degraded_db(dir: &std::path::Path) -> Db {
        let main = dir.join("main.db");
        {
            let w = Writer::open_writable(&main, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    r#"CREATE TABLE "projects" ("id" TEXT PRIMARY KEY, "officialMountPointId" TEXT);
                       CREATE TABLE "groups" ("id" TEXT PRIMARY KEY, "officialMountPointId" TEXT);
                       CREATE TABLE "characters" ("id" TEXT PRIMARY KEY, "characterDocumentMountPointId" TEXT);
                       INSERT INTO "projects" VALUES ('p1', 'mp-p1'), ('p-null', NULL);
                       INSERT INTO "groups" VALUES ('g1', 'mp-g1');
                       INSERT INTO "characters" VALUES ('c-vault', 'mp-c'), ('c-bare', NULL), ('c-empty', '');"#,
                )
                .unwrap();
        }
        Db::open(
            DbPaths {
                main,
                mount_index: None,
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap()
    }

    fn store_refusal(e: DbError) -> (&'static str, String, Option<String>, String) {
        match e {
            DbError::StoreUnavailable {
                entity_label,
                id,
                mount_point_id,
                message,
            } => (entity_label, id, mount_point_id, message),
            other => panic!("expected StoreUnavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn the_neutral_refusal_reads_v4s_guard_sentence() {
        let dir = tempfile::tempdir().unwrap();
        let db = degraded_db(dir.path());
        let e = with_both_conns(&db, |_, _| Ok(())).await.unwrap_err();
        assert!(matches!(
            e,
            DbError::PartitionUnavailable(crate::write_partition::WriteDbTarget::MountIndex)
        ));
        assert_eq!(
            crate::db::fallback::error_text(&e),
            "Mount index database is in degraded mode"
        );
    }

    #[tokio::test]
    async fn a_store_backed_entity_refuses_as_v4s_overlay() {
        let dir = tempfile::tempdir().unwrap();
        let db = degraded_db(dir.path());
        let ran = |_: &Connection, _: &Connection| -> Result<(), DbError> {
            panic!("the closure must not run for a store-backed entity")
        };
        let (label, id, mp, message) = store_refusal(
            with_both_conns_for(&db, EntityRef::Project("p1".into()), ran)
                .await
                .unwrap_err(),
        );
        assert_eq!(
            (label, id.as_str(), mp.as_deref()),
            ("project", "p1", Some("mp-p1"))
        );
        assert_eq!(
            message,
            "Project p1 has no usable document store (officialMountPointId=mp-p1): properties.json missing"
        );
        // A NULL pointer refuses too (v4's overlay), with v4's detail.
        let (_, _, mp, message) = store_refusal(
            with_both_conns_for(&db, EntityRef::Project("p-null".into()), ran)
                .await
                .unwrap_err(),
        );
        assert_eq!(mp, None);
        assert!(
            message.ends_with("officialMountPointId is null"),
            "{message}"
        );
        let (label, ..) = store_refusal(
            with_both_conns_for(&db, EntityRef::Group("g1".into()), ran)
                .await
                .unwrap_err(),
        );
        assert_eq!(label, "group");
        let (label, id, mp, message) = store_refusal(
            with_both_conns_for(&db, EntityRef::Character("c-vault".into()), ran)
                .await
                .unwrap_err(),
        );
        assert_eq!(
            (label, id.as_str(), mp.as_deref()),
            ("character", "c-vault", Some("mp-c"))
        );
        assert_eq!(
            message,
            "Character c-vault has no usable vault (characterDocumentMountPointId=mp-c): properties.json missing"
        );
    }

    #[tokio::test]
    async fn an_absent_or_vault_less_entity_runs_over_the_stand_in() {
        let dir = tempfile::tempdir().unwrap();
        let db = degraded_db(dir.path());
        for entity in [
            EntityRef::Project("missing".into()),
            EntityRef::Character("missing".into()),
            EntityRef::Character("c-bare".into()),
            // An empty pointer names no vault (v4 reads it falsy).
            EntityRef::Character("c-empty".into()),
        ] {
            let label = format!("{entity:?}");
            let main_rows = with_both_conns_for(&db, entity, |main, _| {
                Ok(
                    main.query_row(r#"SELECT COUNT(*) FROM "characters""#, [], |r| {
                        r.get::<_, i64>(0)
                    })?,
                )
            })
            .await
            .unwrap_or_else(|e| panic!("{label}: {e:?}"));
            assert_eq!(main_rows, 3, "{label}: main is the real partition");
        }
        // A statement that reaches the stand-in is v4's guard throw: the
        // neutral refusal.
        let e = with_both_conns_for(&db, EntityRef::Character("c-bare".into()), |_, mount| {
            Ok(
                mount.query_row(r#"SELECT COUNT(*) FROM "doc_mount_documents""#, [], |r| {
                    r.get::<_, i64>(0)
                })?,
            )
        })
        .await
        .unwrap_err();
        assert_eq!(
            crate::db::fallback::error_text(&e),
            "Mount index database is in degraded mode"
        );
    }

    #[tokio::test]
    async fn a_fail_soft_reader_sees_no_mount() {
        let dir = tempfile::tempdir().unwrap();
        let db = degraded_db(dir.path());
        let saw = with_main_and_mount_if_open(&db, |_, mount| Ok(mount.is_none()))
            .await
            .unwrap();
        assert!(saw);
    }
}
