//! The host-zone production-wiring census (P4.119, dogfood #121, ruled (a)
//! 2026-09-29).
//!
//! Every display formatter takes its zone as an ARGUMENT, and every test and
//! differential passes one explicitly — so no differential can see whether
//! PRODUCTION passes the host zone. That wiring lives at the entry points the
//! tool executor, the dispatch engine, `build_context`, the Salon load, the job
//! runner and the Almanack pipeline call (files outside this lane's ownership,
//! whose signatures therefore stayed put): each resolves
//! `crate::host_zone::system_display_zone()` ONCE and delegates to an explicit-
//! zone sibling. This census pins that, two ways:
//!
//! 1. **One row per surface** ([`CENSUS`]): the exact number of production
//!    `system_display_zone()` calls per file, and the list IS the set of files
//!    that call it. A new production caller that forgets the zone (formats
//!    through a sibling with a hard-coded zone) or a new surface that reads the
//!    host zone somewhere unlisted fails here.
//! 2. **Nothing else reads the system zone, and nothing hard-codes UTC for
//!    display** ([`UTC_ALLOWED`]): `TimeZone::system()` / `try_system(` appear
//!    in `host_zone.rs` alone, and `TimeZone::UTC` in production code only
//!    where it is NOT a display zone (named per row).
//!
//! And behaviourally ([`production_entries_render_in_the_host_zone`]): this
//! test binary re-runs itself with `TZ=America/Chicago` set on the CHILD's
//! `Command` (never `std::env::set_var` — that races every test thread) and the
//! child drives the DB-free production entries, asserting Chicago output. The
//! DB-bound entries are held by the census rows (each is a one-line delegation
//! to an `_in_zone` sibling that its family's differential drives).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test host_zone_sites_census

mod source_census;

use source_census::{code_only, core_src_root, production_zone, rust_sources};

/// `(path under crates/quilltap-core/src, production system_display_zone()
/// calls, the surfaces)`.
const CENSUS: &[(&str, usize, &str)] = &[
    (
        "host_zone.rs",
        2,
        "the home: the `fn` itself + `system_zone_name`, which reads the value \
         through it (cron, the markdown-transcript export, the autonomous-room \
         schedule, the host)",
    ),
    (
        "api/chat_post_office.rs",
        1,
        "`chat_send_mail` — the Salon Compose reply preface (PERSISTED)",
    ),
    (
        "tools/list_mail.rs",
        1,
        "`execute_list_mail` — the listing's letter dates",
    ),
    (
        "tools/read_mail.rs",
        1,
        "`execute_read_mail` — the letter's `posted` date",
    ),
    (
        "tools/send_mail.rs",
        1,
        "`execute_send_mail` — the tool's reply preface (PERSISTED)",
    ),
    (
        "tools/read_conversation.rs",
        1,
        "`execute_read_conversation` — the live transcript render",
    ),
    (
        "tools/annotations.rs",
        1,
        "`execute_upsert_annotation` — the live render that numbers messages",
    ),
    (
        "tools/web_search.rs",
        1,
        "`format_web_search_results` — the `Published:` dates",
    ),
    (
        "services/conversation_render_job.rs",
        1,
        "`handle_conversation_render` — the PERSISTED + embedded chunk text \
         (the #121 re-embed churn)",
    ),
    (
        "services/suparna_mail.rs",
        1,
        "`resolve_suparna_mail_context` — the turn's mail context",
    ),
    (
        "services/suparna_notifications.rs",
        2,
        "`build_suparna_mail_whisper` (build_context's seam) + \
         `surface_operator_mail_for_chat` (the Salon load) — PERSISTED whispers",
    ),
    (
        "progressions/engine.rs",
        2,
        "`render_progression_report` + `progression_placeholders` — the \
         absent/unresolvable-zone fallback (the greeting, Carina, build_context)",
    ),
];

/// The Almanack takes its zone as an INPUT (`AlmanackPaths::display_zone` —
/// the pipeline's machine-dependent values are all inputs, so its tier-2
/// differential can pin them); the one production site that fills it is the
/// host's `paths()`, held by [`host_almanack_paths_carry_the_host_zone`].
const HOST_ALMANACK_SITE: (&str, &str) = (
    "crates/quilltap-host/src/almanack_services.rs",
    "display_zone: quilltap_core::host_zone::system_display_zone(),",
);

