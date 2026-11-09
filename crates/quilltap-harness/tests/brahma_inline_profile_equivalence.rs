//! P4.D374 tier-2 differential (v4 `c539c82e0`): the inline `@Brahma`
//! connection-profile chain —
//! `quilltap_core::services::brahma_console::inline_profile::resolve_inline_brahma_connection_profile`
//! against v4's REAL `resolveInlineBrahmaConnectionProfile`
//! (`lib/services/brahma-console/inline-profile.ts`).
//!
//! Both sides start from a COPY of the same fixture (built by v4's real
//! repositories — `build-brahma-inline-profile-fixture.ts`) and run the
//! corpus (`brahma-inline-profile.json`) IN ORDER on that one copy: write (or
//! delete) `instance_settings['brahmaConsole']` raw, then resolve for
//! `(userId, chatId)`. **Comparand, per case:** the resolved profile id (or
//! `null`), and every `BrahmaInlineProfile` line — level, message and each
//! context key in v4's order — rendered as the capture rig renders v5's
//! (`service=BrahmaInlineProfile module=service` first, a v4 `null` as
//! `null`). Every case is its own silence leg: a missing, extra or
//! re-ordered line is a red.
//!
//! The corpus reaches every arm reachable at the pin: the project profile
//! used (and preferred over a set global), a stale / foreign project id →
//! the WARN then the global or the default, a `null` / absent project key, an
//! UNPARSEABLE project store (v4's `applyOverlayOne` throws → the project
//! WARN carrying the overlay error's message), the global used, a stale /
//! foreign / `null` global, a stored global the schema refuses (the reader's
//! own WARN, on another logger, then the default), a missing chat (no
//! project, no WARN), and no default profile (`profileId: null`). The chat-
//! read and settings-read WARNs are unreachable at the pin (both reads are v4
//! fallbacks that never throw — `inline_profile`'s module doc).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!
//!     N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!     cd ~/source/quilltap-server
//!     QT_FIXTURE_BIP_MAIN=/tmp/qt-bip-main.db QT_FIXTURE_BIP_MOUNT=/tmp/qt-bip-mount.db \
//!       $N/npx tsx $V5W/harness/oracle/fixtures/build-brahma-inline-profile-fixture.ts
//!     QT_FIXTURE_BIP_MAIN=/tmp/qt-bip-main.db QT_FIXTURE_BIP_MOUNT=/tmp/qt-bip-mount.db \
//!       $N/npx tsx $V5W/harness/oracle/cases/brahma-inline-profile.ts > /tmp/oracle-brahma-inline-profile.ndjson
//!
//! Then:
//!
//!     QT_ORACLE_BRAHMA_INLINE_PROFILE=/tmp/oracle-brahma-inline-profile.ndjson \
//!     QT_FIXTURE_BIP_MAIN=/tmp/qt-bip-main.db QT_FIXTURE_BIP_MOUNT=/tmp/qt-bip-mount.db \
//!       cargo test -p quilltap-harness --test brahma_inline_profile_equivalence -- --nocapture

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::brahma_console::inline_profile::resolve_inline_brahma_connection_profile;
use serde_json::Value;

const TARGET: &str = "quilltap_core::services::brahma_console::inline_profile";

/// v4's `{ level, message, context }` rendered as v5's capture line.
fn render(line: &Value) -> String {
    let mut s = format!(
        "{} {TARGET} {} service=BrahmaInlineProfile module=service",
        line["level"].as_str().unwrap().to_uppercase(),
        line["message"].as_str().unwrap()
    );
    if let Some(ctx) = line["context"].as_object() {
        for (k, v) in ctx {
            let rendered = match v {
                Value::String(s) => s.clone(),
                Value::Null => "null".to_string(),
                other => other.to_string(),
            };
            s.push_str(&format!(" {k}={rendered}"));
        }
    }
    s
}

#[test]
fn brahma_inline_profile_matches_oracle() {
    let (Ok(oracle_path), Ok(fix_main), Ok(fix_mount)) = (
        std::env::var("QT_ORACLE_BRAHMA_INLINE_PROFILE"),
        std::env::var("QT_FIXTURE_BIP_MAIN"),
        std::env::var("QT_FIXTURE_BIP_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_BRAHMA_INLINE_PROFILE, QT_FIXTURE_BIP_MAIN and QT_FIXTURE_BIP_MOUNT (see header)."
        );
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/brahma-inline-profile.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");
    let cases = spec["cases"].as_array().expect("cases");
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect();
    assert_eq!(oracle.len(), cases.len(), "oracle rows != spec cases");

    let tmp = tempfile::tempdir().unwrap();
    let (main, mount) = (tmp.path().join("main.db"), tmp.path().join("mount.db"));
    std::fs::copy(&fix_main, &main).unwrap();
    std::fs::copy(&fix_mount, &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        spec["testPepperBase64"].as_str().unwrap(),
    )
    .expect("open the fixture copy");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut reds = Vec::new();
    let (mut warned, mut debugged) = (0usize, 0usize);
    for (c, want) in cases.iter().zip(&oracle) {
        let name = c["name"].as_str().unwrap();
        assert_eq!(want["name"].as_str(), Some(name), "case order");
        let setting = c["setting"].as_str().map(str::to_string);
        rt.block_on(db.write(move |w| {
            let conn = w.main().connection();
            conn.execute(
                "DELETE FROM \"instance_settings\" WHERE \"key\" = 'brahmaConsole'",
                [],
            )?;
            if let Some(v) = setting {
                conn.execute(
                    "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('brahmaConsole', ?1)",
                    [v],
                )?;
            }
            Ok(())
        }))
        .expect("plant the setting");
        let user = spec["users"][c["user"].as_str().unwrap()].as_str().unwrap();
        let chat_id = c["chatId"].as_str().unwrap();
        let (profile, lines) = quilltap_core::test_support::captured_with(|| {
            resolve_inline_brahma_connection_profile(&db, user, chat_id)
        });
        let got_id = profile
            .as_ref()
            .and_then(|p| p["id"].as_str())
            .map(str::to_string);
        if got_id.as_deref() != want["profileId"].as_str() {
            reds.push(format!(
                "{name}: PROFILE — v4 {} v5 {got_id:?}",
                want["profileId"]
            ));
        }
        let got_lines: Vec<String> = lines
            .into_iter()
            .filter(|l| l.contains("service=BrahmaInlineProfile"))
            .collect();
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .expect("lines")
            .iter()
            .map(render)
            .collect();
        warned += want_lines.iter().filter(|l| l.starts_with("WARN ")).count();
        debugged += want_lines
            .iter()
            .filter(|l| l.starts_with("DEBUG "))
            .count();
        if got_lines != want_lines {
            reds.push(format!(
                "{name}: LINES\n  v4 {want_lines:#?}\n  v5 {got_lines:#?}"
            ));
        }
    }
    assert!(
        reds.is_empty(),
        "{} case(s) differ from v4:\n{}",
        reds.len(),
        reds.join("\n")
    );
    // Floors: every DEBUG arm and the two reachable WARN messages are armed.
    assert!(
        warned >= 6 && debugged >= cases.len(),
        "corpus trimmed: {warned} WARN / {debugged} DEBUG"
    );
    eprintln!(
        "OK: brahma inline profile — {} cases matched v4 ({warned} WARN, {debugged} DEBUG lines).",
        cases.len()
    );
}
