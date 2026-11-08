//! The extraction prompts — v4 `lib/memory/extraction/prompts/` (pin
//! `83334b9a5`). The big bodies live in the generated [`prompt_text`]
//! submodule (extracted mechanically from the four v4 files by
//! `harness/oracle/scripts/extract-memory-task-prompts.py`, never
//! hand-transcribed).

pub mod episode;
pub mod other;
pub(crate) mod prompt_text;
pub mod self_turn;
pub mod shared;

/// P4.D373 — v4 `lib/memory/extraction/__tests__/repairs-prompt.test.ts`
/// (`ba6c8fe0f`, the probation's mechanism 7) over the GENERATED bodies, plus
/// the block order the regen carries (`AGREEMENTS` → `REPAIRS` → `ESTABLISHED
/// FACTS` in SELF and OTHER; the episode's `ESTABLISHED FACTS` paragraph right
/// before `Return a JSON array only.`). Like v4's test, this proves the
/// instruction is sent, not that a model obeys it.
#[cfg(test)]
mod repairs_prompt_tests {
    use super::prompt_text::*;

    const REPAIRS_HEAD: &str = "REPAIRS AND RESOLUTIONS — a repair is a hinge\n";
    const FACTS_HEAD: &str = "ESTABLISHED FACTS\nLines labelled or marked as established fact are settled truth in this story. Carry them forward as facts of the world, not as anyone's claim, and do not let a later line's doubt undo them.";

    fn self_body() -> String {
        format!("{SELF_BODY_BEFORE_CAP}{SELF_BODY_AFTER_CAP}")
    }
    fn other_body() -> String {
        format!("{OTHER_BODY_BEFORE_CAP}{OTHER_BODY_AFTER_CAP}")
    }
    fn episode_body() -> String {
        format!("{FOLD_EPISODE_PROMPT_BEFORE_CAP}{FOLD_EPISODE_PROMPT_AFTER_CAP}")
    }
    fn squash(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }
    fn repairs_block(body: &str) -> &str {
        let start = body.find(REPAIRS_HEAD).expect("the REPAIRS block");
        let end = body[start..]
            .find("\n\n")
            .expect("the block ends at a blank line");
        &body[start..start + end]
    }

    #[test]
    fn the_shared_block_makes_a_repair_a_hinge_and_refuses_manufactured_forgiveness() {
        let block = squash(repairs_block(&self_body()));
        assert!(block.contains("a repair is a hinge"));
        assert!(block.contains("at least as important as the hurt it resolves"));
        assert!(block.contains("An apology alone is not forgiveness"));
        assert!(block.contains("not provisional"));
        // ONE shared block: SELF and OTHER carry the same bytes.
        assert_eq!(repairs_block(&self_body()), repairs_block(&other_body()));
    }

    #[test]
    fn self_carries_the_block_and_a_repair_anchor_but_not_the_reliability_carve_out() {
        let body = self_body();
        assert!(body.contains(REPAIRS_HEAD));
        assert!(squash(&body)
            .contains("or made a repair: forgave, accepted an apology, or made amends"));
        assert!(!body.contains("Kept promises"));
    }

    #[test]
    fn other_carries_the_block_a_repair_anchor_and_the_reliability_carve_out() {
        let body = squash(&other_body());
        assert!(other_body().contains(REPAIRS_HEAD));
        assert!(body.contains(
            "A repair — forgiveness given or an apology accepted — is scored at least as high"
        ));
        assert!(body.contains("Kept promises, a time the subject turned out to be right"));
    }

    #[test]
    fn the_episode_pass_records_a_repair_that_ended_the_episode() {
        assert!(episode_body().contains("ended in a repair"));
        assert!(!episode_body().contains(REPAIRS_HEAD));
    }

    #[test]
    fn the_block_order_is_agreements_repairs_established_facts() {
        for (what, body) in [("self", self_body()), ("other", other_body())] {
            let a = body
                .find("AGREEMENTS, PROPOSALS, AND CONDITIONS")
                .expect(what);
            let r = body.find(REPAIRS_HEAD).expect(what);
            let f = body.find(FACTS_HEAD).expect(what);
            assert!(a < r && r < f, "{what}: {a} < {r} < {f}");
            assert_eq!(body.matches(FACTS_HEAD).count(), 1, "{what}");
            // Each block its own paragraph.
            assert!(body.contains(&format!("\n\n{REPAIRS_HEAD}")), "{what}");
            assert!(body.contains(&format!("\n\n{FACTS_HEAD}\n\n")), "{what}");
        }
        let ep = episode_body();
        assert!(
            ep.contains(&format!("\n\n{FACTS_HEAD}\n\nReturn a JSON array only.")),
            "the episode's ESTABLISHED FACTS paragraph sits right before the JSON instruction"
        );
    }
}
