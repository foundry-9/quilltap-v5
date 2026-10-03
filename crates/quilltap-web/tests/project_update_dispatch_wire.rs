//! P4.D246 Tier-2 item 14: the project roster wire — the Shared contract
//! P4.D246 ↔ P4.D247 held AT THE DISPATCH TRANSPORT.
//!
//! v5 has **no** REST `PUT /api/v1/projects/{id}` (measured at planning:
//! `projectUpdate`, `projectCharacterAdd` and `projectCharacterRemove` are
//! dispatch-only verbs — `api/types.rs:1330-1358`, `api/engine.rs:3255-3287`),
//! so `POST /api/dispatch` is the ONLY transport the SPA's Characters card
//! crosses. `projects_routes_equivalence` proves the handler against v4's
//! real route over the same committed pair; what it cannot see is the trip
//! through serde and the envelope on the way back — the blind spot P4.96's
//! `image_profile_generate_dispatch_wire.rs` and P4.D201's
//! `character_prompt_set_default_dispatch_wire.rs` were written for.
//!
//! What it proves, in order, over the committed `groups-projects-{main,mount}.db`
//! pair (Iota's roster is Aria + Cleo; Kappa's is empty; Bram is unrostered):
//!
//!   1. `projectUpdate` on Iota answers the ENRICHED project (v4 `9753d0eb2`'s
//!      `enrichProject`): `data.project.characterRoster[0]` is an OBJECT with
//!      the contract's keys (`id`, `name`, `defaultImage`, `tags`, `chatCount`,
//!      and `defaultImageId` only when the character has one), and
//!      `data.project._count` is present with `chats`/`files`/`characters`.
//!      Before P4.D246 the roster came back as bare id strings and `_count`
//!      was absent — the shape the SPA's card could not render.
//!   2. The PUT body IS the `projectGet` body apart from the minted
//!      `updatedAt` (the "same shape as GET" half of contract item 2).
//!   3. `projectCharacterAdd` (Kappa ← Bram) answers `{success: true}` and
//!      carries NO project (the SPA refetches `projectGet`, as v4's hook does);
//!      the refetch shows Bram as an enriched entry.
//!   4. `projectCharacterRemove` (Kappa ← Bram again) answers `{success: true}`
//!      and the refetch shows the roster empty.
//!   5. `projectUpdate` on an unknown id answers 404 `Project not found`.
//!   6. `projectUpdate` with `allowAnyCharacter: "yes"` answers 400
//!      `Validation error` with nothing written (the name beside it stays).
//!
//! Run:
//!   cargo test -p quilltap-web --test project_update_dispatch_wire

mod common;

use quilltap_core::db::Writer;
use serde_json::{json, Value};

/// The committed pair's ids (`projects_routes_equivalence`'s constants).
const IOTA: &str = "a3000000-0000-4000-8000-000000000001";
const KAPPA: &str = "a3000000-0000-4000-8000-000000000003";
const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const BRAM: &str = "a1000000-0000-4000-8000-000000000002";
const CLEO: &str = "a1000000-0000-4000-8000-000000000003";
const NOBODY: &str = "a3000000-0000-4000-8000-0000000000ff";

/// Materialize an instance dir from the committed groups-projects pair (the
/// P4.D185 idiom — a family that materializes its own instance rather than
/// adding a `materialize_*` twin to `common`).
fn materialize() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        common::fixtures_dir().join("groups-projects-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        common::fixtures_dir().join("groups-projects-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
        // The fixture builder minted its own user; the engine's reads filter on
        // `SINGLE_USER_ID`.
        common::rewrite_fixture_user_ids(w.connection());
    }
    base
}

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

    async fn get(&self, project_id: &str) -> Value {
        let (status, v) = self
            .post(json!({ "type": "projectGet", "projectId": project_id }))
            .await;
        assert_eq!(status, 200, "{v}");
        v["data"]["project"].clone()
    }

    async fn update(&self, project_id: &str, project: Value) -> (u16, Value) {
        self.post(json!({
            "type": "projectUpdate",
            "projectId": project_id,
            "project": project,
        }))
        .await
    }
}

/// The contract's roster entry (P4.D246 §H item 1 / P4.D247's
/// `ProjectRosterCharacter`): `id`, `name`, `defaultImage` (object or null),
/// `tags` (array), `chatCount` (number); `defaultImageId` a string when present.
fn assert_roster_entry(entry: &Value, context: &str) {
    assert!(
        entry.is_object(),
        "{context}: a roster entry is an object, not an id: {entry}"
    );
    assert!(entry["id"].is_string(), "{context}: id — {entry}");
    assert!(entry["name"].is_string(), "{context}: name — {entry}");
    assert!(
        entry["defaultImage"].is_object() || entry["defaultImage"].is_null(),
        "{context}: defaultImage is an object or null — {entry}"
    );
    assert!(entry["tags"].is_array(), "{context}: tags — {entry}");
    assert!(
        entry["chatCount"].is_number(),
        "{context}: chatCount — {entry}"
    );
    if let Some(v) = entry.get("defaultImageId") {
        assert!(
            v.is_string(),
            "{context}: defaultImageId is a string when present — {entry}"
        );
    }
    let unknown: Vec<&String> = entry
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| {
            !matches!(
                k.as_str(),
                "id" | "name" | "defaultImageId" | "defaultImage" | "tags" | "chatCount"
            )
        })
        .collect();
    assert!(
        unknown.is_empty(),
        "{context}: unexpected roster-entry keys {unknown:?}"
    );
}

