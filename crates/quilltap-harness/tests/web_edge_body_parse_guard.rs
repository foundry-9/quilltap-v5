//! The web-edge wrong-type-collapse census (P4.60).
//!
//! An HTTP edge that reads a request-body key with `.and_then(Value::as_str)`
//! (or `as_bool` / `as_array` / `as_object`) collapses THREE different inputs
//! into one `None`: the key is **absent**, the key is an explicit **null**, and
//! the key carries a present-but-**wrong-typed** value. The third is the
//! dangerous one — v4's Zod schemas refuse it with a 400, while a collapsing
//! edge silently reads it as "the caller didn't say" and carries on.
//!
//! P4.57's tier-2 survey enumerated the sites; P4.60 adjudicated its own set
//! against v4's real routes and named three pockets it did not reach; P4.62
//! adjudicated those (`system_data_routes.rs`'s 13, `files_routes.rs`'s 7, and
//! `llm_logs_routes.rs`'s 1), so every count below is now a measured verdict and
//! not one is deferred. This test is what keeps the verdicts honest:
//!
//! - [`PARSER_WIRING`] pins that each ADJUDICATED-DIVERGENT route still routes
//!   its body through the ported Zod parser. A differential over the parser
//!   alone cannot see the route quietly going back to reading keys itself.
//! - [`COLLAPSE_CENSUS`] holds the whole `crates/quilltap-web/src/*_routes.rs`
//!   surface to an exact per-file count of the collapsing idiom, so a new one
//!   has to be argued for rather than typed. The census IS the enumeration
//!   deliverable: every remaining count is a measured verdict, named below.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test web_edge_body_parse_guard

use std::path::PathBuf;

/// `(file, substring that must appear, what it wires)`.
///
/// Usually the routes file itself; for the Brahma console the wiring that
/// matters is in the dispatch arm, because v4 validates the body only after its
/// 404 gate and moving that check back to the edge would answer the wrong
/// status.
const PARSER_WIRING: &[(&str, &str, &str)] = &[
    (
        "crates/quilltap-web/src/custom_tools_routes.rs",
        "custom_tools::parse_run_body(",
        "POST /api/v1/chats/{id}/custom-tools?action=run — v4's \
         `runSchema.parse`, uncaught, so any wrong-typed key is the flat 400 \
         `Validation error`",
    ),
    (
        "crates/quilltap-web/src/characters_routes.rs",
        "characters::parse_photo_save_by_id_body(",
        "POST /api/v1/characters/{id}/photos (JSON leg) — v4's \
         `saveByIdSchema.safeParse`, whose joined issue sentences are wire \
         payload",
    ),
    (
        "crates/quilltap-web/src/backup_routes.rs",
        "raw_field(&parsed, \"uploadId\")",
        "POST /api/v1/system/restore — v4 guards uploadId, then mode, then the \
         upload lookup, all inside the handler; the edge must forward the raw \
         values so BOTH entrances answer the same sentence",
    ),
    (
        "crates/quilltap-web/src/embedding_profiles_routes.rs",
        "json_body.get(\"scope\").cloned()",
        "POST /api/v1/embedding-profiles/{id}?action=reindex — v4 tests \
         `body.scope !== undefined` and interpolates `String(scope)`, so \
         both the absent/null split and the coercion must survive the edge",
    ),
    (
        "crates/quilltap-web/src/qtap_routes.rs",
        "truthy_export_data(",
        "POST /api/v1/system/tools?action=import-preview|import-execute — v4's \
         `if (!exportData)` is JS falsiness, so `0`/`''`/`false` are missing \
         exactly as `null` is",
    ),
    (
        "crates/quilltap-web/src/system_data_routes.rs",
        "parse_job_concurrency(&body_or_null)",
        "POST /api/v1/system/tools?action=job-concurrency — v4's \
         `jobConcurrencySchema.safeParse` → `validationError`, i.e. the two-key \
         `{error:'Validation error', details:[…]}` envelope; reading \
         `as_i64` answered an invented flat sentence with no `details`",
    ),
    (
        "crates/quilltap-web/src/system_data_routes.rs",
        "js_truthy(raw)",
        "POST /api/v1/system/tools?action=capabilities-report-delete — v4's \
         `if (!reportId)` is JS falsiness, so a TRUTHY non-string passes the \
         gate and answers 404 from the `===` lookup, not 400",
    ),
    (
        "crates/quilltap-web/src/files_routes.rs",
        "parse_mount_write_body(&body)",
        "PUT /api/v1/mount-points/{id}/files/{path} (JSON leg) — v4's \
         `writeBodySchema.safeParse`, whose joined issue messages are wire \
         payload; the four keys read raw also let an unknown `encoding`, a \
         negative/fractional `expected_mtime` and a string `force` through",
    ),
    (
        "crates/quilltap-core/src/api/engine.rs",
        "brahma::brahma_send_prepare(",
        "POST /api/v1/brahma-console/{id}/messages — `verifyBrahmaChat` FIRST, \
         `sendMessageSchema` second; the pair travels together so the 404 \
         cannot become a 400",
    ),
];

