//! P4.139 — the API-key repository-read census, as a guard.
//!
//! **Every v4 API-key read is a FALLBACK read.** The three repository methods
//! (`connection-profiles.repository.ts:218-288`) are 4-argument `safeQuery`s on
//! the repository constructed with `'connection_profiles'`: a failed read logs
//! `Error finding API key by ID` / `… by ID and user ID` / `Error finding API
//! keys by user ID` (`collection: 'connection_profiles'` first) and answers
//! `null` / `[]` — it never throws — and no importer path (the only strict
//! scope, `withStrictRepositoryFailures`) reads `api_keys`. So there is no
//! "v4 lets it through" class at all: a v5 site that propagates an `api_keys`
//! read error where v4 logs and answers "nothing" is a divergence, and this
//! census is the measured list of every raw read, so a NEW one cannot land
//! unclassified.
//!
//! One row per raw call, `(file under crates/, the nearest preceding fn,
//! method, class)`, in source order (files sorted by path). It fails when any
//! row moves: a new caller must route through a home (or a recorded class) and
//! record the choice here; a row going away is also a failure.
//!
//! **Classes.**
//!
//! - `home` — the raw read is an argument of a `db::fallback` API-key home (it
//!   sits inside `find_api_key_by_id_or_none(…)`,
//!   `find_api_key_by_id_and_user_id_or_none(…)` or
//!   `find_api_keys_by_user_id_or_empty(…)`): v4's fallback, v4's line.
//! - `internal` — the read IS a home's body: `db/api_keys.rs`, `db/fallback.rs`,
//!   and the bodies of `services::api_key_service`'s helpers (`read_api_key`,
//!   `read_api_key_scoped`). The by-user-id helper
//!   `api_keys_by_user_id_or_empty` folded onto `db::fallback` at the round's
//!   unification (§S.1) and reads `home`.
//! - `no-v4-counterpart` — `find_active_api_key_for_provider`, the propagating
//!   provider scan `quilltap-host`'s `DbProviderKeys` reads (no v4 model call
//!   scans; one [`OVERRIDES`] row).
//! - `wrapper-no-caller` — `get_all_api_keys` and `find_api_key_by_id_scoped`,
//!   the user-scoped repository wrappers with no production caller (kept and
//!   classified; deletion is a human call — the order's Tier 3 item 21).
//! - `recorded-divergence` — `services/orchestrator.rs`'s
//!   `build_pricing_context`: a CADENCE divergence, not a missing wrap (v4
//!   reads only the first OPENROUTER profile's key, and only on a pricing-cache
//!   miss; v5 reads every key on every turn). Wrapping it would invent lines;
//!   the faithful hunk is P4.139's Tier 3 item 15 (§S.4).
//! - `fallback-in-v4` — everything else: THE CONVERSION LIST. Pinned at ZERO in
//!   [`COUNTS`], so any new raw read is red until its author classifies it.
//!
//! **Scanner.** The shared census lexer (`source_census`): test items stripped,
//! comments and string literals blanked — these are CALLS, not messages. A call
//! is `::<method>(` whose PATH SEGMENT before `::` is `api_keys` (the anchor is
//! mandatory: `connection_profiles::find_by_id` shares the name). The `home`
//! test walks the call's ENCLOSING parentheses (not its statement — a home's
//! closure body opens a `{` the statement head would stop at). A second test
//! fails on any `use …::api_keys::{…}` import of a read fn (the bare-name
//! escape hatch past the anchor).
//!
//! Run: `cargo test -p quilltap-harness --test api_key_read_sites_census`.
//! Set `QT_CENSUS_PRINT=<file>` to write the measured rows there (for pasting
//! into EXPECTED). `QT_CENSUS_ROOT=<dir>` scans another checkout's `crates/`
//! (the red-first measurement over `main`'s sources).

mod source_census;

use std::path::{Path, PathBuf};

use source_census::{code_only, production_zone, workspace_rust_sources};

/// The four `db::api_keys` READ fns the census follows.
const METHODS: [&str; 4] = [
    "find_by_id",
    "find_by_id_and_user_id",
    "get_api_keys_by_user_id",
    "find_label_by_id",
];

