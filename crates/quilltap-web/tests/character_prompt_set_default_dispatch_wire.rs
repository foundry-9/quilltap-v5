//! P4.D201 Tier-2 item 9, landed at the `baa85e19b` round's unification: the
//! `characterPromptSetDefault` verb — and the bug-154 lockstep it now carries —
//! AT THE DISPATCH WIRE.
//!
//! v5 has **no** REST `PUT /api/v1/characters/{id}` and no
//! `/prompts/{promptId}` route (measured at planning — `lib.rs` registers `get`
//! + `post` on `/api/v1/characters/{id}` and nothing under `prompts`); the
//! SPA's star has always posted this verb over `POST /api/dispatch`, which is
//! therefore the ONLY transport the star's write crosses. The handler-level
//! families (`characters_arrays_tier2_equivalence`'s `defaultColumnTrail`,
//! `characters_mutations_equivalence`'s seven `default-prompt` arms) prove the
//! repository and the `characterUpdate` chokepoint are v4-faithful; what they
//! cannot see is the trip through serde and the envelope on the way back —
//! the same blind spot P4.96's `image_profile_generate_dispatch_wire.rs` was
//! written for. The P4.D202 e2e beat (`character-system-prompts-flow.spec.ts`)
//! walks the same lockstep through the SPA on a minted character; this file
//! pins it on the committed fixture with no browser in the loop.
//!
//! What it proves, in order, over the committed `characters-{main,mount}.db`
//! pair (Aria's baked prompts are `Backup` and `Explorer`, `Explorer` the
//! standing default; Fenn is ARCHIVED):
//!
//!   1. `characterPromptSetDefault` moves the `isDefault` flag AND the
//!      character's `defaultSystemPromptId` column together (v4
//!      `systemPromptsPatch`) — before P4.D201 the column never moved, which
//!      is bug 154's server half exactly (the e2e beat's first flipped run
//!      read `undefined` here).
//!   2. `characterUpdate { defaultSystemPromptId: null }` — the CLEAR — reaches
//!      the chokepoint through the raw body map (§S.1: the verb gains no null
//!      arm; the clear rides the character update) and empties the column.
//!   3. `characterUpdate` with a well-formed id the character does not have
//!      answers v4's 400 `System prompt not found on this character`.
//!   4. `characterUpdate` with a string that is NOT a uuid answers 400 with
//!      NOTHING written — v4's `z.uuid()` refuses at the parse; v5 refuses at
//!      the same position (the `baa85e19b` round's §3 review fix — before it,
//!      the generic patch landed first and the `name` beside the bad id
//!      persisted).
//!   5. `characterPromptSetDefault` with a bogus prompt id keeps its own 404
//!      (`not_found("Prompt")`), and writes nothing.
//!
//! Run:
//!   cargo test -p quilltap-web --test character_prompt_set_default_dispatch_wire

mod common;

use serde_json::{json, Value};

/// The committed pair's Aria (`characters_mutations_equivalence`'s `ARIA`).
const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const BOGUS_PROMPT: &str = "9f9f9f9f-0000-4000-8000-00000000dead";

/// The dispatch transport answers `{"type":"error","data":{kind,message}}` and
/// merges v4's flat `{error}` alongside it only where a refusal carries
/// details — so the sentence is read from either home.
fn sentence(v: &Value) -> &str {
    v.get("error")
        .and_then(Value::as_str)
        .or_else(|| v.pointer("/data/message").and_then(Value::as_str))
        .unwrap_or_default()
}

struct Wire {
    client: reqwest::Client,
    url: String,
}

impl Wire {
    async fn post(&self, b: Value) -> (u16, Value) {
        let r = self.client.post(&self.url).json(&b).send().await.unwrap();
        let status = r.status().as_u16();
        let v: Value = r.json().await.unwrap();
        (status, v)
    }

    /// `characterPromptList` → `[(name, id, isDefault)]`.
    async fn prompts(&self, character_id: &str) -> Vec<(String, String, bool)> {
        let (status, v) = self
            .post(json!({ "type": "characterPromptList", "characterId": character_id }))
            .await;
        assert_eq!(status, 200, "{v}");
        v["data"]["prompts"]
            .as_array()
            .unwrap_or_else(|| panic!("no prompts array on the wire: {v}"))
            .iter()
            .map(|p| {
                (
                    p["name"].as_str().unwrap_or_default().to_string(),
                    p["id"].as_str().unwrap_or_default().to_string(),
                    p["isDefault"].as_bool().unwrap_or(false),
                )
            })
            .collect()
    }

    /// `characterGet` → `(name, defaultSystemPromptId)`; the column is `None`
    /// when the key is absent OR null (v5's overlay re-read omits a null
    /// column; both readers treat the two alike).
    async fn character(&self, character_id: &str) -> (String, Option<String>) {
        let (status, v) = self
            .post(json!({ "type": "characterGet", "characterId": character_id }))
            .await;
        assert_eq!(status, 200, "{v}");
        let c = &v["data"]["character"];
        (
            c["name"].as_str().unwrap_or_default().to_string(),
            c.get("defaultSystemPromptId")
                .and_then(Value::as_str)
                .map(str::to_string),
        )
    }

