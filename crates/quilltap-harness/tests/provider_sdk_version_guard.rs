//! The provider-SDK version tripwire (P4.106 item 9 — the gap the P4.D211
//! record named: "no provider-SDK version guard").
//!
//! The recorded provider corpora carry the SDK that built each request in
//! its own headers: `x-stainless-package-version` (the `openai` and
//! `@anthropic-ai/sdk` packages, both Stainless-generated), the OpenRouter
//! user-agent `speakeasy-sdk/typescript <sdk> <gen> <openapi> @openrouter/sdk`,
//! and `x-goog-api-client: google-genai-sdk/<ver> gl-node/<node>`. Those
//! bytes are what v5's request builders and wire reframers are diffed
//! against, so an SDK bump in the v4 checkout is a regen event for every
//! provider family — exactly the obligation `zod_version_guard.rs` makes
//! visible for `zod`. This is that guard's shape, for the four SDKs.
//!
//! Two halves, because the two things can drift independently:
//!
//! 1. **Installed vs expected, per LOCATION.** Each `plugins/dist/*/
//!    node_modules` must carry the RECORDED version — the plugin dirs are
//!    where the recorders resolve every SDK that stamps a recorded request
//!    (each recorder runs FROM `plugins/dist/qtap-plugin-<name>/` and imports
//!    that plugin's source, so its own `node_modules` answers first), and the
//!    pinned-worktree recipe symlinks them from the live checkout, so a pinned
//!    regen cannot prove a bump the plugin dirs never installed (memory
//!    `a-pinned-regen-cannot-prove-an-sdk-bump-the-plugin-dirs-never-
//!    installed`). The ROOT `node_modules` is asserted against the root
//!    LOCK's versions (`INSTALLED_ROOT_*`), not the recorded ones (P4.D260
//!    R-F): since `a9c99a4a0` the root carries newer SDKs than any plugin dir
//!    while resolving NO recorded request, so a root move stays visible — it
//!    is still a regen event to measure — without a false red. Each location
//!    is asserted on its own, and the failure names the location and which
//!    side moved.
//! 2. **Corpora vs recorded.** Every stamp in the three recorded corpora must
//!    equal the constants, and each expected stamp must be PRESENT — so a
//!    re-record without a constant bump is a red, and a constant bump without
//!    a re-record is a red too.
//!
//! Re-measured 2026-10-08 by P4.D260 (the `a9c99a4a0` root dependency move,
//! v4 `f5e953a3f`; the round's §R.4(k) measurement, made BEFORE any constant
//! moved). Installed in the live checkout (the pinned worktree's
//! `node_modules` are symlinks into it): root `openai` **7.30.0** and
//! `@openrouter/sdk` **1.4.25** (moved from 7.23.0 / 1.3.28 by the merge's
//! lockfile); `@anthropic-ai/sdk` and `@google/genai` absent from the root;
//! `qtap-plugin-anthropic` 0.115.0, `qtap-plugin-google` 1.52.0, `openai`
//! 7.23.0 under the six SDK-bundling plugin dirs (`deepseek`, `grok`,
//! `nanogpt`, `openai-compatible`, `openai`, `z-ai`), `@openrouter/sdk` 1.3.28
//! under `qtap-plugin-openrouter` — every plugin dir UNMOVED. Resolution from
//! each plugin dir (Node 24.13.1, `require.resolve(<sdk>, {paths: [<dir>]})`):
//! the six resolve `openai` from their OWN install (7.23.0); the other nine
//! (`anthropic`, `builtin-embeddings`, `curl`, `default-system-prompts`,
//! `google`, `mcp`, `ollama`, `openrouter`, `search-serper`) carry no `openai`
//! and would resolve the ROOT's 7.30.0 — but none of their recorded requests
//! is built through it (no 7.30.0 stamp below); `@openrouter/sdk` resolves
//! the plugin's own 1.3.28 from `qtap-plugin-openrouter` and the root's 1.4.25
//! everywhere else (only openrouter records through it); `@anthropic-ai/sdk`
//! and `@google/genai` resolve only from their own plugin dirs. The eight
//! bundles `b3f937076` rebuilt (the NON-OpenAI plugins, embedding `openai`
//! 7.30.0 via `@quilltap/plugin-utils`) are read by no recorder. Corpus stamps
//! after the re-record from the `f5e953a3f` pin:
//! `request-envelopes.recorded.ndjson` (399 rows) 7.23.0 ×242 (deepseek 30,
//! grok 22, nanogpt 78, openai 32, openai-compatible 44, z-ai 36) + 0.115.0
//! ×46 (anthropic) + the OpenRouter UA ×15 (`1.3.28 2.914.0 1.0.0`) —
//! re-recorded BYTE-IDENTICAL; `image-dialects.recorded.ndjson` (185 rows)
//! 7.23.0 ×8 (openai 4, z-ai 4) + the UA ×3 — byte-identical;
//! `google-wire.recorded.ndjson` (24 rows) `google-genai-sdk/1.52.0` ×24 — the
//! 22 committed rows byte-identical, plus the two `participant-names` rows
//! P4.128 added to the shared recorder without re-recording this corpus.
//! Node runtime stamps: `v24.13.1` ×320 (288 + 8 `x-stainless-runtime-
//! version`, 24 `gl-node/`). (`google_parts.rs` cites `@google/genai@1.52.0`
//! too.)
//!
//! Locator: the shared `common::v4_root` (`QT_V4_CHECKOUT`, then the
//! `QT_V4_ROOT` alias, then `$HOME/source/quilltap-server`). An
//! absent checkout prints a loud `SKIP:`; a moved version is a FAIL.
//!
//! No recipe stage — this reads installed `package.json`s and committed
//! corpora, not an oracle run.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test provider_sdk_version_guard

