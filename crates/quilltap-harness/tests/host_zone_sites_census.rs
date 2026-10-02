//! The host-zone production-wiring census (P4.119, dogfood #121, ruled (a)
//! 2026-09-29; rewritten at P4.127; reshaped at P4.140 — Option V).
//!
//! Every display formatter takes its zone as an ARGUMENT, and every test and
//! differential passes one explicitly — so no differential can see whether
//! PRODUCTION passes the host zone. P4.119 left thirteen ambient wrappers
//! (`system_display_zone()` read at each entry point); **P4.127 retired them**
//! and **P4.140 finished the job**: the composition root (`quilltap-host`)
//! reads the zone ONCE and threads the VALUE everywhere — the tool executor
//! (`with_display_zone`), `CoreConfig`, the render-job handler, the Almanack,
//! the chat spine (its own runner, the Scenario Builder clock, and every
//! `ProcessMessageInput` / `RegenerateSwipeOptions` / `StepDeps` /
//! `ChatCreateDeps` it fills), and from there the Salon turn, `build_context`
//! on the turn and the swipe, the autonomous step and the greeting (Carina
//! through `ToolRunner::display_zone`). Nothing re-derives a display zone from
//! a NAME (P4.127's `display_zone_named` is gone). This census pins that,
//! three ways:
//!
//! 1. **Zero ambient reads in core** ([`CENSUS`]): `system_display_zone()` is
//!    called in `host_zone.rs` alone, and the list IS the set of calling files.
//!    A new production caller that formats through a wrapper that reads the
//!    environment fails here.
//! 2. **The value's every hop** — the host wiring ([`HOST_SITES`]: the one
//!    environment read and each injection point the differentials cannot see)
//!    and the core reads ([`VALUE_SITES`]: each struct carrying the threaded
//!    value reads it where it renders; the list IS the set of core files
//!    declaring a `pub display_zone` field).
//! 3. **Nothing hard-codes UTC for display** ([`UTC_ALLOWED`]).
//!
//! And behaviourally, two self-spawned children (`TZ` set on the CHILD's
//! `Command`, never `std::env::set_var` — that races every test thread):
//! [`production_entries_render_in_the_host_zone`] under `TZ=America/Chicago`,
//! and [`a_posix_tz_rule_reaches_every_display_entry`] under a POSIX rule with
//! no IANA name (`XST6XDT,M3.2.0,M11.1.0` — jiff parses the rule itself, so the
//! child is machine-independent). Each drives the DB-free host wiring it CAN
//! reach — `HostConfig::new` (the one environment read) — and renders the
//! whisper, the web-search dates and the progressions fallback from
//! `HostConfig.display_zone`, the value the spine is handed. The spine-built
//! runner, `build_context`, the swipe, the step and the greeting fills are held
//! by the SOURCE needles above, not by the children.
//!
//! **The calendar NAME residue (P4.140 Tier 3, recorded):** `HostConfig.tz` —
//! `zone_name` of the value, `"UTC"` for a POSIX rule — still drives cron (the
//! tick AND the manual autonomous routes, which must agree), the distill's
//! `server_tz` calendar, `js_local_offset_minutes` (`spine.rs`), the story-zone
//! fallback `timezone: Some(tz)`, the LLM-log cleanup and the Almanack's
//! `timezone` fact; under a POSIX `TZ` those run on UTC while every rendered
//! date honours the rule (the POSIX child pins `cfg.tz == "UTC"` as that
//! residue). **v4 is NOT the reference for a DST rule:** Node 24's ICU ignores a
//! POSIX `TZ` with DST and renders `/etc/localtime`'s zone; v5 honours the rule
//! (the ruled shape — recorded, never ported).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test host_zone_sites_census

mod source_census;

use source_census::{code_only, core_src_root, production_zone, rust_sources};

