//! Admission — how a memory enters or changes in the Commonplace Book: the
//! STATEFUL half of v4's `lib/memory/admission/` at the `83334b9a5` pin
//! (`8af764514` Phase C, `af48f6cc2` Phase B's hand edit, `ac6348c5f` Phase D's
//! rate ledger and dry-run book, bugs 200 / 203 / 204 / 209 / 220 / 226 / 231 /
//! 234 / 237), over P4.D289's pure [`crate::admission`] (the decider, THE ONE
//! COMBINE, the thresholds) and P4.D288's index store over rows.
//!
//! v4's barrel (`admission/index.ts`), carried:
//!
//!  - [`admit_candidate`] — the one way a candidate becomes (or reinforces) a
//!    row;
//!  - [`crate::admission::decide_admission`] — the pure decision;
//!  - [`crate::admission::fold_into`] / [`apply_fold`] — the one combine, and
//!    its writes;
//!  - [`update_memory`] — a hand edit.
//!
//! **v5 has no job child.** v4 splits admission across processes (the child
//! prepares and embeds, the parent searches, decides and writes over
//! host-RPC). v5's in-process runner IS the parent, so every admission runs
//! the parent half in place; the child's half — the host-RPC `admitMemory` /
//! `countRecentAdmissions`, `patchChildIndex` and its DEBUG, the two
//! job-child refusals — is NO-PORT (LR-4, ratified by P4.D303's R-AL record).

mod admit;
mod apply;
mod dry_run_book;
mod links;
mod neighbours;
mod prepare;
mod rate_ledger;
mod update_memory;

pub use admit::{
    admission_content_key, admit_candidate, admit_row, reset_admission_state_for_tests,
    with_admission_lock, AdmissionKeyScope, AdmitOptions, ADMISSION_LEDGER_MAX,
    ADMISSION_LEDGER_TTL_MS, EMBEDDING_RETRY_DELAY_MS,
};
pub use apply::{apply_fold, insert_row, raise_importance, reembed_row, without_embedding};
pub use dry_run_book::{
    book_neighbours, create_dry_run_book, record_book_insert, BookEntry, DryRunBook,
};
pub use links::link_both_ways;
pub use neighbours::{
    gather_neighbours, GatheredNeighbours, NeighbourProbe, ADMISSION_MAX_SEARCH_K, MAX_DIGEST_HOPS,
};
pub use prepare::{apply_episodic_fallback_anchors, apply_name_presence_check};
pub use rate_ledger::{
    count_recent_admissions, record_admission_insert, reset_rate_ledger_for_tests,
    RATE_LEDGER_WINDOW_MS,
};
pub use update_memory::{update_memory, MemoryEditPatch, UpdateMemoryResult};

use serde_json::Value;

use crate::memory_embedding_text::{AnchorEntities, EmbeddingTextSource};

/// The embedded-text view of a memory row's JSON (v4 passes the whole
/// `Memory` to `embeddingTextFor` / `lexicalEmbeddingTextFor`).
pub(crate) fn text_source_of(memory: &Value) -> EmbeddingTextSource<'_> {
    let text = |key: &str| memory.get(key).and_then(Value::as_str);
    EmbeddingTextSource {
        summary: text("summary").unwrap_or_default(),
        content: text("content").unwrap_or_default(),
        occurred_at: text("occurredAt"),
        narrative_time: text("narrativeTime"),
        entities: memory
            .get("entities")
            .and_then(Value::as_array)
            .map_or(AnchorEntities::None, |a| AnchorEntities::Json(a.as_slice())),
    }
}

/// `String(x ?? null)` for a log field — v4 writes an explicit `null`, which
/// the file layer renders as the bare word.
pub(crate) fn or_null(value: Option<&str>) -> &str {
    value.unwrap_or("null")
}

/// The current wall clock in ms as a JS number.
pub(crate) fn now_ms() -> f64 {
    crate::clock::now_unix_ms() as f64
}

#[cfg(test)]
mod tests {
    //! Unit tests of the stateful admission half over a real (temp, encrypted)
    //! database — the arms `admission_tier3` / `update_memory_tier2` cannot
    //! reach through v4's corpus, each line with its capture pin AND its silence
    //! leg (§R.5). Mirrors of v4's `admission-fold.test.ts` (the seven
    //! `applyFold` cases), `admission-neighbours.test.ts` (the merge, the probe
    //! and the exclude cases; the stale-cache case as the NEGATIVE pin, LR-11)
    //! and the prepare / raise-importance WARNs.
    //!
    //! The races v4's tests stage with an in-memory store ("retired between the
    //! plan and the write", "a concurrent reinforcement lands first") are staged
    //! here with SQLite triggers on the temp table, so the REAL repository
    //! verbs meet them.

    use serde_json::{json, Value};

    use super::*;
    use crate::admission::{fold_into, FoldSource, FOLD_CAPS};
    use crate::db::runtime::Db;
    use crate::db::Writer;
    use crate::embedding_blob::float32_to_blob;
    use crate::memory_embedding_text::EmbeddingTextSource;
    use crate::model::embedding::CannedEmbeddingProvider;
    use crate::test_support::global_capture::capture_async;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR: &str = "c0000000-0000-4000-8000-00000000c001";
    const SURVIVOR: &str = "a0000000-0000-4000-8000-00000000c0a1";
    const TS: &str = "2026-06-01T12:00:00.000Z";

    const DDL: &str = "
    CREATE TABLE memories (
        id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
        projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
        importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
        occurredAt TEXT, narrativeTime TEXT, entities TEXT DEFAULT '[]',
        kind TEXT DEFAULT 'semantic', sourceMessageId TEXT, lastAccessedAt TEXT,
        reinforcementCount REAL, lastReinforcedAt TEXT, relatedMemoryIds TEXT,
        reinforcedImportance REAL, tier TEXT DEFAULT 'hot', supersededById TEXT, supersededAt TEXT, consolidatedFrom TEXT DEFAULT '[]', consolidatedAt TEXT, createdAt TEXT, updatedAt TEXT);
    CREATE TABLE chat_settings (id TEXT PRIMARY KEY, timezone TEXT);
";