use std::path::{Path, PathBuf};

// P4.157 R-H: the ONE v4-checkout locator (`common::v4_root`).
mod common;

/// `openai` — the Stainless `x-stainless-package-version` on every
/// non-anthropic OpenAI-shaped request.
const RECORDED_OPENAI_SDK: &str = "7.23.0";
/// `@anthropic-ai/sdk` — the Stainless stamp on `api.anthropic.com` requests.
const RECORDED_ANTHROPIC_SDK: &str = "0.115.0";
/// `@google/genai` — the `x-goog-api-client` `google-genai-sdk/<ver>` token.
const RECORDED_GOOGLE_GENAI_SDK: &str = "1.52.0";
/// `@openrouter/sdk` — the speakeasy user-agent's first version token.
const RECORDED_OPENROUTER_SDK: &str = "1.3.28";
/// The ROOT `node_modules`' `openai` — the root lockfile's version since v4
/// `a9c99a4a0` (P4.D260 R-F). No recorder resolves it for a recorded request
/// (every OpenAI-SDK provider plugin ships its own `RECORDED_OPENAI_SDK`), so
/// it is pinned apart from the recorded constant: a root move is a regen
/// event to MEASURE (does a recorder now resolve the root?), not a re-record.
const INSTALLED_ROOT_OPENAI_SDK: &str = "7.30.0";
/// The ROOT `node_modules`' `@openrouter/sdk` (the root lockfile's, since
/// `a9c99a4a0`); the openrouter plugin records through its own
/// `RECORDED_OPENROUTER_SDK`.
const INSTALLED_ROOT_OPENROUTER_SDK: &str = "1.4.25";
/// The Node runtime every provider corpus is recorded under: the Stainless
/// `x-stainless-runtime-version` and the genai `gl-node/<v>` token (P4.D232).
/// The recipes pin `~/.nvm/versions/node/v24.13.1/bin`; the PATH also carries
/// other Node 24s and Homebrew 26, and a regen under any of them would churn
/// all 290 stamps silently — this constant turns that into a red.
const RECORDED_NODE: &str = "v24.13.1";

