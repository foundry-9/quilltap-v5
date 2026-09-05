//! The container host-gateway resolver — a whole port of v4's
//! `lib/host-rewrite.ts` (`isVMEnvironment`, `resolveHostGateway`,
//! `rewriteLocalhostUrl`).
//!
//! When Quilltap runs inside Docker — or inside a virtual machine the user has
//! built and manages themselves — `localhost` and `127.0.0.1` resolve to the
//! container/VM's own loopback, not the host machine where services like Ollama
//! or LM Studio are running. A user who configures `http://localhost:11434`
//! expects it to Just Work; v4 makes it so by rewriting the URL to the host
//! gateway at every provider construction site, and P4.71 brings that here.
//!
//! ## Where the halves live
//!
//! The URL rewrite itself is **pure** and already lived in the core
//! ([`quilltap_core::provider_manifest::rewrite_localhost_url`], P4.D134,
//! corpus-pinned). What was missing — and what this module adds — is the
//! impure half: the environment probe and the gateway ladder, which read the
//! process environment and the filesystem and therefore belong to the host
//! tier. The host resolves once per process and injects the answer into every
//! provider it constructs.
//!
//! ## The ladder (v4 `resolveHostGateway`, since `1560bd43b`)
//!
//! 1. `QUILLTAP_HOST_IP` — the explicit override, and ALSO the only supported
//!    route for a self-managed VM, which Quilltap has no reliable way to detect
//!    on its own.
//! 2. In Docker: `host.docker.internal`.
//! 3. Otherwise give up gracefully — the URL is returned unchanged.
//!
//! Docker Desktop (macOS/Windows) provides built-in DNS resolution for
//! `host.docker.internal` via its DNS server (127.0.0.11). Linux Docker needs
//! `--add-host=host.docker.internal:host-gateway` (v4's start scripts add it;
//! v5's `docs/developer/running.md` documents it on every `docker run` line).
//! Either way `host.docker.internal` correctly forwards to services bound to
//! the host's loopback (127.0.0.1).
//!
//! **The bridge gateway IP (e.g. `172.17.0.1`) is deliberately NOT used as a
//! fallback:** it is only the Docker bridge interface, and services listening
//! on the host's own localhost are NOT reachable through it. Carrying v4's
//! comment because the omission looks like an oversight and is not — v4 tried
//! it (the deleted `/proc/net/route` strategy) and it was actively wrong.
//!
//! ## Order matters for the log lines
//!
//! v4's `rewriteLocalhostUrl` returns early three times before it ever
//! resolves: not a VM environment, unparseable URL, non-localhost host. Only a
//! localhost URL in a rewriting environment reaches `resolveHostGateway()` and
//! therefore emits a resolution line. [`rewrite_localhost_url_logged`]
//! reproduces that order exactly — which is why the core exposes
//! [`is_localhost_url`](quilltap_core::provider_manifest::is_localhost_url).

use quilltap_core::provider_manifest::{is_localhost_url, rewrite_localhost_url};
use std::sync::OnceLock;

/// v4's env var: the explicit gateway override / the VM opt-in.
const HOST_IP_ENV: &str = "QUILLTAP_HOST_IP";

/// The gateway hostname v4 uses inside Docker.
const DOCKER_GATEWAY_HOST: &str = "host.docker.internal";

// ── v4's four log messages, byte-exact (note the em dashes). They are
// constants so the differential's expected-log rendering and the production
// emission below cannot drift apart. ──

/// v4 `rewriteLogger.info('Host gateway from QUILLTAP_HOST_IP', { host })`.
pub const LOG_ENV_OVERRIDE: &str = "Host gateway from QUILLTAP_HOST_IP";
/// v4 `rewriteLogger.info('Docker environment detected — …')`.
pub const LOG_DOCKER: &str =
    "Docker environment detected — using host.docker.internal as gateway hostname";
/// v4 `rewriteLogger.warn('Could not resolve host gateway — …')`.
pub const LOG_UNRESOLVED: &str =
    "Could not resolve host gateway — localhost URLs will not be rewritten";
