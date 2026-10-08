//! The `.qtap` import's warning-text guard (P4.148 → dogfood #140): every
//! `DbError` that reaches a per-item import warning is rendered through ONE
//! helper, `services::quilltap_import::item_error_text` (or its
//! `overlay_error_text` twin), never by an untyped `to_string()`.
//!
//! v4's per-item warning tail is `error instanceof Error ? error.message :
//! String(error)` — for a SQLite throw, the driver's bare sentence (`UNIQUE
//! constraint failed: chat_messages.id`). v5's `DbError::Sqlite` `Display`
//! prefixes it with `sqlite error: `, and the 2026-10-05 walk found that prefix
//! on every such warning. It reached a warning by TWO routes: a direct `{e}`
//! on a `DbError`, and — half the class — an early `.map_err(|e|
//! e.to_string())` that stringified a read for a later push. So the guard
//! refuses both shapes, over the production zone (test modules stripped by
//! braces, comments dropped, literals KEPT) of every
//! `services/quilltap_import/*.rs` except the seed (`seed*.rs` — its report
//! goes to a boot log, not an import result):
//!
//! 1. a `map_err(…)` whose closure body calls `.to_string()` — route the
//!    conversion through `item_error_text` / `overlay_error_text` /
//!    `serde_error_text` (the last names the recorded serde-vs-Zod arm);
//! 2. an `fn err_msg` built on `to_string()` (the files importer's chokepoint,
//!    which fed five sites);
//! 3. a warning literal (`Failed to …: {x}`, `…restore hard link…: {x}`,
//!    `Import failed: {x}`) whose trailing argument is an inline identifier
//!    other than a KNOWN-BARE one: `{text}` (a `String` tail already rendered
//!    by the helpers upstream), `{msg}` (the wardrobe cycle sentence) or
//!    `{reason}`. The lexer cannot type-check, so the allow-list is a naming
//!    convention: a `DbError` binding interpolated inline is red; the helper is
//!    passed positionally (`: {}", item_error_text(&e)`).
//!
//! **Red-first, measured on unported `main` (`43dc35601`) by pointing
//! `QT_IMPORT_WARNING_GUARD_DIR` at an extract of that tree's
//! `services/quilltap_import/`:** see `MAIN_OFFENDERS` below — the guard was
//! red there by exactly that many sites, and is green on the branch.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test import_warning_text_guard

mod source_census;

use std::path::PathBuf;

use source_census::{code_only, core_src_root, production_zone, string_literals};

/// The offender count on unported `main` (`43dc35601`), measured through the
/// `QT_IMPORT_WARNING_GUARD_DIR` override (P4.148 lane record). Not asserted
/// against the live tree — that must be ZERO — but asserted against an
/// override that names a `main` extract, so the record cannot drift silently.
///
/// **103** = 64 `map_err(…)` closures built on `to_string()` (reconcile 22,
/// mod 17 — the preserve-ids preflight's 16 plus the top-level JSON parse —
/// entities 14, characters 5, profiles 4, document_stores 2) + 1 `err_msg` +
/// 38 warning literals interpolating a raw `{e}` (reconcile 8, entities 7,
/// document_stores 6, mod 5, configuration 4, characters 3, profiles 3,
/// files 1, memories 1).
const MAIN_OFFENDERS: usize = 103;

fn import_dir() -> PathBuf {
    match std::env::var("QT_IMPORT_WARNING_GUARD_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => core_src_root().join("services/quilltap_import"),
    }
}

