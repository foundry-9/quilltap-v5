//! P4.43 unit 7 differential: conversation-summaries regeneration — the
//! `api::memory_maintenance::conversation_summaries_regenerate` verb + the
//! `services::conversation_summaries_regen` handler +
//! `services::queue_service::enqueue_regenerate_conversation_summaries`, vs v4's
//! REAL `enqueueRegenerateConversationSummaries` +
//! `handleRegenerateConversationSummaries` + the status-count route logic.
//!
//! Over a FRESH copy of the committed `conversation-summaries-regen-{main,mount}.db`
//! fixture, both sides run: status → enqueue → enqueue → status → dump
//! background_jobs → run the handler → dump the mirrored vault state.
//!
//! Diffed: the `{success, inFlight}` status counts; the enqueue `{success,
//! jobId:'<jobId>', message}` responses (enqueue2 must dedupe — "already in
//! flight"); the background_jobs state (one REGENERATE_CONVERSATION_SUMMARIES row,
//! maxAttempts 1, payload `{}` — `status` is EXCLUDED: v4's oracle DB has a live
//! processor that claims the row to PROCESSING while the sandbox `Db` leaves it
//! PENDING, and `enqueueJob`'s initial PENDING is proven in `queue_service`);
//! and, after the handler runs, the mirrored vault state as (a) the byte-content
//! of each conversation-summary document (its `updatedAt:` frontmatter line
//! normalized — the one wall-clock field), proving the handler's stats /
//! participant / generation construction, and (b) the summary links as
//! (vault-name, relativePath) pairs, proving Council → both vaults, Cassia's
//! Journal → one, and whitespace/null/no-participant chats skipped. The summary
//! FILE bytes are already proven byte-for-byte with pinned timestamps by the
//! `vault_summary_mirror_tier2` family.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_CSR_MAIN=… QT_CSR_MOUNT=… npx tsx …/conversation-summaries-regen-tier2.ts
//!       > /tmp/oracle-conversation-summaries-regen.ndjson
//! Run:
//!   QT_ORACLE_CSR=/tmp/oracle-conversation-summaries-regen.ndjson \
//!     cargo test -p quilltap-harness --test conversation_summaries_regen_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::{memory_maintenance, Response as CoreResponse};
use quilltap_core::db::background_jobs::BackgroundJobsRepository;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::conversation_summaries_regen::handle_regenerate_conversation_summaries;
use serde_json::{json, Value};

const USER_ID: &str = "a0000000-0000-4000-8000-000000000001";
const PEPPER: &str = "ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn rows(dump: &Value) -> &Vec<Value> {
    dump.get("rows").and_then(Value::as_array).unwrap()
}

