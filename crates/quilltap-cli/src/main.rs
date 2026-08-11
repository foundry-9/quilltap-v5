//! The native `quilltap` binary (P4.3) — the v4 launcher's subcommand surface
//! (`packages/quilltap/bin/quilltap.js`), output-diffed byte-for-byte against
//! it (tests/cli_differential.rs).
//!
//! Routing is v4's `locateSubcommand`: walk argv skipping the value tokens of
//! the global value flags; the first bare token decides iff it names a known
//! subcommand; everything else (including the leading flags) is handed to the
//! subcommand, which re-parses position-independently. Recognized-but-unshipped
//! subcommands exit loud; unknown bare tokens fall through to the launcher
//! path (v4's `parseArgs` rejection). Bare `quilltap` prints a banner pointing
//! at `quilltap-web` instead of forking a server (D12 — the download-manager /
//! native-module-healing machinery does not port).
//!
//! Help texts under `src/help/` are byte transcriptions of the v4 launcher's
//! output (captured from `node bin/quilltap.js … --help` at v4 `2494a84b`;
//! `db_help.txt` and `docs_help.txt` re-captured at v4 `ed8934f1` for the
//! `characters` archive subcommands and `docs docker-mounts`) — the
//! tool-catalog static-data-transcription precedent.

mod completion_cmd;
mod db_characters;
mod db_cmd;
mod dbopen;
mod docs_cmd;
mod http;
mod instances_cmd;
mod nodefmt;
mod out;
mod recall_replay_cmd;
mod resolve;
mod vtable;

const MAIN_HELP: &str = include_str!("help/main_help.txt");

/// v4 `SUBCOMMANDS`.
const SUBCOMMANDS: &[&str] = &[
    "db",
    "themes",
    "docs",
    "memories",
    "instances",
    "memory-diff",
    "recall-replay",
    "completion",
    "logs",
    "migrations",
    "maintenance",
    "file-verify",
];

/// v4 `GLOBAL_VALUE_FLAGS` — global flags that consume the following token.
const GLOBAL_VALUE_FLAGS: &[&str] = &[
    "-p",
    "--port",
    "-d",
    "--data-dir",
    "-i",
    "--instance",
    "--passphrase",
];

/// v4 `locateSubcommand`: index of the subcommand token, or None.
fn locate_subcommand(argv: &[String]) -> Option<usize> {
    let mut k = 0;
    while k < argv.len() {
        let a = argv[k].as_str();
        if GLOBAL_VALUE_FLAGS.contains(&a) {
            k += 2; // skip the flag's value
            continue;
        }
        if a.starts_with('-') {
            k += 1; // boolean / unknown flag
            continue;
        }
        return if SUBCOMMANDS.contains(&a) {
            Some(k)
        } else {
            None
        }; // first bare token decides
    }
    None
}

/// The loud exit for a recognized-but-unshipped subcommand (the
/// runner-fallback idiom).
fn not_yet_available(what: &str) -> ! {
    out::elog(&format!(
        "Error: the '{what}' subcommand is recognized but not yet available in this build of the quilltap CLI."
    ));
    out::exit(1);
}

/// The v4 launcher path (`parseArgs` + `main`) — minus the server fork:
/// flags validate exactly as v4, then the v5 banner points at `quilltap-web`.
fn launcher_main(args: &[String]) {
    let mut data_dir = String::new();
    let mut instance = String::new();
    let mut help = false;
    let mut version = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                let port = nodefmt::js_parse_int(args.get(i + 1).map(String::as_str));
                i += 1;
                match port {
                    Some(p) if (1..=65535).contains(&p) => {}
                    _ => {
                        out::elog("Error: --port must be a number between 1 and 65535");
                        out::exit(1);
                    }
                }
            }
            "--data-dir" | "-d" => {
                data_dir = args.get(i + 1).cloned().unwrap_or_default();
                i += 1;
            }
            "--instance" | "-i" => {
                instance = args.get(i + 1).cloned().unwrap_or_default();
                i += 1;
            }
            "--open" | "-o" => {}
            "--version" | "-v" => version = true,
            "--update" => {}
            "--help" | "-h" => help = true,
            other => {
                out::elog(&format!("Unknown argument: {other}"));
                out::elog("Run \"quilltap --help\" for usage information.");
                out::exit(1);
            }
        }
        i += 1;
    }

    if help {
        out::write_stdout(MAIN_HELP.as_bytes());
        out::exit(0);
    }
    if version {
        out::log(env!("CARGO_PKG_VERSION"));
        out::exit(0);
    }

    // v4 validates the instance/data-dir pairing before starting the server.
    if !data_dir.is_empty() && !instance.is_empty() {
        out::elog("Error: Specify either --instance or --data-dir, not both.");
        out::exit(1);
    }
    let mut resolved_dir = data_dir.clone();
    if !instance.is_empty() {
        let registry = quilltap_host::instances::InstanceRegistry::at_default_location();
        match registry.resolve_instance(&instance) {
            Ok(inst) => resolved_dir = inst.path,
            Err(e) => {
                out::elog(&format!("Error: {e}"));
                out::exit(1);
            }
        }
    }

    // The v5 banner-and-exit (this build does not embed the web server yet —
    // wiring `quilltap` → `quilltap-web` is a next-round decision).
    out::log("");
    out::log(&format!("  Quilltap v{}", env!("CARGO_PKG_VERSION")));
    out::log("");
    if !resolved_dir.is_empty() {
        out::log(&format!("  Data dir:  {resolved_dir}"));
        out::log("");
    }
    out::log("  The native engine is at its post, but this binary does not yet");
    out::log("  seat the web parlour — that duty belongs to `quilltap-web`.");
    out::log("  Run it (or the Quilltap container) to open the Salon in a browser.");
    out::log("");
    out::log("  The tooling verbs are all on duty here:");
    out::log("    quilltap db --help          Query the encrypted databases");
    out::log("    quilltap docs --help        Inspect document mounts");
    out::log("    quilltap instances --help   Register named instances");
    out::log("");
    out::exit(0);
}

