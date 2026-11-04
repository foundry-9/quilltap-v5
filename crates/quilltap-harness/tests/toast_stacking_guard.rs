//! The toast stacking guard (P4.D338, v4 `5a8338b30`).
//!
//! A toast raised from inside a dialog was hidden behind it: v4 had the toast
//! container at `z-50` while every dialog overlay sits at `z-[60]`–`z-[101]`.
//! v4 raised it to `1000` — above every dialog tier, below the full-screen
//! startup gates at `z-[9999]` — and v5's `_toast.css` follows.
//!
//! A vitest spec cannot see a stylesheet in this SPA (no `?raw`, no `node:fs`,
//! and jsdom runs no cascade — `speaking-as-avatar.spec.ts:166-169`), so the
//! rule is pinned here, over source text, the way `public_schemas_vendor_guard`
//! reads `apps/web`:
//!
//! 1. `.qt-toast-container` declares exactly `z-index: 1000;`;
//! 2. every other `z-[N]` utility token and `z-index: N` declaration in
//!    `apps/web/src` is below 1000 or exactly 9999 (the startup gates) — a later
//!    tier raised past the toast would hide it again;
//! 3. the dialog overlay `.qt-dialog-overlay` is `z-[60]` — the floor the toast
//!    must clear (a dialog tier raised toward 1000 reddens (2), a toast lowered
//!    reddens (1)).
//! 4. ⚖ [`V4_FLOATING_LAYER_TIES_THE_TOAST`] — v4's topmost floating layer
//!    (`99463fc56`, the Brahma Console + its launcher, P4.D359) sits at
//!    `--qt-floating-layer-z: 1000`, EQUAL to the toast, by v4's own design at
//!    the `236c04e35` pin. It is pinned at exactly that value, and (2) admits
//!    `1000` only in the floating layer's own files.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test toast_stacking_guard

use std::path::{Path, PathBuf};

const TOAST_CSS: &str = "apps/web/src/styles/qt-components/_toast.css";
const SURFACES_CSS: &str = "apps/web/src/styles/qt-components/_surfaces.css";
const TOAST_Z: u32 = 1000;
const STARTUP_GATES_Z: u32 = 9999;

/// ## ⚖ V4_FLOATING_LAYER_TIES_THE_TOAST (recorded at the `236c04e35` unification)
///
/// v4 `99463fc56` added the topmost floating layer (the always-on Brahma Console
/// and its launcher, portalled to `document.body`) at
/// `--qt-floating-layer-z: 1000` (v4 `app/styles/qt-components/_variables.css:293`
/// at `236c04e35`) — the SAME value as the toast container (`5a8338b30`), not
/// above it. P4.D359 ported it verbatim. A tie is not a climb past the toast; it
/// is v4's own stacking, kept.
///
/// The value lives in a CSS custom property, which [`z_values`] does not read,
/// so it is pinned by [`the_floating_layer_ties_the_toast_as_v4s_does`]; the one
/// place the scanner does see it is `floating-dialog.ts`'s doc comment, which
/// names it (`z-index: 1000` losing to an un-portalled ancestor's stacking
/// context). `(file suffix, the one value admitted there)`.
const V4_FLOATING_LAYER_TIES_THE_TOAST: &[(&str, u32)] = &[("app/ui/floating-dialog.ts", 1000)];

/// The custom property [`V4_FLOATING_LAYER_TIES_THE_TOAST`] pins.
const VARIABLES_CSS: &str = "apps/web/src/styles/qt-components/_variables.css";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Every source file under `apps/web/src` whose text can carry a z-index.
fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            source_files(&p, out);
        } else if matches!(
            p.extension().and_then(|e| e.to_str()),
            Some("css" | "ts" | "html")
        ) {
            out.push(p);
        }
    }
}

/// `(line number, z value)` for each `z-[N]` token and `z-index: N` declaration.
fn z_values(text: &str) -> Vec<(usize, u32)> {
    let mut found = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut from = 0;
        while let Some(at) = line[from..].find("z-[") {
            let start = from + at + 3;
            let digits: String = line[start..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if !digits.is_empty() && bytes.get(start + digits.len()) == Some(&b']') {
                found.push((i + 1, digits.parse().unwrap()));
            }
            from = start;
        }
        let mut from = 0;
        while let Some(at) = line[from..].find("z-index:") {
            let start = from + at + "z-index:".len();
            let digits: String = line[start..]
                .trim_start()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if !digits.is_empty() {
                found.push((i + 1, digits.parse().unwrap()));
            }
            from = start;
        }
    }
    found
}

