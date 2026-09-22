//! Tier-1 differential: the sync's MANIFEST and SIDECAR modules (v4
//! `lib/mount-index/sync/{manifest,sidecar}.ts`, `23da0b322`) vs
//! `quilltap_core::services::mount_index::sync::{manifest,sidecar}`.
//!
//! Four row kinds, all against v4's REAL modules:
//!
//!   - `sidecar-path` / `sidecar-text` / `descriptions-equal` — the pure string
//!     functions. The whitespace rows are why this family exists: v4 strips with
//!     `/\s+$/`, whose class is ECMAScript's `\s` and NOT Rust's
//!     `char::is_whitespace`. The two disagree on exactly two characters
//!     (U+FEFF, which JS strips and Rust does not; U+0085, which Rust strips and
//!     JS does not), and a caption ending in either would round-trip differently
//!     for ever — the sha is the comparison currency, so a one-character
//!     disagreement is a permanent false "edited".
//!   - `manifest-bytes` — what `write_manifest` puts on disk, byte for byte
//!     (`JSON.stringify(m, null, 2)` + a trailing newline), so key order,
//!     indent, and the `null` vs absent distinction on the two nullable fields
//!     are all in the comparand. Plus the base the planner then sees.
//!   - `manifest-read` — the degrade-to-null warnings WITH THEIR WORDING and the
//!     mismatch sentence. Zod's issue message reaches the operator, so it is
//!     part of the port; the first draft of `parse_manifest` guessed at three of
//!     these four shapes and this corpus is what caught them.
//!
//! Regenerate the oracle (from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   npx tsx $V5W/harness/oracle/cases/sync-manifest-sidecar.ts > /tmp/oracle-sync-manifest-sidecar.ndjson
//! Run:
//!   QT_ORACLE_SYNC_MANIFEST=/tmp/oracle-sync-manifest-sidecar.ndjson cargo test -p quilltap-harness --test sync_manifest_sidecar_equivalence -- --nocapture

use std::collections::BTreeMap;

use quilltap_core::services::mount_index::sync::manifest::{
    base_from_manifest, manifest_path_for, read_manifest, render_manifest, write_manifest,
    ReadManifestError,
};
use quilltap_core::services::mount_index::sync::sidecar::{
    description_sha256, descriptions_equal, is_sidecar_path, parse_sidecar, partner_path_for,
    render_sidecar, sidecar_path_for,
};
use quilltap_core::services::mount_index::sync::types::{ManifestEntry, SyncManifest};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "sidecar-path")]
    SidecarPath {
        id: String,
        value: String,
        #[serde(rename = "sidecarPathFor")]
        sidecar_path_for: String,
        #[serde(rename = "isSidecarPath")]
        is_sidecar_path: bool,
        #[serde(rename = "partnerPathFor")]
        partner_path_for: Option<String>,
    },
    #[serde(rename = "sidecar-text")]
    SidecarText {
        id: String,
        value: String,
        #[serde(rename = "renderSidecar")]
        render_sidecar: String,
        #[serde(rename = "parseSidecar")]
        parse_sidecar: String,
        #[serde(rename = "descriptionSha256")]
        description_sha256: String,
    },
    #[serde(rename = "descriptions-equal")]
    DescriptionsEqual {
        id: String,
        a: Option<String>,
        b: Option<String>,
        out: bool,
    },
    #[serde(rename = "manifest-bytes")]
    ManifestBytes {
        id: String,
        manifest: SyncManifest,
        bytes: String,
        #[serde(rename = "readBackWarnings")]
        read_back_warnings: Vec<String>,
        base: Value,
    },
    #[serde(rename = "manifest-read")]
    ManifestRead {
        id: String,
        write: Option<String>,
        #[serde(rename = "storeId")]
        store_id: String,
        manifest: Option<SyncManifest>,
        warnings: Vec<String>,
        error: Option<String>,
        #[serde(rename = "errorName")]
        error_name: Option<String>,
        base: Value,
    },
}