/// v4 `rewriteLogger.debug('Rewrote localhost URL', { original, rewritten, gatewayHost })`.
pub const LOG_REWROTE: &str = "Rewrote localhost URL";

/// v4's `rewriteLogger = logger.child({ module: 'host-rewrite' })` — carried as
/// a field on every event this module emits.
const LOG_MODULE: &str = "host-rewrite";

/// One line v4's logger emits, with its fields. Returned by the pure ladder so
/// the differential can compare v4's emissions field-for-field without a
/// subscriber, and rendered to `tracing` by the process resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayLog {
    /// `info` — strategy 1 won. Carries v4's `{ host }` field.
    EnvOverride { host: String },
    /// `info` — strategy 2 won.
    Docker,
    /// `warn` — neither strategy applied.
    Unresolved,
    /// `debug` — a URL was rewritten. Carries v4's three fields.
    Rewrote {
        original: String,
        rewritten: String,
        gateway_host: String,
    },
}

impl GatewayLog {
    /// The message text, for the differential's comparand and the assertions.
    pub fn message(&self) -> &'static str {
        match self {
            GatewayLog::EnvOverride { .. } => LOG_ENV_OVERRIDE,
            GatewayLog::Docker => LOG_DOCKER,
            GatewayLog::Unresolved => LOG_UNRESOLVED,
            GatewayLog::Rewrote { .. } => LOG_REWROTE,
        }
    }

    /// v4's level word for this line (`info` / `warn` / `debug`).
    pub fn level(&self) -> &'static str {
        match self {
            GatewayLog::EnvOverride { .. } | GatewayLog::Docker => "info",
            GatewayLog::Unresolved => "warn",
            GatewayLog::Rewrote { .. } => "debug",
        }
    }
}

// ============================================================================
// The pure halves (the differential's comparands)
// ============================================================================

/// v4 `isVMEnvironment()` (`lib/host-rewrite.ts:54`) with the environment
/// injected: `isDockerEnvironment() || !!process.env.QUILLTAP_HOST_IP`.
///
/// ⚠ JS `!!` — an EMPTY `QUILLTAP_HOST_IP` is falsy, i.e. "unset". Rust's
/// `std::env::var` hands back `Ok("")` for an empty var, so `Some("")` has to
/// read as unset here or a `QUILLTAP_HOST_IP=` in a compose file would flip
/// rewriting on outside Docker and then resolve to nothing.
pub fn is_vm_environment_pure(host_ip: Option<&str>, is_docker: bool) -> bool {
    is_docker || host_ip.is_some_and(|v| !v.is_empty())
}

/// v4 `resolveHostGateway()` (`:64-97`) minus the cache — the strategy ladder
/// itself, plus the line v4's logger emits for the branch taken.
pub fn resolve_host_gateway_pure(
    host_ip: Option<&str>,
    is_docker: bool,
) -> (Option<String>, Vec<GatewayLog>) {
    // Strategy 1: explicit env var override (v4's `if (envIP)` — truthy, so an
    // empty string falls through exactly as it does in `isVMEnvironment`).
    if let Some(ip) = host_ip.filter(|v| !v.is_empty()) {
        return (
            Some(ip.to_string()),
            vec![GatewayLog::EnvOverride {
                host: ip.to_string(),
            }],
        );
    }

    // Strategy 2: Docker — `host.docker.internal` as a hostname (see the module
    // header for why the bridge IP is not a fallback).
    if is_docker {
        return (
            Some(DOCKER_GATEWAY_HOST.to_string()),
            vec![GatewayLog::Docker],
        );
    }

    (None, vec![GatewayLog::Unresolved])
}

