//! The `quilltap-web` binary: the first-class HTTP deployment (D1/D2).
//!
//! ```text
//! quilltap-web [--host 127.0.0.1] [--port 3000] [--data-dir <path>]
//!              [--instance <name>] [--spa-dir <path>]
//! ```
//!
//! Bind policy (D2): the bare binary defaults to `127.0.0.1:3000` (v4's port
//! parity); the container entrypoint passes `--host 0.0.0.0`. No
//! authentication — localhost trust; put a proxy in front for more.
//!
//! The SPA dist resolves through `quilltap_web::spa` (P4.10): `--spa-dir` →
//! `QUILLTAP_SPA_DIR` → `<exe dir>/spa` → `<exe dir>/../share/quilltap/spa`
//! → the embedded placeholder pages. The startup banner says which.
//!
//! The timezone knobs (`QUILLTAP_TIMEZONE` / `TZ`) are reconciled at the very
//! top of `main` — see [`resolve_process_timezone`].

use std::net::SocketAddr;
use std::path::PathBuf;

use quilltap_web::{
    boot_startup_status, build_router, production_host_config, resolve_instance_base_dir, web_state,
};

/// What the two timezone knobs add up to, before anything is written back.
///
/// Kept as an enum rather than an `Option<String>` so `main` can say *why* it
/// left the clock alone: "nobody asked" and "you asked for something I refuse
/// to forward" are different startup stories, and the second one is the bug
/// this whole seam exists to prevent.
#[derive(Debug, PartialEq, Eq)]
enum TimezoneResolution {
    /// Neither knob is set. The process clock is left exactly as it was —
    /// which, in a container with no `/etc/localtime`, means UTC.
    Unset,
    /// A well-formed IANA zone name. Both knobs get this value.
    Resolved {
        zone: String,
        /// The knob it came from, for the startup line.
        source: &'static str,
    },
    /// Set, but not an IANA zone name (`CDT`, `EST`, `GMT+2`). Refused rather
    /// than forwarded — see the note in [`resolve_process_timezone`].
    Rejected { value: String, source: &'static str },
}

/// Reconcile `QUILLTAP_TIMEZONE` and `TZ` into the one zone the process should
/// run on, or say why it can't.
///
/// **Why this exists.** v5 reads the two knobs in two different places, exactly
/// as v4 does: `QUILLTAP_TIMEZONE` feeds the timestamp-formatting chain
/// (`quilltap_core::chat_timestamp`), while `TZ` is what
/// `jiff::tz::TimeZone::system()` honors — and *that* is what backs the paths
/// which consult the wall clock directly rather than asking how to phrase
/// something: same-day episodic recall (`quilltap_core::day_references`, the
/// "today"/"yesterday" windows P4.d26 ported zone-sensitively), the
/// autonomous-room cron tick, and the daily token-budget rollover at
/// instance-local midnight. A container that sets only one of the pair runs
/// half on the operator's clock and half on UTC: timestamps read handsomely
/// while a room scheduled for 07:00 rings at 02:00.
///
/// **The precedence rule is v4's**, transcribed from `docker/entrypoint.sh`
/// (v4 `a7f691e7`): whichever knob is set fills in the other, and
/// `QUILLTAP_TIMEZONE` wins when both are set and disagree. Neither set is
/// unchanged. v5 has no entrypoint script to hang that on — the image runs the
/// binary directly (`Dockerfile`'s `ENTRYPOINT`) — so the reconciliation lives
/// here instead, at the top of `main`.
///
/// **The validation rule is also v4's**, from `scripts/start-quilltap-docker.ts`'s
/// `detectTimezone` (v4 `298563eb`): a value is accepted only if it is `UTC` or
/// contains a `/`. An abbreviation like `CDT` is *rejected, not forwarded* —
/// v4's reasoning was that ICU silently falls back to UTC on one, which is the
/// very bug being fixed, and "better to pass nothing than to pass a lie". The
/// same holds here for a different engine: jiff would fail the lookup and fall
/// back too. Note the check does not fall through from a bad
/// `QUILLTAP_TIMEZONE` to a good `TZ` — v4's `detectTimezone` doesn't either;
/// the first knob that is set is the one that answers.
///
/// Pure by construction (no env reads) so the table below is testable without
/// mutating process-global state; `main` does the reading and the writing.
fn resolve_process_timezone(
    quilltap_timezone: Option<&str>,
    tz: Option<&str>,
) -> TimezoneResolution {
    // v4's entrypoint tests with `-n`, so an empty string is "not set".
    fn non_empty(v: Option<&str>) -> Option<&str> {
        v.filter(|s| !s.is_empty())
    }

    let (candidate, source) = match (non_empty(quilltap_timezone), non_empty(tz)) {
        (Some(v), _) => (v, "QUILLTAP_TIMEZONE"),
        (None, Some(v)) => (v, "TZ"),
        (None, None) => return TimezoneResolution::Unset,
    };

    if candidate == "UTC" || candidate.contains('/') {
        TimezoneResolution::Resolved {
            zone: candidate.to_string(),
            source,
        }
    } else {
        TimezoneResolution::Rejected {
            value: candidate.to_string(),
            source,
        }
    }
}

struct Args {
    host: String,
    port: u16,
    data_dir: Option<PathBuf>,
    instance: Option<String>,
    spa_dir: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        host: "127.0.0.1".to_string(),
        port: 3000,
        data_dir: None,
        instance: None,
        spa_dir: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} requires a value"));
        match flag.as_str() {
            "--host" => args.host = value("--host")?,
            "--port" => {
                args.port = value("--port")?
                    .parse()
                    .map_err(|_| "--port must be a number".to_string())?
            }
            "--data-dir" => args.data_dir = Some(PathBuf::from(value("--data-dir")?)),
            "--instance" => args.instance = Some(value("--instance")?),
            "--spa-dir" => args.spa_dir = Some(PathBuf::from(value("--spa-dir")?)),
            "--help" | "-h" => {
                println!(
                    "quilltap-web — the Quilltap HTTP host\n\n\
                     USAGE: quilltap-web [--host 127.0.0.1] [--port 3000]\n\
                            [--data-dir <path>] [--instance <name>] [--spa-dir <path>]\n\n\
                     ENVIRONMENT:\n  \
                       QUILLTAP_DATA_DIR  the instance directory, when --data-dir is absent\n  \
                       QUILLTAP_SPA_DIR   the Angular dist, when --spa-dir is absent\n  \
                       QUILLTAP_TIMEZONE  IANA zone name (e.g. America/Chicago) for the whole\n  \
                       TZ                 process — timestamps AND schedules. Either one sets\n  \
                                          both; QUILLTAP_TIMEZONE wins if they disagree.\n\n\
                     Absent both dist knobs, the dist is looked for beside the binary (./spa)\n\
                     and then at ../share/quilltap/spa; absent that, only placeholder pages\n\
                     are served."
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(args)
}

fn main() {
    // Before ANYTHING — no threads spawned yet, nothing has read the clock, the
    // host is not built. `jiff` reads `TZ` lazily, but treating it as
    // boot-time-only is the only rule that stays true as call sites move.
    // (`set_var` is safe here for the same reason: single-threaded still.)
    let timezone = resolve_process_timezone(
        std::env::var("QUILLTAP_TIMEZONE").ok().as_deref(),
        std::env::var("TZ").ok().as_deref(),
    );
    if let TimezoneResolution::Resolved { zone, .. } = &timezone {
        // Both knobs, always — that is the whole point of the reconciliation.
        std::env::set_var("QUILLTAP_TIMEZONE", zone);
        std::env::set_var("TZ", zone);
    }

    // The log surface, before anything else runs (P4.18). Events go to stderr,
    // env-filtered by `RUST_LOG` (default `info`); the startup banner below
    // stays on stdout.
    quilltap_web::init_tracing();

    // Now that the subscriber exists, say what the clock ended up as. A
    // rejected value is a warning, not a silent fall-back to UTC: a room
    // firing five hours early is not a failure anyone reads as a timezone
    // problem unless something said so at boot.
    match &timezone {
        TimezoneResolution::Resolved { zone, source } => {
            tracing::info!(timezone = %zone, source = %source, "process timezone set");
        }
        TimezoneResolution::Rejected { value, source } => {
            tracing::warn!(
                value = %value,
                source = %source,
                "refusing a timezone that is not an IANA zone name (expected `UTC` or \
                 `Area/Location`, e.g. America/Chicago); the process clock is unchanged, \
                 so scheduled rooms, daily budget rollover and same-day recall will follow \
                 the system zone"
            );
        }
        TimezoneResolution::Unset => {}
    }

    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("quilltap-web: {e}");
            std::process::exit(2);
        }
    };
    let base_dir = match resolve_instance_base_dir(args.data_dir.clone(), args.instance.as_deref())
    {
        Ok(d) => d,
        Err(e) => {
            eprintln!("quilltap-web: {e}");
            std::process::exit(2);
        }
    };
    let version = env!("CARGO_PKG_VERSION").to_string();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async move {
        let config = production_host_config(base_dir.clone(), version.clone());
        let startup = boot_startup_status(config);
        // --spa-dir → QUILLTAP_SPA_DIR → beside the binary → placeholders.
        let spa_dir = quilltap_web::spa::resolve_spa_dir_for_process(args.spa_dir.clone());
        let state = web_state(startup, version, base_dir, spa_dir.clone());
        let router = build_router(state);

        let addr: SocketAddr = format!("{}:{}", args.host, args.port)
            .parse()
            .unwrap_or_else(|_| {
                eprintln!("quilltap-web: invalid --host/--port");
                std::process::exit(2);
            });
        println!(
            "Quilltap is at home — the parlour door stands open at http://{addr}/ \
             (dispatch at /api/dispatch, the wire at /api/events)."
        );
        // Say where the furniture came from. A silent fall-back to the
        // placeholder pages is precisely the failure that went unnoticed
        // from P4.2 to P4.10 — it must never again be quiet.
        match &spa_dir {
            Some(dir) => println!("The parlour is furnished from {}.", dir.display()),
            None => eprintln!(
                "The parlour stands unfurnished: no Angular dist was found, so only the \
                 placeholder pages will be served. Name one with --spa-dir <path> or \
                 QUILLTAP_SPA_DIR, or set the dist down beside the binary as ./spa."
            ),
        }
        if let Err(e) = quilltap_web::serve(router, addr).await {
            eprintln!("quilltap-web: server error: {e}");
            std::process::exit(1);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table from v4's `docker/entrypoint.sh` (`a7f691e7`): whichever knob
    /// is set fills in the other, `QUILLTAP_TIMEZONE` wins a disagreement,
    /// neither set changes nothing.
    #[test]
    fn precedence_matches_v4_entrypoint() {
        // Both set and disagreeing: QUILLTAP_TIMEZONE wins.
        assert_eq!(
            resolve_process_timezone(Some("America/Chicago"), Some("Europe/Paris")),
            TimezoneResolution::Resolved {
                zone: "America/Chicago".to_string(),
                source: "QUILLTAP_TIMEZONE",
            }
        );
        // Only QUILLTAP_TIMEZONE: it fills TZ.
        assert_eq!(
            resolve_process_timezone(Some("Asia/Tokyo"), None),
            TimezoneResolution::Resolved {
                zone: "Asia/Tokyo".to_string(),
                source: "QUILLTAP_TIMEZONE",
            }
        );
        // Only TZ: it is kept, and fills QUILLTAP_TIMEZONE.
        assert_eq!(
            resolve_process_timezone(None, Some("Europe/London")),
            TimezoneResolution::Resolved {
                zone: "Europe/London".to_string(),
                source: "TZ",
            }
        );
        // Neither: unchanged (UTC in a container).
        assert_eq!(
            resolve_process_timezone(None, None),
            TimezoneResolution::Unset
        );
    }

    /// v4 tests with `-n`, so an empty variable is "not set" — an exported but
    /// blank `TZ=` must not shadow a good `QUILLTAP_TIMEZONE`, nor register as
    /// a rejected value.
    #[test]
    fn empty_strings_count_as_unset() {
        assert_eq!(
            resolve_process_timezone(Some(""), Some("")),
            TimezoneResolution::Unset
        );
        assert_eq!(
            resolve_process_timezone(Some(""), Some("America/Chicago")),
            TimezoneResolution::Resolved {
                zone: "America/Chicago".to_string(),
                source: "TZ",
            }
        );
    }

    /// The validation rule from v4's `detectTimezone` (`298563eb`): `UTC` or an
    /// `Area/Location` name, else refuse. `date +%Z`-style abbreviations are the
    /// named hazard — they resolve to nothing and fall back to UTC silently.
    #[test]
    fn abbreviations_are_rejected_not_forwarded() {
        for bogus in ["CDT", "EST", "PST8PDT", "GMT+2", "Chicago"] {
            assert_eq!(
                resolve_process_timezone(Some(bogus), None),
                TimezoneResolution::Rejected {
                    value: bogus.to_string(),
                    source: "QUILLTAP_TIMEZONE",
                },
                "{bogus} should be refused rather than forwarded"
            );
        }
        // `UTC` is the one bare name v4 accepts — `-e QUILLTAP_TIMEZONE=UTC`
        // is how an operator deliberately pins a container to UTC.
        assert_eq!(
            resolve_process_timezone(Some("UTC"), None),
            TimezoneResolution::Resolved {
                zone: "UTC".to_string(),
                source: "QUILLTAP_TIMEZONE",
            }
        );
    }

    /// A bad first knob does NOT fall through to a good second one — v4's
    /// `detectTimezone` reads `QUILLTAP_TIMEZONE || TZ || Intl` and validates
    /// the *result*, so a set-but-bogus `QUILLTAP_TIMEZONE` is the answer, and
    /// the answer is refused.
    #[test]
    fn a_bad_first_knob_does_not_fall_through() {
        assert_eq!(
            resolve_process_timezone(Some("CDT"), Some("America/Chicago")),
            TimezoneResolution::Rejected {
                value: "CDT".to_string(),
                source: "QUILLTAP_TIMEZONE",
            }
        );
    }

    /// The accepted values must actually resolve in the engine that consumes
    /// them — `jiff::tz::TimeZone::system()` reads `TZ`, and on Unix that means
    /// a tzdb on disk (`/usr/share/zoneinfo`; present in the image's
    /// `debian:bookworm-slim` base via `tzdata`). A validation rule that let
    /// through names jiff cannot look up would be the same silent-fallback bug
    /// wearing v4's clothes.
    #[test]
    fn accepted_zones_resolve_in_jiff() {
        for zone in ["UTC", "America/Chicago", "Europe/London", "Asia/Tokyo"] {
            assert!(
                jiff::tz::TimeZone::get(zone).is_ok(),
                "{zone} passes validation but jiff cannot resolve it"
            );
        }
        for bogus in ["CDT", "GMT+2"] {
            assert!(
                jiff::tz::TimeZone::get(bogus).is_err(),
                "{bogus} is refused by validation; jiff should not resolve it either"
            );
        }
    }
}