/// The base map as the oracle emits it: a JS object, insertion-ordered.
fn base_as_json(
    base: &quilltap_core::services::mount_index::sync::types::OrderedMap<ManifestEntry>,
) -> Value {
    serde_json::to_value(base).expect("serialize the base")
}

#[test]
fn sync_manifest_and_sidecar_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SYNC_MANIFEST") else {
        eprintln!("SKIP: set QT_ORACLE_SYNC_MANIFEST to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("cannot read oracle {oracle_path}: {e}"));

    let scratch = std::env::temp_dir().join(format!("qt-sync-manifest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");

    let mut mismatches: Vec<String> = Vec::new();
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut n = 0usize;
    let mut case = 0usize;

    let check = |label: String, got: String, want: String, out: &mut Vec<String>| {
        if got != want {
            out.push(format!("{label}\n      v4: {want:?}\n      v5: {got:?}"));
        }
    };

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        n += 1;
        case += 1;
        match row {
            Row::SidecarPath {
                id,
                value,
                sidecar_path_for: want_path,
                is_sidecar_path: want_is,
                partner_path_for: want_partner,
            } => {
                *seen.entry("sidecar-path").or_default() += 1;
                check(
                    format!("[sidecar-path/{id}] sidecarPathFor"),
                    sidecar_path_for(&value),
                    want_path,
                    &mut mismatches,
                );
                let got_is = is_sidecar_path(&value);
                if got_is != want_is {
                    mismatches.push(format!(
                        "[sidecar-path/{id}] isSidecarPath: v4 {want_is}, v5 {got_is}"
                    ));
                }
                let got_partner = partner_path_for(&value);
                if got_partner != want_partner {
                    mismatches.push(format!(
                        "[sidecar-path/{id}] partnerPathFor\n      v4: {want_partner:?}\n      v5: {got_partner:?}"
                    ));
                }
            }
            Row::SidecarText {
                id,
                value,
                render_sidecar: want_render,
                parse_sidecar: want_parse,
                description_sha256: want_sha,
            } => {
                *seen.entry("sidecar-text").or_default() += 1;
                check(
                    format!("[sidecar-text/{id}] renderSidecar"),
                    render_sidecar(&value),
                    want_render,
                    &mut mismatches,
                );
                check(
                    format!("[sidecar-text/{id}] parseSidecar"),
                    parse_sidecar(&value),
                    want_parse,
                    &mut mismatches,
                );
                check(
                    format!("[sidecar-text/{id}] descriptionSha256"),
                    description_sha256(&value),
                    want_sha,
                    &mut mismatches,
                );
            }
            Row::DescriptionsEqual { id, a, b, out } => {
                *seen.entry("descriptions-equal").or_default() += 1;
                let got = descriptions_equal(a.as_deref(), b.as_deref());
                if got != out {
                    mismatches.push(format!("[descriptions-equal/{id}]: v4 {out}, v5 {got}"));
                }
            }
            Row::ManifestBytes {
                id,
                manifest,
                bytes,
                read_back_warnings,
                base,
            } => {
                *seen.entry("manifest-bytes").or_default() += 1;
                // The pure renderer first: these are the bytes, and a mismatch
                // here is a key-order or indent defect, not an IO one.
                check(
                    format!("[manifest-bytes/{id}] rendered bytes"),
                    render_manifest(&manifest),
                    bytes.clone(),
                    &mut mismatches,
                );
                // …then the atomic write actually performed, read back.
                let dir = scratch.join(format!("w-{case}"));
                std::fs::create_dir_all(&dir).expect("case dir");
                write_manifest(&dir, &manifest).expect("write the manifest");
                let on_disk =
                    std::fs::read_to_string(manifest_path_for(&dir)).expect("read it back");
                check(
                    format!("[manifest-bytes/{id}] bytes on disk"),
                    on_disk,
                    bytes,
                    &mut mismatches,
                );
                // The temp file must be gone — the rename is the whole point.
                let temp = dir.join(".quilltap-sync.json.tmp");
                if temp.exists() {
                    mismatches.push(format!(
                        "[manifest-bytes/{id}] the temp file survived the rename"
                    ));
                }

                let mut warnings: Vec<String> = Vec::new();
                let read_back = read_manifest(&dir, &manifest.store_id, &mut warnings)
                    .expect("read back the manifest we just wrote");
                if warnings != read_back_warnings {
                    mismatches.push(format!(
                        "[manifest-bytes/{id}] read-back warnings\n      v4: {read_back_warnings:?}\n      v5: {warnings:?}"
                    ));
                }
                let got_base = base_as_json(&base_from_manifest(read_back.as_ref()));
                if got_base != base {
                    mismatches.push(format!(
                        "[manifest-bytes/{id}] base\n      v4: {base}\n      v5: {got_base}"
                    ));
                }
            }
            Row::ManifestRead {
                id,
                write,
                store_id,
                manifest: want_manifest,
                warnings: want_warnings,
                error: want_error,
                error_name: want_error_name,
                base,
            } => {
                *seen.entry("manifest-read").or_default() += 1;
                let dir = scratch.join(format!("r-{case}"));
                std::fs::create_dir_all(&dir).expect("case dir");
                if let Some(body) = &write {
                    std::fs::write(dir.join(".quilltap-sync.json"), body).expect("seed");
                }
                let mut warnings: Vec<String> = Vec::new();
                let result = read_manifest(&dir, &store_id, &mut warnings);

                let (got_manifest, got_error, got_error_name): (
                    Option<SyncManifest>,
                    Option<String>,
                    Option<String>,
                ) = match result {
                    Ok(m) => (m, None, None),
                    Err(ReadManifestError::Mismatch(e)) => (
                        None,
                        Some(e.to_string()),
                        Some("ManifestMismatchError".to_string()),
                    ),
                    Err(ReadManifestError::Io(e)) => {
                        (None, Some(e.to_string()), Some("Error".to_string()))
                    }
                };

                if warnings != want_warnings {
                    mismatches.push(format!(
                        "[manifest-read/{id}] warnings\n      v4: {want_warnings:?}\n      v5: {warnings:?}"
                    ));
                }
                if got_error != want_error {
                    mismatches.push(format!(
                        "[manifest-read/{id}] error\n      v4: {want_error:?}\n      v5: {got_error:?}"
                    ));
                }
                if got_error_name != want_error_name {
                    mismatches.push(format!(
                        "[manifest-read/{id}] error name\n      v4: {want_error_name:?}\n      v5: {got_error_name:?}"
                    ));
                }
                if got_manifest != want_manifest {
                    mismatches.push(format!(
                        "[manifest-read/{id}] manifest\n      v4: {want_manifest:?}\n      v5: {got_manifest:?}"
                    ));
                }
                let got_base = base_as_json(&base_from_manifest(got_manifest.as_ref()));
                if got_base != base {
                    mismatches.push(format!(
                        "[manifest-read/{id}] base\n      v4: {base}\n      v5: {got_base}"
                    ));
                }
            }
        }
    }

    let _ = std::fs::remove_dir_all(&scratch);

    assert!(
        mismatches.is_empty(),
        "{} disagreements over {n} rows:\n  {}",
        mismatches.len(),
        mismatches.join("\n  ")
    );
    assert_eq!(
        n, 74,
        "expected the full corpus (74 rows), saw {n} — stale oracle?"
    );
    for kind in [
        "sidecar-path",
        "sidecar-text",
        "descriptions-equal",
        "manifest-bytes",
        "manifest-read",
    ] {
        assert!(
            seen.get(kind).copied().unwrap_or(0) > 0,
            "no `{kind}` rows in the corpus ({seen:?})"
        );
    }
    eprintln!("OK: manifest + sidecar matched v4 on {n} rows; kinds: {seen:?}");
}