/// v4 `rewriteLocalhostUrl(url)` (`:114-151`) with the environment injected and
/// no cache — the whole function, in v4's order, returning the URL plus every
/// line v4 would emit for this call.
///
/// This is the differential's top-level comparand: one call reproduces v4 on a
/// freshly-reset module (which is how the oracle drives each row).
pub fn rewrite_localhost_url_pure(
    host_ip: Option<&str>,
    is_docker: bool,
    url: &str,
) -> (String, Vec<GatewayLog>) {
    // No-op on bare metal — before any parsing, and before any resolution.
    if !is_vm_environment_pure(host_ip, is_docker) {
        return (url.to_string(), Vec::new());
    }

    // Unparseable, or not a localhost host → unchanged, still without
    // resolving (v4's two early returns).
    if !is_localhost_url(url) {
        return (url.to_string(), Vec::new());
    }

    let (gateway, mut logs) = resolve_host_gateway_pure(host_ip, is_docker);
    let Some(gateway) = gateway else {
        return (url.to_string(), logs);
    };

    let rewritten = rewrite_localhost_url(url, Some(&gateway));
    logs.push(GatewayLog::Rewrote {
        original: url.to_string(),
        rewritten: rewritten.clone(),
        gateway_host: gateway,
    });
    (rewritten, logs)
}

// ============================================================================
// The process resolver (v4's module-level cache + the real logger)
// ============================================================================

/// v4's `cachedGatewayHost` — resolved once per process lifetime. v4's
/// `undefined` is this `OnceLock` being empty; v4's `null` is a stored `None`.
static CACHED_GATEWAY: OnceLock<Option<String>> = OnceLock::new();

/// `QUILLTAP_HOST_IP` as v4 reads it (an unset var and an empty one are the
/// same thing to the ladder above, which filters).
fn env_host_ip() -> Option<String> {
    std::env::var(HOST_IP_ENV).ok()
}

/// Emit one of v4's lines through `tracing`, carrying v4's child-logger
/// `module` field and its per-line fields under v4's own key names.
fn emit(line: &GatewayLog) {
    match line {
        GatewayLog::EnvOverride { host } => {
            tracing::info!(module = LOG_MODULE, host = %host, "{LOG_ENV_OVERRIDE}")
        }
        GatewayLog::Docker => tracing::info!(module = LOG_MODULE, "{LOG_DOCKER}"),
        GatewayLog::Unresolved => tracing::warn!(module = LOG_MODULE, "{LOG_UNRESOLVED}"),
        GatewayLog::Rewrote {
            original,
            rewritten,
            gateway_host,
        } => tracing::debug!(
            module = LOG_MODULE,
            original = %original,
            rewritten = %rewritten,
            gatewayHost = %gateway_host,
            "{LOG_REWROTE}"
        ),
    }
}

/// v4 `isVMEnvironment()` against the live process environment.
pub fn is_vm_environment() -> bool {
    is_vm_environment_pure(
        env_host_ip().as_deref(),
        crate::env::is_docker_environment(),
    )
}

/// v4 `resolveHostGateway()` against the live process environment, cached for
/// the life of the process as v4 caches (so the filesystem probes and the
/// resolution log lines happen exactly once).
///
/// `None` = "could not resolve" — callers pass it straight to
/// [`rewrite_localhost_url`], whose `None` arm is the same no-op.
pub fn resolve_host_gateway() -> Option<String> {
    CACHED_GATEWAY
        .get_or_init(|| {
            let (gateway, logs) = resolve_host_gateway_pure(
                env_host_ip().as_deref(),
                crate::env::is_docker_environment(),
            );
            for line in &logs {
                emit(line);
            }
            gateway
        })
        .clone()
}

/// The gateway to INJECT into a provider under construction — v4's
/// gate-then-resolve order, hoisted out of the per-URL rewrite.
///
/// v5 resolves once and injects an `Option<String>` that the core's pure
/// rewrite then applies per request; v4 resolves lazily inside each rewrite.
/// The order still has to match, and this is where: **`is_vm_environment()`
/// first.** On bare metal v4 returns from `rewriteLocalhostUrl` before it ever
/// calls `resolveHostGateway()`, so it never emits the "Could not resolve"
/// warn. A construction site that called [`resolve_host_gateway`] directly
/// would emit that line once per process on every desktop install — a log v4
/// never writes.
///
/// ⚠ **Measured (P4.71): `LOG_UNRESOLVED` is UNREACHABLE in v4.**
/// `resolveHostGateway` is module-private and has exactly one caller, sitting
/// behind the `isVMEnvironment()` gate — and `isVMEnvironment()` is true
/// exactly when one of the two strategies will succeed. So the warn can only
/// fire for a caller that skips the gate, which in `lib/` there is none of.
/// (The separate `packages/plugin-utils` copy of this module *exports*
/// `resolveHostGateway`, so a plugin could reach it; nothing in v5 does.) The
/// branch is carried anyway because v4 carries it — a port that "helpfully"
/// deleted the dead arm would diverge the moment v4's gate changed.
pub fn resolve_injected_gateway() -> Option<String> {
    if !is_vm_environment() {
        return None;
    }
    resolve_host_gateway()
}

