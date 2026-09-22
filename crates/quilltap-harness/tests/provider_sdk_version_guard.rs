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
//! 1. **Installed vs recorded.** Every INSTALLED location in the checkout —
//!    the root `node_modules` AND each `plugins/dist/*/node_modules` — must
//!    carry the recorded version. The plugin dirs are separate installs (the
//!    anthropic and google SDKs live ONLY there, absent from the root), and
//!    the pinned-worktree recipe symlinks them from the live checkout, so a
//!    pinned regen cannot prove a bump the plugin dirs never installed
//!    (memory `a-pinned-regen-cannot-prove-an-sdk-bump-the-plugin-dirs-never-
//!    installed`). Each location is asserted on its own.
//! 2. **Corpora vs recorded.** Every stamp in the three recorded corpora must
//!    equal the constants, and each expected stamp must be PRESENT — so a
//!    re-record without a constant bump is a red, and a constant bump without
//!    a re-record is a red too.
//!
//! Measured 2026-09-22 from the `f45a517a9` pin (P4.106): root `openai`
//! 7.20.0 and `@openrouter/sdk` 1.3.11; `@anthropic-ai/sdk` and
//! `@google/genai` absent from the root; `qtap-plugin-anthropic` 0.115.0,
//! `qtap-plugin-google` 1.52.0, `openai` 7.20.0 under the six SDK-bundling
//! plugin dirs (`deepseek`, `grok`, `nanogpt`, `openai-compatible`, `openai`,
//! `z-ai`), `@openrouter/sdk` 1.3.11 under `qtap-plugin-openrouter`. Corpus
//! stamps: `request-envelopes.recorded.ndjson` 7.20.0 ×216 + 0.115.0 ×44 +
//! the OpenRouter UA ×14; `image-dialects.recorded.ndjson` 7.20.0 ×8 + the
//! UA ×3; `google-wire.recorded.ndjson` `google-genai-sdk/1.52.0` ×22.
//! (`google_parts.rs` cites `@google/genai@1.52.0` too.)
//!
//! Locator: `QT_V4_CHECKOUT` (default `$HOME/source/quilltap-server`). An
//! absent checkout prints a loud `SKIP:`; a moved version is a FAIL.
//!
//! No recipe stage — this reads installed `package.json`s and committed
//! corpora, not an oracle run.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test provider_sdk_version_guard

use std::path::{Path, PathBuf};

/// `openai` — the Stainless `x-stainless-package-version` on every
/// non-anthropic OpenAI-shaped request.
const RECORDED_OPENAI_SDK: &str = "7.20.0";
/// `@anthropic-ai/sdk` — the Stainless stamp on `api.anthropic.com` requests.
const RECORDED_ANTHROPIC_SDK: &str = "0.115.0";
/// `@google/genai` — the `x-goog-api-client` `google-genai-sdk/<ver>` token.
const RECORDED_GOOGLE_GENAI_SDK: &str = "1.52.0";
/// `@openrouter/sdk` — the speakeasy user-agent's first version token.
const RECORDED_OPENROUTER_SDK: &str = "1.3.11";

/// (package, recorded version) — the four SDKs this guard pins.
const SDKS: [(&str, &str); 4] = [
    ("openai", RECORDED_OPENAI_SDK),
    ("@anthropic-ai/sdk", RECORDED_ANTHROPIC_SDK),
    ("@google/genai", RECORDED_GOOGLE_GENAI_SDK),
    ("@openrouter/sdk", RECORDED_OPENROUTER_SDK),
];

fn v4_checkout() -> PathBuf {
    match std::env::var("QT_V4_CHECKOUT") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = std::env::var("HOME").expect("HOME must be set to locate the v4 checkout");
            PathBuf::from(home).join("source/quilltap-server")
        }
    }
}

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
    let checkout = v4_checkout();
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

    let mut found: Vec<(&str, usize)> = SDKS.iter().map(|(p, _)| (*p, 0usize)).collect();
    let mut mismatches = Vec::new();
    for loc in install_locations(&checkout) {
        for (i, (pkg, recorded)) in SDKS.iter().enumerate() {
            let pkg_json = loc.join(pkg).join("package.json");
            if !pkg_json.is_file() {
                continue;
            }
            found[i].1 += 1;
            let installed = package_version(&pkg_json);
            if installed != *recorded {
                mismatches.push(format!(
                    "  {pkg}: recorded {recorded}, installed {installed} at {}",
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
         RECORDED_* constant here. The corpora half of this guard fails until both move.",
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
