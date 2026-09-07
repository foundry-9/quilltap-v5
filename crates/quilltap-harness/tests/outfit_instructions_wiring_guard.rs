//! P4.D119 — the dressing-instructions WIRING guard for the outfit-selection
//! entrances (v4 `b86bb1a5`).
//!
//! v4 resolves the cascade at ONE place: `applyOutfitSelections`' `llm_choose`
//! arm. v5 split that entrance in two — `resolve_llm_choose` (the chat-create
//! spine) and `run_llm_choose_via_db` (add-participant + merge, behind
//! [`OutfitLlmChooseRunner`]) — so the resolver is threaded into
//! `choose_llm_outfit` as a closure and both call sites must pass a real one.
//!
//! `outfit_llm_choose_tier3_equivalence` drives the SECOND entrance and its
//! `add_llm_choose_with_*_instructions` rows redden when that closure is
//! replaced with `|| None`. **No differential drives the create entrance's
//! consult at all** — no oracle case reaches `resolve_llm_choose` with a mocked
//! model — so its wiring would be free to rot in silence. This walks the source
//! instead: both production call sites must pass a resolver, and the shared
//! helper must reach the cascade.
//!
//! A `|| None` at either site fails this test. If a call site legitimately has
//! no instructions to resolve, say so here with its reason.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test outfit_instructions_wiring_guard

use std::path::PathBuf;

fn source() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src/services/outfit_selections.rs");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

#[test]
fn both_llm_choose_entrances_pass_a_dressing_instructions_resolver() {
    let src = source();

    // The create spine (`resolve_llm_choose`) — connections in hand.
    assert_eq!(
        src.matches("resolve_dressing_instructions_conn(").count(),
        3,
        "expected the conn-flavoured resolver's definition, the create \
         entrance's call, and the `_for` wrapper's call — no more, no fewer"
    );
    // The out-of-create entrance (`run_llm_choose_via_db`) — a `Db` in hand.
    assert_eq!(
        src.matches("resolve_dressing_instructions_for(").count(),
        2,
        "expected the Db-flavoured resolver's definition and \
         `run_llm_choose_via_db`'s call"
    );

    // Neither production site may hand `choose_llm_outfit` a null resolver. The
    // `|| None` spellings that DO appear are the unit tests' (a stalled
    // provider, a batch of consults, the timeout, and — P4.D164 — the two
    // consults of the green-room debug-line pin).
    assert_eq!(
        src.matches("|| None,").count(),
        5,
        "a production `choose_llm_outfit` call site has been given a null \
         resolver (only the five unit-test call sites may pass `|| None`)"
    );

    // …and the shared helper must actually reach the cascade.
    assert!(
        src.contains("resolve_wardrobe_instructions("),
        "the shared helper no longer calls the cascade"
    );
}

/// P4.D164 (v4 `2f4254b42`) — the SIBLING wiring: the green-room subprompts
/// resolver (`resolveSubpromptsForSeat`) is threaded into `choose_llm_outfit`
/// as a second closure, and BOTH production entrances must pass a real one.
/// `outfit_llm_choose_tier3_equivalence`'s `apply_llm_choose_*` rows drive the
/// create-spine entrance (`apply_outfit_selections` → `resolve_llm_choose`) and
/// redden when its closure is replaced; the `run_llm_choose_via_db` entrance
/// is reachable only through a seat that already carries ids, which neither
/// route can produce (v4's add body has no `selectedSubpromptIds`; its merge
/// builds the joining seat from explicit fields) — so its wiring is held HERE,
/// by source, the P4.D119 idiom.
#[test]
fn both_llm_choose_entrances_pass_a_subprompts_resolver() {
    let src = source();

    // The create spine — connections in hand: the definition, the create
    // entrance's call, and the `_for` wrapper's call.
    assert_eq!(
        src.matches("resolve_subprompts_for_seat_conn(").count(),
        5,
        "expected the conn-flavoured seat resolver's definition, the create \
         entrance's call, the `_for` wrapper's call, and the two calls of the \
         warn/silence unit pin — no more, no fewer"
    );
    // The out-of-create entrance — a `Db` in hand: the definition and
    // `run_llm_choose_via_db`'s call.
    assert_eq!(
        src.matches("resolve_subprompts_for_seat_for(").count(),
        2,
        "expected the Db-flavoured seat resolver's definition and \
         `run_llm_choose_via_db`'s call"
    );
    // Neither production site may hand `choose_llm_outfit` an empty resolver.
    // The `Vec::new,` spellings that DO appear are the unit tests' (the three
    // P4.D119 consults + the debug-line pin's SILENT second consult); clippy's
    // `redundant_closure` is why they are not `|| Vec::new()`.
    assert_eq!(
        src.matches("Vec::new,").count(),
        4,
        "a production `choose_llm_outfit` call site has been given an empty \
         subprompts resolver (only the four unit-test call sites may)"
    );
    assert_eq!(
        src.matches("|| Vec::new(),").count(),
        0,
        "an empty-closure resolver spelled the long way — a production site?"
    );
    // …and the seat resolver must actually reach the P4.D163 resolver.
    assert!(
        src.contains("resolve_selected_subprompts(main, mount, character_id, &selected)"),
        "the seat resolver no longer calls `resolve_selected_subprompts`"
    );
}