/// `(routes file, expected collapsing sites, the adjudication)`.
///
/// A count here is not an excuse — it is a verdict with a reason. Anything not
/// listed fails outright.
const COLLAPSE_CENSUS: &[(&str, usize, &str)] = &[
    (
        "crates/quilltap-web/src/system_data_routes.rs",
        13,
        "ADJUDICATED site by site (P4.62), and every arm is driven by \
         `system_body_guards_equivalence` over v4's real handlers. ELEVEN are \
         FAITHFUL: the almanack download's `content`/`filename` read the \
         server's OWN response entity; `action` (tasks-queue) lands every shape \
         on v4's one `!action || !includes(action)` sentence; `confirm` fails \
         `!== 'DELETE_ALL_MY_DATA'` identically and \
         `keepArchivedCharacterBundles` is v4's `!== false`, where None means \
         KEEP; `threshold` is v4's own `typeof === 'number' ? … : 0.80`; `type` \
         is refused for every non-member by one enum sentence and \
         `priority`/`maxAttempts` are literally `typeof === 'number' ? … : \
         undefined`; the `str_field` passphrase reads are v4's `typeof x === \
         'string' ? x : ''`; and `progressId` is deliberately collapsed to \
         'untracked' by v4's `if (parsed.success)` — though its GATE was not \
         faithful and now runs Zod's own uuid regex (`zod_uuid`). TWO were \
         DIVERGENT and are fixed: `reportId` and `concurrency`, both wired \
         above.",
    ),
    (
        "crates/quilltap-web/src/characters_routes.rs",
        6,
        "FAITHFUL, measured (P4.60 unit 2): NONE of the six reads caller input. \
         The export leg's `format` is a QUERY parameter and \
         `defaultImageId`/`name` are ENTITY fields v4 reads off the typed \
         character record (v4 `handlers/get.ts:72-98`); the ST-import PNG leg's \
         three read the server's OWN echo. The sixth occurrence is the comment \
         naming what `parse_photo_save_by_id_body` replaced.",
    ),
    (
        "crates/quilltap-web/src/qtap_target_route.rs",
        3,
        "FAITHFUL: entity fields off a chat row, not request-body keys.",
    ),
    (
        "crates/quilltap-web/src/qtap_routes.rs",
        2,
        "FAITHFUL, measured (P4.60 unit 6's confirm-only pass): \
         `manifest.format`/`version` mirror v4's `validateExportFile` strict \
         `!==` comparisons, which reject a wrong-typed value on both sides. The \
         `exportData` truthiness and the non-JSON-body 500 that the same pass \
         found ARE fixed — see `qtap_import_guards_equivalence`.",
    ),
    (
        "crates/quilltap-web/src/llm_logs_routes.rs",
        1,
        "FAITHFUL, measured (P4.62). ⚠ P4.60's prose called this a \
         'request-preview `content` projection'; it is not. It is the \
         image-aesthetics PUT's `content`, and it IS caller input — but v4 is \
         `aestheticContentSchema.safeParse(await req.json().catch(() => ({}))) \
         .data?.content ?? ''`, which coalesces a malformed body, an absent key \
         AND a wrong-typed value to `''` — and `''` DELETES the file. All three \
         must collapse; the handler defaults `None` to `''`.",
    ),
    (
        "crates/quilltap-web/src/custom_tools_routes.rs",
        1,
        "PROSE ONLY: the surviving occurrence is the comment naming what \
         `parse_run_body` replaced.",
    ),
];

const NEEDLE: &str = "and_then(Value::as_";

/// The same collapse spelled with a closure — `and_then(|v| v.as_str())`. P4.57
/// enumerated the `Value::as_*` form only; this second needle is what makes
/// P4.60's tier-2 sweep executable rather than a paragraph, and it is how
/// `files_routes.rs`'s seven body-key reads were found.
const CLOSURE_NEEDLE: &str = ".as_str())";

