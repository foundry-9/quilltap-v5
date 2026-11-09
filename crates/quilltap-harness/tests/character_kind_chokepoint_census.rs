//! The character-kind CHOKEPOINT census (P4.D369 Tier 1 item 12) — the twin of
//! v4's `__tests__/unit/lib/characters/kind-chokepoint.test.ts` (`b09ca46bb`).
//!
//! **A character's kind is read and compared in ONE place.** v4's
//! `lib/characters/kind.ts` says so in its module doc: callers ask
//! `resolveCharacterKind` / `isAssistantCharacter` "rather than testing the raw
//! field, so a third kind, or a change to the default, lands here alone". A
//! hand comparison against a kind literal is how a default drifts: one site
//! treats an absent value as roleplay, another as assistant, and a third kind
//! is missed in half the places that branch. v5's home is
//! `crates/quilltap-core/src/characters/kind.rs` — the ONE exempt file.
//!
//! The census scans every production `.rs` file of core, web, host, cli and
//! tauri (`production_zone` — `#[cfg(test)]` items removed — with comments
//! blanked and string literals KEPT, because the thing it searches for IS a
//! literal). Two arms:
//!
//! - **(i) the literal arm** — v4's regex, Rust-shaped: `kind == "assistant"`,
//!   `"roleplay" != x.kind`, `kind == Some("assistant")`, `kind ==
//!   json!("assistant")`; plus `matches!(… kind …, … "roleplay" | "assistant"
//!   …)` and a `match` on a `kind` scrutinee with a kind-literal arm.
//! - **(ii) the typed arm** — a comparison against `CharacterKind::Roleplay` /
//!   `CharacterKind::Assistant` (`==` / `!=` either side, inside `matches!`, a
//!   `match` arm, an `if let`). v4 compares strings because TS has no enum; in
//!   Rust a typed comparison is the same drift, so callers branch on the
//!   `KindPolicy` fields `character_kind_policy` derives, never on the kind
//!   itself (the planner's R-2). Constructing a kind (`kind:
//!   CharacterKind::Roleplay`) is not a comparison and is allowed.
//!
//! v4's five probes, translated, plus typed probes, are the self-test; one
//! committed evasion fixture per form under
//! `tests/fixtures/character_kind_chokepoint_evasions/` proves each trips.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test character_kind_chokepoint_census

mod source_census;

use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;
use source_census::{next_token, production_zone, rust_sources};

/// The chokepoint — v4's `CHOKEPOINT = 'lib/characters/kind.ts'`.
const CHOKEPOINT: &str = "crates/quilltap-core/src/characters/kind.rs";

const CRATES: &[&str] = &[
    "crates/quilltap-core/src",
    "crates/quilltap-web/src",
    "crates/quilltap-host/src",
    "crates/quilltap-cli/src",
    "crates/quilltap-tauri/src",
];

const KIND_LIT: &str = r#""(?:roleplay|assistant)""#;

/// (i) v4's `KIND_COMPARISON`, Rust-shaped (the survey's §5.4 form).
static LITERAL_EQ: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"\bkind\b[^;\n]{{0,60}}?(?:==|!=)\s*(?:Some\()?(?:&?json!\()?{KIND_LIT}|{KIND_LIT}\s*(?:==|!=)[^;\n]{{0,40}}\bkind\b"#
    ))
    .unwrap()
});
/// (i) `matches!(<something with kind>, … "roleplay" / "assistant" …)`.
static LITERAL_MATCHES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r#"\bmatches!\s*\([^;]*?\bkind\b[^;]*?{KIND_LIT}"#)).unwrap()
});
/// (i) `match <scrutinee naming kind> { … "assistant" => …` — the arm must
/// follow the scrutinee's `{` within the same block (no `}` in between).
static LITERAL_MATCH_ARM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"\bmatch\s[^{{;]*?\bkind\b[^{{;]*\{{[^}}]*?{KIND_LIT}[^=}}]*=>"#
    ))
    .unwrap()
});
/// (ii) the typed arm: a comparison against a `CharacterKind` variant.
static TYPED: LazyLock<Regex> = LazyLock::new(|| {
    let v = r"CharacterKind::(?:Roleplay|Assistant)\b";
    Regex::new(&format!(
        r"(?:==|!=)\s*&?\s*{v}|{v}\s*(?:==|!=|=>|\|[^|])|\bmatches!\s*\([^;]*?{v}|\blet\s+(?:Some\()?{v}"
    ))
    .unwrap()
});

/// The arms, by name.
fn offences(code: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    if LITERAL_EQ.is_match(code) {
        out.push("literal-eq");
    }
    if LITERAL_MATCHES.is_match(code) {
        out.push("literal-matches");
    }
    if LITERAL_MATCH_ARM.is_match(code) {
        out.push("literal-match-arm");
    }
    if TYPED.is_match(code) {
        out.push("typed");
    }
    out
}

/// The zone with every COMMENT blanked and every string literal KEPT (the
/// literal arm searches for a literal; a comment naming one is no offence).
fn without_comments(zone: &str) -> String {
    let mut out = String::with_capacity(zone.len());
    let mut i = 0usize;
    while i < zone.len() {
        let (end, is_code) = next_token(zone, i);
        let end = end.max(i + 1).min(zone.len());
        let tok = &zone[i..end];
        if !is_code && (tok.starts_with("//") || tok.starts_with("/*")) {
            // Keep the newlines so line numbers survive.
            out.extend(tok.chars().map(|c| if c == '\n' { '\n' } else { ' ' }));
        } else {
            out.push_str(tok);
        }
        i = end;
    }
    out
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .to_path_buf()
}

