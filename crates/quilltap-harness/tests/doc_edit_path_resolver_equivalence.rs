//! Read-differential (W4.1d batch 3a): the doc-edit path resolver + URI producers
//! (`doc_edit::path_resolver::resolve_doc_edit_path` + `doc_edit::uri_producers`)
//! vs v4's REAL `resolveDocEditPath` / `docStoreUriFor` / `uriForResolvedPath` /
//! `buildDocStoreUriResolver`. Both sides READ the SAME baked fixtures and run the
//! SAME matrix; every id is pinned/shared so resolved paths, error code+message,
//! and produced URIs compare EXACTLY (no normalization). Every store is
//! database-backed, so the host-FS scopes are never touched.
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_DPR_MAIN=/tmp/qt-dpr-main.db QT_FIXTURE_DPR_MOUNT=/tmp/qt-dpr-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-doc-edit-path-resolver-fixture.ts
//!   QT_FIXTURE_DPR_MAIN=/tmp/qt-dpr-main.db QT_FIXTURE_DPR_MOUNT=/tmp/qt-dpr-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/doc-edit-path-resolver.ts > /tmp/oracle-dpr.ndjson
//! Run:
//!   QT_ORACLE_DPR=/tmp/oracle-dpr.ndjson \
//!   QT_FIXTURE_DPR_MAIN=/tmp/qt-dpr-main.db QT_FIXTURE_DPR_MOUNT=/tmp/qt-dpr-mount.db \
//!     cargo test -p quilltap-harness --test doc_edit_path_resolver_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::doc_edit::path_resolver::ResolvedPath;
use quilltap_core::doc_edit::path_resolver::{
    resolve_doc_edit_path, PathResolutionContext, ResolveError,
};
use quilltap_core::doc_edit::qtap_uri::ScopedAuthority;
use quilltap_core::doc_edit::uri_producers::{
    doc_store_uri_for, uri_for_resolved_path, DocStoreUriResolver,
};
use quilltap_core::doc_edit::DocEditScope;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "charAId")]
    char_a_id: String,
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "normalStore")]
    normal_store: String,
    #[serde(rename = "sharedStoreA")]
    shared_store_a: String,
}

#[derive(Deserialize)]
struct Row {
    kind: String,
    id: String,
    #[serde(default)]
    result: Value,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/doc-edit-path-resolver.json")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see header).");
            None
        }
    }
}

/// Serialize a resolve result to the oracle's shape.
fn resolve_json(r: Result<ResolvedPath, ResolveError>) -> Value {
    match r {
        Ok(resolved) => json!({ "ok": true, "resolved": serde_json::to_value(&resolved).unwrap() }),
        Err(ResolveError::Path { code, message }) => {
            json!({ "ok": false, "code": code.as_str(), "message": message })
        }
        Err(ResolveError::FsSeam) => panic!("corpus hit the host-FS seam (should never happen)"),
    }
}