/// v4 `rewriteLocalhostUrl(url)` against the live process environment — the
/// logged wrapper, kept here so the core's rewrite stays pure.
///
/// Provider construction sites do NOT call this: they inject
/// [`resolve_host_gateway`]'s answer once and let the core rewrite per request
/// (that is what `with_localhost_gateway` is for). This is the direct
/// equivalent for a caller holding a bare URL.
pub fn rewrite_localhost_url_logged(url: &str) -> String {
    if !is_vm_environment() {
        return url.to_string();
    }
    // v4 checks the host BEFORE resolving — a non-localhost URL must not emit a
    // resolution line (module header, "Order matters").
    if !is_localhost_url(url) {
        return url.to_string();
    }
    let Some(gateway) = resolve_host_gateway() else {
        return url.to_string();
    };
    let rewritten = rewrite_localhost_url(url, Some(&gateway));
    emit(&GatewayLog::Rewrote {
        original: url.to_string(),
        rewritten: rewritten.clone(),
        gateway_host: gateway,
    });
    rewritten
}

#[cfg(test)]
mod tests {
    use super::*;
    // The capture rig is `quilltap_core::test_support` (P4.77 — the shared
    // consolidation of what was the `title_verdict.rs` / `cheap_llm_exec.rs`
    // idiom copy-pasted here). Reached across the crate boundary via the
    // `test-support` feature, enabled only under `[dev-dependencies]`.
    use quilltap_core::test_support::captured;

    // ── the ladder ──

    #[test]
    fn env_override_wins_over_docker() {
        let (gw, logs) = resolve_host_gateway_pure(Some("10.0.0.5"), true);
        assert_eq!(gw.as_deref(), Some("10.0.0.5"));
        assert_eq!(
            logs,
            vec![GatewayLog::EnvOverride {
                host: "10.0.0.5".to_string()
            }]
        );
    }

    #[test]
    fn docker_resolves_to_the_internal_hostname() {
        let (gw, logs) = resolve_host_gateway_pure(None, true);
        assert_eq!(gw.as_deref(), Some("host.docker.internal"));
        assert_eq!(logs, vec![GatewayLog::Docker]);
    }

    #[test]
    fn bare_metal_resolves_to_nothing() {
        let (gw, logs) = resolve_host_gateway_pure(None, false);
        assert_eq!(gw, None);
        assert_eq!(logs, vec![GatewayLog::Unresolved]);
    }

    #[test]
    fn an_empty_env_var_is_unset() {
        // v4's `!!process.env.QUILLTAP_HOST_IP` and `if (envIP)`.
        assert!(!is_vm_environment_pure(Some(""), false));
        let (gw, logs) = resolve_host_gateway_pure(Some(""), false);
        assert_eq!(gw, None);
        assert_eq!(logs, vec![GatewayLog::Unresolved]);
        // …but inside Docker an empty var changes nothing: Docker still wins.
        assert!(is_vm_environment_pure(Some(""), true));
        assert_eq!(
            resolve_host_gateway_pure(Some(""), true).0.as_deref(),
            Some("host.docker.internal")
        );
    }

    // ── the order of the early returns (what keeps the log quiet) ──

    #[test]
    fn a_non_localhost_url_never_resolves() {
        // v4 returns before `resolveHostGateway()`, so NO line is emitted even
        // though the environment would have resolved happily.
        let (out, logs) =
            rewrite_localhost_url_pure(Some("10.0.0.5"), false, "https://api.openai.com/v1");
        assert_eq!(out, "https://api.openai.com/v1");
        assert!(logs.is_empty(), "expected silence, got {logs:?}");
    }