/// A rule's own comments (the toast's carries v4's why-note naming the tiers it
/// clears) are prose, not declarations.
fn strip_css_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        out.push_str(&rest[..at]);
        match rest[at..].find("*/") {
            Some(end) => rest = &rest[at + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn the_toast_container_sits_above_every_dialog_tier() {
    let css = read(TOAST_CSS);
    let rule_start = css
        .find(".qt-toast-container {")
        .unwrap_or_else(|| panic!("{TOAST_CSS}: no .qt-toast-container rule"));
    let rule = &css[rule_start..rule_start + css[rule_start..].find('}').unwrap()];
    let declared = z_values(&strip_css_comments(rule));
    assert_eq!(
        declared.iter().map(|(_, z)| *z).collect::<Vec<_>>(),
        vec![TOAST_Z],
        "{TOAST_CSS}: .qt-toast-container must declare exactly `z-index: {TOAST_Z};` \
         (above every dialog overlay, below the startup gates at {STARTUP_GATES_Z})"
    );
}

#[test]
fn no_other_layer_climbs_past_the_toast() {
    let mut files = Vec::new();
    source_files(&repo_root().join("apps/web/src"), &mut files);
    assert!(files.len() > 100, "found only {} source files", files.len());
    let mut offenders = Vec::new();
    for file in &files {
        let rel = file
            .strip_prefix(repo_root())
            .unwrap_or(file)
            .to_string_lossy()
            .replace("/../..", "")
            .to_string();
        let is_toast_css = rel.ends_with("qt-components/_toast.css");
        let text = std::fs::read_to_string(file).unwrap();
        for (line, z) in z_values(&text) {
            if is_toast_css && z == TOAST_Z {
                continue; // the toast container's own declaration
            }
            if V4_FLOATING_LAYER_TIES_THE_TOAST
                .iter()
                .any(|(suffix, tie)| rel.ends_with(suffix) && z == *tie)
            {
                continue; // v4's floating layer, tied with the toast (recorded)
            }
            if z >= TOAST_Z && z != STARTUP_GATES_Z {
                offenders.push(format!("{rel}:{line} z={z}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a layer at or above the toast's z-{TOAST_Z} (other than the startup gates at \
         {STARTUP_GATES_Z}) would hide a toast raised from inside it: {offenders:?}"
    );
}

#[test]
fn the_dialog_overlay_is_the_floor_the_toast_must_clear() {
    let css = read(SURFACES_CSS);
    let at = css
        .find(".qt-dialog-overlay {")
        .unwrap_or_else(|| panic!("{SURFACES_CSS}: no .qt-dialog-overlay rule"));
    let rule = &css[at..at + css[at..].find('}').unwrap()];
    assert_eq!(
        z_values(&strip_css_comments(rule))
            .iter()
            .map(|(_, z)| *z)
            .collect::<Vec<_>>(),
        vec![60],
        "{SURFACES_CSS}: .qt-dialog-overlay must stay z-[60] — the toast guard reasons from it"
    );
}

#[test]
fn the_floating_layer_ties_the_toast_as_v4s_does() {
    let css = read(VARIABLES_CSS);
    let decls: Vec<&str> = css
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("--qt-floating-layer-z:"))
        .collect();
    assert_eq!(
        decls,
        vec!["--qt-floating-layer-z: 1000;"],
        "{VARIABLES_CSS}: the floating layer must stay at v4's `1000` — tied with the toast \
         (V4_FLOATING_LAYER_TIES_THE_TOAST); raised past it, it would hide a toast"
    );
    assert_eq!(
        V4_FLOATING_LAYER_TIES_THE_TOAST
            .iter()
            .map(|(_, z)| *z)
            .collect::<Vec<_>>(),
        vec![TOAST_Z],
        "the recorded tie admits exactly the toast's value, nothing above it"
    );
}

#[test]
fn the_z_scanner_reads_both_spellings() {
    assert_eq!(
        z_values("a z-[60] b\n  z-index: 1000;\nz-[9999]\nz-[x]\nz-index: auto;"),
        vec![(1, 60), (2, 1000), (3, 9999)]
    );
}