fn strip_updated_at(project: &Value) -> Value {
    let mut p = project.clone();
    if let Some(o) = p.as_object_mut() {
        o.remove("updatedAt");
    }
    p
}

#[tokio::test(flavor = "multi_thread")]
async fn the_project_roster_verbs_carry_the_contract_over_the_wire() {
    let base = materialize();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let wire = Wire {
        client: reqwest::Client::new(),
        url: format!("http://{addr}/api/dispatch"),
    };

    // ---- 0. The fixture as committed: Iota's roster is Aria + Cleo, closed.
    let iota0 = wire.get(IOTA).await;
    let ids = |p: &Value| -> Vec<String> {
        p["characterRoster"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(ids(&iota0), vec![ARIA.to_string(), CLEO.to_string()]);
    assert_eq!(iota0["allowAnyCharacter"], json!(false));

    // ---- 1. The PUT answers the ENRICHED project (v4 9753d0eb2).
    let (status, v) = wire
        .update(IOTA, json!({ "allowAnyCharacter": true }))
        .await;
    assert_eq!(status, 200, "{v}");
    let put = v["data"]["project"].clone();
    assert_eq!(
        put["allowAnyCharacter"],
        json!(true),
        "the one field the SPA's toast reads"
    );
    let roster = put["characterRoster"]
        .as_array()
        .unwrap_or_else(|| panic!("characterRoster is an array on the wire: {put}"));
    assert_eq!(
        roster.len(),
        2,
        "both of Iota's rostered characters still exist"
    );
    for (i, entry) in roster.iter().enumerate() {
        assert_roster_entry(entry, &format!("projectUpdate roster[{i}]"));
    }
    assert_eq!(
        roster[0]["id"],
        json!(ARIA),
        "roster order is the stored order"
    );
    assert_eq!(roster[0]["name"], json!("Aria"));
    let count = &put["_count"];
    assert!(count["chats"].is_number(), "_count.chats — {put}");
    assert!(count["files"].is_number(), "_count.files — {put}");
    assert_eq!(
        count["characters"],
        json!(2),
        "_count.characters is the stored roster length"
    );
    // `_count` is the LAST key (v4 `{ ...project, characterRoster, _count }`).
    assert_eq!(
        put.as_object().unwrap().keys().last().map(String::as_str),
        Some("_count"),
        "_count is appended last"
    );

    // ---- 2. The PUT body IS the GET body (apart from the minted updatedAt).
    let get = wire.get(IOTA).await;
    assert_eq!(
        strip_updated_at(&put),
        strip_updated_at(&get),
        "the PUT's body must be the GET's body (one enrichProject)"
    );

    // ---- 3. projectCharacterAdd carries no project; the refetch shows Bram.
    let kappa0 = wire.get(KAPPA).await;
    assert_eq!(kappa0["characterRoster"], json!([]), "Kappa starts empty");
    assert_eq!(kappa0["_count"]["characters"], json!(0));
    let (status, v) = wire
        .post(json!({ "type": "projectCharacterAdd", "projectId": KAPPA, "characterId": BRAM }))
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        v["data"],
        json!({ "success": true }),
        "the add answers only {{success}}: {v}"
    );
    let kappa1 = wire.get(KAPPA).await;
    assert_eq!(ids(&kappa1), vec![BRAM.to_string()]);
    assert_roster_entry(&kappa1["characterRoster"][0], "projectGet after add");
    assert_eq!(kappa1["_count"]["characters"], json!(1));

    // ---- 4. projectCharacterRemove; the refetch shows the roster empty.
    let (status, v) = wire
        .post(json!({ "type": "projectCharacterRemove", "projectId": KAPPA, "characterId": BRAM }))
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["data"], json!({ "success": true }), "{v}");
    let kappa2 = wire.get(KAPPA).await;
    assert_eq!(kappa2["characterRoster"], json!([]));
    assert_eq!(kappa2["_count"]["characters"], json!(0));

    // ---- 5. An unknown project is v4's 404.
    let (status, v) = wire.update(NOBODY, json!({ "name": "Nobody" })).await;
    assert_eq!(status, 404, "{v}");
    assert_eq!(sentence(&v), "Project not found");

    // ---- 6. A bad field is v4's 400 with nothing written.
    let (status, v) = wire
        .update(
            IOTA,
            json!({ "name": "Must Not Land", "allowAnyCharacter": "yes" }),
        )
        .await;
    assert_eq!(status, 400, "{v}");
    assert_eq!(sentence(&v), "Validation error");
    let iota2 = wire.get(IOTA).await;
    assert_eq!(
        iota2["name"], iota0["name"],
        "the refused patch wrote nothing"
    );
    assert_eq!(
        iota2["allowAnyCharacter"],
        json!(true),
        "step 1's flip is still in place"
    );
}