/// `(routes file, expected closure-form sites, the adjudication)`.
const CLOSURE_CENSUS: &[(&str, usize, &str)] = &[
    (
        "crates/quilltap-web/src/query.rs",
        2,
        "NOT a body read — the only two sites are the shared query reader's \
         `first` and `all` (P4.67), which map a `&String` from the URL pair \
         list to `&str`. There is no JSON value here, so no type can collapse: \
         a query parameter is a string or it is absent, full stop. The row \
         exists so the file stays UNDER the census — if a real body read ever \
         lands in it, the count moves and this guard fires.",
    ),
    (
        "crates/quilltap-web/src/files_routes.rs",
        5,
        "ADJUDICATED (P4.62), down from SEVEN: `content` and `encoding` on the \
     mount-file write left the needle entirely when they moved into \
     `parse_mount_write_body` (wired above). Of the five that remain, TWO are \
     the 201-vs-200 `createdAt`/`updatedAt` pick — FAITHFUL entity reads off \
     the server's own `Files` response, never a request body. The \
     `attach-mount-file` `str_field` is FAITHFUL: v4's `!v || typeof v !== \
     'string'` refuses absent, null, wrong-typed and empty alike, and the core \
     reproduces both sentences AFTER v4's chat-404. `fileId` on the link leg \
     WAS divergent twice over (an empty string rode through, and the 400 \
     preceded v4's chat-404) and is fixed — the collapse stays because the \
     coerced `\"\"` is what carries the guard past the 404. `tags[].tagId` is \
     SPLIT: the truthy-non-array 500 is fixed, while a wrong-typed tagId is a \
     DIVERGENT-RECORDED escalation (v4 carries the raw value into `linkedTo`; \
     closing it needs `Request::FileUpload.tags` widened past `Vec<String>` in \
     `quilltap-core/src/api/types.rs`). Arms: \
     `files_body_guards_equivalence`.",
    ),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("harness crate sits two levels under the repo root")
        .to_path_buf()
}

#[test]
fn adjudicated_edges_still_route_through_their_parser() {
    let root = repo_root();
    for (rel, needle, what) in PARSER_WIRING {
        let text = std::fs::read_to_string(root.join(rel))
            .unwrap_or_else(|e| panic!("read {rel}: {e} — did the routes file move?"));
        assert!(
            text.contains(needle),
            "{rel} no longer contains `{needle}`. The route's differential runs \
             the parser directly, so it would stay green while the edge itself \
             went back to collapsing. What this wires: {what}"
        );
    }
}

/// Walk `quilltap-web/src` and hold one needle's per-file counts to a census.
fn census_walk(needle: &str, census: &[(&str, usize, &str)], failures: &mut Vec<String>) {
    let root = repo_root();
    let dir = root.join("crates/quilltap-web/src");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".rs"))
        })
        .collect();
    files.sort();
    assert!(
        files.len() > 20,
        "the walk found only {} files in quilltap-web/src — it is not reaching \
         the transport crate",
        files.len()
    );

    let mut seen: Vec<String> = Vec::new();

    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let count = text.matches(needle).count();
        if count == 0 {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .expect("under the repo root")
            .to_string_lossy()
            .replace('\\', "/");
        seen.push(rel.clone());
        match census.iter().find(|(p, ..)| *p == rel) {
            None => failures.push(format!(
                "{rel}: {count} `{needle}` site(s) with no census entry. A body \
                 key read this way cannot tell absent from wrong-typed — read \
                 v4's route, then either port its schema or record the verdict \
                 here (P4.60)."
            )),
            Some((_, expected, _)) if count != *expected => failures.push(format!(
                "{rel}: {count} `{needle}` site(s), census says {expected}. \
                 Adjudicate the new one against v4's route before moving the \
                 number."
            )),
            Some(_) => {}
        }
    }

    for (rel, expected, why) in census {
        if !seen.contains(&(*rel).to_string()) {
            failures.push(format!(
                "{rel}: census expects {expected} `{needle}` site(s), found none. \
                 If the collapse is genuinely gone, drop the row ({why})."
            ));
        }
    }
}

#[test]
fn web_route_body_reads_match_the_census() {
    let mut failures: Vec<String> = Vec::new();
    census_walk(NEEDLE, COLLAPSE_CENSUS, &mut failures);
    census_walk(CLOSURE_NEEDLE, CLOSURE_CENSUS, &mut failures);
    assert!(
        failures.is_empty(),
        "the web-edge wrong-type-collapse census has drifted:\n  {}",
        failures.join("\n  ")
    );
}
