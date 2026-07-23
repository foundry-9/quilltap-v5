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

use std::net::SocketAddr;
use std::path::PathBuf;

use quilltap_web::{
    boot_startup_status, build_router, production_host_config, resolve_instance_base_dir, web_state,
};

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
                       QUILLTAP_SPA_DIR   the Angular dist, when --spa-dir is absent\n\n\
                     Absent both, the dist is looked for beside the binary (./spa) and then\n\
                     at ../share/quilltap/spa; absent that, only placeholder pages are served."
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(args)
}

fn main() {
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