/// The `db::fallback` API-key homes (the third is P4.142's, delivered for the
/// §S.1 fold).
const HOMES: [&str; 3] = [
    "find_api_key_by_id_or_none",
    "find_api_key_by_id_and_user_id_or_none",
    "find_api_keys_by_user_id_or_empty",
];

/// The helper bodies in `services/api_key_service.rs` that ARE a home's read.
/// (`api_keys_by_user_id_or_empty` left this list at the §S.1 fold: its body is
/// now an argument of `db::fallback::find_api_keys_by_user_id_or_empty`.)
const HELPER_BODIES: [&str; 2] = ["read_api_key", "read_api_key_scoped"];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    file: String,
    func: String,
    method: String,
    class: String,
}

fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("QT_CENSUS_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .to_path_buf()
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The nearest `fn <ident>` declaration above `at`, and where it starts.
fn enclosing_fn(code: &str, at: usize) -> (String, usize) {
    let head = &code[..at];
    let mut best = (String::from("<module>"), 0usize);
    let mut from = 0usize;
    while let Some(i) = head[from..].find("fn ") {
        let abs = from + i;
        let before_ok = abs == 0 || !is_ident(head.as_bytes()[abs - 1]);
        if before_ok {
            let name: String = head[abs + 3..]
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                best = (name, abs);
            }
        }
        from = abs + 3;
    }
    best
}

/// The callee names of every parenthesis ENCLOSING `at`, innermost first,
/// back to `floor` (the enclosing fn's start): walk left counting parens; an
/// unmatched `(` is an enclosing call, named by the identifier just before it.
fn enclosing_calls(code: &str, at: usize, floor: usize) -> Vec<String> {
    let bytes = code.as_bytes();
    let mut depth = 0i32;
    let mut out = Vec::new();
    let mut k = at;
    while k > floor {
        k -= 1;
        match bytes[k] {
            b')' => depth += 1,
            b'(' if depth > 0 => depth -= 1,
            b'(' => {
                let mut end = k;
                while end > 0 && bytes[end - 1].is_ascii_whitespace() {
                    end -= 1;
                }
                let mut start = end;
                while start > 0 && is_ident(bytes[start - 1]) {
                    start -= 1;
                }
                out.push(code[start..end].to_string());
            }
            _ => {}
        }
    }
    out
}

/// The path segment just before the `::` at `colons` (`crate::db::api_keys::`
/// ⇒ `api_keys`).
fn segment_before(code: &str, colons: usize) -> &str {
    let bytes = code.as_bytes();
    let end = colons;
    let mut start = end;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }
    &code[start..end]
}

/// Sites whose class the scanner cannot read off the text, with the reason.
const OVERRIDES: &[(&str, &str, &str, &str)] = &[
    // The propagating provider scan `quilltap-host`'s `DbProviderKeys` reads —
    // no v4 model call scans for a key (its own doc). The web search's v4
    // twin reads through `find_active_api_key_for_provider_or_none` instead.
    (
        "quilltap-core/src/services/api_key_service.rs",
        "find_active_api_key_for_provider",
        "get_api_keys_by_user_id",
        "no-v4-counterpart",
    ),
    // The user-scoped repository wrappers with no production caller (their
    // only caller is the module's own test).
    (
        "quilltap-core/src/services/api_key_service.rs",
        "get_all_api_keys",
        "get_api_keys_by_user_id",
        "wrapper-no-caller",
    ),
    (
        "quilltap-core/src/services/api_key_service.rs",
        "find_api_key_by_id_scoped",
        "find_by_id_and_user_id",
        "wrapper-no-caller",
    ),
    // The pricing context's cadence divergence (P4.139 Tier 3 item 15, §S.4 —
    // P4.140's file): v4 reads the FIRST OPENROUTER profile's key, scoped, and
    // only when the pricing cache misses; v5 reads every key-naming profile's
    // key on every non-Courier turn. Wrapping this read would log, every turn,
    // for keys v4 never reads.
    (
        "quilltap-core/src/services/orchestrator.rs",
        "build_pricing_context",
        "find_by_id_and_user_id",
        "recorded-divergence",
    ),
];

