//! P4.D274 (v4 `ed8b15b50`, bug 186) — every production SQL that writes a
//! document-store NAME lives in a sanctioned file, by census.
//!
//! Store names are one case-insensitive namespace. v4's standing rule (its
//! CLAUDE.md bullet): pick a free name with `nextUniqueMountPointName`, never
//! insert one raw; a path that creates, renames or unlinks a vault calls the
//! reconcile rather than naming one by hand. In v5 the name writers are:
//!
//!   - `db/doc_mount_points.rs` — the repository (`create` / `update`), behind
//!     the name gate;
//!   - `db/store_name_reconcile.rs` — the reconcile's two-phase rename;
//!   - `db/mount_index_case_repair.rs` — the boot collision repair;
//!   - `services/builtin_mounts.rs` — the built-in re-provision (v4's raw
//!     migration INSERT), which picks a free name (R-B).
//!
//! A literal anywhere else that `INSERT`s into `doc_mount_points`, or
//! `UPDATE`s it setting `name`, is red: route it through the repository (or
//! extend this list with its reason). Standing on the shared source-census
//! lexer (`source_census`), which keeps string literals — the SQL IS a literal.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test doc_mount_points_name_writers_census

mod source_census;

use std::path::PathBuf;

use source_census::{contains_word, core_src_root, production_zone, rust_sources, string_literals};

const SANCTIONED: &[&str] = &[
    "quilltap-core/src/db/doc_mount_points.rs",
    "quilltap-core/src/db/mount_index_case_repair.rs",
    "quilltap-core/src/db/store_name_reconcile.rs",
    "quilltap-core/src/services/builtin_mounts.rs",
];

/// Does `sql` write a `doc_mount_points` name?
fn writes_a_store_name(sql: &str) -> bool {
    // The lexer hands back the literal's SOURCE text, so `\"` arrives escaped.
    let flat = sql
        .replace("\\\"", "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let upper = flat.to_uppercase();
    let table = flat.contains("doc_mount_points\"") || contains_word(&flat, "doc_mount_points");
    if !table {
        return false;
    }
    let insert = upper.contains("INSERT INTO \"DOC_MOUNT_POINTS\"")
        || upper.contains("INSERT INTO DOC_MOUNT_POINTS ")
        || upper.contains("INSERT INTO DOC_MOUNT_POINTS(")
        || upper.contains("INSERT OR REPLACE INTO DOC_MOUNT_POINTS")
        || upper.contains("INSERT OR REPLACE INTO \"DOC_MOUNT_POINTS\"");
    let update = (upper.contains("UPDATE \"DOC_MOUNT_POINTS\"")
        || upper.contains("UPDATE DOC_MOUNT_POINTS "))
        && (upper.contains("SET NAME")
            || upper.contains("SET \"NAME\"")
            || upper.contains(", NAME =")
            || upper.contains(", \"NAME\" ="));
    insert || update
}

#[test]
fn every_store_name_writer_is_sanctioned() {
    let crates = core_src_root()
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/")
        .to_path_buf();
    let mut offenders: Vec<String> = Vec::new();
    let mut sanctioned_seen: Vec<String> = Vec::new();
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
            let rel = f
                .strip_prefix(&crates)
                .expect("under crates/")
                .display()
                .to_string();
            let zone = production_zone(&std::fs::read_to_string(&f).expect("read source"));
            for lit in string_literals(&zone) {
                if !writes_a_store_name(lit) {
                    continue;
                }
                if SANCTIONED.contains(&rel.as_str()) {
                    if !sanctioned_seen.contains(&rel) {
                        sanctioned_seen.push(rel.clone());
                    }
                } else {
                    offenders.push(format!(
                        "{rel}: {}",
                        lit.split_whitespace().collect::<Vec<_>>().join(" ")
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a production SQL writes a document-store name outside the sanctioned writers — pick a \
         free name through the repository:\n{}",
        offenders.join("\n")
    );
    // The detector still sees the sanctioned writers (a lexer change that blinds
    // it would otherwise pass silently).
    sanctioned_seen.sort();
    assert_eq!(
        sanctioned_seen, SANCTIONED,
        "the census no longer sees every sanctioned writer"
    );
    eprintln!(
        "OK: every document-store name writer is sanctioned ({} sanctioned files seen).",
        sanctioned_seen.len()
    );
}

#[test]
fn the_detector_sees_both_write_shapes() {
    assert!(writes_a_store_name(
        "INSERT INTO \"doc_mount_points\" (id, name, createdAt) VALUES (?1, ?2, ?3)"
    ));
    assert!(writes_a_store_name(
        "UPDATE doc_mount_points SET name = ?1 WHERE id = ?2"
    ));
    assert!(writes_a_store_name(
        "UPDATE \"doc_mount_points\" SET \"name\" = ?1, \"updatedAt\" = ?2 WHERE \"id\" = ?3"
    ));
    assert!(!writes_a_store_name(
        "UPDATE doc_mount_points SET enabled = 0 WHERE id = ?1"
    ));
    assert!(!writes_a_store_name("SELECT name FROM doc_mount_points"));
    // The source-text spelling the lexer hands back (escaped quotes).
    assert!(writes_a_store_name(
        r#"INSERT INTO \"doc_mount_points\" (id, name) VALUES (?1, ?2)"#
    ));
}
