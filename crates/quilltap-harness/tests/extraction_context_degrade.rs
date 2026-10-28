//! P4.D296 L-17 — the ONE `[Context Summary] Fold memory passes failed:` ERROR
//! is UNREACHABLE outside the strict scope, and v5's context build degrades as
//! v4's does (work order P4.D296 §L-17, §R.5: pinned NEGATIVE with a v5 home).
//!
//! **Measured through v4's real code at the `83334b9a5` pin (2026-10-15):** the
//! fold hook (`lib/chat/context-summary.ts:559-574`) hands
//! `buildExtractionContext` the chat and the profile, and its two `runFoldPass`
//! calls catch their own failures, so only the build's reads could reach the
//! catch. A scratch jest case planted a THROWING read (a VIRTUAL
//! `json('not json')` column, so every `SELECT *` fails `malformed JSON`) on
//! each table the build reads and called the real build as the hook does:
//! `chat_settings`, `connection_profiles`, `characters` and `chats` each threw on
//! a probe and the build NEVER threw (`chatSettings.findByUserId` falls back to
//! `null`, `connections.findByUserId` to `[]`, `hydrate` leaves a character
//! out, `refreshMemoryTimeZone` and the settings getters catch). The first v5
//! cut `?`-propagated the chat-settings and profile-list reads (and the chat /
//! profile by-id reads, which v4 also answers `null`), so a failed read failed
//! every unit of work's extraction where v4 carries on — fixed in
//! `services/extraction/context.rs`, pinned here.
//!
//! This test makes the same four reads fail on v5 (each table renamed away, so
//! every read errors) over a COPY of the turn family's fixture, then builds the
//! context as the fold hook does (chat + profile handed in) and as a job does
//! (by id): both must succeed with the cast left empty, and the degrade lines —
//! not the fold ERROR — must be what is logged.
//!
//! Run (the turn family's fixture pair; `memory_processor_tier3_equivalence`'s
//! header builds it):
//!   QT_FIXTURE_PROCESSOR_MAIN=/tmp/qt-memory-processor-main.db \
//!   QT_FIXTURE_PROCESSOR_MOUNT=/tmp/qt-memory-processor-mount.db \
//!     cargo test -p quilltap-harness --test extraction_context_degrade

use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::Db;
use quilltap_core::services::extraction::context::{
    build_extraction_context, BuildExtractionContextOptions,
};
use serde_json::Value;

#[path = "admission_support/mod.rs"]
mod admission_support;

fn spec() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memory-processor-tier3.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("read spec")).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn the_context_build_degrades_on_failed_reads() {
    let env = |k: &str| std::env::var(k).ok();
    let (Some(main), Some(mount)) = (
        env("QT_FIXTURE_PROCESSOR_MAIN"),
        env("QT_FIXTURE_PROCESSOR_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_FIXTURE_PROCESSOR_MAIN / _MOUNT (see header).");
        return;
    };
    let spec = spec();
    let dir = tempfile::Builder::new()
        .prefix("qt-extraction-degrade-rust-")
        .tempdir()
        .unwrap();
    let paths = admission_support::copy_pair(&main, &mount, dir.path());
    let db =
        Db::open(paths, spec["testPepperBase64"].as_str().unwrap()).expect("open fixture copy");
    db.write(|w| {
        let conn = w.main().connection();
        quilltap_core::test_support::ensure_p4d171_columns(conn);
        quilltap_core::test_support::ensure_p4d182_columns(conn);
        quilltap_core::test_support::ensure_p4d225_columns(conn);
        Ok(())
    })
    .await
    .unwrap();

    let user_id = spec["userId"].as_str().unwrap().to_string();
    let chat_id = spec["chats"][0]["id"].as_str().unwrap().to_string();
    let profile_id = spec["connectionProfiles"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    // The fold hook's inputs, read BEFORE the plant (v4 hands them in).
    let (cid, pid) = (chat_id.clone(), profile_id.clone());
    let (chat, profile) = db
        .read_main(move |c| {
            Ok((
                quilltap_core::db::chats_read::find_by_id(c, &cid)?.expect("chat"),
                quilltap_core::db::connection_profiles::find_by_id(c, &pid)?.expect("profile"),
            ))
        })
        .unwrap();

    // The plant: every read of the four tables now fails.
    db.write(|w| {
        w.main().connection().execute_batch(
            r#"ALTER TABLE "chat_settings" RENAME TO "qt_gone_chat_settings";
               ALTER TABLE "connection_profiles" RENAME TO "qt_gone_connection_profiles";
               ALTER TABLE "characters" RENAME TO "qt_gone_characters";"#,
        )?;
        Ok(())
    })
    .await
    .unwrap();

    use tracing_subscriber::layer::SubscriberExt;
    let logs = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(admission_support::Capture {
        lines: logs.clone(),
        owned: &[],
        prefixes: &[
            "Error finding",
            "[Memory] Character unavailable",
            "[Context Summary] Fold memory passes failed",
        ],
    });
    let _guard = tracing::subscriber::set_default(subscriber);

    // As the fold hook builds it: chat + profile handed in.
    let hook = build_extraction_context(
        &db,
        &chat_id,
        &user_id,
        BuildExtractionContextOptions {
            chat: Some(chat),
            connection_profile: Some(profile),
            messages: Some(Vec::new()),
            chat_context_summary: Some(Some("A summary.".into())),
            host_zone_fallback: "UTC".into(),
            ..Default::default()
        },
    );
    let hook = hook.unwrap_or_else(|e| panic!("the hook's build failed: {}", e.message()));
    assert!(
        hook.cast.is_empty(),
        "an unreadable character is left out of the cast (v4 `hydrate`)"
    );
    let lines = logs.lock().unwrap().clone();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Error finding entity by filter")
                && l.contains("collection=chat_settings")),
        "the chat-settings read degrades with v4's line; got:\n{}",
        lines.join("\n")
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Error finding entities by filter")
                && l.contains("connection_profiles")),
        "the profile list degrades with v4's line; got:\n{}",
        lines.join("\n")
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("[Memory] Character unavailable to extraction")),
        "an unreadable character degrades with v4's DEBUG; got:\n{}",
        lines.join("\n")
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("Fold memory passes failed")),
        "the fold ERROR is unreachable here (L-17)"
    );

    // As a job builds it, by id: the chat read fails too → v4's missing-chat
    // error (its `findById` answers `null`), never a database error.
    db.write(|w| {
        w.main()
            .connection()
            .execute_batch(r#"ALTER TABLE "chats" RENAME TO "qt_gone_chats";"#)?;
        Ok(())
    })
    .await
    .unwrap();
    let by_id = build_extraction_context(
        &db,
        &chat_id,
        &user_id,
        BuildExtractionContextOptions {
            connection_profile_id: Some(profile_id),
            host_zone_fallback: "UTC".into(),
            ..Default::default()
        },
    );
    match by_id {
        Ok(_) => panic!("a chat that cannot be read is v4's missing chat"),
        Err(e) => assert_eq!(e.message(), format!("Chat not found: {chat_id}")),
    }
    eprintln!("OK: the context build degraded on every failed read (L-17).");
}