/// `(path under crates/quilltap-core/src, production system_display_zone()
/// calls, the surfaces)`. One row: the home.
const CENSUS: &[(&str, usize, &str)] = &[(
    "host_zone.rs",
    1,
    "the home: the `fn` itself (P4.140 retired `system_zone_name` with its two \
     Tier-3 callers — the markdown export now takes the VALUE and the \
     autonomous-room routes the engine's `zone_name` of it)",
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
        ".with_display_zone(self.display_zone.clone())",
        1,
        "`ChatSpine::tool_runner()` (carina / ask_carina / Brahma / Run Tool) — the \
         whole call, so the ARGUMENT is pinned, not just the builder (P4.140: the \
         VALUE, no longer the `tz` NAME through `display_zone_named`)",
    ),
    (
        "crates/quilltap-host/src/spine.rs",
        ".to_zoned(self.display_zone.clone())",
        1,
        "the Scenario Builder prompt's clock (v4's `new Date()` in the process \
         zone) — NAME-fed until P4.140 (it read `+00:00` under a POSIX `TZ`)",
    ),
    (
        "crates/quilltap-host/src/spine.rs",
        "display_zone: self.display_zone.clone(),",
        7,
        "the spine's own value hops: the swipe's `RegenerateSwipeOptions`, the \
         turn's `ProcessMessageInput`, the greeting's `ChatCreateDeps`, both \
         `clone_state`s (`ChatSpine` / `ChatCreateSpine`) and both \
         `ProductionSpineFactory::build` fills",
    ),
    (
        "crates/quilltap-host/src/spine.rs",
        "let chain_zone = self.display_zone.clone();",
        1,
        "the chained turns' closure capture (`self` is out of reach inside it)",
    ),
    (
        "crates/quilltap-host/src/spine.rs",
        "display_zone: chain_zone.clone(),",
        1,
        "each chained turn's `ProcessMessageInput`",
    ),
    (
        "crates/quilltap-host/src/spine.rs",
        "display_zone: &self.display_zone,",
        1,
        "the autonomous turn's `StepDeps`",
    ),
    (
        "crates/quilltap-web/src/lib.rs",
        ".with_display_zone(config.display_zone.clone())",
        1,
        "`production_host_config` (shared by the HTTP binary and the Tauri \
         shell) hands the ONE read to the spine factory — dropped, every spine \
         would render the factory's UTC default",
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

/// `(path under crates/quilltap-core/src, needle, production occurrences, the
/// entry)` — each core struct carrying the threaded display zone VALUE, read
/// where it renders (P4.140). A read that drops the value (back to
/// `TimeZone::UTC`, or a zone re-derived from a NAME) is invisible to every
/// differential — they all pass UTC — so each is pinned by source; and the
/// list IS the set of core files declaring a `pub display_zone` field, so a new
/// struct carrying the value must name its read here.
const VALUE_SITES: &[(&str, &str, usize, &str)] = &[
    (
        "services/orchestrator.rs",
        "input.display_zone.clone()",
        2,
        "`process_message` — the turn's tool runner AND its `BuildContextArgs` fill",
    ),
    (
        "services/orchestrator.rs",
        "display_zone: args.display_zone.clone(),",
        1,
        "`build_context_input` — the turn's and the swipe's `BuildContextInput`",
    ),
    (
        "services/build_context.rs",
        "let display_zone = input.display_zone.clone();",
        1,
        "`build_context` — the mail context, the whisper seam and the progressions \
         fallback (NOT the story `timezone`, NOT the `server_tz` NAME)",
    ),
    (
        "services/regenerate_swipe.rs",
        "display_zone: display_zone.clone(),",
        1,
        "the swipe's `BuildContextArgs` (it reaches the same whisper seam)",
    ),
    (
        "services/chat_create.rs",
        "&deps.display_zone,",
        1,
        "the greeting's `build_chat_context` (from `ChatCreateDeps.display_zone`)",
    ),
    (
        "enclave/step.rs",
        "display_zone: sdeps.display_zone.clone(),",
        1,
        "the autonomous turn's `ProcessMessageInput` (from `StepDeps.display_zone`)",
    ),
    (
        "api/engine.rs",
        "&self.inner.config.display_zone",
        8,
        "`CoreConfig.display_zone` — the Compose reply preface, the Salon-load \
         whispers (`salon::chat_get`), the markdown export's zone-less offsets \
         (P4.140 item (g)), and the five autonomous-room routes' cron NAME \
         (`host_zone::zone_name` of it — the tick's own derivation)",
    ),
    (
        "services/conversation_render_job.rs",
        "&self.display_zone,",
        1,
        "the `CONVERSATION_RENDER` handler's persisted chunk text",
    ),
    (
        "almanack/phase1_premises.rs",
        "let zone = &paths.display_zone;",
        1,
        "the Almanack's `AlmanackPaths`",
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
        1,
        "the fallback when the platform zone cannot be read at all (P4.140 \
         retired `display_zone_named`'s unknown-or-absent-name fallback)",
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

    // (1b) Each struct carrying the threaded display zone VALUE reads it where
    // it renders, exactly as many times as the table says — and the list IS
    // the set of core files declaring a `pub display_zone` field (P4.140).
    for (rel, needle, want, why) in VALUE_SITES {
        let got = count(&production_code(rel), needle);
        assert_eq!(
            got, *want,
            "{rel}: `{needle}` appears {got}x in production code, the census says {want} ({why})"
        );
    }
    let carriers: Vec<&str> = files
        .iter()
        .filter(|(_, code)| code.contains("pub display_zone:"))
        .map(|(rel, _)| rel.as_str())
        .collect();
    let mut value_listed: Vec<&str> = VALUE_SITES.iter().map(|(rel, _, _, _)| *rel).collect();
    value_listed.sort();
    value_listed.dedup();
    assert_eq!(
        carriers, value_listed,
        "the core files carrying a display zone VALUE differ from VALUE_SITES — a new \
         carrier must name its read here with its reason"
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
            // own allowances are `UTC_ALLOWED`); its two production
            // `TimeZone::UTC`s are both in `spine.rs`: `js_local_offset_minutes`
            // falling back for an unparseable NAME (a calendar offset lookup,
            // not a display default — the census once called it "the cron
            // parser"; cron parses inside core) and `ProductionSpineFactory::
            // new`'s test default, which production overrides with
            // `with_display_zone` (the `quilltap-web` row above). The
            // `HostAssembler` fill pinned above is the line this guards.
            if dir == "crates/quilltap-host/src" {
                let utc = code.matches("TimeZone::UTC").count();
                let want = if rel == "crates/quilltap-host/src/spine.rs" {
                    2
                } else {
                    0
                };
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
        out.status.success() && stdout.contains("CHILD OK: 5 value-threaded entries"),
        "the TZ=America/Chicago child failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Runs ONLY in the child (the parent sets [`CHILD_MARKER`] and `TZ`); a no-op
/// in an ordinary run. Proves the HOST wiring — not the ambient wrappers P4.127
/// retired — under a real Chicago process zone.
#[test]
fn child_production_entries() {
    use quilltap_core::host_zone::{system_display_zone, TimeZone};
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
    //    threaded VALUE — P4.140): web-search dates render in Chicago. A
    //    `BuiltInToolRunner::new` with no builder stays UTC.
    let results = vec![quilltap_core::tools::web_search::WebSearchResult {
        title: "Late edition".into(),
        url: "https://example.test/a".into(),
        snippet: "s".into(),
        published_date: Some("2026-06-15T03:00:00.000Z".into()),
    }];
    let zone = cfg.display_zone.clone();
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
    let whisper =
        quilltap_core::services::suparna_notifications::build_suparna_mail_whisper_in_zone(
            &[bertie_letter()],
            &cfg.display_zone,
        );
    assert!(
        whisper.contains("September 29, 2026 at 02:40 PM"),
        "{whisper}"
    );

    // 4. `HostConfig::set_display_zone` (P4.140) moves the VALUE and the NAME
    //    together: a config pinned to UTC (the web and Tauri test commons)
    //    renders UTC on this Chicago host — a `tz` alone would have left the
    //    value on Chicago, which every display entry now reads.
    let mut pinned = quilltap_host::HostConfig::new(&scratch);
    pinned.set_display_zone(TimeZone::UTC);
    assert_eq!(pinned.tz, "UTC");
    assert_eq!(pinned.display_zone.to_offset(ts).seconds(), 0);

    // 5. The progressions fallback: an absent zone renders like the passed
    //    Chicago, not UTC — over a zone-sensitive corpus row.
    let chicago = TimeZone::get("America/Chicago").unwrap();
    let sensitive = progressions_render_like(&cfg.display_zone, &chicago);
    assert!(
        sensitive > 0,
        "no zone-sensitive null-timezone progression row"
    );

    println!("CHILD OK: 5 value-threaded entries rendered in America/Chicago");
}

/// The walk's D2 letter (2026-09-29T19:40Z).
fn bertie_letter() -> quilltap_core::post_office::mailbox::DeliveredLetterSummary {
    quilltap_core::post_office::mailbox::DeliveredLetterSummary {
        path: "Mail/1790710800000-from-bertie.md".into(),
        from: "Bertie".into(),
        sent_at: "2026-09-29T19:40:00.000Z".into(),
        body: "Dinner at eight.".into(),
        alerted: false,
        in_reply_to: None,
    }
}

/// Over the progressions corpus's null-timezone rows, assert that the report
/// rendered in `host` equals the one rendered in `expected` wherever the zone
/// matters (the render differs from UTC); answers how many rows did.
fn progressions_render_like(
    host: &quilltap_core::host_zone::TimeZone,
    expected: &quilltap_core::host_zone::TimeZone,
) -> usize {
    use quilltap_core::host_zone::TimeZone;
    use quilltap_core::progressions::engine::render_progression_report_in_zone;
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../harness/oracle/fixtures/progressions-engine.json"
        ))
        .unwrap(),
    )
    .unwrap();
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
        let utc = render_progression_report_in_zone(&p, &d, &opts, &TimeZone::UTC);
        let local = render_progression_report_in_zone(&p, &d, &opts, expected);
        if utc != local {
            sensitive += 1;
            // …and the zone the host hands `build_progressions_section` is the
            // same zone the fallback should resolve.
            assert_eq!(
                render_progression_report_in_zone(&p, &d, &opts, host),
                local
            );
        }
    }
    sensitive
}

/// The POSIX-`TZ` child's env marker (set on its `Command` only).
const POSIX_CHILD_MARKER: &str = "QT_P4140_POSIX_ZONE_CHILD";

/// The POSIX rule the B5 walk set (Chicago's own US rule under a made-up
/// abbreviation — so it has NO IANA name). jiff parses it itself, so this child
/// is machine-independent.
const POSIX_RULE: &str = "XST6XDT,M3.2.0,M11.1.0";

/// P4.140 (Option V): under a POSIX `TZ` rule the host's ONE read is the rule's
/// zone (jiff 0.2.31's `try_system` parses it — the dogfood premise "the VALUE
/// resolves to UTC" was refuted), and the value — not the `tz` NAME, which
/// falls back to `"UTC"` — is what reaches every display entry.
///
/// Red-first (recorded in the lane record): on `main`, with one extra assertion
/// that the production derivation `display_zone_named(Some(&cfg.tz))` — how
/// every name-fed entry built its zone — had offset −5h, the child failed
/// `0` vs `-18000` and that zone's whisper read `07:40 PM`; the `HostConfig`
/// value assertions below were already green.
#[test]
fn a_posix_tz_rule_reaches_every_display_entry() {
    if std::env::var_os(POSIX_CHILD_MARKER).is_some() || std::env::var_os(CHILD_MARKER).is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("this test binary");
    let out = std::process::Command::new(exe)
        .args(["child_posix_zone_entries", "--exact", "--nocapture"])
        .env(POSIX_CHILD_MARKER, "1")
        .env("TZ", POSIX_RULE)
        .output()
        .expect("spawn the child test");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("CHILD OK: POSIX rule reached 4 display entries"),
        "the TZ={POSIX_RULE} child failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Runs ONLY in the POSIX child (the parent sets [`POSIX_CHILD_MARKER`] and
/// `TZ`); a no-op in an ordinary run.
#[test]
fn child_posix_zone_entries() {
    use quilltap_core::host_zone::{TimeZone, Timestamp};
    if std::env::var_os(POSIX_CHILD_MARKER).is_none() {
        return;
    }
    // 1. `HostConfig::new` — the ONE read — is the rule's zone: no IANA name,
    //    CDT at the walk's D2 instant, CST in January. `tz` is the documented
    //    calendar NAME residue (`zone_name` → "UTC").
    let scratch = std::env::temp_dir().join("qt-p4140-hostconfig");
    let cfg = quilltap_host::HostConfig::new(&scratch);
    let d2 = Timestamp::from_second(1_790_710_800).unwrap();
    let january = Timestamp::from_second(1_768_478_400).unwrap();
    assert_eq!(cfg.display_zone.iana_name(), None);
    assert_eq!(cfg.display_zone.to_offset(d2).seconds(), -5 * 3600);
    assert_eq!(cfg.display_zone.to_offset(january).seconds(), -6 * 3600);
    assert_eq!(cfg.tz, "UTC");

    // 2. The Suparṇā whisper built from the value (build_context's seam).
    let whisper =
        quilltap_core::services::suparna_notifications::build_suparna_mail_whisper_in_zone(
            &[bertie_letter()],
            &cfg.display_zone,
        );
    assert!(
        whisper.contains("September 29, 2026 at 02:40 PM"),
        "{whisper}"
    );

    // 3. The web-search formatter (every executor tool's runner).
    let results = vec![quilltap_core::tools::web_search::WebSearchResult {
        title: "Late edition".into(),
        url: "https://example.test/a".into(),
        snippet: "s".into(),
        published_date: Some("2026-06-15T03:00:00.000Z".into()),
    }];
    let formatted = quilltap_core::tools::web_search::format_web_search_results_in_zone(
        &results,
        &cfg.display_zone,
    );
    assert!(formatted.contains("(Published: 6/14/2026)"), "{formatted}");

    // 4. The progressions fallback renders as Chicago does (the rule IS
    //    Chicago's US rule) on every zone-sensitive corpus row.
    let chicago = TimeZone::get("America/Chicago").unwrap();
    assert!(progressions_render_like(&cfg.display_zone, &chicago) > 0);

    println!("CHILD OK: POSIX rule reached 4 display entries");
}
