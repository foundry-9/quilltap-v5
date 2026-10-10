//! P4.D274 (v4 `ed8b15b50`, bug 186) — `CharactersRepository::delete` has
//! exactly its sanctioned production callers, by census.
//!
//! v4's `CharactersRepository.delete` reconciles store names after the row
//! goes (`characters.repository.ts:496-512`: INFO `Character deleted`, then
//! `reconcileVaultNames('character-deleted', id)`), so EVERY v4 character
//! delete retires the vault's name. v5's repository delete is main-only and
//! cannot reach the mount index, so the hook lives one level up:
//!
//!   - `db/character_vault.rs` — `delete_character_and_reconcile`, the ONE
//!     wrapper every per-character delete (the cascade delete, the `.qtap`
//!     import's overwrite) goes through;
//!   - `services/delete_all.rs` — the wipe, which deletes every character
//!     first and then replays v4's concurrent reconciles (R-F's measured
//!     cadence) itself.
//!
//! A new direct caller would delete a character without retiring its vault's
//! name: route it through the wrapper (or extend this list with its reason).
//! Standing on the shared source-census lexer (`source_census`).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test characters_delete_callers_census

mod source_census;

use std::path::PathBuf;

use source_census::{core_src_root, production_zone, rust_sources};

const SANCTIONED: &[&str] = &[
    "quilltap-core/src/db/character_vault.rs",
    "quilltap-core/src/services/delete_all.rs",
];

/// The files whose production zone calls `delete(` on a
/// `CharactersRepository` — directly (`CharactersRepository::new(x).delete(`)
/// or through a binding (`let r = CharactersRepository::new(x); … r.delete(`).
fn callers() -> Vec<String> {
    let crates = core_src_root()
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/")
        .to_path_buf();
    let mut out = Vec::new();
    for krate in [
        "quilltap-core",
        "quilltap-web",
        "quilltap-host",
        "quilltap-cli",
    ] {
        let mut files: Vec<PathBuf> = Vec::new();
        rust_sources(&crates.join(krate).join("src"), &mut files);
        files.sort();
        for f in files {
            let zone = production_zone(&std::fs::read_to_string(&f).expect("read source"));
            let rel = f
                .strip_prefix(&crates)
                .expect("under crates/")
                .display()
                .to_string();
            if calls_delete(&zone) {
                out.push(rel);
            }
        }
    }
    out
}

fn calls_delete(zone: &str) -> bool {
    const NEW: &str = "CharactersRepository::new(";
    let mut bindings: Vec<String> = Vec::new();
    for (at, _) in zone.match_indices(NEW) {
        // The constructor's closing paren (one level of nesting is enough for
        // `CharactersRepository::new(main)` / `new(&conn)`).
        let rest = &zone[at + NEW.len()..];
        let Some(close) = rest.find(')') else {
            continue;
        };
        if rest[close + 1..].trim_start().starts_with(".delete(") {
            return true;
        }
        // `let <name> = CharactersRepository::new(…)`.
        let before = zone[..at].trim_end();
        if let Some(stmt) = before.strip_suffix('=') {
            let stmt = stmt.trim_end();
            let name: String = stmt
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if !name.is_empty() {
                bindings.push(name);
            }
        }
    }
    bindings
        .iter()
        .any(|b| zone.contains(&format!("{b}.delete(")))
}

#[test]
fn characters_repository_delete_has_only_its_sanctioned_callers() {
    let found = callers();
    assert_eq!(
        found, SANCTIONED,
        "a production `CharactersRepository::delete` caller outside the sanctioned list deletes a \
         character without v4's store-name reconcile — route it through \
         `character_vault::delete_character_and_reconcile`"
    );
    eprintln!(
        "OK: CharactersRepository::delete has exactly its {} sanctioned callers.",
        SANCTIONED.len()
    );
}

/// The detector itself: the three spellings it must see, and a chat delete it
/// must not.
#[test]
fn the_detector_sees_both_call_shapes() {
    assert!(calls_delete("CharactersRepository::new(main).delete(id)?;"));
    assert!(calls_delete(
        "let repo = CharactersRepository::new(main);\nfor id in ids { repo.delete(id)?; }"
    ));
    assert!(calls_delete(
        "let r = CharactersRepository::new(&conn); r.delete(&x)"
    ));
    assert!(!calls_delete(
        "let repo = ChatsRepository::new(main); repo.delete(id)"
    ));
    assert!(!calls_delete(
        "CharactersRepository::new(main).update(id, &u)"
    ));
}
