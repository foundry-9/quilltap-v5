//! The Memories API's consolidation actions — v4
//! `app/api/v1/memories/route.ts` (`d58548051`, byte-identical at the
//! `01a83539d` pin): GET/POST `?action=consolidation-config`, GET/POST
//! `?action=extraction-mode-config`, POST `?action=consolidate`. They fill the
//! five C2 wire arms P4.D265 landed with refusal bodies.
//!
//! **The settings POSTs port v4's clobber FAITHFULLY (P4.D271 R-A, the named
//! both-ways table `PARTIAL_DEFAULTS_CLOBBER`).** v4 parses with
//! `<Schema>.partial()` and "drops undefined keys so a partial write never
//! clobbers stored values" — but under Zod 4, `.partial()` over `.default()`
//! fields still fills the default for an ABSENT key, so nothing is ever
//! `undefined`: every POST rewrites EVERY field, and a tuned stored value is
//! lost to its default. v5 reproduces it — the patch is the FULL default-filled
//! parse (measured equal to v4's `.partial()` parse on every corpus input),
//! merged `{...current, ...patch}` (= the patch), validated, written, re-read;
//! the INFO's `fields` always lists every key in schema order. A v4 fix turns
//! the table's second half red — the convergence signal.
//!
//! **Validation (R-K):** a Zod failure answers v5's house class for the
//! memories config surface — 400 `Validation error`, message only.
//!
//! **The dry run (R-E):** in-process, through core's process-global
//! [`consolidation_runner`] the host installs. An engine with no runner
//! installed answers a loud typed refusal AFTER v4's validation and 404 arms.

use serde_json::{json, Value};

use crate::api::types::{ErrorKind, Response};
use crate::api::zod_issues::zod_uuid_ok;
use crate::db::characters_read;
use crate::db::instance_settings::{
    get_memory_consolidation_settings, get_memory_extraction_mode_settings,
    parse_memory_consolidation_settings, parse_memory_extraction_mode_settings,
    set_memory_consolidation_settings, set_memory_extraction_mode_settings,
};
use crate::db::runtime::Db;
use crate::services::consolidation::{
    consolidation_runner, ConsolidationTrigger, RunConsolidationOptions,
};
use crate::services::queue_service::{enqueue_memory_consolidation, MemoryConsolidationPayload};

/// Dry-run clusters are bounded: each cluster is one model call, in-process.
const CONSOLIDATE_DRY_RUN_DEFAULT_CLUSTERS: f64 = 10.0;
const CONSOLIDATE_DRY_RUN_MAX_CLUSTERS: f64 = 40.0;
const CONSOLIDATE_DRY_RUN_TIME_BUDGET_MS: i64 = 120_000;

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

fn validation_error() -> Response {
    Response::error(ErrorKind::BadRequest, "Validation error")
}

fn fields_json(settings_json: &Value) -> String {
    let keys: Vec<&String> = settings_json
        .as_object()
        .map(|o| o.keys().collect())
        .unwrap_or_default();
    serde_json::to_string(&keys).unwrap_or_default()
}

/// v4 GET `?action=consolidation-config` — `{success, settings}`.
pub fn memory_consolidation_config_get(db: &Db) -> Response {
    match db.read_main(|c| Ok(get_memory_consolidation_settings(c))) {
        Ok(settings) => {
            Response::Memory(json!({ "success": true, "settings": settings.to_json() }))
        }
        Err(e) => internal(e),
    }
}

/// v4 POST `?action=consolidation-config` (R-A: the faithful clobber).
pub async fn memory_consolidation_config_set(db: &Db, config: Value) -> Response {
    // `.partial()` over `.default()` fields = the full default-filled parse.
    let parsed = match parse_memory_consolidation_settings(&config) {
        Ok(p) => p,
        Err(_) => return validation_error(),
    };
    let patch = parsed.to_json();
    let fields = fields_json(&patch);
    let wrote = db
        .write(move |w| set_memory_consolidation_settings(w.main().connection(), &patch))
        .await;
    if let Err(e) = wrote {
        return internal(e);
    }
    let settings = match db.read_main(|c| Ok(get_memory_consolidation_settings(c))) {
        Ok(s) => s,
        Err(e) => return internal(e),
    };
    tracing::info!(
        enabled = settings.enabled,
        fieldsJson = %fields,
        "[Memories API] Consolidation settings updated (instance-wide)"
    );
    Response::Memory(json!({ "success": true, "settings": settings.to_json() }))
}

/// v4 GET `?action=extraction-mode-config` — `{success, settings}`.
pub fn memory_extraction_mode_config_get(db: &Db) -> Response {
    match db.read_main(|c| Ok(get_memory_extraction_mode_settings(c))) {
        Ok(settings) => {
            Response::Memory(json!({ "success": true, "settings": settings.to_json() }))
        }
        Err(e) => internal(e),
    }
}

/// v4 POST `?action=extraction-mode-config` (R-A: the faithful clobber).
pub async fn memory_extraction_mode_config_set(db: &Db, config: Value) -> Response {
    let parsed = match parse_memory_extraction_mode_settings(&config) {
        Ok(p) => p,
        Err(_) => return validation_error(),
    };
    let patch = parsed.to_json();
    let fields = fields_json(&patch);
    let wrote = db
        .write(move |w| set_memory_extraction_mode_settings(w.main().connection(), &patch))
        .await;
    if let Err(e) = wrote {
        return internal(e);
    }
    let settings = match db.read_main(|c| Ok(get_memory_extraction_mode_settings(c))) {
        Ok(s) => s,
        Err(e) => return internal(e),
    };
    tracing::info!(
        otherPass = %settings.other_pass,
        fieldsJson = %fields,
        "[Memories API] Extraction-mode settings updated (instance-wide)"
    );
    Response::Memory(json!({ "success": true, "settings": settings.to_json() }))
}