/// `(path, production TimeZone::UTC uses, why it is not a display zone)`.
const UTC_ALLOWED: &[(&str, usize, &str)] = &[
    (
        "host_zone.rs",
        1,
        "the fallback when the platform zone cannot be read at all",
    ),
    (
        "format_time.rs",
        1,
        "`wall_ms_in_zone` reads wall digits encoded as UTC epoch ms back out",
    ),
    (
        "day_references.rs",
        1,
        "untouched precedent (Mandate 5): an unresolvable chat zone",
    ),
    (
        "services/memory_recap/distill.rs",
        1,
        "untouched precedent (Mandate 5): `local_date_stamp`'s fallback",
    ),
];

/// `(path, production `civil_from_days(` + `div_euclid(86_400_000)` uses, why
/// it is calendar math and not a display zone)`. Filled from a census of the
/// tree at unification; a new entry needs the same sentence.
const UTC_ARITHMETIC_ALLOWED: &[(&str, usize, &str)] = &[
    (
        "clock.rs",
        5,
        "the civil-day arithmetic itself (`civil_from_days` lives here)",
    ),
    (
        "chat_timestamp.rs",
        3,
        "`get_date_parts_in_timezone`'s zone-shifted epoch → civil split (v4 `getDatePartsInTimezone`)",
    ),
    (
        "enclave/cron.rs",
        4,
        "cron field arithmetic over an already-zoned wall time",
    ),
    (
        "api/system_backup.rs",
        2,
        "the retention window's whole-day maths, not a rendered date",
    ),
];

fn production_code(rel: &str) -> String {
    let path = core_src_root().join(rel);
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {rel}: {e}"));
    code_only(&production_zone(&src))
}

fn count(code: &str, needle: &str) -> usize {
    code.matches(needle).count()
}

fn all_core_files() -> Vec<(String, String)> {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let code = production_code(&rel);
            (rel, code)
        })
        .collect()
}

#[test]
fn host_zone_sites_census() {
    let files = all_core_files();

    // (1) The per-surface rows, and the list IS the set of calling files.
    for (rel, want, why) in CENSUS {
        let got = count(&production_code(rel), "system_display_zone(");
        assert_eq!(
            got, *want,
            "{rel}: {got} production system_display_zone() calls, the census says {want} ({why})"
        );
    }
    let callers: Vec<&str> = files
        .iter()
        .filter(|(_, code)| code.contains("system_display_zone("))
        .map(|(rel, _)| rel.as_str())
        .collect();
    let mut listed: Vec<&str> = CENSUS.iter().map(|(rel, _, _)| *rel).collect();
    listed.sort();
    assert_eq!(
        callers, listed,
        "the files reading the host display zone differ from the census — a new surface \
         must be added here with its reason"
    );

    // (2) Only the home reads the system zone.
    for (rel, code) in &files {
        let reads = count(code, "TimeZone::system(") + count(code, "try_system(");
        let want = usize::from(rel == "host_zone.rs");
        assert_eq!(
            reads, want,
            "{rel}: reads the system zone directly — go through crate::host_zone"
        );
    }

    // (2b) …and none re-derived by hand: the pre-P4.119 formatters did their
    // UTC arithmetic through `clock::civil_from_days(ms.div_euclid(86_400_000))`,
    // which `clock.rs` still exports for the calendar math that IS UTC by
    // definition. A new display formatter written that way never names
    // `TimeZone::UTC`, so the literal needle above cannot see it — this one can
    // (unified at the `97b25fc53` follow-ups round).
    for (rel, code) in &files {
        let got = count(code, "civil_from_days(") + count(code, "div_euclid(86_400_000)");
        let want = UTC_ARITHMETIC_ALLOWED
            .iter()
            .find(|(r, _, _)| r == rel)
            .map_or(0, |(_, n, _)| *n);
        assert_eq!(
            got, want,
            "{rel}: {got} production UTC civil-day derivations, {want} allowed — a display \
             formatter renders through a zone (P4.119)"
        );
    }

    // (2) No display zone hard-coded to UTC in production code.
    for (rel, code) in &files {
        let got = count(code, "TimeZone::UTC");
        let want = UTC_ALLOWED
            .iter()
            .find(|(r, _, _)| r == rel)
            .map_or(0, |(_, n, _)| *n);
        assert_eq!(
            got, want,
            "{rel}: {got} production `TimeZone::UTC` uses, {want} allowed — a display \
             formatter's zone comes from the caller (P4.119)"
        );
    }
}