/// (package, recorded version) — the four SDKs this guard pins.
const SDKS: [(&str, &str); 4] = [
    ("openai", RECORDED_OPENAI_SDK),
    ("@anthropic-ai/sdk", RECORDED_ANTHROPIC_SDK),
    ("@google/genai", RECORDED_GOOGLE_GENAI_SDK),
    ("@openrouter/sdk", RECORDED_OPENROUTER_SDK),
];

fn package_version(pkg_json: &Path) -> String {
    let text = std::fs::read_to_string(pkg_json)
        .unwrap_or_else(|e| panic!("read {}: {e}", pkg_json.display()));
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", pkg_json.display()));
    value["version"]
        .as_str()
        .unwrap_or_else(|| panic!("no string \"version\" field in {}", pkg_json.display()))
        .to_string()
}

/// The version a LOCATION must carry: the root's lock-pinned SDKs for the
/// root (P4.D260 R-F), the recorded version everywhere else.
fn expected_at(is_root: bool, pkg: &str, recorded: &'static str) -> (&'static str, &'static str) {
    match (is_root, pkg) {
        (true, "openai") => (INSTALLED_ROOT_OPENAI_SDK, "INSTALLED_ROOT_OPENAI_SDK"),
        (true, "@openrouter/sdk") => (
            INSTALLED_ROOT_OPENROUTER_SDK,
            "INSTALLED_ROOT_OPENROUTER_SDK",
        ),
        _ => (recorded, "RECORDED"),
    }
}

/// Every `node_modules` dir the oracle resolves an SDK from: the root, then
/// each `plugins/dist/*/node_modules` (sorted, so a failure list is stable).
fn install_locations(checkout: &Path) -> Vec<PathBuf> {
    let mut out = vec![checkout.join("node_modules")];
    let plugins = checkout.join("plugins/dist");
    if let Ok(rd) = std::fs::read_dir(&plugins) {
        let mut dirs: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path().join("node_modules"))
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        out.extend(dirs);
    }
    out
}

