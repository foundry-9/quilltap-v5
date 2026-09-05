//! The `zod` version tripwire (P4.77 — phase-4 candidate 4 from the
//! `d883a5ee1` round).
//!
//! v5 hand-rolls Zod semantics in TWO engines (`pascal/custom_tool_types.rs`
//! plus its SPA twin, `apps/web/src/app/pascal/custom-tool-types.ts`) and
//! transcribes Zod's own `ZodError.message` sentences at ~150 edge sites
//! (settings routes, the wardrobe/chat-outfits/brahma `zod_uuid` gates, the
//! `web_edge_body_parse_guard` census, …). The oracle's `node_modules`
//! resolve v4's LIVE tree, so a `zod` dependency bump in the checkout is a
//! regen event for every one of them — and it moved once already without
//! anyone ordering a regen: `6e1a64ea6` bumped 4.4.3 to 4.5.4 mid-round, and
//! the `d883a5ee1` drift catch-up round (P4.D158 unit 2 item 4, see
//! `docs/developer/porting/status-log.md`) only caught it at a family's
//! first red, by consequence, not by design. This guard is the design: it
//! fails the workspace gate the moment the installed `zod` moves past the
//! version recorded below, so the obligation is visible at ordering time
//! instead of discovered mid-round.
//!
//! The locator is `QT_V4_CHECKOUT` (default `$HOME/source/quilltap-server`,
//! the convention every recipe header uses). An absent checkout directory
//! prints a loud `SKIP:` line, since a CI box with no v4 checkout must not
//! fail here, and never a silent pass. A checkout whose `zod` has moved is a
//! FAIL, never a skip, naming the obligation the P4.D158 lane record proved
//! out: re-run that same three-part measurement (the `v4/locales/en.js`
//! sentence diff, the `v4/core/regexes.js` regex diff, and a grep for any
//! newly-reachable validator) against the new version, then regenerate every
//! Zod-transcribing family, both hand-rolled engines, the SPA corpus, and
//! the ~150 edge sites, before trusting any of them again.
//!
//! Shape precedent: `spelling_guard.rs` (a repo-reading guard with no
//! fixture); `db_error_key_guard.rs` (an executable census against a
//! recorded constant).
//!
//! No recipe stage — this reads the checkout's installed `zod`, not the v4
//! oracle, so there is nothing to regenerate.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test zod_version_guard

use std::path::PathBuf;

/// v4's `zod` version at the `d883a5ee1` oracle baseline (measured
/// 2026-09-05: `node -p "require('./node_modules/zod/package.json')
/// .version"` from the checkout — `4.5.4`, post-`6e1a64ea6`). Bump this ONLY
/// alongside the re-measurement + regen the module doc above describes.
const RECORDED_ZOD_VERSION: &str = "4.5.4";

fn v4_checkout() -> PathBuf {
    match std::env::var("QT_V4_CHECKOUT") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = std::env::var("HOME").expect("HOME must be set to locate the v4 checkout");
            PathBuf::from(home).join("source/quilltap-server")
        }
    }
}

/// Read `<checkout>/node_modules/zod/package.json`'s `"version"` field
/// without pulling in a JSON dependency this test alone would need — the
/// field is a short quoted string near the top of a small, well-formed file.
fn installed_zod_version(pkg_json: &std::path::Path) -> String {
    let text = std::fs::read_to_string(pkg_json)
        .unwrap_or_else(|e| panic!("read {}: {e}", pkg_json.display()));
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", pkg_json.display()));
    value["version"]
        .as_str()
        .unwrap_or_else(|| panic!("no string \"version\" field in {}", pkg_json.display()))
        .to_string()
}

#[test]
fn v4s_installed_zod_matches_the_recorded_version() {
    let checkout = v4_checkout();
    if !checkout.is_dir() {
        println!(
            "SKIP: no v4 checkout at {} (set QT_V4_CHECKOUT) — cannot verify the installed \
             zod version on this machine",
            checkout.display()
        );
        return;
    }

    let pkg_json = checkout.join("node_modules/zod/package.json");
    if !pkg_json.is_file() {
        println!(
            "SKIP: {} has no node_modules/zod/package.json (run `npm ci` in the checkout) — \
             cannot verify the installed zod version",
            pkg_json.display()
        );
        return;
    }

    let installed = installed_zod_version(&pkg_json);
    assert_eq!(
        installed,
        RECORDED_ZOD_VERSION,
        "v4's installed `zod` moved {RECORDED_ZOD_VERSION} -> {installed} at {}.\n\n\
         The oracle's node_modules resolve v4's LIVE tree, so this dependency bump is a \
         regen event for every family that transcribes Zod semantics: the two hand-rolled \
         engines (`crates/quilltap-core/src/pascal/custom_tool_types.rs` + its SPA twin \
         `apps/web/src/app/pascal/custom-tool-types.ts`), the SPA corpus, and the ~150 \
         Zod-sentence/regex edge sites (settings routes, the `zod_uuid` gates, the \
         `web_edge_body_parse_guard` census, …).\n\n\
         Before trusting any of them again, repeat the P4.D158 unit 2 item 4 measurement \
         (`docs/developer/porting/status-log.md`, \"Lane record — P4.D158 unit 2 item 4: \
         the Zod 4.4.3 -> 4.5.4 read\") against the new version — diff `v4/locales/en.js`'s \
         sentences, `v4/core/regexes.js`'s regexes, and grep for any validator v4 newly \
         reaches — then regenerate the affected families and bump RECORDED_ZOD_VERSION here.",
        pkg_json.display()
    );
}