/// The balanced-paren body of every `map_err(` call in `code`.
fn map_err_bodies(code: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(off) = code[from..].find("map_err(") {
        let open = from + off + "map_err".len();
        let mut depth = 0i32;
        let mut end = open;
        for (i, ch) in code[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(&code[open + 1..end.max(open + 1)]);
        from = open + 1;
    }
    out
}

/// The body of `fn err_msg` (to its first `}`), if the file defines one.
fn err_msg_body(code: &str) -> Option<&str> {
    let at = code.find("fn err_msg")?;
    let open = at + code[at..].find('{')?;
    let close = open + code[open..].find('}')?;
    Some(&code[open..close])
}

const KNOWN_BARE: &[&str] = &["text", "msg", "reason"];

/// Rule 3: a warning literal whose trailing inline argument is not known-bare.
fn bad_warning_literal(body: &str) -> Option<String> {
    // A literal that is NOTHING but one placeholder — `format!("{e}")` — is a
    // whole error rendered raw, wherever it sits (a `map_err` body included).
    if body.starts_with('{') && body.ends_with('}') && body.matches('{').count() == 1 {
        let arg = body[1..body.len() - 1]
            .split_once(':')
            .map_or(&body[1..body.len() - 1], |(name, _)| name);
        if !arg.is_empty()
            && arg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !KNOWN_BARE.contains(&arg)
        {
            return Some(arg.to_string());
        }
        return None;
    }
    let is_warning = body.starts_with("Failed to ")
        || body.starts_with("Import failed: ")
        || body.contains("restore hard link");
    if !is_warning {
        return None;
    }
    // The LAST placeholder ENDS the sentence — bar trailing punctuation, so
    // `{e}.` is still the error rendered raw (`{name} during reset` is a
    // subject, not an error); a `:?` / `:#?` spec is Debug, never bare. A
    // positional `{}` carries its argument outside the literal and cannot be
    // judged here (the `item_error_text(&e)` shape IS the good one).
    let open = body.rfind('{')?;
    let close = open + body[open..].find('}')?;
    if !body[close + 1..]
        .chars()
        .all(|c| matches!(c, '.' | ')' | ']' | '!'))
    {
        return None;
    }
    let arg = body[open + 1..close]
        .split_once(':')
        .map_or(&body[open + 1..close], |(name, _)| name);
    if arg.is_empty() || !arg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    if KNOWN_BARE.contains(&arg) {
        return None;
    }
    Some(arg.to_string())
}

fn offenders_in(dir: &std::path::Path) -> Vec<String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .filter(|p| {
            !p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("seed"))
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no sources under {}", dir.display());
    let mut out = Vec::new();
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(f).unwrap();
        let zone = production_zone(&src);
        let code = code_only(&zone);
        for body in map_err_bodies(&code) {
            // `to_string()` on the error renders a `DbError` through `Display`
            // (the only bare shape is the helper). A `map_err(|e| format!("{e}"))`
            // is the same render, but `code_only` blanks literals, so rule 3
            // catches it as the bare literal `{e}` instead.
            if body.contains(".to_string()") {
                let squashed: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
                out.push(format!("{name}: map_err({squashed})"));
            }
        }
        if err_msg_body(&code).is_some_and(|b| b.contains("to_string()")) {
            out.push(format!("{name}: fn err_msg built on to_string()"));
        }
        for lit in string_literals(&zone) {
            let body = lit
                .trim_start_matches('r')
                .trim_matches('#')
                .trim_matches('"');
            if let Some(arg) = bad_warning_literal(body) {
                out.push(format!(
                    "{name}: warning literal interpolates {{{arg}}}: {body:?}"
                ));
            }
        }
    }
    out
}

