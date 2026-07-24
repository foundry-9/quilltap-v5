//! P4.9G1 **Shared contract** guard (§1) — the sixteen Data & System CoreRequest
//! wire shapes. G2 mirrors these name-for-name in `core-contract.ts`; the
//! unifier diffs the two by name. These asserts pin the `type` tag + every
//! payload field name against a later rename or serde-attribute slip.
//!
//! Needs no oracle or fixture: this is a serialization contract, not a behavior
//! diff (the tasks-queue behavior lives in `system_jobs_routes_equivalence`; the
//! export/import/backup/delete verbs are wired to loud refusals until their units
//! land, but their WIRE SHAPES are frozen here so G2 can build against them now).

use quilltap_core::api::types::Request;
use serde_json::json;

/// Assert a request JSON deserializes to `expected` AND re-serializes with the
/// same `type` tag (the adjacently-tagged envelope carries the payload under no
/// wrapper — fields are top-level here because `Request` is `tag = "type"`).
fn round_trip(wire: serde_json::Value, expected: Request) {
    let decoded: Request =
        serde_json::from_value(wire.clone()).unwrap_or_else(|e| panic!("decode {wire}: {e}"));
    assert_eq!(decoded, expected, "decode mismatch for {wire}");
    let encoded = serde_json::to_value(&expected).unwrap();
    assert_eq!(encoded["type"], wire["type"], "type tag drift for {wire}");
}

#[test]
fn p4_9g1_sixteen_wire_shapes() {
    // Backup / restore.
    round_trip(
        json!({ "type": "systemBackupCreate" }),
        Request::SystemBackupCreate,
    );
    round_trip(
        json!({ "type": "systemRestorePreview", "uploadId": "u1" }),
        Request::SystemRestorePreview {
            upload_id: "u1".into(),
        },
    );
    round_trip(
        json!({ "type": "systemRestoreExecute", "uploadId": "u1", "mode": "replace" }),
        Request::SystemRestoreExecute {
            upload_id: "u1".into(),
            mode: "replace".into(),
        },
    );

    // Export / import. `entityType` is the wire name for v4's `?type=` param (a
    // literal `type` field would collide with the envelope key).
    round_trip(
        json!({ "type": "systemExportEntities", "entityType": "characters" }),
        Request::SystemExportEntities {
            entity_type: "characters".into(),
        },
    );
    round_trip(
        json!({
            "type": "systemExportPreview", "entityType": "chats",
            "scope": "selected", "selectedIds": ["a", "b"], "includeMemories": true
        }),
        Request::SystemExportPreview {
            entity_type: "chats".into(),
            scope: Some("selected".into()),
            selected_ids: vec!["a".into(), "b".into()],
            include_memories: true,
        },
    );
    // Optional fields default: scope None, empty ids, includeMemories false.
    round_trip(
        json!({ "type": "systemExportPreview", "entityType": "tags" }),
        Request::SystemExportPreview {
            entity_type: "tags".into(),
            scope: None,
            selected_ids: vec![],
            include_memories: false,
        },
    );
    round_trip(
        json!({ "type": "systemImportPreview", "exportData": { "manifest": {} } }),
        Request::SystemImportPreview {
            export_data: json!({ "manifest": {} }),
        },
    );
    round_trip(
        json!({
            "type": "systemImportExecute",
            "exportData": { "manifest": {} },
            "options": { "conflictStrategy": "skip" }
        }),
        Request::SystemImportExecute {
            export_data: json!({ "manifest": {} }),
            options: json!({ "conflictStrategy": "skip" }),
        },
    );

    // Tasks queue + jobs + concurrency.
    round_trip(
        json!({ "type": "systemTasksQueue" }),
        Request::SystemTasksQueue,
    );
    round_trip(
        json!({ "type": "systemTasksQueueControl", "action": "start" }),
        Request::SystemTasksQueueControl {
            action: "start".into(),
        },
    );
    round_trip(
        json!({ "type": "systemJobConcurrencyGet" }),
        Request::SystemJobConcurrencyGet,
    );
    round_trip(
        json!({ "type": "systemJobConcurrencySet", "maxConcurrentJobs": 8 }),
        Request::SystemJobConcurrencySet {
            max_concurrent_jobs: 8,
        },
    );
    round_trip(
        json!({ "type": "systemJobGet", "jobId": "j1" }),
        Request::SystemJobGet {
            job_id: "j1".into(),
        },
    );
    round_trip(
        json!({ "type": "systemJobControl", "jobId": "j1", "action": "pause" }),
        Request::SystemJobControl {
            job_id: "j1".into(),
            action: "pause".into(),
        },
    );
    round_trip(
        json!({ "type": "systemJobDelete", "jobId": "j1" }),
        Request::SystemJobDelete {
            job_id: "j1".into(),
        },
    );

    // Delete-all.
    round_trip(
        json!({ "type": "systemDeleteDataPreview" }),
        Request::SystemDeleteDataPreview,
    );
    round_trip(
        json!({ "type": "systemDeleteData", "confirm": "DELETE_ALL_MY_DATA" }),
        Request::SystemDeleteData {
            confirm: "DELETE_ALL_MY_DATA".into(),
        },
    );
}