/// Normalize the one wall-clock field in a summary document's markdown content:
/// the `updatedAt: <iso>` frontmatter line → `updatedAt: <ts>`.
fn normalize_content(content: &str) -> String {
    content
        .lines()
        .map(|l| {
            if l.starts_with("updatedAt: ") {
                "updatedAt: <ts>".to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The sorted set of conversation-summary document contents (updatedAt normalized).
/// Proves which chats were mirrored + the handler's frontmatter/stats construction.
fn summary_contents(documents: &Value) -> Value {
    let mut out: Vec<String> = rows(documents)
        .iter()
        .filter_map(|d| d.get("content").and_then(Value::as_str))
        .filter(|c| c.contains("type: conversation-summary"))
        .map(normalize_content)
        .collect();
    out.sort();
    json!(out)
}

/// The sorted set of `[vaultName, relativePath]` for summary links. Proves each
/// summary was mirrored into the RIGHT participant vaults.
fn summary_placements(points: &Value, links: &Value) -> Value {
    let name_by_id: HashMap<&str, &str> = rows(points)
        .iter()
        .filter_map(|p| {
            Some((
                p.get("id")?.as_str()?,
                p.get("name")?.as_str().unwrap_or_default(),
            ))
        })
        .collect();
    let mut out: Vec<Value> = rows(links)
        .iter()
        .filter_map(|l| {
            let rp = l.get("relativePath")?.as_str()?;
            if !rp.starts_with("Conversation Summaries/") {
                return None;
            }
            let mp = l.get("mountPointId")?.as_str()?;
            Some(json!([name_by_id.get(mp).copied().unwrap_or("?"), rp]))
        })
        .collect();
    out.sort_by_key(|v| v.to_string());
    json!(out)
}

/// Unwrap a `MemoryMaintenance` response to its inner Value, normalizing a minted
/// `jobId`.
fn maintenance_value(resp: CoreResponse) -> Value {
    let CoreResponse::MemoryMaintenance(mut v) = resp else {
        panic!("expected MemoryMaintenance response, got {resp:?}");
    };
    if v.get("jobId").is_some() {
        v["jobId"] = json!("<jobId>");
    }
    v
}

#[test]
fn conversation_summaries_regen_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CSR") else {
        eprintln!("SKIP: set QT_ORACLE_CSR (see test header).");
        return;
    };
    let oracle: Value =
        serde_json::from_str(std::fs::read_to_string(&oracle_path).unwrap().trim()).unwrap();

    let scratch = std::env::temp_dir().join(format!("qt-csr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(
        fixtures_dir().join("conversation-summaries-regen-main.db"),
        &main,
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("conversation-summaries-regen-mount.db"),
        &mount,
    )
    .unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut failed = false;
    let mut check = |label: &str, got: &Value, want: &Value| {
        if got != want {
            eprintln!("[{label}] MISMATCH:\n  got:  {got}\n  want: {want}");
            failed = true;
        } else {
            eprintln!("[{label}] OK.");
        }
    };

    // status → enqueue → enqueue → status.
    let status_before = rt.block_on(memory_maintenance::conversation_summaries_regenerate(
        &db, USER_ID, true,
    ));
    check(
        "statusBefore",
        &maintenance_value(status_before),
        &oracle["statusBefore"],
    );

    let enqueue1 = rt.block_on(memory_maintenance::conversation_summaries_regenerate(
        &db, USER_ID, false,
    ));
    check(
        "enqueue1",
        &maintenance_value(enqueue1),
        &oracle["enqueue1"]["response"],
    );

    let enqueue2 = rt.block_on(memory_maintenance::conversation_summaries_regenerate(
        &db, USER_ID, false,
    ));
    check(
        "enqueue2",
        &maintenance_value(enqueue2),
        &oracle["enqueue2"]["response"],
    );

    let status_after = rt.block_on(memory_maintenance::conversation_summaries_regenerate(
        &db, USER_ID, true,
    ));
    check(
        "statusAfter",
        &maintenance_value(status_after),
        &oracle["statusAfter"],
    );

    // background_jobs state: one REGENERATE row, maxAttempts 1, payload {} (status
    // excluded — see the header). Build the SAME shape from the oracle dump.
    let jobs_got = db
        .read_main(|conn| {
            let mut stmt = conn
                .prepare("SELECT type, maxAttempts, payload FROM background_jobs ORDER BY type")?;
            let rows: Vec<Value> = stmt
                .query_map([], |r| {
                    // maxAttempts is bound as f64 (REAL affinity); v4's dump shows
                    // the integral value as a JSON integer — cast to match.
                    Ok(json!({
                        "type": r.get::<_, String>(0)?,
                        "maxAttempts": r.get::<_, f64>(1)? as i64,
                        "payload": r.get::<_, String>(2)?,
                    }))
                })?
                .collect::<Result<_, _>>()?;
            Ok(json!(rows))
        })
        .unwrap();
    let jobs_want: Value = json!(rows(&oracle["jobs"])
        .iter()
        .map(|r| json!({
            "type": r["type"],
            "maxAttempts": r["maxAttempts"],
            "payload": r["payload"],
        }))
        .collect::<Vec<_>>());
    check("jobs", &jobs_got, &jobs_want);

    // Run the handler (the backfill) over the enqueued job.
    let job = db
        .read_main(|conn| {
            let repo = BackgroundJobsRepository::new(conn);
            let jobs = repo.find_by_user_id(USER_ID, Some("PENDING"))?;
            let mut jobs2 = repo.find_by_user_id(USER_ID, Some("PROCESSING"))?;
            let mut all = jobs;
            all.append(&mut jobs2);
            Ok(all
                .into_iter()
                .find(|j| j.job_type == "REGENERATE_CONVERSATION_SUMMARIES")
                .expect("enqueued regen job"))
        })
        .unwrap();
    rt.block_on(handle_regenerate_conversation_summaries(&db, &job))
        .expect("handler");

    // Dump the mount-index tables the vault proofs read.
    let dump = |table: &'static str, order_by: &'static str| -> Value {
        db.read_mount_index(move |c| dump_table_json_conn(c, table, order_by))
            .unwrap()
    };
    let points = dump("doc_mount_points", "name");
    let documents = dump("doc_mount_documents", "contentSha256");
    let links = dump("doc_mount_file_links", "relativePath");

    // (a) the summary-document byte-content (updatedAt normalized).
    check(
        "summary contents",
        &summary_contents(&documents),
        &summary_contents(&oracle["documents"]),
    );
    // (b) the summary placements: (vaultName, relativePath).
    check(
        "summary placements",
        &summary_placements(&points, &links),
        &summary_placements(&oracle["points"], &oracle["links"]),
    );

    let _ = std::fs::remove_dir_all(&scratch);
    assert!(
        !failed,
        "conversation-summaries-regen differential mismatched"
    );
    eprintln!("OK: conversation_summaries_regen matched oracle.");
}
