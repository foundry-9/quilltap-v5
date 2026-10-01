//! The host-zone production-wiring census (P4.119, dogfood #121, ruled (a)
//! 2026-09-29; rewritten at P4.127).
//!
//! Every display formatter takes its zone as an ARGUMENT, and every test and
//! differential passes one explicitly — so no differential can see whether
//! PRODUCTION passes the host zone. P4.119 left thirteen ambient wrappers
//! (`system_display_zone()` read at each entry point); **P4.127 retired them**:
//! the composition root (`quilltap-host`) reads the zone ONCE and injects it —
//! a `TimeZone` VALUE on the tool executor (`with_display_zone`), `CoreConfig`
//! and the render-job handler, and the already-threaded `server_tz` NAME through
//! [`quilltap_core::host_zone::display_zone_named`] at the Salon turn,
//! `build_context` and the greeting (Carina through `ToolRunner::display_zone`).
//! This census pins that, three ways:
//!
//! 1. **Zero ambient reads in core** ([`CENSUS`]): `system_display_zone()` is
//!    called in `host_zone.rs` alone (the `fn` + `system_zone_name`), and the
//!    list IS the set of calling files. A new production caller that formats
//!    through a wrapper that reads the environment fails here.
//! 2. **The host wiring** ([`HOST_SITES`]): the one environment read, and each
//!    injection point the differentials cannot see (the `CoreConfig` fill, the
//!    render handler, the spine's runner, the Almanack paths); plus the helper's
//!    three core call sites ([`HELPER_SITES`]).
//! 3. **Nothing hard-codes UTC for display** ([`UTC_ALLOWED`]).
//!
//! And behaviourally ([`production_entries_render_in_the_host_zone`]): this
//! test binary re-runs itself with `TZ=America/Chicago` set on the CHILD's
//! `Command` (never `std::env::set_var` — that races every test thread) and the
//! child drives the DB-free host wiring it CAN reach without a database —
//! `HostConfig::new` (the one environment read) and the name helper — and
//! renders through an executor it builds itself with the same builder call,
//! asserting Chicago output AND that a `server_tz: "UTC"` input still renders
//! UTC on that Chicago host. The spine-built runner, `build_context` and the
//! greeting fills are held by the SOURCE needles above, not by the child
//! (the `97b25fc53` smalls unification corrected this claim).
//!
//! **Ruled divergence (P4.127, recorded for the human; RULED TO CLOSE
//! 2026-09-30 — Option V ordered, `phase-4.md` NEXT 2(k), after which the
//! name-fed entries and `HELPER_SITES` below retire):** every NAME-fed entry
//! resolves its zone from an IANA name, so a host whose zone has no IANA name
//! (a POSIX `TZ` string, a fixed offset) displays UTC there where v4 displays
//! the host zone. The name-fed entries are the Salon turn's tool runner AND
//! the spine-built runner (so every executor tool — `read_conversation`,
//! `list_mail`, `read_mail`, `send_mail`, `upsert_annotation`, `search_web` —
//! and Carina), `build_context` and the greeting; the VALUE-fed entries
//! (`CoreConfig`'s Compose reply preface, the render job, the Almanack) keep
//! the real offsets — so on such a host the Compose reply preface and the
//! `send_mail` tool's preface, both persisted, render in different zones.
//! See `host_zone.rs`'s [`display_zone_named`].
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test host_zone_sites_census

mod source_census;

use source_census::{code_only, core_src_root, production_zone, rust_sources};

/// `(path under crates/quilltap-core/src, production system_display_zone()
/// calls, the surfaces)`. One row: the home.
const CENSUS: &[(&str, usize, &str)] = &[(
    "host_zone.rs",
    2,
    "the home: the `fn` itself + `system_zone_name`, which reads the value \
     through it (the markdown-transcript export and the autonomous-room \
     schedule — the two recorded Tier-3 name readers; cron reads `HostConfig.tz`)",
)];

