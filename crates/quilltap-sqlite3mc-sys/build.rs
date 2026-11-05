//! Compile the SQLite3MultipleCiphers amalgamation and link it as `sqlite3`,
//! so rusqlite/libsqlite3-sys bind against a ChaCha20/sqleet-capable library
//! instead of stock SQLite or AES-only SQLCipher. This is the workspace's single
//! source of the linked `sqlite3` — quilltap-core depends on this crate, so every
//! crate that depends on quilltap-core (the harness, future cli/tauri) inherits
//! the correct cipher from here.
//!
//! WHY ITS OWN CRATE: Cargo's build-script fingerprint includes the package
//! version, so a version bump throws away the cached `libsqlite3.a` and
//! recompiles the 12 MB amalgamation from scratch (~3–4 min, one translation
//! unit). This crate's version is **pinned and never bumped** — the per-commit
//! version bumps live on quilltap-core/quilltap-harness, not here — so the
//! expensive C compile caches permanently across commits. Do not bump this
//! crate's version unless the amalgamation source or build flags actually change.
//!
//! WHAT IS VENDORED, AND WHY THESE BYTES (P4.D365, 2026-11): v4's own driver,
//! `better-sqlite3-multiple-ciphers`, is what defines "the build v4 runs". It
//! does NOT ship utelle's release amalgamation: its `deps/update-sqlite3mc.sh`
//! builds SQLite's amalgamation from the canonical source with
//! `make OPTIONS="<every define in defines.gypi>"`, patches it with
//! SQLite3MC's scripts, and re-amalgamates — so its `sqlite3.c` is GENERATED
//! WITH v4's defines (its parser is built with
//! `-DSQLITE_ENABLE_UPDATE_DELETE_LIMIT`; it defines
//! `SQLITE_UDL_CAPABLE_PARSER 1`). SQLite documents that grammar-affecting and
//! `SQLITE_OMIT_*` options belong to amalgamation generation, so "the driver's
//! bytes" and "the driver's defines" are ONE decision. utelle's release
//! amalgamation is option-free and is NOT v4's bytes (the 2.3.5 vendor this
//! replaced was utelle's — measured: its `.c` differed from the 12.11.1
//! driver's in exactly the UDL parser).
//!
//! HOW TO SUPPLY THE AMALGAMATION (pick one; the build errors with guidance if
//! neither is present):
//!
//!   A) Vendor it (the committed state): from the
//!      `better-sqlite3-multiple-ciphers` version v4's lockfile pins (13.0.3 at
//!      v4 `624d0d25c`: SQLite3 Multiple Ciphers 2.4.0 on SQLite 3.53.4), copy
//!      byte-exact, with no line-ending or other normalisation:
//!        deps/sqlite3/sqlite3.c -> vendor/sqlite3mc_amalgamation.c
//!        deps/sqlite3/sqlite3.h -> vendor/sqlite3mc_amalgamation.h
//!        deps/defines.gypi      -> vendor/defines.gypi
//!      (`deps/sqlite3/sqlite3ext.h` is not needed: v5 loads no extensions.)
//!      The three move TOGETHER — a re-vendor is one commit, and this crate's
//!      version bumps once for it.
//!
//!   B) Point at an existing copy:
//!      SQLITE3MC_AMALGAMATION_DIR=/path/to/dir   (containing the three files)
//!
//! THE DEFINES are read from `defines.gypi` itself (every `'NAME'` /
//! `'NAME=VALUE'` entry of its `'defines': [ … ]` list, in file order), so v4's
//! own file is the one source of truth. A define v5 cannot yet take is
//! subtracted through [`HELD`] — each entry NAMED with its measured reason and
//! printed as a `cargo:warning`. The amalgamation already #includes SQLite and
//! every cipher; sqleet (ChaCha20-Poly1305) is the default, so no `cipher=`
//! pragma or extra define is needed to open Quilltap's databases.

use std::path::{Path, PathBuf};

/// Defines held back from `defines.gypi`: `(define name, the named reason)`.
/// Filled only by P4.D365's decision rule (a define that alone explains a red
/// in a v5 test-fixture premise, never a production divergence from v4).
/// Must equal the `HELD` table of
/// `crates/quilltap-harness/tests/sqlite_build_parity_equivalence.rs`.
///
/// - `SQLITE_DEFAULT_FOREIGN_KEYS` — `SQLITE_DEFAULT_FK_HELD`: with it,
///   `restore_vintage_state::annotations_restore_onto_a_vintage_instance_without_colliding`
///   fails `FOREIGN KEY constraint failed` seeding its fixture through a raw
///   connection (an annotation whose chat the vintage instance lacks); the
///   one-define flip back turns it green. v5's production writer already sets
///   `PRAGMA foreign_keys = ON` explicitly, so the engine matches v4 either
///   way; converging the default is a test-suite migration (the next round's).
const HELD: &[(&str, &str)] = &[("SQLITE_DEFAULT_FOREIGN_KEYS", "SQLITE_DEFAULT_FK_HELD")];