/// v4 `consolidateSchema`, parsed.
struct ConsolidateOptions {
    character_id: String,
    dry_run: Option<bool>,
    max_clusters_per_run: Option<f64>,
    cluster_threshold: Option<f64>,
}

/// `z.object({ characterId: z.uuid(…), dryRun: z.boolean().optional(),
/// maxClustersPerRun: z.number().int().min(1).max(1000).optional(),
/// clusterThreshold: z.number().min(0).max(1).optional() })` — any issue is
/// the 400 (R-K).
fn parse_consolidate_options(v: &Value) -> Option<ConsolidateOptions> {
    let o = v.as_object()?;
    let character_id = o
        .get("characterId")
        .and_then(Value::as_str)
        .filter(|s| zod_uuid_ok(s))?;
    let dry_run = match o.get("dryRun") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return None,
    };
    let max_clusters_per_run = match o.get("maxClustersPerRun") {
        None => None,
        Some(Value::Number(n)) => {
            let x = n.as_f64()?;
            let safe_int = x.fract() == 0.0 && x.abs() <= crate::api::zod_issues::MAX_SAFE_INTEGER;
            if !safe_int || !(1.0..=1000.0).contains(&x) {
                return None;
            }
            Some(x)
        }
        Some(_) => return None,
    };
    let cluster_threshold = match o.get("clusterThreshold") {
        None => None,
        Some(Value::Number(n)) => {
            let x = n.as_f64()?;
            if !(0.0..=1.0).contains(&x) {
                return None;
            }
            Some(x)
        }
        Some(_) => return None,
    };
    Some(ConsolidateOptions {
        character_id: character_id.to_string(),
        dry_run,
        max_clusters_per_run,
        cluster_threshold,
    })
}

/// v4 POST `?action=consolidate`: validate; the overlay character read (404
/// `Character not found`); a dry run in-process (≤ 40 clusters, a 120 s budget,
/// a `clusterThreshold` override) → `{success, dryRun: true, report}`; else
/// enqueue a manual job (`clusterThreshold` IGNORED) → `{success, dryRun:
/// false, jobId}`.
pub async fn memory_consolidate(db: &Db, user_id: &str, options: Value) -> Response {
    let Some(opts) = parse_consolidate_options(&options) else {
        return validation_error();
    };
    let cid = opts.character_id.clone();
    let character = db.read_main(|main| {
        db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid))
    });
    match character {
        Ok(Some(_)) => {}
        Ok(None) => return Response::error(ErrorKind::NotFound, "Character not found"),
        Err(e) => return internal(e),
    }

    if opts.dry_run == Some(true) {
        let max_clusters = opts
            .max_clusters_per_run
            .unwrap_or(CONSOLIDATE_DRY_RUN_DEFAULT_CLUSTERS)
            .min(CONSOLIDATE_DRY_RUN_MAX_CLUSTERS);
        match opts.cluster_threshold {
            Some(t) => tracing::debug!(
                characterId = %opts.character_id,
                maxClusters = max_clusters,
                clusterThreshold = t,
                "[Memories API] Consolidation dry run (in-process)"
            ),
            None => tracing::debug!(
                characterId = %opts.character_id,
                maxClusters = max_clusters,
                "[Memories API] Consolidation dry run (in-process)"
            ),
        }
        let Some(runner) = consolidation_runner() else {
            return Response::error(
                ErrorKind::Internal,
                "The consolidation dry run needs the host's consolidation runner, which is not installed in this engine.",
            );
        };
        let run = runner
            .run(
                db,
                &opts.character_id,
                RunConsolidationOptions {
                    dry_run: true,
                    max_clusters_per_run: Some(max_clusters),
                    settings_override: opts
                        .cluster_threshold
                        .map(|t| json!({ "clusterThreshold": t })),
                    user_id: Some(user_id.to_string()),
                    time_budget_ms: Some(CONSOLIDATE_DRY_RUN_TIME_BUDGET_MS),
                    trigger: ConsolidationTrigger::Manual,
                    ..Default::default()
                },
            )
            .await;
        return match run {
            Ok(report) => Response::Memory(
                json!({ "success": true, "dryRun": true, "report": report.to_json() }),
            ),
            Err(e) => {
                tracing::error!(
                    characterId = %opts.character_id,
                    error = %crate::db::fallback::error_text(&e),
                    "[Memories API] Consolidation dry run failed"
                );
                Response::error(ErrorKind::Internal, "Consolidation dry run failed")
            }
        };
    }

    let payload = MemoryConsolidationPayload {
        character_id: opts.character_id.clone(),
        dry_run: None,
        max_clusters_per_run: opts.max_clusters_per_run,
        trigger: Some("manual".to_string()),
    };
    match enqueue_memory_consolidation(db, user_id, &payload).await {
        Ok(job_id) => {
            tracing::info!(
                characterId = %opts.character_id,
                jobId = %job_id,
                "[Memories API] Consolidation job enqueued"
            );
            Response::Memory(json!({ "success": true, "dryRun": false, "jobId": job_id }))
        }
        Err(e) => {
            tracing::error!(
                characterId = %opts.character_id,
                error = %crate::db::fallback::error_text(&e),
                "[Memories API] Failed to enqueue consolidation"
            );
            Response::error(ErrorKind::Internal, "Failed to enqueue consolidation")
        }
    }
}