#[test]
fn host_almanack_paths_carry_the_host_zone() {
    let root = core_src_root();
    let repo = root
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .unwrap();
    let (rel, line) = HOST_ALMANACK_SITE;
    let src = std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
    let code = code_only(&production_zone(&src));
    assert_eq!(
        code.matches(line).count(),
        1,
        "{rel}: the host's AlmanackPaths must carry the host display zone"
    );
    assert_eq!(
        code.matches("display_zone:").count(),
        1,
        "{rel}: one AlmanackPaths site"
    );
}

/// The env marker the parent sets on the child `Command` only.
const CHILD_MARKER: &str = "QT_P4119_HOST_ZONE_CHILD";

#[test]
fn production_entries_render_in_the_host_zone() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        return; // the child runs `child_production_entries` alone
    }
    let exe = std::env::current_exe().expect("this test binary");
    let out = std::process::Command::new(exe)
        .args(["child_production_entries", "--exact", "--nocapture"])
        .env(CHILD_MARKER, "1")
        .env("TZ", "America/Chicago")
        .output()
        .expect("spawn the child test");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("CHILD OK: 3 production entries"),
        "the TZ=America/Chicago child failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Runs ONLY in the child (the parent sets [`CHILD_MARKER`] and `TZ`); a no-op
/// in an ordinary run.
#[test]
fn child_production_entries() {
    use quilltap_core::host_zone::{system_display_zone, TimeZone};

    if std::env::var_os(CHILD_MARKER).is_none() {
        return;
    }
    // The child's own reading of the host zone IS Chicago — else the proof
    // below would be vacuous.
    assert_eq!(system_display_zone().iana_name(), Some("America/Chicago"));

    // 1. The web-search formatter (the executor's entry).
    let results = vec![quilltap_core::tools::web_search::WebSearchResult {
        title: "Late edition".into(),
        url: "https://example.test/a".into(),
        snippet: "s".into(),
        published_date: Some("2026-06-15T03:00:00.000Z".into()),
    }];
    let formatted = quilltap_core::tools::web_search::format_web_search_results(&results);
    assert!(formatted.contains("(Published: 6/14/2026)"), "{formatted}");

    // 2. The Suparṇā whisper (build_context's seam) — the walk's D2 instant.
    let letter = quilltap_core::post_office::mailbox::DeliveredLetterSummary {
        path: "Mail/1790710800000-from-bertie.md".into(),
        from: "Bertie".into(),
        sent_at: "2026-09-29T19:40:00.000Z".into(),
        body: "Dinner at eight.".into(),
        alerted: false,
        in_reply_to: None,
    };
    let whisper =
        quilltap_core::services::suparna_notifications::build_suparna_mail_whisper(&[letter]);
    assert!(
        whisper.contains("September 29, 2026 at 02:40 PM"),
        "{whisper}"
    );

    // 3. The progressions fallback: an absent zone renders like the passed
    //    Chicago, not UTC — over a zone-sensitive corpus row.
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../harness/oracle/fixtures/progressions-engine.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let chicago = TimeZone::get("America/Chicago").unwrap();
    let opts = quilltap_core::progressions::RenderProgressionOptions { timezone: None };
    let mut sensitive = 0;
    for c in corpus["renderProgressionReport"].as_array().unwrap() {
        if !c["timezone"].is_null() {
            continue;
        }
        let p = quilltap_core::progressions::parse_progression(&c["entry"]).unwrap();
        let d = quilltap_core::progressions::derive_progression(
            c["id"].as_str().unwrap(),
            &p,
            c["nowMs"].as_i64().unwrap(),
        );
        use quilltap_core::progressions::engine::{
            render_progression_report, render_progression_report_in_zone,
        };
        let utc = render_progression_report_in_zone(&p, &d, &opts, &TimeZone::UTC);
        let local = render_progression_report_in_zone(&p, &d, &opts, &chicago);
        if utc != local {
            sensitive += 1;
            assert_eq!(render_progression_report(&p, &d, &opts), local);
        }
    }
    assert!(
        sensitive > 0,
        "no zone-sensitive null-timezone progression row"
    );

    println!("CHILD OK: 3 production entries rendered in America/Chicago");
}
