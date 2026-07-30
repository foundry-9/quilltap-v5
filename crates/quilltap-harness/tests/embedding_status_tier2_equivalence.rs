//! Tier-2 differential test: the `embedding_status` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-embedding-status-fixture.ts), run the SAME
//! create / update / delete / markEmbedded / markFailed op sequence from the
//! committed spec, dump the `embedding_status` table canonically, and assert the
//! post-op state is identical.
//!
//! ## P4.d27 — the `listFailedEntityIds` read
//!
//! v4 `7391404e` adds one read to this repo, which the `mismatched-dim` reindex
//! scope and the boot dimension reconcile both use as an exclusion set. It is
//! driven here, over the post-op state, as `{kind:'listFailed', name, ids}` rows
//! that precede the state dump. A `Set` has no JSON form, so the ids are compared
//! SORTED on both sides — the consumer is a membership test, never an ordered
//! walk. The corpus covers a matching read, the same entityType under the OTHER
//! profile (an EMBEDDED row lives there — the profileId predicate), a
//! non-FAILED-only (entityType, profileId) pair (the status predicate), and a
//! pair with no rows at all. The missing-table fail-soft arm is not reachable
//! from a provisioned fixture and lives in the module's own unit test.
//!
//! ⚠️ P4.D25 — the mark* CREATE arm mints. Since v4 `a5d6cee5`,
//! `markAsEmbedded`/`markAsFailed` UPSERT, and the create path reaches v4's
//! `create` with NO `CreateOptions`, so `id` and `createdAt` (and, for
//! markAsEmbedded, `embeddedAt`) are all minted. Rows are therefore sorted by
//! the natural key (entityType, entityId, profileId) rather than by id, and an
//! `id`/`createdAt`/`embeddedAt` cell is placeholdered ONLY when its value does
//! not appear literally in the committed spec — so every pinned value still
//! compares exactly. That is what keeps "markAsFailed left `embeddedAt` alone"
//! and "the other profile's row was not touched" real assertions.
//!
//! ⚠️ MINTED `updatedAt` — single-column placeholder normalization. Like
//! `tfidf_vocabulary` (and unlike `folders`/`tags`/`image_profiles`, which pin
//! every field), v4's `EmbeddingStatusRepository` OVERRIDES the base
//! create/update and sets `updatedAt = getCurrentTimestamp()` unconditionally —
//! `options.updatedAt` / patch `updatedAt` are ignored. The Rust port mints
//! `updatedAt` the same way (`clock::now_iso`). So `id`, `createdAt`, and every
//! payload column are pinned and diffed EXACTLY; only `updatedAt` is collapsed to
//! a `<ts>` placeholder on both sides. `createdAt` is honored on create and
//! preserved on update, so it stays exact and is asserted as-is.
//!
//! This banks an all-TEXT marshaling surface: UUID columns (userId, entityId,
//! profileId) as TEXT, two enum columns as TEXT (entityType, status), and two
//! NULLABLE TEXT columns (embeddedAt, error). No booleans, numbers, JSON, or BLOB.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-es-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-embedding-status-fixture.ts
//!   QT_FIXTURE_ES=/tmp/qt-es-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/embedding-status-tier2.ts \
//!     > /tmp/oracle-es.ndjson
//! Run:
//!   QT_ORACLE_EMBEDDING_STATUS=/tmp/oracle-es.ndjson \
//!   QT_FIXTURE_EMBEDDING_STATUS=/tmp/qt-es-fixture.db \
//!     cargo test -p quilltap-harness --test embedding_status_tier2_equivalence

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use quilltap_core::db::embedding_status::{CreateOptions, EsCreate, EsUpdate};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// The committed fixture spec — the single source driving both ports.
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
    /// P4.d27 — `listFailedEntityIds` reads, run after the whole op sequence.
    #[serde(rename = "listFailedReads")]
    list_failed_reads: Vec<ListFailedRead>,
}

