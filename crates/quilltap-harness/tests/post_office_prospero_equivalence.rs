//! Tier-1 differential for the W4.6b Prospero notifications PURE content
//! builders — the byte-exact visible + opaque bodies, diffed against v4's REAL
//! exports (`harness/oracle/cases/post-office-prospero.ts`).
//!
//! Covers every branch + opaque variant of the five families:
//! `buildConnectionProfileChangeContent`, `buildGeneralContextContent`,
//! `buildCombinedContextContent`, `buildGroupAndVaultWhisperContent`,
//! `buildCarinaErrorContent`. The `post*` functions' `chat_messages` rows are
//! covered by the central combined tier-3 the parent owns; the DB loaders are
//! stateful and out of scope for this tier-1 differential.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5/harness/oracle/cases/post-office-prospero.ts \
//!       > /tmp/oracle-po-prospero.ndjson
//! Run:
//!   QT_ORACLE_PO_PROSPERO=/tmp/oracle-po-prospero.ndjson \
//!     cargo test -p quilltap-harness --test post_office_prospero_equivalence

use serde_json::Value;

use quilltap_core::services::prospero_notifications::{
    build_carina_error_content, build_carina_error_opaque_content, build_combined_context_content,
    build_combined_context_opaque_content, build_connection_profile_change_content,
    build_connection_profile_change_opaque_content, build_general_context_content,
    build_general_context_opaque_content, build_group_and_vault_whisper_content,
    build_group_and_vault_whisper_opaque_content, CarinaErrorKind, ProsperoDocumentStoreInfo,
    ProsperoGeneralContext, ProsperoProjectContext,
};

// ---- shared fixtures (mirror of the TS oracle) ----

fn general() -> ProsperoGeneralContext {
    ProsperoGeneralContext {
        mount_point_id: "gen-mp-1".into(),
        name: "Quilltap General".into(),
        mount_type: "database".into(),
    }
}
fn general_backtick() -> ProsperoGeneralContext {
    ProsperoGeneralContext {
        mount_point_id: "gen-mp-2".into(),
        name: "My `Shelf`".into(),
        mount_type: "database".into(),
    }
}

fn store(
    id: &str,
    name: &str,
    mount_type: &str,
    store_type: &str,
    is_official: bool,
    enabled: bool,
) -> ProsperoDocumentStoreInfo {
    ProsperoDocumentStoreInfo {
        id: id.into(),
        name: name.into(),
        mount_type: mount_type.into(),
        store_type: store_type.into(),
        is_official,
        enabled,
    }
}

fn proj(
    name: &str,
    description: Option<&str>,
    stores: Vec<ProsperoDocumentStoreInfo>,
) -> ProsperoProjectContext {
    ProsperoProjectContext {
        name: name.into(),
        description: description.map(str::to_string),
        document_stores: stores,
    }
}

fn conn_case(id: &str) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    match id {
        "both" => ("Ada", Some("Claude"), Some("GPT")),
        "old-null" => ("Ada", None, Some("GPT")),
        "new-null" => ("Bea", Some("Claude"), None),
        "both-null" => ("Cy", None, None),
        "empty-strings" => ("Dot", Some(""), Some("")),
        other => panic!("unknown conn case {other}"),
    }
}

fn general_case(id: &str) -> ProsperoGeneralContext {
    match id {
        "plain" => general(),
        "backtick-name" => general_backtick(),
        other => panic!("unknown general case {other}"),
    }
}

fn proj_full() -> ProsperoProjectContext {
    proj(
        "Novel",
        Some("  A grand tale.  "),
        vec![
            store(
                "mp-off",
                "Official Shelf",
                "database",
                "documents",
                true,
                true,
            ),
            store("mp-a", "Alpha", "filesystem", "documents", false, true),
            store("mp-b", "Beta", "obsidian", "character", false, false),
        ],
    )
}

fn combined_case(
    id: &str,
) -> (
    Option<ProsperoProjectContext>,
    Option<ProsperoGeneralContext>,
) {
    match id {
        "null-project-with-general" => (None, Some(general())),
        "full-with-general" => (Some(proj_full()), Some(general())),
        "full-no-general" => (Some(proj_full()), None),
        "desc-only-general" => (
            Some(proj("DescOnly", Some("Only a description."), vec![])),
            Some(general()),
        ),
        // P4.D103 (v4 `8f868109`): `instructions` left the whisper's context type
        // entirely. This case keeps its name as the RETIRED-SECTION TRIPWIRE — a
        // project whose only former content was instructions now has none at all,
        // so the whisper must come back empty. (Proven load-bearing: restoring
        // v5's old block reddens this row with the whole `**Project
        // instructions:**` whisper on the left and `""` on the right.)
        "instr-only-no-general" => (Some(proj("InstrOnly", Some("   "), vec![])), None),
        "stores-only-general" => (
            Some(proj(
                "StoresOnly",
                None,
                vec![store("mp-x", "Xeno", "database", "documents", false, true)],
            )),
            Some(general()),
        ),
        "ambiguous-names" => (
            Some(proj(
                "Ambiguous",
                Some("x"),
                vec![
                    store("mp-d1", "Dup", "database", "documents", false, true),
                    store("mp-d2", "Dup", "filesystem", "documents", false, true),
                ],
            )),
            None,
        ),
        "empty-project-with-general" => (Some(proj("Empty", Some("   "), vec![])), Some(general())),
        "empty-project-no-general" => (Some(proj("Empty", Some("   "), vec![])), None),
        other => panic!("unknown combined case {other}"),
    }
}

