//! Tier-3 (mocked-engine) differential: `POST
//! /api/v1/mount-points/[id]?action=sync` — v4's REAL `handleSync`
//! (`23da0b322`) vs `quilltap_core::api::mount_points`'s schema half and
//! refusal ladder.
//!
//! The oracle drives v4's real route module with `syncMountPoint` mocked, which
//! is what v4's own `sync-action.test.ts` does and for the same reason: the
//! boundary is the thing under test. The body schema is the SINGLE SOURCE OF
//! TRUTH for the CLI's flags — the CLI does not re-validate anything — and the
//! refusal ladder decides whether an operator sees something they can act on or
//! a 500.
//!
//! Three row kinds:
//!
//!   - `schema` — the body in, the status + error sentence out, plus the OPTIONS
//!     BAG the engine was handed (where the five defaults live). v5's
//!     [`decode_sync_options`] is the same half, extracted so it can be driven
//!     without a store or a directory.
//!   - `store` — the lookup's 404, and the ordering that puts the body parse
//!     FIRST (a bad body on a missing store is a 400, not a 404).
//!   - `refusal` — each error the engine can raise, mapped through the ladder.
//!     The three "busy" codes are 409s; `NOT_DATABASE_BACKED`,
//!     `CHARACTER_ARCHIVED` and an unrecognized code are 400s; `ENOENT` gets its
//!     own Docker sentence; everything else is a 500.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout — cp to a /tmp mirror;
//! jest ignores `.claude/` paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-mount-sync-action-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/mount-sync-action.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-mount-sync-action.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- mount-sync-action
//! Run:
//!   QT_ORACLE_MOUNT_SYNC_ACTION=/tmp/oracle-mount-sync-action.ndjson cargo test -p quilltap-harness --test mount_sync_action_equivalence -- --nocapture

use std::collections::BTreeMap;

use quilltap_core::api::mount_points::{decode_sync_options, sync_error_response};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::services::mount_index::sync::manifest::ManifestMismatchError;
use quilltap_core::services::mount_index::sync::{SyncError, SyncRefusalCode, SyncRefusedError};
use serde_json::{json, Value};

/// v4's status code for an [`ErrorKind`], as the web edge maps it.
fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Internal => 500,
        other => panic!("unexpected error kind on the sync route: {other:?}"),
    }
}

/// The `(status, body)` a [`Response`] puts on the wire, in the shape the
/// oracle records.
fn wire(response: &Response) -> (u16, Value) {
    match response {
        Response::Error(e) => (status_of(e.kind), json!({ "error": e.message })),
        Response::MountSync(v) => (200, v.clone()),
        other => panic!("unexpected response on the sync route: {other:?}"),
    }
}

/// The body key as `Request::MountSync` carries it: ABSENT (`None`), explicit
/// `null` (`Some(None)`), or a value.
fn tri(body: &Value, key: &str) -> Option<Option<Value>> {
    match body.as_object().and_then(|o| o.get(key)) {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(v.clone())),
    }
}

