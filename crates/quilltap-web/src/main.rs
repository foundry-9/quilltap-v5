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

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use quilltap_host::{HostConfig, InstanceRegistry, ProductionSpineFactory};
use quilltap_web::{boot_startup_status, build_router, web_state};

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
                            [--data-dir <path>] [--instance <name>] [--spa-dir <path>]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(args)
}

/// Resolve the instance root: `--data-dir` → `--instance` (the launcher
/// registry) → `QUILLTAP_DATA_DIR` → the platform default (docker-aware).
fn resolve_base_dir(args: &Args) -> Result<PathBuf, String> {
    if let Some(dir) = &args.data_dir {
        return Ok(dir.clone());
    }
    if let Some(name) = &args.instance {
        use quilltap_core::api::InstanceDirectory as _;
        let registry = InstanceRegistry::at_default_location();
        let listed = registry.list().map_err(|e| e.to_string())?;
        let found = listed
            .instances
            .iter()
            .find(|i| &i.name == name)
            .ok_or_else(|| format!("instance '{name}' is not registered"))?;
        return Ok(PathBuf::from(&found.path));
    }
    Ok(quilltap_host::paths::resolve_base_dir(None))
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("quilltap-web: {e}");
            std::process::exit(2);
        }
    };
    let base_dir = match resolve_base_dir(&args) {
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
        let mut config = HostConfig::new(base_dir.clone());
        config.version = version.clone();
        let tz = config.tz.clone();
        config.spine = Some(Arc::new(ProductionSpineFactory::new(
            base_dir.clone(),
            version.clone(),
            tz,
        )));
        let startup = boot_startup_status(config);
        let state = web_state(startup, version, base_dir, args.spa_dir.clone());
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
        if let Err(e) = quilltap_web::serve(router, addr).await {
            eprintln!("quilltap-web: server error: {e}");
            std::process::exit(1);
        }
    });
}