/// One `listFailedEntityIds(entityType, profileId)` read.
#[derive(Deserialize)]
struct ListFailedRead {
    name: String,
    #[serde(rename = "entityType")]
    entity_type: String,
    #[serde(rename = "profileId")]
    profile_id: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create {
        data: CreateData,
        options: CreateOpts,
    },
    #[serde(rename = "update")]
    Update { id: String, data: UpdateData },
    #[serde(rename = "delete")]
    Delete { id: String },
    /// P4.D25 — v4's upserting `markAsEmbedded` (both arms + profile masking).
    #[serde(rename = "markEmbedded")]
    MarkEmbedded {
        #[serde(rename = "entityType")]
        entity_type: String,
        #[serde(rename = "entityId")]
        entity_id: String,
        #[serde(rename = "profileId")]
        profile_id: String,
        #[serde(rename = "userId")]
        user_id: String,
    },
    /// P4.D25 — v4's upserting `markAsFailed`.
    #[serde(rename = "markFailed")]
    MarkFailed {
        #[serde(rename = "entityType")]
        entity_type: String,
        #[serde(rename = "entityId")]
        entity_id: String,
        #[serde(rename = "profileId")]
        profile_id: String,
        error: String,
        #[serde(rename = "userId")]
        user_id: String,
    },
}

#[derive(Deserialize)]
struct CreateData {
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "entityType")]
    entity_type: String,
    #[serde(rename = "entityId")]
    entity_id: String,
    #[serde(rename = "profileId")]
    profile_id: String,
    status: String,
    #[serde(default, rename = "embeddedAt")]
    embedded_at: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// Create options carry ONLY id + createdAt — v4's override ignores
/// `options.updatedAt`, so the spec omits it.
#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

/// Update patch — note NO `updatedAt`: v4 mints it (overwrites any caller value).
#[derive(Deserialize)]
struct UpdateData {
    #[serde(default, rename = "userId")]
    user_id: Option<String>,
    #[serde(default, rename = "entityType")]
    entity_type: Option<String>,
    #[serde(default, rename = "entityId")]
    entity_id: Option<String>,
    #[serde(default, rename = "profileId")]
    profile_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default, rename = "embeddedAt")]
    embedded_at: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// Always minted by the repo's override, on every write — placeholdered
/// unconditionally on both sides.
const TS_COLUMNS: &[&str] = &["updatedAt"];

/// Minted only on the mark* CREATE arm (v4's `create` falls through to
/// `options?.id || generateId()` / `options?.createdAt || now`, and
/// `markAsEmbedded` stamps a fresh `embeddedAt`). These are placeholdered ONLY
/// when the value is absent from the committed spec — so every pinned id and
/// pinned timestamp still compares exactly, and "markAsFailed left `embeddedAt`
/// alone" stays an assertion rather than a normalization artifact.
const PINNABLE_COLUMNS: &[&str] = &["id", "createdAt", "embeddedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/embedding-status-tier2.json")
}

/// Every string literal anywhere in the committed spec — the values BOTH sides
/// pinned. Anything else in a dumped `id`/`createdAt`/`embeddedAt` cell was
/// minted at run time.
fn pinned_literals(spec: &Value) -> BTreeSet<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::String(s) => {
                out.insert(s.clone());
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(spec, &mut out);
    out
}

/// Collapse the minted `updatedAt` column to a `<ts>` placeholder and any
/// unpinned `id`/`createdAt`/`embeddedAt` to `<minted>`/`<ts>`, in place, on a
/// `{ table, columns, rows }` dump; then re-sort by the natural key
/// (entityType, entityId, profileId), which is unique in this corpus and — unlike
/// `id` — survives the mark* create arm's minted UUID.
fn normalize(dump: &mut Value, label: &str, pinned: &BTreeSet<String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        for col in TS_COLUMNS {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
        for col in PINNABLE_COLUMNS {
            let minted = match obj.get(*col) {
                Some(Value::String(s)) => !pinned.contains(s),
                _ => false,
            };
            if minted {
                let placeholder = if *col == "id" { "<minted>" } else { "<ts>" };
                obj.insert((*col).to_string(), Value::String(placeholder.to_string()));
            }
        }
    }
    rows.sort_by_key(|r| {
        ["entityType", "entityId", "profileId"]
            .iter()
            .map(|c| r.get(*c).and_then(Value::as_str).unwrap_or("").to_string())
            .collect::<Vec<_>>()
            .join(" ")
    });
}

