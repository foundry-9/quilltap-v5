//! Tier-1 differential for the W4.6b Commonplace-Book PURE whisper builders —
//! `build_commonplace_persona_whisper` / `build_commonplace_llm_context`, diffed
//! byte-exact against v4's REAL exports
//! (`harness/oracle/cases/post-office-commonplace.ts`).
//!
//! The `post_commonplace_whisper` row + the `refresh_relevant_conversations_on_-
//! fold` end-to-end are exercised by the central tier-3 differentials the parent
//! owns (the combined orchestration + the regenerated `context_summary_service_-
//! tier3`).
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5/harness/oracle/cases/post-office-commonplace.ts \
//!       > /tmp/oracle-po-commonplace.ndjson
//! Run:
//!   QT_ORACLE_PO_COMMONPLACE=/tmp/oracle-po-commonplace.ndjson \
//!     cargo test -p quilltap-harness --test post_office_commonplace_equivalence

use serde_json::Value;

use quilltap_core::services::commonplace_notifications::{
    build_commonplace_llm_context, build_commonplace_persona_whisper, CommonplaceParts,
};

/// Rebuild the `CommonplaceParts` for one oracle case id (mirrors the corpus).
fn parts_case(id: &str) -> CommonplaceParts {
    let mut p = CommonplaceParts::default();
    match id {
        "all-empty" => {}
        "only-current-state" => p.current_state = Some("It is dusk in the parlour.".into()),
        "only-recap" => p.recap = Some("## Recap\nThey argued, then reconciled.".into()),
        "only-relevant" => p.relevant = Some("## Relevant Memories\n- A kept promise.".into()),
        "only-inter-char" => p.inter_char = Some("You recall Ada distrusts locksmiths.".into()),
        "only-knowledge" => p.knowledge = Some("### Ledger\nThe accounts balance.".into()),
        "only-relevant-conversations" => {
            p.relevant_conversations = Some(
                "### Relevant Past Conversations\n\n#### The Heist (`conv-1`)\n\n_note_".into(),
            )
        }
        "only-retrospective-recall" => p.retrospective_recall = Some(
            "### Relevant Past Conversations\n\n#### The Heist (2024-06-05) (`conv-1`)\n\n_note_"
                .into(),
        ),
        "all-six" => {
            p.current_state = Some("The clock reads noon.".into());
            p.recap = Some("A long recap.".into());
            p.relevant = Some("Relevant entries.".into());
            p.inter_char = Some("About the others.".into());
            p.knowledge = Some("From the shelves.".into());
            p.relevant_conversations = Some("Past dialogues.".into());
        }
        "all-seven" => {
            p.current_state = Some("The clock reads noon.".into());
            p.recap = Some("A long recap.".into());
            p.relevant = Some("Relevant entries.".into());
            p.inter_char = Some("About the others.".into());
            p.knowledge = Some("From the shelves.".into());
            p.relevant_conversations = Some("Past dialogues.".into());
            p.retrospective_recall = Some("Dated pages.".into());
        }
        "whitespace-only-retrospective-recall" => p.retrospective_recall = Some(" \n\t ".into()),
        "padded-retrospective-recall-trimmed" => {
            p.retrospective_recall = Some("   riffled pages   ".into())
        }
        "relevant-and-retrospective-recall" => {
            p.relevant = Some("The entries.".into());
            p.retrospective_recall = Some("The dated list.".into());
        }
        "whitespace-only-current-state" => p.current_state = Some("   \n\t ".into()),
        "whitespace-dropped-mixed" => {
            p.current_state = Some("  ".into());
            p.recap = Some("Kept recap.".into());
            p.relevant = Some("\n\n".into());
        }
        "recap-and-relevant" => {
            p.recap = Some("The gist.".into());
            p.relevant = Some("The entries.".into());
        }
        "padded-values-trimmed" => p.knowledge = Some("   padded knowledge body   ".into()),
        "current-state-and-relevant-conversations" => {
            p.current_state = Some("Now.".into());
            p.relevant_conversations = Some("Then.".into());
        }
        other => panic!("unknown commonplace case {other}"),
    }
    p
}

fn rust_value(kind: &str, id: &str) -> Value {
    let parts = parts_case(id);
    let s = match kind {
        "persona" => build_commonplace_persona_whisper(&parts),
        "llm" => build_commonplace_llm_context(&parts),
        other => panic!("unknown oracle kind {other}"),
    };
    Value::String(s)
}

#[test]
fn post_office_commonplace_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PO_COMMONPLACE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("QT_ORACLE_PO_COMMONPLACE unset; skipping");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let kind = row["kind"].as_str().unwrap();
        let id = row["id"].as_str().unwrap();
        let want = &row["value"];
        let got = rust_value(kind, id);
        assert_eq!(&got, want, "commonplace builder {kind}/{id} diverges");
        count += 1;
    }
    assert!(count > 0, "oracle had no rows");
    eprintln!("post-office-commonplace: {count} rows matched");
}