#[test]
fn doc_edit_path_resolver_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_DPR"),
        env_or_skip("QT_FIXTURE_DPR_MAIN"),
        env_or_skip("QT_FIXTURE_DPR_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Row = serde_json::from_str(line).expect("oracle line parses");
        oracle.insert(format!("{}:{}", row.kind, row.id), row.result);
    }

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-dpr-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-dpr-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main_w = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let main = main_w.connection();
    let mount = mount_w.connection();

    let a = spec.char_a_id.clone();
    // The minted vault mount (read back identically on both sides).
    let char_a_vault = quilltap_core::db::characters_read::find_by_id_raw(main, &a)
        .unwrap()
        .and_then(|v| {
            v.get("characterDocumentMountPointId")
                .and_then(|x| x.as_str())
                .map(String::from)
        })
        .expect("char vault");

    let want = |kind: &str, id: &str| -> Value {
        oracle
            .get(&format!("{kind}:{id}"))
            .cloned()
            .unwrap_or_else(|| panic!("oracle missing {kind}:{id}"))
    };

    // ---- resolve matrix (MUST mirror the oracle) ----
    let ds = DocEditScope::DocumentStore;
    let proj = DocEditScope::Project;
    let ctx = |character_id: Option<&str>,
               project_id: Option<&str>,
               mount_point: Option<&str>,
               operator: bool| PathResolutionContext {
        project_id: project_id.map(String::from),
        character_id: character_id.map(String::from),
        character_ids: Vec::new(),
        mount_point: mount_point.map(String::from),
        operator_override: operator,
    };

    let p = spec.project_id.as_str();
    type Case<'a> = (
        &'a str,
        DocEditScope,
        Option<&'a str>,
        PathResolutionContext,
    );
    let cases: Vec<Case> = vec![
        (
            "self-token",
            ds,
            Some("Notes/a.md"),
            ctx(Some(&a), None, Some("self"), false),
        ),
        (
            "self-token-upper",
            ds,
            Some("x.md"),
            ctx(Some(&a), None, Some("SELF"), false),
        ),
        (
            "name-match",
            ds,
            Some("k.md"),
            ctx(Some(&a), Some(p), Some("Project Docs"), false),
        ),
        (
            "id-match",
            ds,
            Some("k.md"),
            ctx(Some(&a), Some(p), Some(&spec.normal_store), false),
        ),
        (
            "ambiguous-name",
            ds,
            Some("a.md"),
            ctx(Some(&a), Some(p), Some("Shared Name"), false),
        ),
        (
            "unknown",
            ds,
            Some("a.md"),
            ctx(Some(&a), Some(p), Some("no-such-store"), false),
        ),
        (
            "disabled",
            ds,
            Some("a.md"),
            ctx(Some(&a), Some(p), Some("Disabled Store"), false),
        ),
        (
            "traversal",
            ds,
            Some("../etc"),
            ctx(Some(&a), None, Some("self"), false),
        ),
        (
            "absolute",
            ds,
            Some("/etc/passwd"),
            ctx(Some(&a), None, Some("self"), false),
        ),
        (
            "missing-path",
            ds,
            None,
            ctx(Some(&a), None, Some("self"), false),
        ),
        (
            "no-mountpoint",
            ds,
            Some("a.md"),
            ctx(Some(&a), None, None, false),
        ),
        (
            "missing-context",
            ds,
            Some("a.md"),
            ctx(None, None, Some("Project Docs"), false),
        ),
        (
            "project-alias",
            proj,
            Some("Outline.md"),
            ctx(None, Some(p), None, false),
        ),
        (
            "project-no-id",
            proj,
            Some("a.md"),
            ctx(None, None, None, false),
        ),
        (
            "operator-override",
            ds,
            Some("a.md"),
            ctx(None, None, Some("Project Docs"), true),
        ),
    ];

    for (id, scope, path, context) in &cases {
        let got = resolve_json(resolve_doc_edit_path(main, mount, *scope, *path, context));
        assert_eq!(got, want("resolve", id), "resolve mismatch for '{id}'");
    }

    // ---- URI producers ----
    let uri = |id: &str, got: String| {
        assert_eq!(json!(got), want("uri", id), "uri mismatch for '{id}'");
    };
    uri(
        "uri-self",
        doc_store_uri_for(
            main,
            mount,
            &char_a_vault,
            "",
            "Mail/x.md",
            Some(&a),
            None,
            None,
        ),
    );
    uri(
        "uri-name",
        doc_store_uri_for(
            main,
            mount,
            &spec.normal_store,
            "Project Docs",
            "k.md",
            None,
            None,
            None,
        ),
    );
    uri(
        "uri-ambiguous",
        doc_store_uri_for(
            main,
            mount,
            &spec.shared_store_a,
            "Shared Name",
            "a.md",
            None,
            None,
            None,
        ),
    );
    let proj_resolved = ResolvedPath {
        absolute_path: String::new(),
        scope: DocEditScope::Project,
        mount_point_id: None,
        mount_point_name: None,
        mount_type: None,
        base_path: String::new(),
        relative_path: "Outline.md".to_string(),
    };
    uri(
        "uri-resolved-project",
        uri_for_resolved_path(main, mount, &proj_resolved, Some(&a), None, None),
    );

    let resolver = DocStoreUriResolver::build(main, mount, Some(&a));
    uri(
        "res-self",
        resolver.uri_for_mount("", &char_a_vault, "x.md"),
    );
    uri(
        "res-name",
        resolver.uri_for_mount("Project Docs", &spec.normal_store, "k.md"),
    );
    uri(
        "res-ambiguous",
        resolver.uri_for_mount("Shared Name", &spec.shared_store_a, "a.md"),
    );
    uri(
        "res-scope",
        resolver.uri_for_scope(ScopedAuthority::General, "g.md"),
    );

    eprintln!(
        "doc_edit_path_resolver: {} resolve + 8 uri cases matched the oracle.",
        cases.len()
    );
}