/// `(path from the repo root, needle, production occurrences, what it pins)` —
/// the host crate's injection points. A dropped fill is invisible to every
/// differential (they all pass a zone explicitly), so each is pinned by source.
const HOST_SITES: &[(&str, &str, usize, &str)] = &[
    (
        "crates/quilltap-host/src/host.rs",
        "quilltap_core::host_zone::system_display_zone()",
        1,
        "the host's ONE environment read (`HostConfig::new`)",
    ),
    (
        "crates/quilltap-host/src/host.rs",
        "display_zone: config.display_zone.clone(),",
        1,
        "the `HostAssembler` fill from `HostConfig` — the ONE hop every value-fed \
         entry below reads from (a `TimeZone::UTC` here would silently render \
         UTC in the render job and the Almanack: the P4.119 / #121 regression, \
         unpinned until the `97b25fc53` smalls unification)",
    ),
    (
        "crates/quilltap-host/src/host.rs",
        "display_zone: config.display_zone,",
        1,
        "the `CoreConfig` fill (the engine's reply preface + Salon-load whispers)",
    ),
    (
        "crates/quilltap-host/src/host.rs",
        "self.display_zone.clone()",
        2,
        "the `CONVERSATION_RENDER` handler's fill (the persisted chunk text) and \
         the Almanack services' positional argument",
    ),
    (
        "crates/quilltap-host/src/almanack_services.rs",
        "display_zone: self.display_zone.clone(),",
        1,
        "the Almanack's `AlmanackPaths` (`paths()`)",
    ),
    (
        "crates/quilltap-host/src/spine.rs",
        ".with_display_zone(quilltap_core::host_zone::display_zone_named(Some(&self.tz)))",
        1,
        "`ChatSpine::tool_runner()` (carina / ask_carina / Brahma / Run Tool) — the \
         whole call, so the ARGUMENT is pinned, not just the builder",
    ),
    (
        "crates/quilltap-host/src/spine.rs",
        ".to_zoned(quilltap_core::host_zone::display_zone_named(Some(&self.tz)))",
        1,
        "the Scenario Builder prompt's clock (v4's `new Date()` in the process \
         zone) — the host's last ambient `Zoned::now()`, retired at the \
         `97b25fc53` smalls unification",
    ),
];

/// Ambient zone/time reads a host-side crate may still carry, by file:
/// `(path from the repo root, needle, allowed count, why)`. Anything not
/// listed here is red — the host reads the zone once.
const AMBIENT_READ_ALLOWED: &[(&str, &str, usize, &str)] = &[(
    "crates/quilltap-cli/src/docs_cmd.rs",
    "TimeZone::system(",
    1,
    "the CLI's docs listing renders in the process zone (pre-P4.119, a \
     display-only CLI surface with no host config to read — named, not hidden)",
)];

/// The ambient zone/time reads the loop below hunts: a `TimeZone::system()`,
/// jiff's fallible twin, and a zoned `now` (`Zoned::now()` reads the system
/// zone), besides the home's own `system_display_zone(`.
const AMBIENT_READ_NEEDLES: &[&str] = &[
    "system_display_zone(",
    "TimeZone::system(",
    "try_system(",
    "Zoned::now(",
];

/// `(path under crates/quilltap-core/src, `display_zone_named(` call sites in
/// production code, the entry)`. The helper resolves the already-threaded
/// `server_tz` NAME; each entry is one call.
const HELPER_SITES: &[(&str, usize, &str)] = &[
    ("host_zone.rs", 1, "the helper's own definition"),
    (
        "services/orchestrator.rs",
        1,
        "`process_message` — the turn's tool runner (from `input.server_tz`)",
    ),
    (
        "services/build_context.rs",
        1,
        "`build_context` — the mail context, the whisper seam and the progressions \
         fallback (from `input.server_tz`, NOT the story `timezone`)",
    ),
    (
        "services/chat_create.rs",
        1,
        "the greeting's `build_chat_context` (from `ChatCreateDeps.tz`)",
    ),
];

/// Host-side crates that must NOT read the environment for a display zone
/// themselves (the one read is `host.rs`, pinned above).
const NO_AMBIENT_READ_DIRS: &[&str] = &[
    "crates/quilltap-web/src",
    "crates/quilltap-tauri/src",
    "crates/quilltap-cli/src",
];