#[test]
fn every_installed_provider_sdk_matches_the_recorded_version() {
    let checkout = common::v4_root().unwrap_or_else(|| PathBuf::from(common::V4_DEFAULT_CHECKOUT));
    if !checkout.is_dir() {
        println!(
            "SKIP: no v4 checkout at {} (set QT_V4_CHECKOUT) — cannot verify the installed \
             provider SDK versions on this machine",
            checkout.display()
        );
        return;
    }
    if !checkout.join("node_modules").is_dir() {
        println!(
            "SKIP: {} has no node_modules (run `npm ci` in the checkout) — cannot verify the \
             installed provider SDK versions",
            checkout.display()
        );
        return;
    }

    // `@anthropic-ai/sdk` and `@google/genai` live ONLY under the plugin dirs'
    // installs (`plugins/dist/*/node_modules`), never the root. A checkout with a
    // root `npm ci` but no plugin installs (a fresh clone, a bare pinned worktree
    // without the third symlink class) can prove nothing about them — SKIP with
    // its own sentence rather than FAIL for a reason that has nothing to do
    // with v5 (the `a2db63da7` unification's §3 review).
    let locations = install_locations(&checkout);
    if locations.len() == 1 {
        println!(
            "SKIP: {} has no plugins/dist/*/node_modules installs (run `npm ci` in each plugin \
             dir, or symlink them into a pinned worktree) — the plugin-only SDKs cannot be \
             verified",
            checkout.display()
        );
        return;
    }

    let mut found: Vec<(&str, usize)> = SDKS.iter().map(|(p, _)| (*p, 0usize)).collect();
    let mut mismatches = Vec::new();
    let root = checkout.join("node_modules");
    for loc in locations {
        let is_root = loc == root;
        for (i, (pkg, recorded)) in SDKS.iter().enumerate() {
            let pkg_json = loc.join(pkg).join("package.json");
            if !pkg_json.is_file() {
                continue;
            }
            found[i].1 += 1;
            let installed = package_version(&pkg_json);
            let (expected, constant) = expected_at(is_root, pkg, recorded);
            if installed != expected {
                let side = if is_root {
                    "the ROOT install moved (re-measure whether any recorder now resolves \
                     the root before touching a RECORDED_* constant)"
                } else {
                    "a PLUGIN install moved (the recorders resolve this copy — re-record)"
                };
                mismatches.push(format!(
                    "  {pkg}: expected {expected} ({constant}), installed {installed} at {} — {side}",
                    pkg_json.display()
                ));
            }
        }
    }

    // Each SDK must be installed SOMEWHERE — an SDK that vanished from every
    // location is as much a regen event as one that moved.
    for (pkg, n) in &found {
        assert!(
            *n > 0,
            "`{pkg}` is installed in NO location under {} (root or plugins/dist/*) — the \
             provider corpora record requests built by it; re-measure before trusting them",
            checkout.display()
        );
    }
    assert!(
        mismatches.is_empty(),
        "a provider SDK moved in the v4 checkout:\n{}\n\n\
         The recorded provider corpora (`request-envelopes.recorded.ndjson`, \
         `google-wire.recorded.ndjson`, `image-dialects.recorded.ndjson`, and every family \
         that replays them) carry the recorded SDK's header bytes, so this is a regen event: \
         re-record the corpora through the pinned recorders with the plugin dirs INSTALLED at \
         the new version (a pinned worktree's plugin node_modules are symlinks into the live \
         checkout — prove the bump reached them), re-run every provider family, then bump the \
         RECORDED_* constant here. The corpora half of this guard fails until both move. \
         A ROOT-only move is a measurement first (the P4.D260 §R.4(k) shape): record which \
         copy each plugin dir resolves (`require.resolve(<sdk>, {{paths: [<dir>]}})`), re-record \
         to prove no stamp moved, then move INSTALLED_ROOT_* alone.",
        mismatches.join("\n")
    );
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

#[derive(Default)]
struct Stamps {
    openai: Vec<String>,
    anthropic: Vec<String>,
    openrouter: Vec<String>,
    google: Vec<String>,
    node: Vec<String>,
}

/// Walk a recorded row; every object carrying a `headers` object is one
/// recorded request (its sibling `url` attributes the Stainless stamp).
fn collect(v: &serde_json::Value, stamps: &mut Stamps) {
    match v {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::Object(h)) = map.get("headers") {
                let url = map.get("url").and_then(|u| u.as_str()).unwrap_or("");
                if let Some(ver) = h
                    .get("x-stainless-package-version")
                    .and_then(|x| x.as_str())
                {
                    if url.contains("anthropic.com") {
                        stamps.anthropic.push(ver.to_string());
                    } else {
                        stamps.openai.push(ver.to_string());
                    }
                }
                if let Some(rt) = h
                    .get("x-stainless-runtime-version")
                    .and_then(|x| x.as_str())
                {
                    stamps.node.push(rt.to_string());
                }
                if let Some(ua) = h.get("user-agent").and_then(|x| x.as_str()) {
                    if let Some(rest) = ua.strip_prefix("speakeasy-sdk/typescript ") {
                        if ua.ends_with("@openrouter/sdk") {
                            let ver = rest.split(' ').next().unwrap_or("");
                            stamps.openrouter.push(ver.to_string());
                        }
                    }
                }
                if let Some(g) = h.get("x-goog-api-client").and_then(|x| x.as_str()) {
                    for tok in g.split(' ') {
                        if let Some(ver) = tok.strip_prefix("google-genai-sdk/") {
                            stamps.google.push(ver.to_string());
                        }
                        if let Some(ver) = tok.strip_prefix("gl-node/") {
                            stamps.node.push(ver.to_string());
                        }
                    }
                }
            }
            for child in map.values() {
                collect(child, stamps);
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|c| collect(c, stamps)),
        _ => {}
    }
}

