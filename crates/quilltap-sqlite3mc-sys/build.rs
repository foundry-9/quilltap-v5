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
//! HOW TO SUPPLY THE AMALGAMATION (pick one; the build errors with guidance if
//! neither is present):
//!
//!   A) Vendor it (recommended for reproducibility): download
//!      `sqlite3mc-<ver>-amalgamation.zip` from
//!      https://github.com/utelle/SQLite3MultipleCiphers/releases
//!      (use a 2.3.x release to match better-sqlite3-multiple-ciphers@12.11.1),
//!      unzip, and drop `sqlite3mc_amalgamation.c` + `sqlite3mc_amalgamation.h`
//!      into `crates/quilltap-sqlite3mc-sys/vendor/`.
//!
//!   B) Point at an existing copy:
//!      SQLITE3MC_AMALGAMATION_DIR=/path/to/dir   (containing the two files)
//!
//! The amalgamation already #includes SQLite itself and every cipher; sqleet
//! (ChaCha20-Poly1305) is the default cipher, so no extra defines are required
//! to open Quilltap's databases. We still set a couple of recommended flags.

use std::path::PathBuf;

fn main() {
    let dir = locate_amalgamation();
    let c_file = dir.join("sqlite3mc_amalgamation.c");

    if !c_file.exists() {
        panic!(
            "\n\nSQLite3MC amalgamation not found.\n\
             Expected: {}\n\n\
             Fix: download sqlite3mc-2.3.x-amalgamation.zip from\n\
             https://github.com/utelle/SQLite3MultipleCiphers/releases\n\
             and unzip sqlite3mc_amalgamation.c + .h into\n\
             crates/quilltap-core/vendor/  (or set SQLITE3MC_AMALGAMATION_DIR).\n",
            c_file.display()
        );
    }

    let mut build = cc::Build::new();
    build
        .file(&c_file)
        .include(&dir)
        // Recommended SQLite build flags (mirror common bundled configs).
        .define("SQLITE_ENABLE_COLUMN_METADATA", None)
        .define("SQLITE_ENABLE_FTS5", None)
        .define("SQLITE_ENABLE_RTREE", None)
        .define("SQLITE_ENABLE_DBSTAT_VTAB", None)
        .define("SQLITE_USE_URI", None)
        // SQLite3MC: ensure the encryption codec is compiled in. (Default in
        // the amalgamation, set explicitly to be safe.)
        .define("SQLITE_HAS_CODEC", None)
        .define("SQLITE3MC_USE_RANDOM_FILL_MEMORY", None)
        .warnings(false);

    // The amalgamation links against the C math lib on some platforms.
    if cfg!(not(target_os = "windows")) {
        println!("cargo:rustc-link-lib=dylib=m");
    }

    build.compile("sqlite3"); // produces libsqlite3.a; libsqlite3-sys links it

    // Tell libsqlite3-sys / rusqlite to use our header for bindgen and to link
    // the static lib we just built (cc already emits the link search path).
    println!("cargo:rerun-if-changed={}", c_file.display());
    println!("cargo:rerun-if-env-changed=SQLITE3MC_AMALGAMATION_DIR");
    // SQLITE3_LIB_DIR + SQLITE3_INCLUDE_DIR steer libsqlite3-sys to our build.
    let out = std::env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:include={}", dir.display());
}

fn locate_amalgamation() -> PathBuf {
    if let Ok(d) = std::env::var("SQLITE3MC_AMALGAMATION_DIR") {
        return PathBuf::from(d);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor")
}