/// `(path, production TimeZone::UTC uses, why it is not a display zone)`.
const UTC_ALLOWED: &[(&str, usize, &str)] = &[
    (
        "host_zone.rs",
        2,
        "the fallback when the platform zone cannot be read at all + \
         `display_zone_named`'s unknown-or-absent-name fallback",
    ),
    (
        "tools/executor.rs",
        1,
        "`BuiltInToolRunner::new`'s default — a test/CLI default; production calls \
         `with_display_zone`",
    ),
    (
        "services/tool_execution.rs",
        1,
        "`ToolRunner::display_zone`'s default — canned runners; \
         `BuiltInToolRunner` overrides it",
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

    // (1b) The name-fed entries each call the helper exactly as many times as
    // the table says, and the list IS the set of calling files.
    for (rel, want, why) in HELPER_SITES {
        let got = count(&production_code(rel), "display_zone_named(");
        assert_eq!(
            got, *want,
            "{rel}: {got} production display_zone_named() calls, the census says {want} ({why})"
        );
    }
    let helper_callers: Vec<&str> = files
        .iter()
        .filter(|(_, code)| code.contains("display_zone_named("))
        .map(|(rel, _)| rel.as_str())
        .collect();
    let mut helper_listed: Vec<&str> = HELPER_SITES.iter().map(|(rel, _, _)| *rel).collect();
    helper_listed.sort();
    assert_eq!(
        helper_callers, helper_listed,
        "the files resolving a display zone from a name differ from HELPER_SITES — a new \
         entry must be added here with its reason"
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

fn repo_root() -> std::path::PathBuf {
    core_src_root()
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

#[test]
fn host_injection_points_carry_the_host_zone() {
    let repo = repo_root();
    for (rel, needle, want, why) in HOST_SITES {
        let src =
            std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        let code = code_only(&production_zone(&src));
        assert_eq!(
            code.matches(needle).count(),
            *want,
            "{rel}: `{needle}` must appear {want}x in production code — {why}"
        );
    }
    // No other host-side crate reads the environment for a display zone or a
    // zoned `now` — the host crate's only read is the one pinned above, and the
    // one recorded CLI read is named in `AMBIENT_READ_ALLOWED`. (The first
    // shape of this loop hunted `system_display_zone(` alone and could not see
    // `TimeZone::system()` or `Zoned::now()` — the `97b25fc53` smalls
    // unification widened it, and found the spine's Scenario Builder clock.)
    for dir in NO_AMBIENT_READ_DIRS
        .iter()
        .copied()
        .chain(["crates/quilltap-host/src"])
    {
        let mut files = Vec::new();
        rust_sources(&repo.join(dir), &mut files);
        for f in files {
            let rel = f
                .strip_prefix(&repo)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&f).unwrap();
            let code = code_only(&production_zone(&src));
            for needle in AMBIENT_READ_NEEDLES {
                let n = code.matches(needle).count();
                let want = if *needle == "system_display_zone("
                    && rel == "crates/quilltap-host/src/host.rs"
                {
                    1
                } else {
                    AMBIENT_READ_ALLOWED
                        .iter()
                        .find(|(p, nd, _, _)| *p == rel && nd == needle)
                        .map_or(0, |(_, _, n, _)| *n)
                };
                assert_eq!(
                    n, want,
                    "{rel}: {n} production `{needle}` reads, {want} allowed — the host reads the \
                     zone once, in `HostConfig::new`; a display-only survivor is named in \
                     AMBIENT_READ_ALLOWED"
                );
            }
            // The host crate hard-codes UTC nowhere for display (the core's
            // own allowances are `UTC_ALLOWED`); its ONE production
            // `TimeZone::UTC` is `spine.rs`'s cron parser falling back for an
            // unparseable NAME — a zone lookup, not a display default. The
            // `HostAssembler` fill pinned above is the line this guards.
            if dir == "crates/quilltap-host/src" {
                let utc = code.matches("TimeZone::UTC").count();
                let want = usize::from(rel == "crates/quilltap-host/src/spine.rs");
                assert_eq!(
                    utc, want,
                    "{rel}: {utc} production `TimeZone::UTC` uses, {want} allowed"
                );
            }
        }
    }
}

/// A throwaway main DB — the runner only needs a handle; no tool runs here.
fn scratch_db() -> (tempfile::TempDir, quilltap_core::db::runtime::Db) {
    use quilltap_core::db::runtime::{Db, DbPaths};
    use quilltap_core::db::Writer;
    const PEPPER: &str = "dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHktMzJieXRl";
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.db");
    drop(Writer::open_writable(&main, PEPPER).expect("open main writer"));
    let db = Db::open(
        DbPaths {
            main,
            mount_index: None,
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db");
    (dir, db)
}

fn scratch_env() -> quilltap_core::tools::self_inventory::SelfInventoryEnv {
    use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
    SelfInventoryEnv {
        version: "0.0.0-census".to_string(),
        runtime_mode: "desktop".to_string(),
        client_shell: ClientShell::Unknown,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    }
}

/// The env marker the parent sets on the child `Command` only.
const CHILD_MARKER: &str = "QT_P4127_HOST_ZONE_CHILD";

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
        out.status.success() && stdout.contains("CHILD OK: 5 host-wired entries"),
        "the TZ=America/Chicago child failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Runs ONLY in the child (the parent sets [`CHILD_MARKER`] and `TZ`); a no-op
/// in an ordinary run. Proves the HOST wiring — not the ambient wrappers P4.127
/// retired — under a real Chicago process zone.
#[test]
fn child_production_entries() {
    use quilltap_core::host_zone::{display_zone_named, system_display_zone, TimeZone};
    use quilltap_core::services::tool_execution::ToolRunner;
    use quilltap_core::tools::executor::BuiltInToolRunner;

    if std::env::var_os(CHILD_MARKER).is_none() {
        return;
    }
    // The child's own reading of the host zone IS Chicago — else the proof
    // below would be vacuous.
    assert_eq!(system_display_zone().iana_name(), Some("America/Chicago"));

    // 1. `HostConfig::new` — the ONE read — carries the value, and `tz` is the
    //    name derived from it.
    let scratch = std::env::temp_dir().join("qt-p4127-hostconfig");
    let cfg = quilltap_host::HostConfig::new(&scratch);
    assert_eq!(cfg.display_zone.iana_name(), Some("America/Chicago"));
    assert_eq!(cfg.tz, "America/Chicago");

    // 2. The executor, built as `ChatSpine::tool_runner()` builds it (the
    //    already-threaded `tz` NAME through the helper): web-search dates render
    //    in Chicago. A `BuiltInToolRunner::new` with no builder stays UTC.
    let results = vec![quilltap_core::tools::web_search::WebSearchResult {
        title: "Late edition".into(),
        url: "https://example.test/a".into(),
        snippet: "s".into(),
        published_date: Some("2026-06-15T03:00:00.000Z".into()),
    }];
    let zone = display_zone_named(Some(&cfg.tz));
    let formatted =
        quilltap_core::tools::web_search::format_web_search_results_in_zone(&results, &zone);
    assert!(formatted.contains("(Published: 6/14/2026)"), "{formatted}");
    // Carina reads the zone off the runner it runs under (`ToolRunner::display_zone`).
    let probe = quilltap_core::host_zone::Timestamp::from_second(1_787_000_000).unwrap(); // a CDT (UTC-5) instant
    let ts = probe;
    let runner_zone = |r: &dyn Fn() -> TimeZone| r().to_offset(ts).seconds();
    let (_dir, db) = scratch_db();
    let wired = BuiltInToolRunner::new(db.clone(), scratch_env()).with_display_zone(zone);
    let bare = BuiltInToolRunner::new(db, scratch_env());
    assert_eq!(runner_zone(&|| wired.display_zone()), -5 * 3600);
    assert_eq!(runner_zone(&|| bare.display_zone()), 0);

    // 3. The Suparṇā whisper (build_context's seam) — the walk's D2 instant.
    let letter = quilltap_core::post_office::mailbox::DeliveredLetterSummary {
        path: "Mail/1790710800000-from-bertie.md".into(),
        from: "Bertie".into(),
        sent_at: "2026-09-29T19:40:00.000Z".into(),
        body: "Dinner at eight.".into(),
        alerted: false,
        in_reply_to: None,
    };
    let whisper =
        quilltap_core::services::suparna_notifications::build_suparna_mail_whisper_in_zone(
            &[letter],
            &display_zone_named(Some(&cfg.tz)),
        );
    assert!(
        whisper.contains("September 29, 2026 at 02:40 PM"),
        "{whisper}"
    );

    // 4. The helper honours the NAME it is given, never the environment: a
    //    `server_tz: "UTC"` input renders UTC on this Chicago host (M2's target).
    let utc_zone = display_zone_named(Some("UTC"));
    assert_eq!(utc_zone.to_offset(ts).seconds(), 0);
    assert_eq!(display_zone_named(None).to_offset(ts).seconds(), 0);

    // 5. The progressions fallback: an absent zone renders like the passed
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
        use quilltap_core::progressions::engine::render_progression_report_in_zone;
        let utc = render_progression_report_in_zone(&p, &d, &opts, &TimeZone::UTC);
        let local = render_progression_report_in_zone(&p, &d, &opts, &chicago);
        if utc != local {
            sensitive += 1;
            // …and the zone the host hands `build_progressions_section` is the
            // same Chicago the fallback resolves.
            assert_eq!(
                render_progression_report_in_zone(
                    &p,
                    &d,
                    &opts,
                    &display_zone_named(Some(&cfg.tz))
                ),
                local
            );
        }
    }
    assert!(
        sensitive > 0,
        "no zone-sensitive null-timezone progression row"
    );

    println!("CHILD OK: 5 host-wired entries rendered in America/Chicago");
}
