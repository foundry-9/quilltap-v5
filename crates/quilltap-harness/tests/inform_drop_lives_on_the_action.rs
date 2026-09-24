//! **Where the departing seat's informs are dropped** (P4.D205, v4 `e7d77bb60`)
//! — the unification §3 finding-5 pin.
//!
//! v4 drops a removed seat's still-pending informs in
//! `app/api/v1/chats/[id]/actions/participants.ts::handleRemoveParticipantAction`
//! (`:566-582`) — the `?action=remove-participant` ROUTE — and **not** in the
//! shared low-level `helpers.ts::handleRemoveParticipant`, which the chat-`PUT`
//! bag (`processChatUpdates`) also reaches. So the two entrances deliberately
//! differ on this one effect, and putting the drop in the repository (where the
//! port first landed it) made the bag entrance drop them too.
//!
//! This family pins the difference in BOTH directions over the committed
//! `chat-cast-{main,mount}.db` pair — the same fixture, the same chat and the
//! same seat `chat_cast_routes_equivalence` removes in its `remove_ok` /
//! `bag_remove_participant` pair:
//!
//!   - `chat_cast::chat_remove_participant` → the pending row is GONE;
//!   - `salon::chat_update(..., remove_participant_id)` → the pending row
//!     SURVIVES.
//!
//! A consumed row is seeded alongside the pending one in the action case, so the
//! test also shows the drop is `deletePendingForParticipant` and not a blanket
//! delete — and a second seat's pending row is seeded in both cases, so a
//! delete that ignored `participantId` would redden the action case too.
//!
//! No oracle env var: v4's placement is read off its source, and what is
//! measured here is v5's own two entrances. Run standalone:
//!   cargo test -p quilltap-harness --test inform_drop_lives_on_the_action

use std::path::PathBuf;

use quilltap_core::api::{chat_cast, salon};
use quilltap_core::db::chat_informs::{ChatInformCreate, ChatInformsRepository};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    ids: std::collections::BTreeMap<String, String>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/chat-cast.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-inform-drop-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-cast-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-cast-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

fn row(id: &str, chat_id: &str, participant_id: &str, consumed: Option<&str>) -> ChatInformCreate {
    ChatInformCreate {
        id: id.to_string(),
        chat_id: chat_id.to_string(),
        batch_id: "bbbbbbbb-0000-4000-8000-000000000001".to_string(),
        participant_id: participant_id.to_string(),
        content_markdown: "The clock in the hall has stopped.".to_string(),
        record_message_id: None,
        created_at: "2026-05-01T00:00:00.000Z".to_string(),
        updated_at: "2026-05-01T00:00:00.000Z".to_string(),
        consumed_at: consumed.map(str::to_string),
        consumed_by_message_id: consumed
            .map(|_| "cccccccc-0000-4000-8000-000000000001".to_string()),
    }
}

/// Seed the three rows both cases use, creating the table first (the committed
/// pair predates `chat_informs`, exactly as a real pre-4.10 instance does — the
/// boot chain's `ensure_chat_informs_table` is what closes that on a live open).
async fn seed(db: &Db, chat_id: &str, target: &str, bystander: &str) {
    let chat = chat_id.to_string();
    let target = target.to_string();
    let bystander = bystander.to_string();
    db.write(move |w| {
        let conn = w.main().connection();
        // The committed pair predates `chat_informs` (a TABLE, not a
        // column — P4.111 widened columns only), so the boot chain's
        // `ensure_chat_informs_table` still applies here. The P4.D171/
        // P4.D182 column heals that used to run beside it are dead:
        // P4.111 widened the pair to carry them natively (`pragma_
        // table_info` proof: P4.117 lane record).
        quilltap_core::db::chat_informs::ensure_chat_informs_table(conn)?;
        let repo = ChatInformsRepository::new(conn);
        repo.create(&row(
            "11110000-0000-4000-8000-00000000aaa1",
            &chat,
            &target,
            None,
        ))?;
        repo.create(&row(
            "11110000-0000-4000-8000-00000000aaa2",
            &chat,
            &target,
            Some("2026-05-02T00:00:00.000Z"),
        ))?;
        repo.create(&row(
            "11110000-0000-4000-8000-00000000aaa3",
            &chat,
            &bystander,
            None,
        ))?;
        Ok(())
    })
    .await
    .expect("seed chat_informs");
}

fn ids_of(db: &Db, chat_id: &str) -> Vec<String> {
    let chat = chat_id.to_string();
    let mut ids: Vec<String> = db
        .read_main(move |c| ChatInformsRepository::new(c).find_by_chat_id(&chat))
        .expect("read back")
        .into_iter()
        .map(|r| r.id)
        .collect();
    ids.sort();
    ids
}

const PENDING_TARGET: &str = "11110000-0000-4000-8000-00000000aaa1";
const CONSUMED_TARGET: &str = "11110000-0000-4000-8000-00000000aaa2";
const PENDING_BYSTANDER: &str = "11110000-0000-4000-8000-00000000aaa3";

fn spec() -> Spec {
    serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read chat-cast.json"))
        .expect("parse chat-cast.json")
}

#[test]
fn the_action_twin_drops_the_departing_seats_pending_informs() {
    let spec = spec();
    let chat = spec.ids["chatMain"].clone();
    let target = spec.ids["pCleo"].clone();
    let bystander = spec.ids["pBram"].clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let db = fresh_db(&spec, "action");
        seed(&db, &chat, &target, &bystander).await;
        assert_eq!(
            ids_of(&db, &chat),
            vec![PENDING_TARGET, CONSUMED_TARGET, PENDING_BYSTANDER],
            "the seed did not land"
        );

        let resp = chat_cast::chat_remove_participant(&db, &chat, &target).await;
        assert!(
            !matches!(resp, quilltap_core::api::Response::Error(_)),
            "the removal itself must succeed: {resp:?}"
        );

        assert_eq!(
            ids_of(&db, &chat),
            vec![CONSUMED_TARGET, PENDING_BYSTANDER],
            "v4's `?action=remove-participant` drops the departing seat's PENDING \
             rows only (participants.ts:566-582) — its consumed row stays (a later \
             swipe of that turn must still re-apply it) and another seat's row is \
             none of its business"
        );
    });
}

#[test]
fn the_chat_put_bag_twin_leaves_them_alone() {
    let spec = spec();
    let chat = spec.ids["chatMain"].clone();
    let target = spec.ids["pCleo"].clone();
    let bystander = spec.ids["pBram"].clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let db = fresh_db(&spec, "bag");
        seed(&db, &chat, &target, &bystander).await;

        let resp = salon::chat_update(
            &db,
            &spec.user_id,
            &chat,
            &json!({}),
            None, // conciergeState
            None, // updateParticipant
            None, // addParticipant
            Some(target.as_str()),
        )
        .await;
        assert!(
            !matches!(resp, quilltap_core::api::Response::Error(_)),
            "the bag removal itself must succeed: {resp:?}"
        );

        assert_eq!(
            ids_of(&db, &chat),
            vec![PENDING_TARGET, CONSUMED_TARGET, PENDING_BYSTANDER],
            "v4's chat-PUT bag reaches `helpers.ts::handleRemoveParticipant`, which \
             carries NO inform drop — a v5 drop sited in the repository (or in \
             `chats_participants::remove_participant`) would redden here"
        );
    });
}