fn corpus_stamps(rel: &str) -> Stamps {
    let path = fixtures_dir().join(rel);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut stamps = Stamps::default();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("{}:{}: {e}", path.display(), i + 1));
        collect(&row, &mut stamps);
    }
    stamps
}

/// Every stamp equals `recorded`, and the SDK is PRESENT in the corpus iff
/// `present` (a presence check, not a count — the corpora grow every round).
fn assert_all(corpus: &str, sdk: &str, stamps: &[String], recorded: &str, present: bool) {
    let off: Vec<&String> = stamps.iter().filter(|s| s.as_str() != recorded).collect();
    assert!(
        off.is_empty(),
        "{corpus}: {} `{sdk}` stamp(s) are not the recorded {recorded}: {off:?} — the corpus was \
         re-recorded under a different SDK without RECORDED_* moving (or the constant moved \
         without a re-record)",
        off.len()
    );
    assert_eq!(
        !stamps.is_empty(),
        present,
        "{corpus}: `{sdk}` stamps present = {} but this guard expects {present} — the corpus's \
         provider coverage moved; re-measure the stamps and update this guard with the re-record",
        !stamps.is_empty()
    );
}

#[test]
fn the_recorded_corpora_carry_exactly_the_recorded_sdk_versions() {
    let env = corpus_stamps("request-envelopes/request-envelopes.recorded.ndjson");
    assert_all(
        "request-envelopes",
        "openai",
        &env.openai,
        RECORDED_OPENAI_SDK,
        true,
    );
    assert_all(
        "request-envelopes",
        "@anthropic-ai/sdk",
        &env.anthropic,
        RECORDED_ANTHROPIC_SDK,
        true,
    );
    assert_all(
        "request-envelopes",
        "@openrouter/sdk",
        &env.openrouter,
        RECORDED_OPENROUTER_SDK,
        true,
    );
    assert_all(
        "request-envelopes",
        "@google/genai",
        &env.google,
        RECORDED_GOOGLE_GENAI_SDK,
        false,
    );

    let img = corpus_stamps("image-dialects/image-dialects.recorded.ndjson");
    assert_all(
        "image-dialects",
        "openai",
        &img.openai,
        RECORDED_OPENAI_SDK,
        true,
    );
    assert_all(
        "image-dialects",
        "@anthropic-ai/sdk",
        &img.anthropic,
        RECORDED_ANTHROPIC_SDK,
        false,
    );
    assert_all(
        "image-dialects",
        "@openrouter/sdk",
        &img.openrouter,
        RECORDED_OPENROUTER_SDK,
        true,
    );
    assert_all(
        "image-dialects",
        "@google/genai",
        &img.google,
        RECORDED_GOOGLE_GENAI_SDK,
        false,
    );

    let goog = corpus_stamps("request-envelopes/google-wire.recorded.ndjson");
    assert_all(
        "google-wire",
        "@google/genai",
        &goog.google,
        RECORDED_GOOGLE_GENAI_SDK,
        true,
    );
    assert_all(
        "google-wire",
        "openai",
        &goog.openai,
        RECORDED_OPENAI_SDK,
        false,
    );
}

/// Its own test so the designed `image-dialects` SDK red on P4.D232's branch
/// (see the module header) cannot mask a Node-runtime drift.
#[test]
fn the_recorded_corpora_carry_the_recorded_node_runtime() {
    for corpus in [
        "request-envelopes/request-envelopes.recorded.ndjson",
        "image-dialects/image-dialects.recorded.ndjson",
        "request-envelopes/google-wire.recorded.ndjson",
    ] {
        let stamps = corpus_stamps(corpus);
        assert_all(corpus, "node runtime", &stamps.node, RECORDED_NODE, true);
    }
}