#[test]
fn embedding_status_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_EMBEDDING_STATUS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_EMBEDDING_STATUS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_EMBEDDING_STATUS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_EMBEDDING_STATUS to the seed fixture .db (see test header)."
            );
            return;
        }
    };

    // Parse the committed spec (pepper + op sequence) — same file the oracle used.
    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec_json: Value = serde_json::from_str(&spec_text).expect("parse fixture spec json");
    let pinned = pinned_literals(&spec_json);
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    // Parse the oracle NDJSON: the `listFailed` read rows (P4.d27), then the one
    // untagged line carrying the post-op state dump.
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let mut oracle: Option<Value> = None;
    let mut oracle_reads: Vec<(String, Vec<String>)> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("listFailed") => oracle_reads.push((
                v["name"].as_str().expect("read name").to_string(),
                v["ids"]
                    .as_array()
                    .expect("read ids")
                    .iter()
                    .map(|x| x.as_str().expect("id string").to_string())
                    .collect(),
            )),
            Some(other) => panic!("unknown oracle row kind {other:?}"),
            None => oracle = Some(v),
        }
    }
    let mut oracle = oracle.expect("oracle emitted no state dump — regenerate");
    // Corpus-shape guard (not a hand count): the oracle ran EXACTLY this corpus's
    // reads, in order.
    assert_eq!(
        oracle_reads
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>(),
        spec.list_failed_reads
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        "oracle listFailed read set/order diverges from the corpus — regenerate"
    );
    // A read family where every answer is empty would agree with a port that
    // always returns nothing, and one where every answer is non-empty would
    // agree with a port that ignores the predicates. Pin both directions.
    assert!(
        oracle_reads.iter().any(|(_, ids)| !ids.is_empty())
            && oracle_reads.iter().any(|(_, ids)| ids.is_empty()),
        "the corpus stopped exercising both a matching and an empty listFailed read"
    );

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let work = std::env::temp_dir().join(format!("qt-es-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.embedding_status();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &EsCreate {
                            user_id: data.user_id.clone(),
                            entity_type: data.entity_type.clone(),
                            entity_id: data.entity_id.clone(),
                            profile_id: data.profile_id.clone(),
                            status: data.status.clone(),
                            embedded_at: data.embedded_at.clone(),
                            error: data.error.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                        },
                    )
                    .expect("embedding_status.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &EsUpdate {
                                user_id: data.user_id.clone(),
                                entity_type: data.entity_type.clone(),
                                entity_id: data.entity_id.clone(),
                                profile_id: data.profile_id.clone(),
                                status: data.status.clone(),
                                embedded_at: data.embedded_at.clone(),
                                error: data.error.clone(),
                            },
                        )
                        .expect("embedding_status.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("embedding_status.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
                Op::MarkEmbedded {
                    entity_type,
                    entity_id,
                    profile_id,
                    user_id,
                } => {
                    let wrote = repo
                        .mark_as_embedded(entity_type, entity_id, profile_id, user_id)
                        .expect("embedding_status.mark_as_embedded");
                    // Both arms write: the update arm because the row exists,
                    // the create arm because it mints one.
                    assert!(wrote, "mark_as_embedded wrote nothing for {entity_id}");
                }
                Op::MarkFailed {
                    entity_type,
                    entity_id,
                    profile_id,
                    error,
                    user_id,
                } => {
                    let wrote = repo
                        .mark_as_failed(entity_type, entity_id, profile_id, error, user_id)
                        .expect("embedding_status.mark_as_failed");
                    assert!(wrote, "mark_as_failed wrote nothing for {entity_id}");
                }
            }
        }
    }

    // P4.d27 — the new read, over the same post-op state, in corpus order.
    {
        let repo = writer.embedding_status();
        for (read, (want_name, want_ids)) in spec.list_failed_reads.iter().zip(oracle_reads.iter())
        {
            let mut got: Vec<String> = repo
                .list_failed_entity_ids(&read.entity_type, &read.profile_id)
                .into_iter()
                .collect();
            got.sort();
            assert_eq!(
                &got, want_ids,
                "listFailedEntityIds read {want_name} diverges"
            );
        }
    }

    let mut got = writer
        .dump_table_json("embedding_status", "id")
        .expect("dump embedding_status");

    let _ = std::fs::remove_file(&work);

    // One normalization (placeholder the minted columns + natural-key sort),
    // applied to both dumps.
    normalize(&mut got, "rust", &pinned);
    normalize(&mut oracle, "oracle", &pinned);

    // Corpus-shape guard: the mark* CREATE arms must actually have minted rows,
    // or a port that still returned a silent no-op would agree with the oracle.
    let minted = got["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|r| r.get("id").and_then(Value::as_str) == Some("<minted>"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        minted, 3,
        "the corpus stopped exercising the mark* create arms (3 expected)"
    );

    // Structural diff: table + columns + rows must match (ignore the oracle's
    // "case" label). assert_eq on serde_json::Value is order-independent for
    // object keys and exact for the row arrays (both sides sorted by id).
    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: embedding_status tier-2 matched oracle ({n} rows).");
}