fn group_vault_case(
    id: &str,
) -> (
    Vec<ProsperoDocumentStoreInfo>,
    Option<ProsperoDocumentStoreInfo>,
) {
    let g1 = store("grp-1", "Team Shelf", "database", "documents", false, true);
    let g2 = store(
        "grp-2",
        "Beta Group",
        "filesystem",
        "documents",
        false,
        false,
    );
    let g_char = store("grp-3", "Char Store", "obsidian", "character", false, true);
    let vault = store("vlt-1", "Ada Vault", "database", "character", false, true);
    let vault_disabled = store(
        "vlt-2",
        "Bea `Vault`",
        "filesystem",
        "character",
        false,
        false,
    );
    match id {
        "neither" => (vec![], None),
        "groups-only" => (vec![g1, g2], None),
        "vault-only" => (vec![], Some(vault)),
        "both" => (vec![g1, g_char], Some(vault)),
        "both-disabled-vault" => (vec![g2], Some(vault_disabled)),
        other => panic!("unknown group/vault case {other}"),
    }
}

fn carina_case(id: &str) -> (CarinaErrorKind, Option<&'static str>, Option<&'static str>) {
    match id {
        "not-found" => (CarinaErrorKind::NotFound, None, None),
        "no-profile-named" => (CarinaErrorKind::NoProfile, Some("Ada"), None),
        "no-profile-unnamed" => (CarinaErrorKind::NoProfile, None, None),
        "no-profile-empty-name" => (CarinaErrorKind::NoProfile, Some(""), None),
        "llm-failed-full" => (CarinaErrorKind::LlmFailed, Some("Bea"), Some("rate limit")),
        "llm-failed-no-detail" => (CarinaErrorKind::LlmFailed, Some("Bea"), None),
        "llm-failed-no-name" => (CarinaErrorKind::LlmFailed, None, Some("timeout")),
        "llm-failed-empty-detail" => (CarinaErrorKind::LlmFailed, Some("Cy"), Some("")),
        other => panic!("unknown carina case {other}"),
    }
}

/// Recompute the Rust value for one `(kind, id)` oracle row.
fn rust_value(kind: &str, id: &str) -> String {
    match kind {
        "conn_content" | "conn_opaque" => {
            let (name, old, next) = conn_case(id);
            if kind == "conn_content" {
                build_connection_profile_change_content(name, old, next)
            } else {
                build_connection_profile_change_opaque_content(name, old, next)
            }
        }
        "general_content" | "general_opaque" => {
            let g = general_case(id);
            if kind == "general_content" {
                build_general_context_content(&g)
            } else {
                build_general_context_opaque_content(&g)
            }
        }
        "combined_content" | "combined_opaque" => {
            let (project, general) = combined_case(id);
            if kind == "combined_content" {
                build_combined_context_content(project.as_ref(), general.as_ref())
            } else {
                build_combined_context_opaque_content(project.as_ref(), general.as_ref())
            }
        }
        "group_content" | "group_opaque" => {
            let (groups, vault) = group_vault_case(id);
            if kind == "group_content" {
                build_group_and_vault_whisper_content(&groups, vault.as_ref())
            } else {
                build_group_and_vault_whisper_opaque_content(&groups, vault.as_ref())
            }
        }
        "carina_content" | "carina_opaque" => {
            let (k, name, detail) = carina_case(id);
            if kind == "carina_content" {
                build_carina_error_content(k, name, detail)
            } else {
                build_carina_error_opaque_content(k, name, detail)
            }
        }
        other => panic!("unknown oracle kind {other}"),
    }
}

#[test]
fn post_office_prospero_builders_match_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_PO_PROSPERO") else {
        eprintln!("QT_ORACLE_PO_PROSPERO unset — skipping (see the test header for the recipe)");
        return;
    };
    let ndjson = std::fs::read_to_string(&path).expect("read oracle NDJSON");

    let mut count = 0usize;
    for line in ndjson.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line).expect("parse NDJSON row");
        let kind = row["kind"].as_str().expect("kind");
        let id = row["id"].as_str().expect("id");
        let expected = row["value"].as_str().expect("value is a string");

        let actual = rust_value(kind, id);
        assert_eq!(
            actual, expected,
            "mismatch for kind={kind} id={id}\n--- rust ---\n{actual}\n--- v4 ---\n{expected}"
        );
        count += 1;
    }
    assert!(count > 0, "oracle NDJSON had no rows");
    eprintln!("post_office_prospero: {count} rows matched");
}