/// `(path, line, arm, text)` for every offence; multi-line constructs are
/// reported at the line their match starts on.
fn census() -> (usize, Vec<String>) {
    let root = repo_root();
    let mut scanned = 0usize;
    let mut offenders = Vec::new();
    for dir in CRATES {
        let dir = root.join(dir);
        if !dir.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        rust_sources(&dir, &mut files);
        for f in files {
            let rel = f
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if rel == CHOKEPOINT {
                continue;
            }
            scanned += 1;
            let code = without_comments(&production_zone(&std::fs::read_to_string(&f).unwrap()));
            for re in [
                &*LITERAL_EQ,
                &*LITERAL_MATCHES,
                &*LITERAL_MATCH_ARM,
                &*TYPED,
            ] {
                for m in re.find_iter(&code) {
                    let line = code[..m.start()].matches('\n').count() + 1;
                    let text: String = m.as_str().split_whitespace().collect::<Vec<_>>().join(" ");
                    offenders.push(format!("{rel}:{line}: {text}"));
                }
            }
        }
    }
    offenders.sort();
    offenders.dedup();
    (scanned, offenders)
}

#[test]
fn a_kind_is_compared_only_in_characters_kind_rs() {
    let (scanned, offenders) = census();
    assert!(
        scanned > 500,
        "the census scanned only {scanned} files — the walk is broken"
    );
    assert!(
        offenders.is_empty(),
        "a character kind is compared outside {CHOKEPOINT} — ask \
         `resolve_character_kind` / `is_assistant_character`, or branch on \
         `character_kind_policy(…)`'s fields:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_chokepoint_itself_is_where_the_comparisons_live() {
    // The exemption is load-bearing: kind.rs does compare (its `match` arms),
    // so the walk would trip on it if the exemption were lost.
    let src = std::fs::read_to_string(repo_root().join(CHOKEPOINT)).unwrap();
    let code = without_comments(&production_zone(&src));
    assert!(
        !offences(&code).is_empty(),
        "kind.rs no longer compares a kind — is the census looking at the right file?"
    );
}

/// v4's five probes (`kind-chokepoint.test.ts`), translated to Rust, plus the
/// typed probes and a JSON KEY that must not trip.
#[test]
fn the_fence_pattern_catches_the_comparisons_it_is_meant_to() {
    let trips = [
        r#"if character.kind == "assistant" {"#,
        r#"c.kind != "roleplay""#,
        r#""assistant" == character.kind"#,
        r#"if obj.get("kind").and_then(Value::as_str) == Some("assistant") {"#,
        r#"if patch["kind"] == json!("roleplay") {"#,
        r#"matches!(c.get("kind").and_then(Value::as_str), Some("roleplay" | "assistant"))"#,
        "match c.kind.as_str() {\n    \"assistant\" => 1,\n    _ => 0,\n}",
        "if kind == CharacterKind::Assistant {",
        "CharacterKind::Roleplay != policy.kind",
        "matches!(kind, CharacterKind::Assistant)",
        "match k {\n    CharacterKind::Roleplay => a(),\n    CharacterKind::Assistant => b(),\n}",
        "if let CharacterKind::Assistant = kind {",
    ];
    for probe in trips {
        assert!(!offences(probe).is_empty(), "must trip: {probe}");
    }
    let clean = [
        r#"descriptor.kind == "markdown""#,
        r#"message.role == "assistant""#,
        "policy.tool_rules == ToolRulesVariant::Assistant",
        r#"let k = kind_or_default(obj.get("kind"));"#,
        r#"obj.insert("kind".into(), json!(kind.as_str()));"#,
        "kind: CharacterKind::Roleplay,",
        r#"match role {
    "assistant" => 1,
    _ => 0,
}"#,
    ];
    for probe in clean {
        assert!(
            offences(probe).is_empty(),
            "must NOT trip: {probe} ({:?})",
            offences(probe)
        );
    }
}

#[test]
fn a_comment_or_a_test_module_is_no_offence() {
    let src = r#"
// if c.kind == "assistant" { … } — prose only
pub fn fine() {}

#[cfg(test)]
mod tests {
    #[test]
    fn t() { assert!(c.kind == "assistant"); }
}
"#;
    assert!(offences(&without_comments(&production_zone(src))).is_empty());
}

/// One committed evasion fixture per arm; each must trip exactly its arm.
#[test]
fn the_census_trips_on_every_committed_evasion() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/character_kind_chokepoint_evasions");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".rs.txt"))
        .collect();
    names.sort();
    let want = [
        ("literal_eq.rs.txt", "literal-eq"),
        ("literal_match_arm.rs.txt", "literal-match-arm"),
        ("literal_matches.rs.txt", "literal-matches"),
        ("typed_eq.rs.txt", "typed"),
        ("typed_match_arm.rs.txt", "typed"),
    ];
    assert_eq!(
        names,
        want.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>(),
        "the committed evasion forms"
    );
    for (name, arm) in want {
        let src = std::fs::read_to_string(dir.join(name)).unwrap();
        let got = offences(&without_comments(&production_zone(&src)));
        assert!(
            got.contains(&arm),
            "{name}: expected the {arm} arm, got {got:?}"
        );
    }
}