fn main() {
    let dir = locate_amalgamation();
    let c_file = dir.join("sqlite3mc_amalgamation.c");
    let h_file = dir.join("sqlite3mc_amalgamation.h");
    let gypi = dir.join("defines.gypi");

    if !c_file.exists() || !h_file.exists() || !gypi.exists() {
        panic!(
            "\n\nSQLite3MC amalgamation not found.\n\
             Expected: {}, {} and {}\n\n\
             Fix: copy deps/sqlite3/sqlite3.c, deps/sqlite3/sqlite3.h and\n\
             deps/defines.gypi from the better-sqlite3-multiple-ciphers version\n\
             v4's package-lock.json pins, byte-exact, to\n\
             crates/quilltap-sqlite3mc-sys/vendor/ as sqlite3mc_amalgamation.c,\n\
             sqlite3mc_amalgamation.h and defines.gypi\n\
             (or set SQLITE3MC_AMALGAMATION_DIR).\n",
            c_file.display(),
            h_file.display(),
            gypi.display()
        );
    }

    let defines = read_defines(&gypi);
    let mut build = cc::Build::new();
    build.file(&c_file).include(&dir).warnings(false);
    for (name, value) in &defines {
        if let Some((_, reason)) = HELD.iter().find(|(held, _)| held == name) {
            println!("cargo:warning=quilltap-sqlite3mc-sys: HELD {name} ({reason})");
            continue;
        }
        build.define(name, value.as_deref());
    }
    for (held, _) in HELD {
        assert!(
            defines.iter().any(|(name, _)| name == held),
            "HELD {held} is not in defines.gypi — the hold is stale, remove it"
        );
    }

    // The amalgamation links against the C math lib on some platforms.
    if cfg!(not(target_os = "windows")) {
        println!("cargo:rustc-link-lib=dylib=m");
    }

    build.compile("sqlite3"); // produces libsqlite3.a; libsqlite3-sys links it

    println!("cargo:rerun-if-changed={}", c_file.display());
    println!("cargo:rerun-if-changed={}", h_file.display());
    println!("cargo:rerun-if-changed={}", gypi.display());
    println!("cargo:rerun-if-env-changed=SQLITE3MC_AMALGAMATION_DIR");
    // SQLITE3_LIB_DIR + SQLITE3_INCLUDE_DIR steer libsqlite3-sys to our build.
    let out = std::env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:include={}", dir.display());
}

/// `defines.gypi`'s `'defines': [ … ]` entries, in file order, as
/// `(name, Some(value) | None)`. A tiny line parser: one quoted entry per
/// line, `#` comments and everything outside the list ignored. Refuses an
/// empty list (a parse that found nothing must not build an option-free
/// library silently).
fn read_defines(gypi: &Path) -> Vec<(String, Option<String>)> {
    let text = std::fs::read_to_string(gypi)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", gypi.display()));
    let mut in_list = false;
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('#') {
            continue;
        }
        if !in_list {
            if line.starts_with("'defines'") && line.ends_with('[') {
                in_list = true;
            }
            continue;
        }
        if line.starts_with(']') {
            in_list = false;
            continue;
        }
        let entry = line.trim_end_matches(',');
        let Some(inner) = entry.strip_prefix('\'').and_then(|e| e.strip_suffix('\'')) else {
            panic!("{}: unexpected line in 'defines': {raw:?}", gypi.display());
        };
        match inner.split_once('=') {
            Some((name, value)) => out.push((name.to_string(), Some(value.to_string()))),
            None => out.push((inner.to_string(), None)),
        }
    }
    assert!(
        !out.is_empty(),
        "{}: no 'defines' entries parsed",
        gypi.display()
    );
    out
}

fn locate_amalgamation() -> PathBuf {
    if let Ok(d) = std::env::var("SQLITE3MC_AMALGAMATION_DIR") {
        return PathBuf::from(d);
    }
    // Cargo sets CARGO_MANIFEST_DIR in the build script's RUNTIME environment,
    // and that is the copy to trust. `env!` bakes the path in at compile time —
    // and because this crate's version is pinned so the 12 MB C compile stays
    // cached, that baked path outlives whatever tree produced it. A lane
    // worktree sharing this `target/` dir compiles the build script once with
    // its own path inside it; delete the worktree and every later release build
    // in the MAIN tree dies looking for a vendor dir that no longer exists,
    // with nothing to invalidate the binary that remembers it. (Hit 2026-08-18,
    // after the P4.D86 worktree was removed.) The `env!` stays as the fallback
    // for a build script run outside Cargo.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string());
    PathBuf::from(manifest_dir).join("vendor")
}