    #[test]
    fn an_unparseable_url_never_resolves() {
        let (out, logs) = rewrite_localhost_url_pure(Some("10.0.0.5"), false, "not a url");
        assert_eq!(out, "not a url");
        assert!(logs.is_empty(), "expected silence, got {logs:?}");
    }

    #[test]
    fn bare_metal_is_silent_even_for_a_localhost_url() {
        // The environment gate is first: v4 does not even parse.
        let (out, logs) = rewrite_localhost_url_pure(None, false, "http://localhost:11434");
        assert_eq!(out, "http://localhost:11434");
        assert!(logs.is_empty(), "expected silence, got {logs:?}");
    }

    #[test]
    fn a_localhost_url_rewrites_and_logs_both_lines() {
        let (out, logs) = rewrite_localhost_url_pure(None, true, "http://localhost:11434");
        assert_eq!(out, "http://host.docker.internal:11434/");
        assert_eq!(
            logs,
            vec![
                GatewayLog::Docker,
                GatewayLog::Rewrote {
                    original: "http://localhost:11434".to_string(),
                    rewritten: "http://host.docker.internal:11434/".to_string(),
                    gateway_host: "host.docker.internal".to_string(),
                }
            ]
        );
    }

    // ── the message bytes (v4's four strings, em dashes included) ──

    #[test]
    fn the_four_messages_are_v4s_bytes() {
        assert_eq!(LOG_ENV_OVERRIDE, "Host gateway from QUILLTAP_HOST_IP");
        assert_eq!(
            LOG_DOCKER,
            "Docker environment detected \u{2014} using host.docker.internal as gateway hostname"
        );
        assert_eq!(
            LOG_UNRESOLVED,
            "Could not resolve host gateway \u{2014} localhost URLs will not be rewritten"
        );
        assert_eq!(LOG_REWROTE, "Rewrote localhost URL");
    }

    #[test]
    fn the_levels_match_v4s_logger_calls() {
        assert_eq!(
            GatewayLog::EnvOverride {
                host: "h".to_string()
            }
            .level(),
            "info"
        );
        assert_eq!(GatewayLog::Docker.level(), "info");
        assert_eq!(GatewayLog::Unresolved.level(), "warn");
        assert_eq!(
            GatewayLog::Rewrote {
                original: "a".to_string(),
                rewritten: "b".to_string(),
                gateway_host: "g".to_string(),
            }
            .level(),
            "debug"
        );
    }

    // ── the WIRING pin: the constants above are only worth anything if the
    // production emitter actually puts them on the wire (the P4.D108 lesson —
    // a seam without its wire is a no-op). ──

    #[test]
    fn the_emitter_carries_v4s_message_and_fields() {
        let lines = captured(|| {
            emit(&GatewayLog::EnvOverride {
                host: "10.0.0.5".to_string(),
            });
            emit(&GatewayLog::Docker);
            emit(&GatewayLog::Unresolved);
            emit(&GatewayLog::Rewrote {
                original: "http://localhost:11434".to_string(),
                rewritten: "http://gw.test:11434/".to_string(),
                gateway_host: "gw.test".to_string(),
            });
        });
        assert_eq!(lines.len(), 4, "{lines:?}");

        assert!(
            lines[0].starts_with("INFO quilltap_host::host_gateway"),
            "{lines:?}"
        );
        assert!(lines[0].contains(LOG_ENV_OVERRIDE), "{lines:?}");
        assert!(lines[0].contains("module=host-rewrite"), "{lines:?}");
        assert!(lines[0].contains("host=10.0.0.5"), "{lines:?}");

        assert!(lines[1].starts_with("INFO "), "{lines:?}");
        assert!(lines[1].contains(LOG_DOCKER), "{lines:?}");

        assert!(lines[2].starts_with("WARN "), "{lines:?}");
        assert!(lines[2].contains(LOG_UNRESOLVED), "{lines:?}");

        assert!(lines[3].starts_with("DEBUG "), "{lines:?}");
        assert!(lines[3].contains(LOG_REWROTE), "{lines:?}");
        assert!(
            lines[3].contains("original=http://localhost:11434"),
            "{lines:?}"
        );
        assert!(
            lines[3].contains("rewritten=http://gw.test:11434/"),
            "{lines:?}"
        );
        // v4's field name is camelCase on the child logger's bag.
        assert!(lines[3].contains("gatewayHost=gw.test"), "{lines:?}");
    }