#[test]
fn mount_sync_action_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MOUNT_SYNC_ACTION") else {
        eprintln!("SKIP: set QT_ORACLE_MOUNT_SYNC_ACTION to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("cannot read oracle {oracle_path}: {e}"));

    let mut mismatches: Vec<String> = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut n = 0usize;
    let mut accepted = 0usize;
    let mut refused = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let kind = row["kind"].as_str().expect("kind").to_string();
        let id = row["id"].as_str().expect("id").to_string();
        let want_status = row["status"].as_u64().expect("status") as u16;
        let want_body = row["responseBody"].clone();
        n += 1;
        *seen.entry(kind.clone()).or_default() += 1;

        match kind.as_str() {
            "schema" => {
                let body = row["body"].clone();
                // v4's non-object arm is the REST edge's alone (over dispatch a
                // non-object body cannot carry the `type` tag), and it is pinned
                // in `sync_non_object_body`'s own test rather than here — this
                // family drives the schema half, which only ever sees an object.
                if !body.is_object() {
                    let got = quilltap_core::api::mount_points::sync_non_object_body(&body);
                    let (status, wire_body) = wire(&got);
                    if (status, &wire_body) != (want_status, &want_body) {
                        mismatches.push(format!(
                            "[schema/{id}] non-object body\n      v4: {want_status} {want_body}\n      v5: {status} {wire_body}"
                        ));
                    }
                    continue;
                }
                let decoded = decode_sync_options(
                    tri(&body, "targetPath"),
                    tri(&body, "dryRun"),
                    tri(&body, "direction"),
                    tri(&body, "prefer"),
                    tri(&body, "propagateDeletes"),
                    tri(&body, "useManifest"),
                );
                match decoded {
                    Ok(options) => {
                        accepted += 1;
                        if want_status != 200 {
                            mismatches.push(format!(
                                "[schema/{id}] v5 ACCEPTED a body v4 refused with {want_status} {want_body}"
                            ));
                            continue;
                        }
                        // The options bag the engine is handed, key for key.
                        let want_options = row["options"].clone();
                        let got_options =
                            serde_json::to_value(&options).expect("serialize the options");
                        if got_options != want_options {
                            mismatches.push(format!(
                                "[schema/{id}] options\n      v4: {want_options}\n      v5: {got_options}"
                            ));
                        }
                        // v4 calls the engine exactly once on an accepted body.
                        if row["called"].as_u64() != Some(1) {
                            mismatches.push(format!(
                                "[schema/{id}] the oracle accepted this body without calling the engine"
                            ));
                        }
                    }
                    Err(response) => {
                        refused += 1;
                        let (status, wire_body) = wire(&response);
                        if (status, &wire_body) != (want_status, &want_body) {
                            mismatches.push(format!(
                                "[schema/{id}]\n      v4: {want_status} {want_body}\n      v5: {status} {wire_body}"
                            ));
                        }
                        // A refused body must never have reached the engine.
                        if row["called"].as_u64() != Some(0) {
                            mismatches.push(format!(
                                "[schema/{id}] v4 refused this body but still called the engine — \
                                 the oracle's own ordering has moved"
                            ));
                        }
                    }
                }
            }
            "store" => {
                // The two arms v5 answers in the handler rather than the schema
                // half: the 404, and the ordering that keeps the body parse in
                // front of it. The oracle's statuses are the comparand; v5's
                // side is asserted structurally, because a real store lookup
                // needs a database and this family is the boundary's.
                match id.as_str() {
                    "missing-store" => {
                        if want_status != 404 {
                            mismatches.push(format!(
                                "[store/{id}] v4 no longer 404s a missing store (got {want_status})"
                            ));
                        }
                    }
                    "missing-store-and-bad-body" => {
                        if want_status != 400 {
                            mismatches.push(format!(
                                "[store/{id}] v4 no longer puts the body parse BEFORE the lookup \
                                 (got {want_status}) — v5's handler does, so this is a real move"
                            ));
                        }
                    }
                    "the-report-is-the-body" => {
                        // ⚠ The one that matters: v4's `successResponse(report)`
                        // is `NextResponse.json(report)`, so the report is BARE.
                        // The P4.D210 order's §S.3 says `{data: report}`; this
                        // row is the measurement that says otherwise.
                        if want_body.get("data").is_some() {
                            mismatches.push(format!(
                                "[store/{id}] v4 now WRAPS the report — `Response::MountSync` \
                                 carries it bare and the web edge writes it straight out"
                            ));
                        }
                        if want_body.get("summary").is_none() {
                            mismatches.push(format!(
                                "[store/{id}] the oracle's body is not a report: {want_body}"
                            ));
                        }
                    }
                    other => panic!("unknown store row `{other}`"),
                }
            }
            "refusal" => {
                let error = match id.as_str() {
                    "NOT_DATABASE_BACKED" => SyncError::Refused(SyncRefusedError {
                        message: "\"Lore\" is a filesystem store".to_string(),
                        code: SyncRefusalCode::NotDatabaseBacked,
                    }),
                    "CHARACTER_ARCHIVED" => SyncError::Refused(SyncRefusedError {
                        message: "archived".to_string(),
                        code: SyncRefusalCode::CharacterArchived,
                    }),
                    "SYNC_IN_PROGRESS" => SyncError::Refused(SyncRefusedError {
                        message: "already running".to_string(),
                        code: SyncRefusalCode::SyncInProgress,
                    }),
                    "CONVERSION_IN_PROGRESS" => SyncError::Refused(SyncRefusedError {
                        message: "converting".to_string(),
                        code: SyncRefusalCode::ConversionInProgress,
                    }),
                    "SCAN_IN_PROGRESS" => SyncError::Refused(SyncRefusedError {
                        message: "scanning".to_string(),
                        code: SyncRefusalCode::ScanInProgress,
                    }),
                    // v5's refusal codes are a closed enum, so v4's
                    // "a code the ladder has never heard of" has no v5
                    // counterpart to construct — the arm is unreachable by
                    // construction rather than unported. What it PROVES on v4's
                    // side is that the `SyncRefusedError` test precedes the
                    // ENOENT and 500 arms, which the five above already pin, so
                    // the row is asserted as v4's status alone.
                    "an-unknown-refusal-code" => {
                        if want_status != 400 {
                            mismatches.push(format!(
                                "[refusal/{id}] v4 no longer treats an unknown refusal code as a \
                                 400 (got {want_status}) — v5's closed enum assumes it does"
                            ));
                        }
                        continue;
                    }
                    "ManifestMismatchError" => SyncError::ManifestMismatch(ManifestMismatchError {
                        found_store_id: "other".to_string(),
                        expected_store_id: "store-1".to_string(),
                    }),
                    "ENOENT" => {
                        SyncError::NotFound("ENOENT: no such file or directory".to_string())
                    }
                    "EACCES" => SyncError::Io("EACCES: permission denied".to_string()),
                    "a-plain-error" => SyncError::Io("the disk caught fire".to_string()),
                    other => panic!("unknown refusal row `{other}`"),
                };
                let (status, wire_body) = wire(&sync_error_response(error));
                if (status, &wire_body) != (want_status, &want_body) {
                    mismatches.push(format!(
                        "[refusal/{id}]\n      v4: {want_status} {want_body}\n      v5: {status} {wire_body}"
                    ));
                }
            }
            other => panic!("unknown corpus row kind `{other}`"),
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} disagreements over {n} rows:\n  {}",
        mismatches.len(),
        mismatches.join("\n  ")
    );
    assert_eq!(
        n, 36,
        "expected the full corpus (36 rows), saw {n} — stale oracle?"
    );
    for kind in ["schema", "store", "refusal"] {
        assert!(
            seen.get(kind).copied().unwrap_or(0) > 0,
            "no `{kind}` rows in the corpus ({seen:?})"
        );
    }
    // Both halves of the schema must be exercised, or the family proves only
    // that v5 refuses everything (or nothing).
    assert!(
        accepted >= 5 && refused >= 10,
        "the schema corpus is lopsided: {accepted} accepted, {refused} refused"
    );
    eprintln!(
        "OK: the sync action matched v4 on {n} rows ({accepted} accepted, {refused} refused); kinds: {seen:?}"
    );
}