#[test]
fn every_import_warning_renders_db_errors_through_the_helper() {
    let dir = import_dir();
    let offenders = offenders_in(&dir);
    if std::env::var("QT_IMPORT_WARNING_GUARD_DIR").is_ok_and(|d| !d.is_empty()) {
        // The red-first measurement over an extract of unported `main`.
        assert_eq!(
            offenders.len(),
            MAIN_OFFENDERS,
            "the `main` extract at {} no longer measures {MAIN_OFFENDERS} offenders:\n  {}",
            dir.display(),
            offenders.join("\n  ")
        );
        return;
    }
    assert!(
        offenders.is_empty(),
        "{} `.qtap` import site(s) render an error without `item_error_text` \
         (dogfood #140 — the `sqlite error: ` prefix reaches the warning):\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}

/// The three rules see what they are meant to see — and a test module, a
/// comment or a known-bare argument does not trip them.
#[test]
fn the_rules_fire_on_their_shapes_and_nowhere_else() {
    let src = r#"
fn a() -> Result<(), String> { repo.read().map_err(|e| e.to_string())?; Ok(()) }
fn a2() -> Result<(), String> { repo.read().map_err(|e| format!("{e}"))?; Ok(()) }
fn d2() { warnings.push(format!("Failed to import tag \"{name}\": {e:?}")); }
fn d3() { warnings.push(format!("Failed to import tag \"{name}\": {e}.")); }
fn b() -> Result<(), String> { repo.read().map_err(|e| DbError::Internal(e.to_string()))?; Ok(()) }
fn c() -> Result<(), String> { repo.read().map_err(|e| super::item_error_text(&e))?; Ok(()) }
fn err_msg(e: DbError) -> String { e.to_string() }
fn d() { warnings.push(format!("Failed to import tag \"{name}\": {e}")); }
fn d4() { warnings.push(format!("Failed to delete {name} during reset")); }
fn e() { warnings.push(format!("Failed to import tag \"{name}\": {text}")); }
fn f() { warnings.push(format!("Failed to import tag \"{name}\": {}", item_error_text(&e))); }
// warnings.push(format!("Failed to import memory: {e}"));
#[cfg(test)]
mod tests {
    fn t() { x.map_err(|e| e.to_string()); let _ = "Failed to import memory: {e}"; }
}
"#;
    let zone = production_zone(src);
    let code = code_only(&zone);
    let bodies: Vec<&str> = map_err_bodies(&code)
        .into_iter()
        .filter(|b| b.contains(".to_string()"))
        .collect();
    assert_eq!(bodies.len(), 2, "{bodies:?}");
    assert!(err_msg_body(&code).is_some_and(|b| b.contains("to_string()")));
    let bad: Vec<String> = string_literals(&zone)
        .into_iter()
        .filter_map(|l| bad_warning_literal(l.trim_matches('"')))
        .collect();
    // `a2`'s bare `{e}` literal, `d2` (`{e:?}`), `d3` (`{e}.`) and `d` — never
    // `d4`'s mid-sentence `{name}`, `e`'s known-bare `{text}` or `f`'s `{}`.
    assert_eq!(bad, vec!["e".to_string(); 4], "{bad:?}");
}

// === P4.D264 (append-only) ===

/// P4.D264 R-C — the wardrobe carriers' seven warning literals, VERBATIM in
/// the production source (the `format!` text, interpolations as written).
/// Five live in the importer (the rule above already holds their tails to the
/// helpers: `{msg}` / `{title}` / `{original_filename}` are rendered upstream);
/// the restore's two go through `WarnText`. The v4 text each pins:
/// `import-wardrobe-wear.ts:281`, `execute.ts:910`, `restore.ts:999` / `:779`,
/// `import-wardrobe-images.ts:71` / `:102-104` / `:116-118`.
#[test]
fn the_wardrobe_carrier_warnings_are_v4s_text() {
    let core = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-core/src/services");
    let read = |rel: &str| std::fs::read_to_string(core.join(rel)).expect("read source");
    for (file, literal) in [
        (
            "quilltap_import/wardrobe_wear.rs",
            "\"Dropped {} malformed wardrobe wear-ledger row(s).\"",
        ),
        (
            "quilltap_import/mod.rs",
            "\"Failed to import the wardrobe wear ledger: {msg}\"",
        ),
        (
            "quilltap_import/wardrobe_images.rs",
            "\"Wardrobe item \\\"{title}\\\" lost a picture whose bytes were not in the bundle ({original_filename}).\"",
        ),
        (
            "quilltap_import/wardrobe_images.rs",
            "\"Failed to import a picture of wardrobe item \\\"{title}\\\": {msg}\"",
        ),
        (
            "quilltap_import/wardrobe_images.rs",
            "\"Failed to repoint the picture of wardrobe item \\\"{title}\\\": {}\"",
        ),
        (
            "backup/restore/orchestrator.rs",
            "\"Failed to restore the wardrobe wear ledger: {error}\"",
        ),
        (
            "backup/restore/orchestrator.rs",
            "\"Failed to repoint a wardrobe item's picture ({}): {error}\"",
        ),
    ] {
        assert_eq!(
            read(file).matches(literal).count(),
            1,
            "{file} must carry v4's warning text exactly once: {literal}"
        );
    }
}
// === end P4.D264 ===
