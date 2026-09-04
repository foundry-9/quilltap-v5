//! The activity-span wiring census (P4.D123, v4 `664cfca84`).
//!
//! v4 instruments ten call sites so the toolbar chips cover work that is not a
//! job row. **No differential can see any of them**: an in-flight counter never
//! touches DB state, so the `system_jobs_collection` diff runs with the registry
//! quiet on both sides (`differential-blind-to-a-log-only-fix.md`). Deleting a
//! wrap would leave every test in the workspace green.
//!
//! So the wiring is held here, mechanically: each row names the file, the
//! function the wrap must sit in, and the exact kind. A deleted or re-kinded
//! wrap fails this test by name. **The census IS the record** — including the
//! rows v4 instruments that v5 has no surface for yet, so the next lane to port
//! one inherits the obligation instead of rediscovering it.
//!
//! Three of the sites are additionally driven BEHAVIOURALLY, where the entry
//! point has a cheap injectable seam to sample the live count from — the job
//! runner, the cheap-LLM executor, and the embedding provider, which between
//! them exercise all three shapes (`run_attributed_to_job` at the job seam,
//! `track_activity` with a computed kind, `track_activity` inside a trait impl).
//! Those live beside their subjects; see `services::job_runner::tests`,
//! `services::cheap_llm_exec::tests`, and
//! `services::embedding_provider::tests`. The remaining sites are census-only:
//! driving them means standing up an image runner, an avatar renderer, or a
//! seeded vault, and the mechanism they share is already pinned by the
//! registry's own eighteen unit tests.
//!
//! Run standalone (no oracle):
//!   cargo test -p quilltap-harness --test activity_span_sites_guard

use std::path::PathBuf;

/// `(repo-relative file, the fn the wrap must sit in, the wrap text, why)`.
///
/// The `#` column is v4's site number from the work order's table.
const CENSUS: &[(&str, &str, &str, &str)] = &[
    // 1 — v4 `child-entry.ts` (the forked child's handler invocation).
    (
        "crates/quilltap-core/src/services/job_runner.rs",
        "async fn run_one",
        "run_attributed_to_job(\n                    activity_kind_for_job_type(&job.job_type),",
        "the job row IS the count, so the handler is attributed without adding one",
    ),
    // 3 — v4 `cheap-llm-tasks/core-execution.ts`.
    (
        "crates/quilltap-core/src/services/cheap_llm_exec.rs",
        "pub async fn execute<C: CompletionProvider, T>",
        "track_activity(\n            activity_kind_for_task(task_type),",
        "every cheap-LLM task funnels through here; the kind comes from TASK_TYPE_ACTIVITY",
    ),
    // 4 — v4 `memory-gate.ts`.
    (
        "crates/quilltap-core/src/services/memory_gate.rs",
        "async fn run_memory_gate<P: EmbeddingProvider>",
        "track_activity(\n        ActivityKind::Memory,",
        "a memory formed from a tool call has no job row to count it",
    ),
    // 5 — v4 `dangerous-content/gatekeeper.service.ts`.
    (
        "crates/quilltap-core/src/services/dangerous_content/gatekeeper.rs",
        "pub async fn classify_content<M, C>",
        "track_activity(\n        ActivityKind::Danger,",
        "every Concierge classification, per-message and chat-level",
    ),
    // 6 — v4 `embedding/embedding-service.ts`.
    (
        "crates/quilltap-core/src/services/embedding_provider.rs",
        "impl<T: WireTransport> EmbeddingProvider for ApiEmbeddingProvider<T>",
        "crate::services::activity_kinds::ActivityKind::Embedding,",
        "the whole call including the wait for interactive quiet; the REAL provider only",
    ),
    // 7 — v4 `tools/handlers/image-generation-handler.ts`.
    (
        "crates/quilltap-core/src/tools/generate_image.rs",
        "pub async fn execute_image_generation_tool<I, C, M, A, T, L>",
        "track_activity(\n        ActivityKind::Image,",
        "lit from the first token of prompt crafting until the image lands or fails",
    ),
    // 8 — v4 `chat/file-attachment-fallback.ts`.
    (
        "crates/quilltap-core/src/services/file_fallback.rs",
        "pub async fn generate_image_description<CMP: CompletionProvider>",
        "track_activity(\n        ActivityKind::Image,",
        "reading an image with a vision model is image work",
    ),
    // 10 — v4 `app/api/v1/wardrobe/preview-avatar/route.ts`.
    (
        "crates/quilltap-core/src/api/wardrobe.rs",
        "pub async fn wardrobe_preview_avatar",
        "track_activity(\n        ActivityKind::Image,",
        "previews generate synchronously rather than through the job queue",
    ),
];