fn classify(rel: &str, func: &str, method: &str, enclosing: &[String]) -> &'static str {
    if let Some((_, _, _, class)) = OVERRIDES
        .iter()
        .find(|(f, n, m, _)| *f == rel && *n == func && *m == method)
    {
        return class;
    }
    if matches!(
        rel,
        "quilltap-core/src/db/api_keys.rs" | "quilltap-core/src/db/fallback.rs"
    ) || (rel == "quilltap-core/src/services/api_key_service.rs"
        && HELPER_BODIES.contains(&func))
    {
        return "internal";
    }
    if enclosing.iter().any(|c| HOMES.contains(&c.as_str())) {
        return "home";
    }
    "fallback-in-v4"
}

fn scan_code(rel: &str, code: &str) -> Vec<Row> {
    let bytes = code.as_bytes();
    let mut rows = Vec::new();
    for (colons, _) in code.match_indices("::") {
        if segment_before(code, colons) != "api_keys" {
            continue;
        }
        let start = colons + 2;
        let mut end = start;
        while end < code.len() && is_ident(bytes[end]) {
            end += 1;
        }
        let name = &code[start..end];
        if !METHODS.contains(&name) || bytes.get(end) != Some(&b'(') {
            continue;
        }
        let (func, fn_start) = enclosing_fn(code, colons);
        let enclosing = enclosing_calls(code, colons, fn_start);
        rows.push(Row {
            file: rel.to_string(),
            class: classify(rel, &func, name, &enclosing).to_string(),
            func,
            method: name.to_string(),
        });
    }
    rows
}

fn rel_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root.join("crates"))
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

fn sources() -> (PathBuf, Vec<PathBuf>) {
    let root = repo_root();
    let mut files = Vec::new();
    for krate in ["quilltap-core", "quilltap-host", "quilltap-web"] {
        let dir = root.join("crates").join(krate).join("src");
        if dir.is_dir() {
            workspace_rust_sources(&dir, &mut files);
        }
    }
    files.sort();
    (root, files)
}

fn measured() -> Vec<Row> {
    let (root, files) = sources();
    files
        .iter()
        .flat_map(|f| {
            let src = std::fs::read_to_string(f).unwrap_or_else(|e| panic!("read {f:?}: {e}"));
            scan_code(&rel_of(&root, f), &code_only(&production_zone(&src)))
        })
        .collect()
}