    async fn update(&self, character_id: &str, body: Value) -> (u16, Value) {
        self.post(json!({
            "type": "characterUpdate",
            "characterId": character_id,
            "character": body,
        }))
        .await
    }
}

fn flagged(prompts: &[(String, String, bool)]) -> Vec<&str> {
    prompts
        .iter()
        .filter(|(_, _, d)| *d)
        .map(|(n, _, _)| n.as_str())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_star_verb_moves_both_faces_of_the_default_over_the_wire() {
    let base = common::materialize_characters_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let wire = Wire {
        client: reqwest::Client::new(),
        url: format!("http://{addr}/api/dispatch"),
    };

    // ---- 0. The fixture as committed: `Explorer` carries the flag, and the
    // column does NOT yet name `Backup` — the one precondition that keeps step
    // 1's move from being vacuous. (Measured on the first run: the P4.D201
    // widen added COLUMNS to the pair and populated nothing, so Aria's column
    // is absent here, not `Explorer`'s id — exactly the pre-lockstep shape a
    // real instance carries, which is what makes this fixture a good venue.)
    let before = wire.prompts(ARIA).await;
    let backup = before
        .iter()
        .find(|(n, _, _)| n == "Backup")
        .map(|(_, id, _)| id.clone())
        .expect("Aria's baked `Backup` prompt");
    let explorer = before
        .iter()
        .find(|(n, _, _)| n == "Explorer")
        .map(|(_, id, _)| id.clone())
        .expect("Aria's baked `Explorer` prompt");
    assert_eq!(
        flagged(&before),
        vec!["Explorer"],
        "the fixture's standing default"
    );
    let (aria_name, column0) = wire.character(ARIA).await;
    assert_ne!(
        column0.as_deref(),
        Some(backup.as_str()),
        "the column must not ALREADY name Backup, or step 1 proves nothing"
    );

    // ---- 1. The star: BOTH faces move together (bug 154's server half).
    let (status, v) = wire
        .post(json!({
            "type": "characterPromptSetDefault",
            "characterId": ARIA,
            "promptId": backup,
        }))
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        flagged(&wire.prompts(ARIA).await),
        vec!["Backup"],
        "the flag moved"
    );
    let (_, column1) = wire.character(ARIA).await;
    assert_eq!(
        column1.as_deref(),
        Some(backup.as_str()),
        "the column moved WITH the flag — before P4.D201 it never moved at all"
    );

    // ---- 2. The CLEAR rides the character update's raw body (§S.1): the
    // column empties. (The read overlay re-promotes `prompts[0]` in memory when
    // the folder declares no default — the trail family records that on both
    // sides — so the flags are not the discriminator here; the column is.)
    let (status, v) = wire
        .update(ARIA, json!({ "defaultSystemPromptId": Value::Null }))
        .await;
    assert_eq!(status, 200, "{v}");
    let (_, column2) = wire.character(ARIA).await;
    assert_eq!(column2, None, "the clear emptied the column");

    // ---- 3. A well-formed id the character does not have: v4's 400 sentence.
    let (status, v) = wire
        .update(ARIA, json!({ "defaultSystemPromptId": BOGUS_PROMPT }))
        .await;
    assert_eq!(status, 400, "{v}");
    assert_eq!(sentence(&v), "System prompt not found on this character");

    // ---- 4. A string that is NOT a uuid: refused at v4's parse position with
    // NOTHING written — the `name` riding beside it must not persist.
    let (status, v) = wire
        .update(
            ARIA,
            json!({ "name": "Aria Over The Wire", "defaultSystemPromptId": "not-a-uuid" }),
        )
        .await;
    assert_eq!(status, 400, "{v}");
    assert_eq!(sentence(&v), "System prompt not found on this character");
    let (name4, column4) = wire.character(ARIA).await;
    assert_eq!(
        name4, aria_name,
        "the generic patch beside a bad id must NOT land"
    );
    assert_eq!(column4, None, "…and the column is untouched");

    // ---- 5. The verb's own miss keeps its 404 and writes nothing.
    let (status, v) = wire
        .post(json!({
            "type": "characterPromptSetDefault",
            "characterId": ARIA,
            "promptId": BOGUS_PROMPT,
        }))
        .await;
    assert_eq!(status, 404, "{v}");
    assert_eq!(sentence(&v), "Prompt not found");
    let (_, column5) = wire.character(ARIA).await;
    assert_eq!(column5, None, "a refused set-default writes nothing");

    // ---- 6. And back: starring Explorer again restores the committed shape,
    // which proves the clear left the prompts themselves intact.
    let (status, v) = wire
        .post(json!({
            "type": "characterPromptSetDefault",
            "characterId": ARIA,
            "promptId": explorer,
        }))
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(flagged(&wire.prompts(ARIA).await), vec!["Explorer"]);
    let (_, column6) = wire.character(ARIA).await;
    assert_eq!(column6.as_deref(), Some(explorer.as_str()));
}