/// v4 sites with **no v5 surface to wrap**. Each row is an obligation the lane
/// that ports that surface inherits. They are asserted ABSENT: if one of these
/// names starts existing in v5, this test fails and the row must move into
/// `CENSUS` with its wrap.
///
/// The `#` column is again the order's site number.
const NO_V5_SURFACE: &[(&str, &str, &str)] = &[
    // 2 — v4 `host/processor-host.ts` (the child mirror + reset).
    (
        "applyChildActivityDelta / resetChildActivity",
        "apply_child_activity_delta",
        "NO-PORT by design: v5's job runner is in-process, so there is no child \
         to mirror and no crash mirror to zero (job_runner.rs's header).",
    ),
    // 9 — v4 `app/api/v1/images/route.ts` POST ?action=generate.
    (
        "POST /api/v1/images?action=generate",
        "handle_generate_image",
        "v5 serves the images COLLECTION route since P4.73 (list / upload / \
         import-from-URL / the {id} DELETE) but NOT its ?action=generate leg — \
         the edge answers a NAMED refusal (`images_routes::images_generate_not_available`); \
         v5's Generate Image surface goes through image-profiles ?action=generate → \
         execute_image_generation_tool, which site 7 already wraps. v4's \
         handleGenerateImage is a separate route-level implementation, not a \
         caller of that tool.",
    ),
    // — v4 `services/character-wizard.service.ts`.
    (
        "the character wizard's image description",
        "character_wizard",
        "v5 has no character-wizard twin at all (verified across crates/ and \
         apps/web; character_enrichment and creation_progress are not it). The \
         span lands when that surface ports.",
    ),
    // — v4 `wardrobe/image-analysis.ts`.
    (
        "the wardrobe image analyzer",
        "wardrobe_analyze_image_impl",
        "v5's api::wardrobe::wardrobe_analyze_image is a REFUSAL ARM (the tier-3 \
         deferral noted in that module's header), so there is no work to count. \
         The span lands with the surface.",
    ),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("repo root")
        .to_path_buf()
}

#[test]
fn every_activity_span_site_is_wired() {
    let root = repo_root();
    let mut failures: Vec<String> = Vec::new();

    for (file, function, wrap, why) in CENSUS {
        let path = root.join(file);
        let Ok(src) = std::fs::read_to_string(&path) else {
            failures.push(format!("{file}: unreadable (moved? renamed?)"));
            continue;
        };
        let Some(fn_at) = src.find(function) else {
            failures.push(format!(
                "{file}: `{function}` is gone — the span site moved; update the census ({why})"
            ));
            continue;
        };
        let Some(wrap_at) = src.find(wrap) else {
            failures.push(format!(
                "{file}: `{function}` no longer wraps its work — expected `{}` ({why})",
                wrap.replace('\n', " ")
            ));
            continue;
        };
        // The wrap must sit INSIDE the named function, not merely in the file.
        // A wrap that drifted to another function would otherwise pass.
        if wrap_at < fn_at {
            failures.push(format!(
                "{file}: the wrap sits BEFORE `{function}` — it is not this site's ({why})"
            ));
        }
        // …and EXACTLY ONCE per file (the §3 unification review's hardening):
        // presence alone would let a census function LOSE its wrap while an
        // identical wrap elsewhere in the file satisfied `find`, and it would
        // never see an added double-wrap (which double-counts a chip up to the
        // registry's same-kind collapse). The publish-sites guard counts; so
        // does this one.
        let count = src.match_indices(wrap).count();
        if count != 1 {
            failures.push(format!(
                "{file}: expected exactly one `{}`, found {count} ({why})",
                wrap.replace('\n', " ")
            ));
        }
    }

    for (v4_site, v5_symbol, why) in NO_V5_SURFACE {
        for entry in walk(&root.join("crates")) {
            let Ok(src) = std::fs::read_to_string(&entry) else {
                continue;
            };
            // This guard names every symbol in its own census.
            if entry.ends_with("activity_span_sites_guard.rs") {
                continue;
            }
            if src.contains(v5_symbol) {
                failures.push(format!(
                    "{}: `{v5_symbol}` now exists — v4's `{v4_site}` has a v5 surface at last, so \
                     it needs its activity span and a CENSUS row. ({why})",
                    entry.display()
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} activity-span wiring failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}