/// The CLI's log surface (P4.18): a `tracing-subscriber` fmt subscriber on
/// **stderr**, env-filtered by `RUST_LOG` (default `info`). Same recipe as
/// `quilltap_web::init_tracing`, inlined because the CLI does not (and should
/// not) depend on `quilltap-web`. Stderr only — the CLI's stdout is piped user
/// output (`db`/`docs` tables), which must never carry log lines. Idempotent by
/// `try_init`. In practice silent for normal CLI verbs (they run no job pump or
/// cheap-LLM path); it exists so `RUST_LOG=debug quilltap …` can surface
/// core/host events when a verb does touch an instrumented path.
fn init_cli_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn main() {
    init_cli_tracing();

    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    let sub_idx = locate_subcommand(&cli_args);

    match sub_idx {
        Some(idx) => {
            let sub_name = cli_args[idx].clone();
            // Everything except the subcommand token (leading flags kept).
            let mut sub_args: Vec<String> = Vec::with_capacity(cli_args.len().saturating_sub(1));
            sub_args.extend_from_slice(&cli_args[..idx]);
            sub_args.extend_from_slice(&cli_args[idx + 1..]);

            match sub_name.as_str() {
                "db" => out::exit(db_cmd::run(&sub_args)),
                "docs" => docs_cmd::run(&sub_args),
                "instances" => instances_cmd::run(&sub_args),
                "completion" => completion_cmd::run(&sub_args),
                "recall-replay" => recall_replay_cmd::run(&sub_args),
                other => not_yet_available(other),
            }
        }
        None => launcher_main(&cli_args),
    }
    out::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn locate_subcommand_matches_v4_semantics() {
        // Plain subcommand.
        assert_eq!(locate_subcommand(&v(&["db", "--tables"])), Some(0));
        // Global value flags before the verb are skipped WITH their values —
        // an instance literally named "db" is not mistaken for the verb.
        assert_eq!(
            locate_subcommand(&v(&["--instance", "db", "db", "schema"])),
            Some(2)
        );
        assert_eq!(
            locate_subcommand(&v(&["-i", "Friday", "docs", "list"])),
            Some(2)
        );
        // A boolean/unknown flag is skipped without a value.
        assert_eq!(locate_subcommand(&v(&["-o", "db"])), Some(1));
        // First bare token decides: an unknown bare token → launcher path.
        assert_eq!(locate_subcommand(&v(&["frobnicate", "db"])), None);
        // No bare token at all → launcher path.
        assert_eq!(locate_subcommand(&v(&["-p", "8080", "--open"])), None);
        // The value of a global flag is never itself the verb.
        assert_eq!(locate_subcommand(&v(&["--passphrase", "docs"])), None);
    }

    /// The resolution precedence matrix (v4 `resolveDataDirAndPassphrase`).
    /// Env-var branches are exercised via the differential (env mutation is
    /// process-global); here the registry-driven branches run against a temp
    /// registry file.
    #[test]
    fn resolution_precedence_matrix() {
        let dir = tempfile::tempdir().unwrap();
        let reg_path = dir.path().join("instances.json");
        let reg = quilltap_host::instances::InstanceRegistry::at(&reg_path);
        reg.upsert_instance("Friday", "/tmp/friday", "pw").unwrap();
        reg.set_default_instance("Friday").unwrap();

        // Both flags → error.
        let err = resolve::resolve_data_dir_and_passphrase("/x", "Friday", "", &reg).unwrap_err();
        assert_eq!(err, "Specify either --instance or --data-dir, not both.");

        // --instance wins over the default; registry passphrase used unless
        // the flag overrides it.
        let r = resolve::resolve_data_dir_and_passphrase("", "friday", "", &reg).unwrap();
        assert_eq!(r.data_dir, "/tmp/friday/data");
        assert_eq!(r.passphrase, "pw");
        assert!(!r.used_platform_default);
        let r = resolve::resolve_data_dir_and_passphrase("", "friday", "flagpw", &reg).unwrap();
        assert_eq!(r.passphrase, "flagpw");

        // --data-dir beats the registered default (and appends /data).
        let r = resolve::resolve_data_dir_and_passphrase("/mnt/qt", "", "", &reg).unwrap();
        assert_eq!(r.data_dir, "/mnt/qt/data");
        assert!(!r.used_platform_default);

        // No flags → the registered default instance.
        let r = resolve::resolve_data_dir_and_passphrase("", "", "", &reg).unwrap();
        assert_eq!(r.data_dir, "/tmp/friday/data");
        assert_eq!(r.instance_name.as_deref(), Some("Friday"));

        // Unknown instance → v4's error with the known list.
        let err = resolve::resolve_data_dir_and_passphrase("", "Nope", "", &reg).unwrap_err();
        assert_eq!(err, "Unknown instance \"Nope\". Known instances: Friday.");

        // Default cleared → platform default (flag-free, env-free path).
        reg.clear_default_instance().unwrap();
        let r = resolve::resolve_data_dir_and_passphrase("", "", "", &reg).unwrap();
        // Whether QUILLTAP_DATA_DIR is set in the ambient test env decides the
        // flag; assert only the both-empty invariant that holds either way.
        if std::env::var("QUILLTAP_DATA_DIR").is_err() {
            assert!(r.used_platform_default);
        }
        assert!(r.instance_name.is_none());
    }
}