    // ── the process cache: v4 resolves (and logs) once per process ──

    #[test]
    fn the_process_resolver_caches_and_logs_once() {
        // Two calls; whichever branch this machine's environment takes, the
        // resolution line appears at most once and the answers agree.
        let first = captured(|| {
            resolve_host_gateway();
        });
        let first_answer = resolve_host_gateway();
        let second = captured(|| {
            resolve_host_gateway();
        });
        assert_eq!(first_answer, resolve_host_gateway());
        assert!(
            second.is_empty(),
            "the cached resolver must not re-log: {second:?}"
        );
        // The first observed call logs one resolution line — unless another
        // test in this binary already warmed the OnceLock, in which case it is
        // silent too. Both are the cache working; what must never happen is a
        // line on a LATER call.
        assert!(first.len() <= 1, "{first:?}");
    }

    #[test]
    fn the_injected_gateway_is_gated_and_silent_on_bare_metal() {
        // The whole point of `resolve_injected_gateway`: on a machine that is
        // not a VM/container it answers None WITHOUT resolving, so v4's
        // unreachable warn stays unreachable here too.
        if is_vm_environment() {
            // A container/VM dev box: the gate is open, so the assertion this
            // test can make is that the gated and ungated answers agree.
            assert_eq!(resolve_injected_gateway(), resolve_host_gateway());
            return;
        }
        let lines = captured(|| {
            assert_eq!(resolve_injected_gateway(), None);
        });
        assert!(
            lines.is_empty(),
            "bare metal must resolve nothing and log nothing, got {lines:?}"
        );
    }

    // ── the WIRING CENSUS (P4.71) ────────────────────────────────────────
    //
    // The injections are a chokepoint cutover: no differential can see them
    // (the memory note `a-chokepoint-cutover-is-differential-invisible`), and
    // the constructors they live in build a real `reqwest` client, so a
    // behavioural test would need the network. The behaviour — "a provider
    // carrying gateway G rewrites a localhost base URL to G" — is pinned in the
    // core beside each seam. What is pinned HERE is that the host actually
    // hands the resolved gateway to each one, and that a NEW construction site
    // cannot be added without one. Reverting any single injection reds exactly
    // its own arm below.

    const PROVIDERS_RS: &str = include_str!("providers.rs");
    const SPINE_RS: &str = include_str!("spine.rs");
    /// The host files that are NOT construction homes — a wire provider built
    /// in any of them would bypass the injection and the two censuses above
    /// (the §3 unification review: the census read `spine.rs` and
    /// `providers.rs` alone).
    const OTHER_HOST_FILES: &[(&str, &str)] = &[
        ("host.rs", include_str!("host.rs")),
        ("avatar_preview.rs", include_str!("avatar_preview.rs")),
        ("almanack_services.rs", include_str!("almanack_services.rs")),
        ("lib.rs", include_str!("lib.rs")),
    ];

    /// Count non-comment occurrences of `needle` (a `//`-led line is prose, and
    /// several of this module's own comments name these very symbols).
    fn code_hits(haystack: &str, needle: &str) -> usize {
        haystack
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .filter(|l| l.contains(needle))
            .count()
    }