    /// A seed row: `(id, content, vector, tier, extra SQL after the insert)`.
    struct Seed<'a> {
        id: &'a str,
        content: &'a str,
        vector: Option<Vec<f32>>,
        tier: &'a str,
        count: f64,
        source: &'a str,
        importance: f64,
    }

    impl<'a> Seed<'a> {
        fn hot(id: &'a str, content: &'a str, vector: Vec<f32>) -> Self {
            Seed {
                id,
                content,
                vector: Some(vector),
                tier: "hot",
                count: 1.0,
                source: "AUTO",
                importance: 0.5,
            }
        }
    }

    fn make_db(seeds: &[Seed<'_>], extra_sql: &str) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("admission-unit.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
            for s in seeds {
                w.connection()
                .execute(
                    "INSERT INTO memories (id, characterId, content, summary, keywords, tags, importance, \
                     embedding, source, entities, kind, reinforcementCount, relatedMemoryIds, \
                     reinforcedImportance, tier, consolidatedFrom, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, ?3, '[]', '[]', ?4, ?5, ?6, '[]', 'semantic', ?7, '[]', ?4, ?8, '[]', ?9, ?9)",
                    rusqlite::params![
                        s.id,
                        CHAR,
                        s.content,
                        s.importance,
                        s.vector.as_deref().map(float32_to_blob),
                        s.source,
                        s.count,
                        s.tier,
                        TS
                    ],
                )
                .unwrap();
            }
            if !extra_sql.is_empty() {
                w.connection().execute_batch(extra_sql).unwrap();
            }
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn read(db: &Db, id: &str) -> Option<Value> {
        db.read_main(|c| crate::db::memories_read::find_by_id(c, id))
            .unwrap()
    }

    fn lines_with<'a>(lines: &'a [String], needle: &str) -> Vec<&'a String> {
        lines.iter().filter(|l| l.contains(needle)).collect()
    }

    fn text(summary: &str, content: &str) -> String {
        format!("{summary}\n\n{content}")
    }

    fn plan_for(
        survivor: &Value,
        content: &str,
        entities: Option<Vec<String>>,
    ) -> crate::admission::FoldPlan {
        fold_into(
            survivor,
            &[FoldSource {
                content: content.into(),
                entities,
                reinforcement_count: Some(1.0),
                last_reinforced_at: Some("2026-06-02T00:00:00.000Z".into()),
                ..Default::default()
            }],
            &FOLD_CAPS,
        )
    }

    // ---------------------------------------------------------------------------
    // applyFold (v4 `admission-fold.test.ts`'s `applyFold` describe)
    // ---------------------------------------------------------------------------

    /// "writes footnotes, links and the count as deltas and re-embeds through
    /// the index".
    #[tokio::test(flavor = "current_thread")]
    async fn apply_fold_writes_footnotes_links_and_the_count_and_re_embeds() {
        let (_d, db) = make_db(
            &[
                Seed::hot(SURVIVOR, "Wren keeps the lamp.", vec![1.0, 0.0]),
                Seed::hot(
                    "a0000000-0000-4000-8000-00000000c0a2",
                    "A neighbour.",
                    vec![0.0, 1.0],
                ),
            ],
            "",
        );
        let survivor = read(&db, SURVIVOR).unwrap();
        let mut plan = plan_for(&survivor, "Wren keeps the lamp with Mariana.", None);
        plan.add_links = vec!["a0000000-0000-4000-8000-00000000c0a2".into()];
        let new_content = plan.content.clone();
        let provider = CannedEmbeddingProvider::new()
            .with_vector(text("Wren keeps the lamp.", &new_content), vec![0.6, 0.8]);
        let (out, lines) = capture_async(apply_fold(
            &db,
            Some(&provider),
            &survivor,
            &plan,
            Some("user"),
            "UTC",
        ))
        .await;
        let out = out.unwrap().expect("the fold landed");
        assert_eq!(out["content"], new_content.as_str());
        assert_eq!(out["reinforcementCount"], 2);
        assert_eq!(
            out["relatedMemoryIds"],
            json!(["a0000000-0000-4000-8000-00000000c0a2"])
        );
        assert_eq!(
            out["embedding"],
            json!({ "0": 0.6000000238418579, "1": 0.800000011920929 })
        );
        let folded = lines_with(&lines, "Folded into survivor");
        assert_eq!(folded.len(), 1, "{lines:#?}");
        assert!(folded[0].contains("module=memory:admission:apply"));
        assert!(folded[0].contains("footnotes=1"));
        assert!(folded[0].contains("anchorsJson=[]"));
        assert!(folded[0].contains("links=1"));
        assert!(folded[0].contains("reinforcementDelta=1"));
        assert!(folded[0].contains("reembedded=true"));
        assert_eq!(lines_with(&lines, "Re-embedded memory").len(), 1);
        // Silence: nothing missed, nothing refused.
        assert!(lines_with(&lines, "Fold step missed").is_empty());
        assert!(lines_with(&lines, "Fold target is gone").is_empty());
        assert!(lines_with(&lines, "no userId").is_empty());
    }

    /// "writes anchor changes in one row update".
    #[tokio::test(flavor = "current_thread")]
    async fn apply_fold_writes_anchor_changes_in_one_row_update() {
        let (_d, db) = make_db(
            &[Seed::hot(SURVIVOR, "Wren keeps the lamp.", vec![1.0, 0.0])],
            "",
        );
        let survivor = read(&db, SURVIVOR).unwrap();
        let plan = plan_for(
            &survivor,
            "Wren keeps the lamp.",
            Some(vec!["Gull Point".into()]),
        );
        assert_eq!(plan.patch.keys(), ["entities"]);
        let (out, lines) = capture_async(apply_fold::<CannedEmbeddingProvider>(
            &db, None, &survivor, &plan, None, "UTC",
        ))
        .await;
        let out = out.unwrap().unwrap();
        assert_eq!(out["entities"], json!(["Gull Point"]));
        assert!(
            lines_with(&lines, "Folded into survivor")[0].contains(r#"anchorsJson=["entities"]"#)
        );
        // The anchor line changed and no userId was given: the WARN, not a
        // re-embed.
        let warned = lines_with(&lines, "Survivor changed but no userId to re-embed with");
        assert_eq!(warned.len(), 1, "{lines:#?}");
        assert!(
            warned[0].starts_with("WARN") && warned[0].contains(&format!("survivorId={SURVIVOR}"))
        );
    }

    /// "does not re-embed when only the count changes".
    #[tokio::test(flavor = "current_thread")]
    async fn apply_fold_does_not_re_embed_when_only_the_count_changes() {
        let (_d, db) = make_db(
            &[Seed::hot(SURVIVOR, "Wren keeps the lamp.", vec![1.0, 0.0])],
            "",
        );
        let survivor = read(&db, SURVIVOR).unwrap();
        let plan = plan_for(&survivor, "Wren keeps the lamp.", None);
        assert!(!plan.reembed && plan.footnotes.is_empty());
        // An empty provider: any call would fail the re-embed loudly.
        let provider = CannedEmbeddingProvider::new();
        let (out, lines) = capture_async(apply_fold(
            &db,
            Some(&provider),
            &survivor,
            &plan,
            Some("user"),
            "UTC",
        ))
        .await;
        assert_eq!(out.unwrap().unwrap()["reinforcementCount"], 2);
        assert!(lines_with(&lines, "Re-embedded memory").is_empty());
        assert!(lines_with(&lines, "Failed to re-embed").is_empty());
        assert!(lines_with(&lines, "Folded into survivor")[0].contains("reembedded=false"));
    }

    /// "commit point: once the footnotes landed, a missed count still returns
    /// the survivor" — the row is retired the moment its footnotes land.
    #[tokio::test(flavor = "current_thread")]
    async fn apply_fold_commit_point_a_missed_step_after_the_footnotes_still_stands() {
        let trigger = format!(
            "CREATE TRIGGER retire_after_footnotes AFTER UPDATE OF content ON memories \
         WHEN NEW.id = '{SURVIVOR}' BEGIN DELETE FROM memories WHERE id = NEW.id; END;"
        );
        let (_d, db) = make_db(
            &[Seed::hot(SURVIVOR, "Wren keeps the lamp.", vec![1.0, 0.0])],
            &trigger,
        );
        let survivor = read(&db, SURVIVOR).unwrap();
        let plan = plan_for(&survivor, "Wren keeps the lamp with Mariana.", None);
        let (out, lines) = capture_async(apply_fold::<CannedEmbeddingProvider>(
            &db, None, &survivor, &plan, None, "UTC",
        ))
        .await;
        let out = out
            .unwrap()
            .expect("the fold stands once its first write landed");
        assert_eq!(out["content"], plan.content.as_str());
        let missed = lines_with(
            &lines,
            "Fold step missed after the fold landed; the fold stands",
        );
        assert_eq!(missed.len(), 1, "{lines:#?}");
        assert!(missed[0].starts_with("WARN") && missed[0].contains("step=reinforcement"));
        assert!(lines_with(&lines, "Fold target is gone").is_empty());
    }

    /// "reports failure when nothing landed (the survivor is gone)".
    #[tokio::test(flavor = "current_thread")]
    async fn apply_fold_reports_failure_when_nothing_landed() {
        let (_d, db) = make_db(&[], "");
        let survivor =
            json!({ "id": SURVIVOR, "characterId": CHAR, "content": "Wren keeps the lamp." });
        let plan = plan_for(&survivor, "Wren keeps the lamp with Mariana.", None);
        let (out, lines) = capture_async(apply_fold::<CannedEmbeddingProvider>(
            &db, None, &survivor, &plan, None, "UTC",
        ))
        .await;
        assert_eq!(out.unwrap(), None);
        let gone = lines_with(&lines, "Fold target is gone; nothing folded");
        assert_eq!(gone.len(), 1, "{lines:#?}");
        assert!(gone[0].contains("step=footnotes"));
        assert!(lines_with(&lines, "Folded into survivor").is_empty());
    }

    /// "bug 226: the delta lands on the committed row, so a concurrent
    /// reinforcement survives" — five observations counted between the plan and
    /// its count.
    #[tokio::test(flavor = "current_thread")]
    async fn apply_fold_bug_226_a_concurrent_reinforcement_survives() {
        let trigger = format!(
            "CREATE TRIGGER concurrent_reinforcement AFTER UPDATE OF content ON memories \
         WHEN NEW.id = '{SURVIVOR}' BEGIN \
         UPDATE memories SET reinforcementCount = reinforcementCount + 5 WHERE id = NEW.id; END;"
        );
        let (_d, db) = make_db(
            &[Seed::hot(SURVIVOR, "Wren keeps the lamp.", vec![1.0, 0.0])],
            &trigger,
        );
        let survivor = read(&db, SURVIVOR).unwrap();
        let plan = plan_for(&survivor, "Wren keeps the lamp with Mariana.", None);
        let out = apply_fold::<CannedEmbeddingProvider>(&db, None, &survivor, &plan, None, "UTC")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            out["reinforcementCount"], 7,
            "1 + 5 concurrent + this fold's 1"
        );
        assert_eq!(read(&db, SURVIVOR).unwrap()["reinforcementCount"], 7);
    }

    /// A failed re-embed keeps the old vector (WARN), and the fold stands.
    #[tokio::test(flavor = "current_thread")]
    async fn apply_fold_a_failed_re_embed_keeps_the_old_vector() {
        let (_d, db) = make_db(
            &[Seed::hot(SURVIVOR, "Wren keeps the lamp.", vec![1.0, 0.0])],
            "",
        );
        let survivor = read(&db, SURVIVOR).unwrap();
        let plan = plan_for(&survivor, "Wren keeps the lamp with Mariana.", None);
        let provider = CannedEmbeddingProvider::new()
            .with_failure(text("Wren keeps the lamp.", &plan.content));
        let (out, lines) = capture_async(apply_fold(
            &db,
            Some(&provider),
            &survivor,
            &plan,
            Some("user"),
            "UTC",
        ))
        .await;
        let out = out.unwrap().unwrap();
        assert!(out.get("embedding").is_some());
        let failed = lines_with(&lines, "Failed to re-embed memory; it keeps its old vector");
        assert_eq!(failed.len(), 1, "{lines:#?}");
        assert!(
            failed[0].starts_with("WARN") && failed[0].contains("error=canned embedding failure")
        );
        assert!(lines_with(&lines, "Folded into survivor")[0].contains("reembedded=false"));
    }

    /// Bug 204: at the footnote cap nothing more is appended, but a changed
    /// anchor line (an entity union) still re-embeds — v4's `reembed = content
    /// changed || patch non-empty`, the cap notwithstanding.
    #[tokio::test(flavor = "current_thread")]
    async fn apply_fold_re_embeds_at_the_footnote_cap_when_the_anchor_line_moves() {
        let capped = format!(
            "Wren keeps the lamp.{}",
            (1..=8)
                .map(|n| format!("\n[+] detail {n}"))
                .collect::<String>()
        );
        let (_d, db) = make_db(&[Seed::hot(SURVIVOR, &capped, vec![1.0, 0.0])], "");
        let survivor = read(&db, SURVIVOR).unwrap();
        let plan = plan_for(
            &survivor,
            "Wren keeps the lamp with Mariana.",
            Some(vec!["Gull Point".into()]),
        );
        assert!(
            plan.at_cap && plan.footnotes.is_empty() && plan.reembed,
            "{plan:?}"
        );
        // `make_db` seeds `summary` = `content`.
        let source = EmbeddingTextSource {
            summary: &capped,
            content: &capped,
            entities: crate::memory_embedding_text::AnchorEntities::Strings(&[
                "Gull Point".to_string()
            ]),
            ..Default::default()
        };
        let provider = CannedEmbeddingProvider::new().with_vector(
            crate::memory_embedding_text::embedding_text_for(&source, "UTC"),
            vec![0.0, 1.0],
        );
        let (out, lines) = capture_async(apply_fold(
            &db,
            Some(&provider),
            &survivor,
            &plan,
            Some("user"),
            "UTC",
        ))
        .await;
        let out = out.unwrap().unwrap();
        assert_eq!(out["embedding"], json!({ "0": 0, "1": 1 }));
        assert!(
            lines_with(&lines, "Folded into survivor")[0].contains("reembedded=true"),
            "{lines:#?}"
        );
    }

    // ---------------------------------------------------------------------------
    // raise_importance
    // ---------------------------------------------------------------------------

    #[tokio::test(flavor = "current_thread")]
    async fn raise_importance_raises_never_lowers_and_warns_on_a_vanished_row() {
        let (_d, db) = make_db(&[Seed::hot(SURVIVOR, "Ash sings.", vec![1.0, 0.0])], "");
        let row = read(&db, SURVIVOR).unwrap();
        // Never lowers; no line.
        let (kept, lines) = capture_async(raise_importance(&db, row.clone(), Some(0.2))).await;
        assert_eq!(kept.unwrap()["importance"], 0.5);
        assert!(
            lines.iter().all(|l| !l.contains("importance")),
            "{lines:#?}"
        );
        // Raises (with the count's reinforced importance).
        let (raised, lines) = capture_async(raise_importance(&db, row.clone(), Some(0.8))).await;
        assert_eq!(raised.unwrap()["importance"], 0.8);
        assert_eq!(
            lines_with(&lines, "Raised importance for a manual write").len(),
            1
        );
        // A row that vanished: the WARN, and the row as it was.
        let ghost = json!({ "id": "a0000000-0000-4000-8000-00000000dead", "characterId": CHAR, "importance": 0.1 });
        let (same, lines) = capture_async(raise_importance(&db, ghost.clone(), Some(0.9))).await;
        assert_eq!(same.unwrap(), ghost);
        let warned = lines_with(
            &lines,
            "Failed to raise the importance of a row a manual memory folded into",
        );
        assert_eq!(warned.len(), 1, "{lines:#?}");
        assert!(warned[0].starts_with("WARN"));
    }

    // ---------------------------------------------------------------------------
    // prepare
    // ---------------------------------------------------------------------------

    /// v4's catch arm: the character read failed (here, an instance with no
    /// mount-index partition) — WARN, and the proposal passes through.
    #[tokio::test(flavor = "current_thread")]
    async fn name_presence_a_failed_read_warns_and_keeps_the_proposal() {
        let (_d, db) = make_db(&[], "");
        let row = crate::admission::AdmissionRow {
            character_id: CHAR.into(),
            content: "She sings.".into(),
            summary: "Singing".into(),
            source: Some("AUTO".into()),
            about_character_id: Some("c0000000-0000-4000-8000-00000000c002".into()),
            ..Default::default()
        };
        let mut notes = Vec::new();
        let (out, lines) =
            capture_async(async { apply_name_presence_check(&db, row.clone(), &mut notes) }).await;
        assert_eq!(out, row);
        assert!(notes.is_empty());
        let warned = lines_with(
            &lines,
            "[Admission] Name-presence check failed; using proposed aboutCharacterId unchanged",
        );
        assert_eq!(warned.len(), 1, "{lines:#?}");
        assert!(warned[0].starts_with("WARN") && !warned[0].contains("module="));
        // Silence: a MANUAL row never reads (so never warns).
        let manual = crate::admission::AdmissionRow {
            source: Some("MANUAL".into()),
            ..row
        };
        let (_, lines) =
            capture_async(async { apply_name_presence_check(&db, manual, &mut notes) }).await;
        assert!(lines_with(&lines, "[Admission]").is_empty(), "{lines:#?}");
    }

    // ---------------------------------------------------------------------------
    // gather_neighbours (v4 `admission-neighbours.test.ts`)
    // ---------------------------------------------------------------------------

    /// "searches both tiers" + "collapses several members of one cluster onto
    /// the digest, best cosine kept" + "passes a hot row through untouched".
    #[tokio::test(flavor = "current_thread")]
    async fn gather_searches_both_tiers_and_collapses_members_onto_their_digest() {
        let digest = "a0000000-0000-4000-8000-00000000d1d1";
        let sql = format!(
            "UPDATE memories SET supersededById = '{digest}' WHERE id IN \
         ('a0000000-0000-4000-8000-00000000c0b1', 'a0000000-0000-4000-8000-00000000c0b2');"
        );
        let mut m1 = Seed::hot(
            "a0000000-0000-4000-8000-00000000c0b1",
            "Member one.",
            vec![0.9, 0.436],
        );
        m1.tier = "cold";
        let mut m2 = Seed::hot(
            "a0000000-0000-4000-8000-00000000c0b2",
            "Member two.",
            vec![0.8, 0.6],
        );
        m2.tier = "cold";
        let (_d, db) = make_db(
            &[
                m1,
                m2,
                Seed::hot(digest, "The digest.", vec![0.0, 1.0]),
                Seed::hot(SURVIVOR, "A hot row.", vec![0.7, 0.714]),
            ],
            &sql,
        );
        let g = gather_neighbours(
            &db,
            &CannedEmbeddingProvider::new(),
            CHAR,
            &[1.0, 0.0],
            None,
            None,
            "UTC",
        )
        .await
        .unwrap();
        let ids: Vec<(&str, Option<&str>)> = g
            .neighbours
            .iter()
            .map(|n| (n.memory["id"].as_str().unwrap(), n.via.as_deref()))
            .collect();
        // The digest stands once, at its best member's cosine, via that member.
        assert_eq!(
            ids,
            [
                (digest, Some("a0000000-0000-4000-8000-00000000c0b1")),
                (SURVIVOR, None),
            ]
        );
        assert!((g.neighbours[0].cosine - 0.9).abs() < 1e-6);
        assert_eq!(g.searched_k, 5);
        assert!(g.orphan_ids.is_empty());
    }

    /// Bug 237: "merges re-embedded rows with indexed hits, best first", "never
    /// re-embeds a row the dry run excludes", and the stale-cache NEGATIVE pin
    /// (LR-11 — v5 keeps no cache, so the second measure sees what the first
    /// saw).
    #[tokio::test(flavor = "current_thread")]
    async fn gather_merges_re_embedded_rows_best_first_and_skips_excluded() {
        let (_d, db) = make_db(
            &[
                Seed::hot(
                    "a0000000-0000-4000-8000-00000000c0c1",
                    "Indexed lighthouse ledger.",
                    vec![0.5, 0.866],
                ),
                Seed::hot(
                    "a0000000-0000-4000-8000-00000000c0c2",
                    "Old lighthouse ledger.",
                    vec![1.0, 0.0, 0.0],
                ),
                Seed::hot(
                    "a0000000-0000-4000-8000-00000000c0c3",
                    "Excluded lighthouse ledger.",
                    vec![0.0, 1.0, 0.0],
                ),
            ],
            "",
        );
        let provider = CannedEmbeddingProvider::new().with_vector(
            text("Old lighthouse ledger.", "Old lighthouse ledger."),
            vec![0.95, 0.312],
        );
        let exclude: std::collections::HashSet<String> =
            ["a0000000-0000-4000-8000-00000000c0c3".to_string()].into();
        let probe = NeighbourProbe {
            text: "The lighthouse ledger",
            user_id: "user",
        };
        let (g, lines) = capture_async(gather_neighbours(
            &db,
            &provider,
            CHAR,
            &[1.0, 0.0],
            Some(&exclude),
            Some(&probe),
            "UTC",
        ))
        .await;
        let g = g.unwrap();
        let ids: Vec<&str> = g
            .neighbours
            .iter()
            .map(|n| n.memory["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            [
                "a0000000-0000-4000-8000-00000000c0c2",
                "a0000000-0000-4000-8000-00000000c0c1"
            ]
        );
        assert_eq!(g.reembedded, Some(1));
        let warned = lines_with(
            &lines,
            "Admission index cannot compare some rows with the candidate; matching them by text",
        );
        assert_eq!(warned.len(), 1, "{lines:#?}");
        assert!(warned[0].contains("unscorable=2") && warned[0].contains("dominantDimensions=3"));
        // LR-11: the stale-cache arm cannot fire without a cache.
        assert!(lines_with(&lines, "Admission index was stale").is_empty());
        // The excluded row was never re-embedded (the provider has no entry for
        // it: a call would have logged the skip).
        assert!(lines_with(&lines, "Could not re-embed an unindexed neighbour").is_empty());
    }

    /// "does not fall back without a probe", and a uniform store measures
    /// nothing (silence for both bug-237 lines).
    #[tokio::test(flavor = "current_thread")]
    async fn gather_without_a_probe_never_re_embeds_and_a_uniform_store_is_silent() {
        let (_d, db) = make_db(
            &[Seed::hot(
                "a0000000-0000-4000-8000-00000000c0d1",
                "Old width.",
                vec![1.0, 0.0, 0.0],
            )],
            "",
        );
        let (g, lines) = capture_async(gather_neighbours(
            &db,
            &CannedEmbeddingProvider::new(),
            CHAR,
            &[1.0, 0.0],
            None,
            None,
            "UTC",
        ))
        .await;
        let g = g.unwrap();
        assert!(g.neighbours.is_empty() && g.reembedded.is_none());
        assert!(lines_with(&lines, "Scored unindexed admission neighbours").is_empty());

        let (_d2, uniform) = make_db(&[Seed::hot(SURVIVOR, "Same width.", vec![1.0, 0.0])], "");
        let (_, lines) = capture_async(gather_neighbours(
            &uniform,
            &CannedEmbeddingProvider::new(),
            CHAR,
            &[1.0, 0.0],
            None,
            None,
            "UTC",
        ))
        .await;
        assert!(
            lines_with(&lines, "cannot compare").is_empty(),
            "{lines:#?}"
        );
        assert!(lines_with(&lines, "was stale").is_empty());
        assert_eq!(lines_with(&lines, "Gathered admission neighbours").len(), 1);
    }

    // ---------------------------------------------------------------------------
    // link_both_ways
    // ---------------------------------------------------------------------------

    #[tokio::test(flavor = "current_thread")]
    async fn link_both_ways_links_each_target_and_the_row_without_itself() {
        let (_d, db) = make_db(
            &[
                Seed::hot(SURVIVOR, "Row.", vec![1.0, 0.0]),
                Seed::hot(
                    "a0000000-0000-4000-8000-00000000c0e1",
                    "Target.",
                    vec![0.0, 1.0],
                ),
            ],
            "",
        );
        let (out, lines) = capture_async(link_both_ways(
            &db,
            CHAR,
            SURVIVOR,
            &[
                SURVIVOR.to_string(),
                "a0000000-0000-4000-8000-00000000c0e1".to_string(),
            ],
        ))
        .await;
        assert_eq!(out.unwrap(), ["a0000000-0000-4000-8000-00000000c0e1"]);
        assert_eq!(
            read(&db, "a0000000-0000-4000-8000-00000000c0e1").unwrap()["relatedMemoryIds"],
            json!([SURVIVOR])
        );
        let linked = lines_with(&lines, "[Admission] Linked memory to neighbours both ways");
        assert_eq!(linked.len(), 1, "{lines:#?}");
        assert!(linked[0].contains("linked=1") && !linked[0].contains("module="));
    }

    // ---------------------------------------------------------------------------
    // The lock (LR-3)
    // ---------------------------------------------------------------------------

    /// FIFO per character, and the map entry is dropped when its last holder
    /// leaves (v4's `if (tail === mine) delete`).
    #[tokio::test(flavor = "current_thread")]
    async fn the_admission_lock_serialises_one_character_in_order() {
        let order = std::sync::Mutex::new(Vec::new());
        let run = |n: u32| {
            let order = &order;
            with_admission_lock("lock-unit-char", async move {
                order.lock().unwrap().push(format!("start {n}"));
                tokio::task::yield_now().await;
                order.lock().unwrap().push(format!("end {n}"));
            })
        };
        tokio::join!(run(1), run(2), run(3));
        assert_eq!(
            *order.lock().unwrap(),
            ["start 1", "end 1", "start 2", "end 2", "start 3", "end 3"]
        );
    }

    // ---------------------------------------------------------------------------
    // The C1-B futures are `Send` over the production provider (the stage-C job
    // handlers spawn them).
    // ---------------------------------------------------------------------------

    #[allow(dead_code)]
    fn c1_b_futures_are_send(db: &Db, book: &mut DryRunBook, scope: &AdmissionKeyScope) {
        fn assert_send<T: Send>(_: T) {}
        let provider = crate::model::embedding::ErasedEmbeddingProvider::none();
        assert_send(admit_row(
            db,
            &provider,
            crate::admission::AdmissionRow::default(),
            AdmitOptions {
                user_id: "u",
                dry_run: true,
                backfill: false,
                dry_run_book: Some(book),
                key_scope: Some(scope),
                host_zone_fallback: "UTC",
            },
        ));
        assert_send(update_memory(
            db,
            &provider,
            "c",
            "m",
            MemoryEditPatch::default(),
            "u",
            "UTC",
        ));
        assert_send(apply_fold(
            db,
            Some(&provider),
            &Value::Null,
            &plan_for(&Value::Null, "", None),
            None,
            "UTC",
        ));
        assert_send(gather_neighbours(
            db,
            &provider,
            "c",
            &[],
            None,
            None,
            "UTC",
        ));
        assert_send(with_admission_lock("c", async {}));
        assert_send(link_both_ways(db, "c", "m", &[]));
    }

    // ---------------------------------------------------------------------------
    // admitCandidate (parent) — v4 `admission-admit.test.ts`, bar its two
    // job-child cases (NO-PORT, LR-4: the child-cache patch and `admitPrepared`'s
    // child refusal).
    // ---------------------------------------------------------------------------

    /// These tests share the process-global lock map and idempotency ledger with
    /// every other test in the binary, so none resets them (a reset in a parallel
    /// test would wipe another's ledger between its two attempts); each test's
    /// rows, keys and job ids are its own.
    mod admit_mirror {
        use super::*;
        use crate::admission::AdmissionRow;
        use crate::model::embedding::EmbeddingResult;

        const LEDGER: [f32; 3] = [1.0, 0.0, 0.0];
        /// cos(LEDGER, NEAR) ≈ 0.87 — the reinforce band.
        const NEAR: [f32; 3] = [0.87, 0.493_051_7, 0.0];
        /// cos ≈ 0.75 — the related band.
        const BAND: [f32; 3] = [0.75, 0.661_437_8, 0.0];
        const SEED: &str = "a0000000-0000-4000-8000-00000000ad01";

        fn cand(text: &str, source: &str) -> AdmissionRow {
            AdmissionRow {
                character_id: CHAR.into(),
                content: text.into(),
                summary: text.into(),
                source: Some(source.into()),
                importance: Some(0.5),
                ..Default::default()
            }
        }

        fn opts() -> AdmitOptions<'static> {
            AdmitOptions {
                user_id: "user",
                dry_run: false,
                backfill: false,
                dry_run_book: None,
                key_scope: None,
                host_zone_fallback: "UTC",
            }
        }

        fn provider(entries: &[(&str, [f32; 3])]) -> CannedEmbeddingProvider {
            let mut p = CannedEmbeddingProvider::new();
            for (t, v) in entries {
                p = p.with_vector(text(t, t), v.to_vec());
            }
            p
        }

        fn count_rows(db: &Db) -> i64 {
            db.read_main(|c| Ok(c.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?))
                .unwrap()
        }

        fn kind(o: &crate::admission::AdmissionOutcome) -> &'static str {
            o.action.kind().as_str()
        }

        #[tokio::test(flavor = "current_thread")]
        async fn inserts_into_an_empty_book_with_its_vector_at_count_1() {
            let (_d, db) = make_db(&[], "");
            let p = provider(&[("Friday keeps the ledger", LEDGER)]);
            let o = admit_row(&db, &p, cand("Friday keeps the ledger", "AUTO"), opts())
                .await
                .unwrap();
            assert_eq!(kind(&o), "insert");
            let m = o.memory.as_ref().unwrap();
            assert!(m.get("embedding").is_none());
            let row = read(&db, m["id"].as_str().unwrap()).unwrap();
            assert_eq!(row["reinforcementCount"], 1);
            assert!(row.get("embedding").is_some());
        }

        #[tokio::test(flavor = "current_thread")]
        async fn absorbs_a_near_duplicate_by_count_only() {
            let (_d, db) = make_db(
                &[Seed::hot(SEED, "Friday keeps the ledger.", LEDGER.to_vec())],
                "",
            );
            let p = provider(&[("Friday keeps the ledger", LEDGER)]);
            let o = admit_row(&db, &p, cand("Friday keeps the ledger", "AUTO"), opts())
                .await
                .unwrap();
            assert_eq!(kind(&o), "absorb");
            let row = read(&db, SEED).unwrap();
            assert_eq!(row["reinforcementCount"], 2);
            assert_eq!(row["content"], "Friday keeps the ledger.");
            assert_eq!(count_rows(&db), 1);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn reinforces_in_the_band_with_a_footnote_the_count_delta_and_a_re_embed() {
            // MANUAL on both sides: an AUTO candidate's fallback anchors would add
            // an anchor line (`Mariana`) to its embedded text.
            let mut seed = Seed::hot(SEED, "Friday keeps the ledger.", LEDGER.to_vec());
            seed.source = "MANUAL";
            let (_d, db) = make_db(&[seed], "");
            let footnoted = "Friday keeps the ledger.\n[+] Mariana";
            let p = provider(&[("Friday keeps the ledger with Mariana", NEAR)]).with_vector(
                text("Friday keeps the ledger.", footnoted),
                vec![0.0, 1.0, 0.0],
            );
            let o = admit_row(
                &db,
                &p,
                cand("Friday keeps the ledger with Mariana", "MANUAL"),
                opts(),
            )
            .await
            .unwrap();
            assert_eq!(kind(&o), "reinforce");
            assert_eq!(o.novel_details, ["Mariana"]);
            let row = read(&db, SEED).unwrap();
            assert_eq!(row["content"], footnoted);
            assert_eq!(row["reinforcementCount"], 2);
            assert_eq!(row["embedding"], json!({ "0": 0, "1": 1, "2": 0 }));
        }

        /// The post-link row (bug 26).
        #[tokio::test(flavor = "current_thread")]
        async fn links_a_related_band_row_both_ways_and_returns_the_post_link_row() {
            let (_d, db) = make_db(
                &[Seed::hot(
                    SEED,
                    "Ink in the counting house.",
                    LEDGER.to_vec(),
                )],
                "",
            );
            let p = provider(&[("Friday balances the books", BAND)]);
            let o = admit_row(&db, &p, cand("Friday balances the books", "AUTO"), opts())
                .await
                .unwrap();
            assert_eq!(kind(&o), "insert-linked");
            assert_eq!(o.linked_ids, [SEED]);
            assert_eq!(
                o.memory.as_ref().unwrap()["relatedMemoryIds"],
                json!([SEED])
            );
            let id = o.memory.as_ref().unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(read(&db, SEED).unwrap()["relatedMemoryIds"], json!([id]));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_hand_written_memory_is_never_absorbed_into_an_extracted_row() {
            let (_d, db) = make_db(
                &[Seed::hot(SEED, "Ash collects sea glass.", LEDGER.to_vec())],
                "",
            );
            let p = provider(&[("Ash collects sea glass", LEDGER)]);
            let o = admit_row(&db, &p, cand("Ash collects sea glass", "MANUAL"), opts())
                .await
                .unwrap();
            assert_eq!(kind(&o), "insert-linked");
            assert_eq!(count_rows(&db), 2);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_hand_written_memory_folded_into_a_manual_row_raises_its_importance() {
            let mut seed = Seed::hot(SEED, "Ash sings.", LEDGER.to_vec());
            seed.source = "MANUAL";
            seed.importance = 0.3;
            let (_d, db) = make_db(&[seed], "");
            let p = provider(&[("Ash sings in the choir", LEDGER)]);
            let mut c = cand("Ash sings in the choir", "MANUAL");
            c.importance = Some(0.8);
            let o = admit_row(&db, &p, c, opts()).await.unwrap();
            assert_eq!(kind(&o), "absorb");
            assert_eq!(read(&db, SEED).unwrap()["importance"], 0.8);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn a_hand_written_memory_never_lowers_importance_and_a_digest_keeps_its_source() {
            let mut seed = Seed::hot(SEED, "Ash and Wren sail.", LEDGER.to_vec());
            seed.source = "CONSOLIDATED";
            seed.importance = 0.9;
            let (_d, db) = make_db(&[seed], "");
            let p = provider(&[("Ash and Wren sail together", LEDGER)]);
            let mut c = cand("Ash and Wren sail together", "MANUAL");
            c.importance = Some(0.4);
            let o = admit_row(&db, &p, c, opts()).await.unwrap();
            assert_eq!(kind(&o), "absorb");
            let row = read(&db, SEED).unwrap();
            assert_eq!(row["importance"], 0.9);
            assert_eq!(row["source"], "CONSOLIDATED");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn promotes_a_cold_row_nothing_supersedes_then_counts_it() {
            let mut seed = Seed::hot(SEED, "Wren hums shanties.", LEDGER.to_vec());
            seed.tier = "cold";
            let (_d, db) = make_db(&[seed], "");
            let p = provider(&[("Wren hums sea shanties", LEDGER)]);
            let o = admit_row(&db, &p, cand("Wren hums sea shanties", "AUTO"), opts())
                .await
                .unwrap();
            assert_eq!(kind(&o), "promote-and-reinforce");
            let row = read(&db, SEED).unwrap();
            assert_eq!(row["tier"], "hot");
            assert_eq!(row["reinforcementCount"], 2);
            assert!(o.trace.notes[0].starts_with("promoted cold row"));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn absorbs_into_the_digest_that_supersedes_a_cold_member() {
            let digest = "a0000000-0000-4000-8000-00000000ad02";
            let mut member = Seed::hot(SEED, "Wren rows at dawn.", LEDGER.to_vec());
            member.tier = "cold";
            let mut d = Seed::hot(digest, "Wren's harbour life.", vec![0.0, 0.0, 1.0]);
            d.source = "CONSOLIDATED";
            let sql =
                format!("UPDATE memories SET supersededById = '{digest}' WHERE id = '{SEED}';");
            let (_dir, db) = make_db(&[member, d], &sql);
            let p = provider(&[("Wren rows out at dawn", LEDGER)]);
            let o = admit_row(&db, &p, cand("Wren rows out at dawn", "AUTO"), opts())
                .await
                .unwrap();
            assert_eq!(
                serde_json::to_string(&o.action).unwrap(),
                format!(r#"{{"kind":"absorb","into":"{digest}","redirectedFrom":"{SEED}"}}"#)
            );
            assert_eq!(read(&db, digest).unwrap()["reinforcementCount"], 2);
        }

        /// Invariant 4: nothing is written without a vector; two attempts.
        #[tokio::test(flavor = "current_thread", start_paused = true)]
        async fn rejects_writing_nothing_when_the_embedding_fails_twice() {
            let (_d, db) = make_db(&[], "");
            let p =
                CannedEmbeddingProvider::new().with_failure(text("Unembeddable", "Unembeddable"));
            let o = admit_row(&db, &p, cand("Unembeddable", "AUTO"), opts())
                .await
                .unwrap();
            assert_eq!(
                serde_json::to_string(&o.action).unwrap(),
                r#"{"kind":"reject","reason":"embedding-failed"}"#
            );
            assert_eq!(o.trace.embedding_attempts, 2);
            assert_eq!(count_rows(&db), 0);
        }

        /// Review #2: the lock serialises two admissions for one character.
        #[tokio::test(flavor = "current_thread")]
        async fn serialises_concurrent_admissions_for_one_character_so_they_make_one_row() {
            let (_d, db) = make_db(&[], "");
            let p = provider(&[
                ("Friday keeps the ledger.", LEDGER),
                ("Friday keeps the ledger!", LEDGER),
            ]);
            let (a, b) = tokio::join!(
                admit_row(&db, &p, cand("Friday keeps the ledger.", "AUTO"), opts()),
                admit_row(&db, &p, cand("Friday keeps the ledger!", "AUTO"), opts()),
            );
            let mut kinds = [kind(&a.unwrap()), kind(&b.unwrap())];
            kinds.sort();
            assert_eq!(kinds, ["absorb", "insert"]);
            assert_eq!(count_rows(&db), 1);
        }

        /// Review #1: a repeated job-attempt key is answered from the ledger.
        #[tokio::test(flavor = "current_thread")]
        async fn answers_a_repeated_job_attempt_key_from_the_ledger_and_writes_nothing() {
            let (_d, db) = make_db(&[], "");
            let p = provider(&[("Dorrit sweeps the steps", LEDGER)]);
            let first_scope = AdmissionKeyScope::new("job-1");
            let first = admit_row(
                &db,
                &p,
                cand("Dorrit sweeps the steps", "AUTO"),
                AdmitOptions {
                    key_scope: Some(&first_scope),
                    ..opts()
                },
            )
            .await
            .unwrap();
            assert_eq!(kind(&first), "insert");
            let retry_scope = AdmissionKeyScope::new("job-1");
            let retried = admit_row(
                &db,
                &p,
                cand("Dorrit sweeps the steps", "AUTO"),
                AdmitOptions {
                    key_scope: Some(&retry_scope),
                    ..opts()
                },
            )
            .await
            .unwrap();
            assert_eq!(retried.action, first.action);
            assert_eq!(retried.memory, first.memory);
            assert_eq!(
                retried.trace.notes.last().map(String::as_str),
                Some("already admitted by an earlier attempt of this job")
            );
            let id = first.memory.as_ref().unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(read(&db, &id).unwrap()["reinforcementCount"], 1);
            // A genuinely new observation (another job) still counts.
            let other = AdmissionKeyScope::new("job-2");
            let next = admit_row(
                &db,
                &p,
                cand("Dorrit sweeps the steps", "AUTO"),
                AdmitOptions {
                    key_scope: Some(&other),
                    ..opts()
                },
            )
            .await
            .unwrap();
            assert_eq!(kind(&next), "absorb");
            assert_eq!(read(&db, &id).unwrap()["reinforcementCount"], 2);
        }

        /// Review #3: a target retired mid-write is decided again.
        #[tokio::test(flavor = "current_thread")]
        async fn decides_again_when_the_target_is_retired_mid_write() {
            let mut seed = Seed::hot(SEED, "Hale keeps bees.", LEDGER.to_vec());
            seed.tier = "cold";
            let trigger = format!(
                "CREATE TRIGGER vanish AFTER UPDATE OF tier ON memories WHEN NEW.id = '{SEED}' \
             AND NEW.tier = 'hot' BEGIN DELETE FROM memories WHERE id = NEW.id; END;"
            );
            let (_d, db) = make_db(&[seed], &trigger);
            let p = provider(&[("Hale keeps bees on the cliff", LEDGER)]);
            let o = admit_row(
                &db,
                &p,
                cand("Hale keeps bees on the cliff", "AUTO"),
                opts(),
            )
            .await
            .unwrap();
            assert_eq!(kind(&o), "insert");
            assert!(o.memory.is_some());
            assert_eq!(count_rows(&db), 1);
            assert!(o
                .trace
                .notes
                .contains(&"target retired before the write; deciding again".to_string()));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn two_near_identical_candidates_make_one_row_with_count_2_bug_209() {
            let (_d, db) = make_db(&[], "");
            let p = provider(&[
                ("Bellamy whittles birds", LEDGER),
                ("Bellamy whittles little birds", LEDGER),
            ]);
            let a = admit_row(&db, &p, cand("Bellamy whittles birds", "AUTO"), opts())
                .await
                .unwrap();
            let b = admit_row(
                &db,
                &p,
                cand("Bellamy whittles little birds", "AUTO"),
                opts(),
            )
            .await
            .unwrap();
            assert_eq!((kind(&a), kind(&b)), ("insert", "absorb"));
            assert_eq!(count_rows(&db), 1);
            let id = a.memory.as_ref().unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(read(&db, &id).unwrap()["reinforcementCount"], 2);
        }

        fn at_half(provider_name: &str) -> CannedEmbeddingProvider {
            CannedEmbeddingProvider::new().with_result(
                text("Plum jam fills the jars", "Plum jam fills the jars"),
                EmbeddingResult {
                    embedding: vec![0.5, 0.866_025_4, 0.0],
                    model: "m".into(),
                    dimensions: 3,
                    provider: provider_name.into(),
                },
            )
        }

        #[tokio::test(flavor = "current_thread")]
        async fn decides_on_the_tfidf_scale_when_embedded_by_the_built_in_profile() {
            let (_d, db) = make_db(&[Seed::hot(SEED, "Plum jam.", LEDGER.to_vec())], "");
            let o = admit_row(
                &db,
                &at_half("BUILTIN"),
                cand("Plum jam fills the jars", "AUTO"),
                opts(),
            )
            .await
            .unwrap();
            assert_eq!(kind(&o), "reinforce");
            assert_eq!(o.trace.thresholds.reinforce, 0.4);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn the_same_cosine_inserts_on_the_neural_scale() {
            let (_d, db) = make_db(&[Seed::hot(SEED, "Plum jam.", LEDGER.to_vec())], "");
            let o = admit_row(
                &db,
                &at_half("OPENAI"),
                cand("Plum jam fills the jars", "AUTO"),
                opts(),
            )
            .await
            .unwrap();
            assert_eq!(kind(&o), "insert");
            assert_eq!(o.trace.thresholds.reinforce, 0.85);
        }
    }
}