fn render(rows: &[Row]) -> String {
    rows.iter()
        .map(|r| {
            format!(
                "    (\"{}\", \"{}\", \"{}\", \"{}\"),",
                r.file, r.func, r.method, r.class
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every raw `api_keys::` read call site, in SOURCE ORDER per file (files
/// sorted by path): `(file under crates/, nearest preceding fn, method,
/// class)`.
#[rustfmt::skip]
const EXPECTED: &[(&str, &str, &str, &str)] = &[
    ("quilltap-core/src/services/api_key_service.rs", "read_api_key", "find_by_id", "internal"),
    ("quilltap-core/src/services/api_key_service.rs", "read_api_key_scoped", "find_by_id_and_user_id", "internal"),
    ("quilltap-core/src/services/api_key_service.rs", "api_keys_by_user_id_or_empty", "get_api_keys_by_user_id", "home"),
    ("quilltap-core/src/services/api_key_service.rs", "find_active_api_key_for_provider", "get_api_keys_by_user_id", "no-v4-counterpart"),
    ("quilltap-core/src/services/api_key_service.rs", "get_all_api_keys", "get_api_keys_by_user_id", "wrapper-no-caller"),
    ("quilltap-core/src/services/api_key_service.rs", "find_api_key_by_id_scoped", "find_by_id_and_user_id", "wrapper-no-caller"),
    ("quilltap-core/src/services/carina_query.rs", "carina_api_key", "find_by_id", "home"),
    ("quilltap-core/src/services/orchestrator.rs", "build_pricing_context", "find_by_id_and_user_id", "recorded-divergence"),
];

#[test]
fn every_api_key_read_site_is_classified() {
    let got = measured();
    if let Ok(out) = std::env::var("QT_CENSUS_PRINT") {
        std::fs::write(&out, render(&got)).expect("write the measured rows");
    }
    let want: Vec<Row> = EXPECTED
        .iter()
        .map(|(file, func, method, class)| Row {
            file: file.to_string(),
            func: func.to_string(),
            method: method.to_string(),
            class: class.to_string(),
        })
        .collect();
    assert_eq!(
        got,
        want,
        "an API-key read site moved — route it through a `db::fallback` home (v4 \
         reads every key through a fallback) or record its class, then update \
         EXPECTED.\nmeasured rows:\n{}",
        render(&got)
    );
}

#[test]
fn the_class_counts_are_pinned() {
    let got = measured();
    let count = |class: &str| got.iter().filter(|r| r.class == class).count();
    assert_eq!(
        (
            count("home"),
            count("internal"),
            count("no-v4-counterpart"),
            count("wrapper-no-caller"),
            count("recorded-divergence"),
            count("fallback-in-v4"),
        ),
        COUNTS,
        "class counts moved (home, internal, no-v4-counterpart, wrapper-no-caller, \
         recorded-divergence, fallback-in-v4)"
    );
}

/// (home, internal, no-v4-counterpart, wrapper-no-caller, recorded-divergence,
/// fallback-in-v4), with the arithmetic:
///
/// - 8 raw call sites = 2 + 2 + 1 + 2 + 1 + 0. Every OTHER v5 key read goes
///   through `api_key_service`'s helpers and is not a raw call at all.
/// - **home 2** — the sites that call the `db::fallback` home DIRECTLY: Carina's
///   `carina_api_key` (P4.140's file, already home since P4.136) and the
///   by-user-id helper's body, folded onto `find_api_keys_by_user_id_or_empty`
///   at unification (§S.1). The greeting's key read and the chat-enrichment
///   summary's were hand-wraps of the home (kept so their files' `api_keys`
///   imports stayed used, outside P4.139's named hunks) until P4.150 folded
///   both onto `read_api_key` — identical bodies; home 4 → 2.
/// - **internal 2** — the two read helpers' bodies.
/// - **no-v4-counterpart 1**, **wrapper-no-caller 2**, **recorded-divergence 1**
///   — see [`OVERRIDES`].
/// - **fallback-in-v4 0** — the conversion list, EMPTY. Red-first: the same
///   scanner over `main` `75219b8dd` (`QT_CENSUS_ROOT`) measured 42 rows, 27 of
///   them here = the survey's 25 NEEDS-HOME, less #39 (the provider scan, an
///   [`OVERRIDES`] row on both trees), plus #32 (the `.qtap` export's
///   unscoped label read — made scoped this lane) and `provider_routing.rs`'s
///   two (read through a local `find_api_key_or_none` wrapper the scanner does
///   not know — folded onto `read_api_key_scoped` this lane).
const COUNTS: (usize, usize, usize, usize, usize, usize) = (2, 2, 1, 2, 1, 0);

/// No bare-name escape hatch: a `use …::api_keys::{find_by_id, …}` import would
/// let a raw read slip past the `api_keys::` anchor.
#[test]
fn no_read_fn_is_imported_by_bare_name() {
    let (root, files) = sources();
    let mut offenders = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap_or_else(|e| panic!("read {f:?}: {e}"));
        let code = code_only(&production_zone(&src));
        offenders.extend(
            bare_imports(&code)
                .into_iter()
                .map(|name| format!("{}: {name}", rel_of(&root, f))),
        );
    }
    assert!(
        offenders.is_empty(),
        "a db::api_keys read fn is imported by its bare name — call it as \
         `api_keys::<fn>` (or through a home):\n{}",
        offenders.join("\n")
    );
}

/// The escape hatches a `use` item can open past the `api_keys::` anchor: a
/// read fn imported by its bare name (in a brace group or not, at any nesting),
/// a glob (`api_keys::*` → `"*"`), or a module ALIAS (`api_keys as ak` →
/// `"as"`, after which `ak::find_by_id` would be invisible). Every `api_keys`
/// segment in the item is examined, not only the first (the `f6426e196`
/// recorded-divergences unification, a §3 finding — the scanner had read one
/// segment and kept a brace-group's trailing comma on a single path).
fn bare_imports(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(i) = code[from..].find("use ") {
        let abs = from + i;
        from = abs + 4;
        if abs > 0 && is_ident(code.as_bytes()[abs - 1]) {
            continue;
        }
        let Some(semi) = code[abs..].find(';') else {
            break;
        };
        let item = &code[abs..abs + semi];
        for (p, _) in item.match_indices("api_keys") {
            let bytes = item.as_bytes();
            if p > 0 && is_ident(bytes[p - 1]) {
                continue;
            }
            let after = &item[p + "api_keys".len()..];
            if after.as_bytes().first().is_some_and(|b| is_ident(*b)) {
                continue;
            }
            let after = after.trim_start();
            if after.starts_with("as ") || after.starts_with("as\n") {
                out.push("as".to_string());
                continue;
            }
            let Some(rest) = after.strip_prefix("::") else {
                continue;
            };
            let rest = rest.trim_start();
            let names: Vec<&str> = if let Some(inner) = rest.strip_prefix('{') {
                let end = inner.find('}').unwrap_or(inner.len());
                inner[..end]
                    .split(',')
                    .map(|n| n.split_whitespace().next().unwrap_or(""))
                    .collect()
            } else {
                vec![rest
                    .split(|c: char| c.is_whitespace() || c == ',' || c == '}')
                    .next()
                    .unwrap_or("")]
            };
            out.extend(
                names
                    .into_iter()
                    .filter(|n| *n == "*" || METHODS.contains(n))
                    .map(str::to_string),
            );
        }
    }
    out
}

/// The scanner itself, on synthetic text: a read inside a home's closure (even
/// a braced one) is `home`, a propagating `?` read is `fallback-in-v4`, a
/// same-named fn on ANOTHER module is not a row, a comment or string mention is
/// not a call, a test module is stripped, and a bare-name import is caught.
#[test]
fn the_scanner_reads_a_synthetic_source() {
    let src = r#"
use crate::db::api_keys::{self, ApiKey, find_by_id};
use crate::db::api_keys::get_api_keys_by_user_id;
use crate::db::api_keys;
fn a(db: &Db, c: &Connection) -> Result<(), DbError> {
    // api_keys::find_by_id(c, "x")? in a comment is nothing
    let s = "api_keys::find_by_id(c, x)?";
    let k = api_keys::find_by_id(c, "x")?;
    let h = crate::db::fallback::find_api_key_by_id_or_none(id, || {
        db.read_main(|conn| crate::db::api_keys::find_by_id_and_user_id(conn, id, u))
    });
    let p = connection_profiles::find_by_id(c, "x")?;
    Ok(())
}
#[cfg(test)]
mod tests { fn t() { api_keys::get_api_keys_by_user_id(c, "u").unwrap(); } }
"#;
    let code = code_only(&production_zone(src));
    let got: Vec<(String, String, String)> = scan_code("quilltap-core/src/x.rs", &code)
        .into_iter()
        .map(|r| (r.func, r.method, r.class))
        .collect();
    let row = |m: &str, c: &str| ("a".to_string(), m.to_string(), c.to_string());
    assert_eq!(
        got,
        vec![
            row("find_by_id", "fallback-in-v4"),
            row("find_by_id_and_user_id", "home"),
        ]
    );
    assert_eq!(
        bare_imports(&code),
        vec![
            "find_by_id".to_string(),
            "get_api_keys_by_user_id".to_string()
        ]
    );
}

/// The widened guard on synthetic text (the `f6426e196` recorded-divergences
/// unification): a module alias, a glob, a single path inside a brace group, a
/// second `api_keys` segment in one item — each caught; an unrelated module
/// whose name merely ends in `api_keys`, and a non-read fn, are not.
#[test]
fn the_import_guard_catches_aliases_globs_and_nested_paths() {
    let code = "use crate::db::api_keys as ak;\n\
                use crate::db::api_keys::*;\n\
                use crate::db::{api_keys::find_by_id, chats};\n\
                use crate::db::{files, api_keys::{get_api_keys_by_user_id}};\n\
                use crate::db::my_api_keys::find_by_id;\n\
                use crate::db::api_keys::ApiKey;\n";
    assert_eq!(
        bare_imports(code),
        vec![
            "as".to_string(),
            "*".to_string(),
            "find_by_id".to_string(),
            "get_api_keys_by_user_id".to_string(),
        ]
    );
}