    /// The ~12 lines following `marker` — enough to see a builder chain, so an
    /// arm can assert about ONE construction site rather than a whole file.
    fn snippet(haystack: &str, marker: &str) -> String {
        let at = haystack
            .find(marker)
            .unwrap_or_else(|| panic!("the census marker moved: {marker}"));
        haystack[at..]
            .lines()
            .take(12)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_bundle_resolves_the_gateway_once() {
        assert_eq!(
            code_hits(
                PROVIDERS_RS,
                "crate::host_gateway::resolve_injected_gateway()"
            ),
            1,
            "ProviderIo must resolve the gateway exactly once, in with_base_url_env"
        );
    }

    #[test]
    fn every_streaming_construction_injects() {
        let news = code_hits(PROVIDERS_RS, "WireStreamingProvider::new(");
        assert_eq!(news, 1, "a new streaming construction site appeared");
        assert_eq!(
            code_hits(
                PROVIDERS_RS,
                ".with_localhost_gateway(self.localhost_gateway.clone())"
            ),
            news,
            "the streaming provider is built without the host gateway"
        );
    }

    /// No wire provider is constructed outside the two census files: a site
    /// in `host.rs` (where the image provider is built) or a new module would
    /// otherwise slip past every arm above.
    #[test]
    fn no_wire_provider_is_built_outside_the_census_files() {
        for (name, text) in OTHER_HOST_FILES {
            for needle in [
                "ApiEmbeddingProvider::new(",
                "WireStreamingProvider::new(",
                "WireCompletionProvider::new(",
            ] {
                assert_eq!(
                    code_hits(text, needle),
                    0,
                    "{name} constructs {needle} outside the injection homes — \
                     route it through WireConfig / the provider bundle"
                );
            }
        }
    }

    #[test]
    fn every_completion_construction_injects() {
        // One construction (WireConfig::completion) — the factory every job
        // handler and the spine bundle go through.
        assert_eq!(
            code_hits(SPINE_RS, "WireCompletionProvider::new("),
            2,
            "a new completion construction site appeared (expected exactly \
             WireConfig::completion and the `profile_timeout_tests` \
             construction in spine.rs's test module)"
        );
        assert!(
            snippet(SPINE_RS, "fn completion(&self, db: &Db)")
                .contains(".with_localhost_gateway(self.localhost_gateway.clone())"),
            "WireConfig::completion must inject the host gateway"
        );
        assert_eq!(
            code_hits(SPINE_RS, "localhost_gateway: io.localhost_gateway()"),
            1,
            "WireConfig::from_io must carry the bundle's resolved gateway"
        );
        // The send path must actually hand it to the core composition.
        assert_eq!(
            code_hits(SPINE_RS, "self.localhost_gateway.as_deref(),"),
            1,
            "the completion send must pass the gateway into execute_completion"
        );
    }

    #[test]
    fn every_embedding_construction_goes_through_the_one_home() {
        // `ApiEmbeddingProvider::new` may appear ONCE — inside
        // `WireConfig::embedding`, which injects. Any other call site would be
        // an embedding provider built without the gateway (there were six
        // before P4.71).
        assert_eq!(
            code_hits(SPINE_RS, "ApiEmbeddingProvider::new("),
            1,
            "an embedding provider is being constructed outside WireConfig::embedding"
        );
        assert!(
            snippet(SPINE_RS, "fn embedding(&self, db: &Db)")
                .contains(".with_localhost_gateway(self.localhost_gateway.clone())"),
            "WireConfig::embedding must inject the host gateway"
        );
    }

    #[test]
    fn the_provider_actions_driver_carries_the_gateway() {
        assert_eq!(
            code_hits(SPINE_RS, "localhost_gateway: self.io.localhost_gateway()"),
            1,
            "RealProviderActions must carry the host gateway (v4 rewrites in the \
             registry for validateApiKey and getAvailableModels)"
        );
    }

    #[test]
    fn the_logged_wrapper_agrees_with_the_pure_one() {
        // The wrapper reads the live environment; whatever that is, it must
        // answer exactly what the pure function answers for the same inputs.
        let host_ip = env_host_ip();
        let is_docker = crate::env::is_docker_environment();
        for url in [
            "http://localhost:11434",
            "http://127.0.0.1:1234/v1",
            "https://api.openai.com/v1",
            "not a url",
        ] {
            let (expected, _) = rewrite_localhost_url_pure(host_ip.as_deref(), is_docker, url);
            assert_eq!(rewrite_localhost_url_logged(url), expected, "url={url}");
        }
    }
}
