//! P4.D225 — the image-failover chokepoint (v4
//! `generateImageWithConciergeFailover`, `lib/services/dangerous-content/
//! image-failover.ts`, NEW at `8bd080267`; the ledger at `49059fb14`) against
//! v4's REAL module on a real encrypted fixture.
//!
//! Both sides run the same per-profile script through the caller's `attempt`
//! (answer, or throw a typed `ModerationRejectionError` — v5's
//! `ImageGenError::moderation` — or a plain error with an optional `code`) and
//! everything else for real: the classifier, the default IMAGE understudy
//! resolver (canned API keys, the danger-routing arrangement), the Concierge's
//! refusal bubbles, the ledger and the auto-switch. The dialog arms supply a
//! CONNECTION-profile resolver of their own, as v4's legacy dialog does.
//!
//! Compared per case: the attempt calls (profile, key) in order; the outcome
//! (the answer, the answering profile + key, `rerouted`, the trail — or the
//! error message + `getConciergeTrail`), the trail rows as JSON in v4's writer
//! key order; and every `ConciergeImageFailover` / `ConciergeRefusal` /
//! `ConciergeRefusalLedger` / `[ConciergeNotification]` line in order (minted
//! message ids placeholdered). Then the `chats` + `chat_messages` dumps.
//!
//! The six exits in order: answered first time; not a refusal (untouched, no
//! trail); refused under a non-Auto-Route mode (`refusal-not-permitted`); no
//! understudy (`refusal-no-understudy`); the understudy answers
//! (`refusal-rerouted`, the ledger `rerouted: true`); the understudy fails —
//! refused / network / unclassified (no announcement, the PRIMARY's verdict on
//! the ledger). Plus: chatless calls (the `No chat to announce` line, no
//! ledger), connection-kind rows (no `profileKind`), a pre-flight-rerouted
//! primary (`via: concierge`, the configured profile excluded), the
//! provider-code and message-pattern evidences, a detail truncated at 200, and
//! a planted ledger the reroute tips into the auto-switch.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout; stage the case OUTSIDE
//! any `.claude/` path — v4's jest ignores `/\.claude/`). While v4 HEAD is past
//! the oracle baseline this needs a PINNED worktree; that is the sweep driver's
//! `--v4`, never a path in this header.
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; TMPO=/tmp/qt-oracle-run-image-failover
//!   cd ~/source/quilltap-server
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/image-failover.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/image-failover.json" "$TMPO/fixtures/"
//!   rm -f /tmp/qt-image-failover.db /tmp/oracle-image-failover.ndjson
//!   QT_REFUSAL_LEDGER_SPEC=image-failover.json QT_FIXTURE_OUT=/tmp/qt-image-failover.db \
//!     $N/npx tsx "$V5W/harness/oracle/fixtures/build-refusal-ledger-fixture.ts"
//!   QT_FIXTURE_IMAGE_FAILOVER=/tmp/qt-image-failover.db \
//!   QT_ORACLE_OUT=/tmp/oracle-image-failover.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/image-failover\.test\.ts$"
//!   grep -c '"typed":true' "$V5W/harness/oracle/fixtures/image-failover.json"
//! Run:
//!   QT_ORACLE_IMAGE_FAILOVER=/tmp/oracle-image-failover.ndjson \
//!   QT_FIXTURE_IMAGE_FAILOVER=/tmp/qt-image-failover.db \
//!     cargo test -p quilltap-harness --test image_failover_tier3_equivalence -- --nocapture

use std::collections::HashMap;
use std::sync::Mutex;

use quilltap_core::db::chat_settings::DangerousContentSettings;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::image::ImageGenError;
use quilltap_core::services::dangerous_content::image_failover::{
    generate_image_with_concierge_failover, FailoverProfile, ImageFailoverContext, ImagePurpose,
    ImageUnderstudySource, UnderstudySource,
};
use quilltap_core::services::dangerous_content::provider_routing::ApiKeyResolver;
use quilltap_core::services::dangerous_content::refusal::RefusalError;
use quilltap_core::services::route_trail::{RouteAttemptVia, RouteProfileKind};
use quilltap_core::test_support::captured_with;
use serde_json::{json, Value};

const SEED_TS: &str = "2020-01-01T00:00:00.000Z";

struct CannedApiKeys(HashMap<String, String>);

impl ApiKeyResolver for CannedApiKeys {
    fn resolve(&self, api_key_id: &str, _user_id: &str) -> Option<String> {
        self.0.get(api_key_id).cloned()
    }
}

/// The legacy dialog's own resolver: one named connection profile, or nobody.
struct ConnectionUnderstudy {
    found: Option<(FailoverProfile, String)>,
}

impl UnderstudySource for ConnectionUnderstudy {
    async fn resolve(&self, _exclude: &[String]) -> Option<(FailoverProfile, String)> {
        self.found.clone()
    }
}

/// Either resolver, so one call site serves every case.
enum Source<'a> {
    Image(ImageUnderstudySource<'a, CannedApiKeys>),
    Connection(ConnectionUnderstudy),
}

impl UnderstudySource for Source<'_> {
    async fn resolve(&self, exclude: &[String]) -> Option<(FailoverProfile, String)> {
        match self {
            Source::Image(s) => s.resolve(exclude).await,
            Source::Connection(s) => s.resolve(exclude).await,
        }
    }
}

fn make_error(t: &Value) -> ImageGenError {
    let message = t["message"].as_str().unwrap().to_string();
    if t["typed"].as_bool() == Some(true) {
        return ImageGenError::moderation(
            message,
            t["status"].as_u64().map(|s| s as u16),
            t["providerReason"].as_str().map(str::to_string),
        );
    }
    match t.get("code").and_then(Value::as_str) {
        Some(code) => ImageGenError {
            message: message.clone(),
            refusal: Some(Box::new(RefusalError {
                message,
                code: Some(code.to_string()),
                name: Some("Error".to_string()),
                ..Default::default()
            })),
        },
        None => ImageGenError::new(message),
    }
}

fn snake(k: &str) -> String {
    let mut out = String::new();
    for c in k.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn render_v4(log: &Value) -> String {
    let target = match log["service"].as_str() {
        Some("ConciergeImageFailover") => "quilltap::concierge_image_failover",
        Some("ConciergeRefusal") => "quilltap::concierge_refusal",
        Some("ConciergeRefusalLedger") => "quilltap::concierge_refusal_ledger",
        None => "quilltap::concierge_notification",
        Some(other) => panic!("unexpected service {other}"),
    };
    let level = log["level"].as_str().unwrap().to_uppercase();
    let mut line = format!("{level} {target} {}", log["message"].as_str().unwrap());
    for (k, v) in log["bag"].as_object().unwrap() {
        let rendered = match (k.as_str(), v) {
            ("messageId", _) => "<id>".to_string(),
            (_, Value::String(s)) => s.clone(),
            (_, other) => other.to_string(),
        };
        line.push_str(&format!(" {}={rendered}", snake(k)));
    }
    line
}

/// The rig renders `message_id=<uuid>`; the id is minted on both sides.
fn normalize_minted_id(line: &str) -> String {
    match line.find(" message_id=") {
        Some(at) => {
            let start = at + " message_id=".len();
            let end = line[start..]
                .find(' ')
                .map(|e| start + e)
                .unwrap_or(line.len());
            format!("{}<id>{}", &line[..start], &line[end..])
        }
        None => line.to_string(),
    }
}

fn rows_of(dump: &Value) -> Vec<Value> {
    let mut rows = dump["rows"].as_array().expect("dump rows").clone();
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().unwrap();
        for col in [
            "updatedAt",
            "lastMessageAt",
            "dangerClassifiedAt",
            "lastModerationRefusalAt",
            "createdAt",
        ] {
            if obj
                .get(col)
                .and_then(Value::as_str)
                .is_some_and(|s| s != SEED_TS)
            {
                obj.insert(col.into(), json!("<ts>"));
            }
        }
        if obj.contains_key("chatId") {
            obj.insert("id".into(), json!("<id>"));
        }
        for v in obj.values_mut() {
            if let Some(f) = v.as_f64() {
                if v.is_f64() && f.fract() == 0.0 {
                    *v = json!(f as i64);
                }
            }
        }
    }
    rows
}

#[test]
fn image_failover_matches_v4() {
    let (Ok(oracle_path), Ok(fixture)) = (
        std::env::var("QT_ORACLE_IMAGE_FAILOVER"),
        std::env::var("QT_FIXTURE_IMAGE_FAILOVER"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_IMAGE_FAILOVER + QT_FIXTURE_IMAGE_FAILOVER (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/image-failover.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();
    let cases = spec["cases"].as_array().unwrap();
    assert_eq!(
        oracle.len(),
        cases.len() + 2,
        "one row per case + two dumps"
    );

    let keys: HashMap<String, String> = spec["apiKeys"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();
    let api_keys = CannedApiKeys(keys.clone());

    let work =
        std::env::temp_dir().join(format!("qt-image-failover-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).expect("copy fixture");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let db = rt.block_on(async { Db::open_main(&work, &pepper).expect("open fixture copy") });
    let plants: Vec<(String, i64, String)> = spec["ledgerPlants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["chatId"].as_str().unwrap().into(),
                p["count"].as_i64().unwrap(),
                p["lastAt"].as_str().unwrap().into(),
            )
        })
        .collect();
    rt.block_on(db.write(move |w| {
        quilltap_core::test_support::ensure_p4d225_columns(w.main().connection());
        for (id, count, at) in &plants {
            w.main().connection().execute(
                "UPDATE chats SET \"moderationRefusalCount\" = ?1, \"lastModerationRefusalAt\" = ?2 WHERE id = ?3",
                rusqlite::params![count, at, id],
            )?;
        }
        Ok(())
    }))
    .expect("plant the ledgers");

    let find = |id: &str, kind: &str| -> Value {
        db.read_main(|c| {
            if kind == "connection" {
                quilltap_core::db::connection_profiles::find_by_id(c, id)
            } else {
                quilltap_core::db::image_profiles::find_by_id(c, id)
            }
        })
        .expect("read profile")
        .unwrap_or_else(|| panic!("profile {id} not seeded"))
    };

    let mut failed: Vec<String> = Vec::new();
    for (case, want) in cases.iter().zip(&oracle) {
        let name = case["name"].as_str().unwrap();
        assert_eq!(want["name"], case["name"], "oracle rows in spec order");
        let kind = case["profileKind"].as_str().unwrap_or("image");
        let primary_row = find(case["primary"].as_str().unwrap(), kind);
        let primary_key = keys[primary_row["apiKeyId"].as_str().unwrap()].clone();
        let settings: DangerousContentSettings = serde_json::from_value(json!({
            "mode": case["settings"]["mode"],
            "threshold": 0.7,
            "scanTextChat": true,
            "scanImagePrompts": true,
            "scanImageGeneration": false,
            "displayMode": "SHOW",
            "showWarningBadges": true,
            "uncensoredImageProfileId": case["settings"].get("uncensoredImageProfileId"),
        }))
        .expect("settings");
        let source = if case.get("customUnderstudy").is_some() {
            Source::Connection(ConnectionUnderstudy {
                found: case["customUnderstudy"].as_str().map(|id| {
                    let row = find(id, "connection");
                    let key = keys[row["apiKeyId"].as_str().unwrap()].clone();
                    (FailoverProfile::from_row(&row), key)
                }),
            })
        } else {
            Source::Image(ImageUnderstudySource {
                db: &db,
                api_keys: &api_keys,
                user_id: case["userId"].as_str().unwrap(),
                uncensored_image_profile_id: settings.uncensored_image_profile_id.as_deref(),
            })
        };
        let ctx = ImageFailoverContext {
            db: &db,
            chat_id: case["chatId"].as_str(),
            purpose: match case["purpose"].as_str().unwrap() {
                "tool" => ImagePurpose::Tool,
                "lantern" => ImagePurpose::Lantern,
                "avatar" => ImagePurpose::Avatar,
                "dialog" => ImagePurpose::Dialog,
                other => panic!("purpose {other}"),
            },
            settings: &settings,
            understudy: &source,
            profile_kind: if kind == "connection" {
                RouteProfileKind::Connection
            } else {
                RouteProfileKind::Image
            },
            primary_via: match case["primaryVia"].as_str() {
                Some("concierge") => RouteAttemptVia::Concierge,
                None => RouteAttemptVia::Primary,
                Some(other) => panic!("via {other}"),
            },
        };
        let script: Mutex<HashMap<String, Vec<Value>>> = Mutex::new(
            case["script"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_array().unwrap().clone()))
                .collect(),
        );
        let calls: Mutex<Vec<Value>> = Mutex::new(Vec::new());
        let attempt =
            async |profile: &FailoverProfile, key: &str| -> Result<String, ImageGenError> {
                calls
                    .lock()
                    .unwrap()
                    .push(json!({ "profileId": profile.id, "apiKey": key }));
                let step = {
                    let mut s = script.lock().unwrap();
                    let steps = s.get_mut(&profile.id).expect("scripted profile");
                    assert!(!steps.is_empty(), "{name}: no step left for {}", profile.id);
                    steps.remove(0)
                };
                match step.get("throws") {
                    Some(t) => Err(make_error(t)),
                    None => Ok(step["answers"].as_str().unwrap().to_string()),
                }
            };
        let (outcome, lines) = captured_with(|| {
            rt.block_on(generate_image_with_concierge_failover(
                (FailoverProfile::from_row(&primary_row), primary_key),
                attempt,
                &ctx,
            ))
        });
        let got_outcome = match outcome {
            Ok(r) => json!({
                "ok": true,
                "result": r.result,
                "profileId": r.profile.id,
                "apiKey": r.api_key,
                "rerouted": r.rerouted,
                "trail": serde_json::to_value(&r.trail).unwrap(),
            }),
            Err(e) => json!({
                "ok": false,
                "message": e.error.message,
                "trail": e.concierge_trail().map(|t| serde_json::to_value(t).unwrap()),
            }),
        };
        assert!(
            script.lock().unwrap().values().all(Vec::is_empty),
            "{name}: unused scripted steps"
        );
        let got_calls = Value::Array(calls.into_inner().unwrap());
        let got_lines: Vec<String> = lines
            .iter()
            .filter(|l| {
                matches!(
                    l.split(' ').nth(1),
                    Some(
                        "quilltap::concierge_image_failover"
                            | "quilltap::concierge_refusal"
                            | "quilltap::concierge_refusal_ledger"
                            | "quilltap::concierge_notification"
                    )
                )
            })
            .map(|l| normalize_minted_id(l))
            .collect();
        let want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4)
            .collect();
        let mut diffs = Vec::new();
        if got_calls != want["calls"] {
            diffs.push(format!(
                "calls\n      rust   {got_calls}\n      oracle {}",
                want["calls"]
            ));
        }
        // The trail's KEY ORDER is v4's writer's (`row()` spreads
        // `profileKind` before `trigger`/`evidence`/`detail`): compared as
        // bytes, since `Value` equality ignores order (`preserve_order` keeps
        // both sides' insertion order for the serialization).
        let trail_bytes = |v: &Value| serde_json::to_string(&v["trail"]).unwrap();
        if trail_bytes(&got_outcome) != trail_bytes(&want["outcome"]) {
            diffs.push(format!(
                "trail bytes\n      rust   {}\n      oracle {}",
                trail_bytes(&got_outcome),
                trail_bytes(&want["outcome"])
            ));
        }
        if got_outcome != want["outcome"] {
            diffs.push(format!(
                "outcome\n      rust   {got_outcome}\n      oracle {}",
                want["outcome"]
            ));
        }
        if got_lines != want_lines {
            diffs.push(format!(
                "logs\n      rust   {got_lines:#?}\n      oracle {want_lines:#?}"
            ));
        }
        if !diffs.is_empty() {
            failed.push(format!("{name}: {}", diffs.join("\n    ")));
        }
    }

    let got_chats = db
        .read_main(|c| dump_table_json_conn(c, "chats", "id"))
        .expect("dump chats");
    let got_msgs = db
        .read_main(|c| dump_table_json_conn(c, "chat_messages", "chatId"))
        .expect("dump chat_messages");
    drop(db);
    drop(rt);
    let _ = std::fs::remove_file(&work);

    assert!(
        failed.is_empty(),
        "{} of {} case(s) failed:\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );
    let tables = &oracle[cases.len()..];
    assert_eq!(
        rows_of(&got_chats),
        rows_of(&tables[0]),
        "chats rows diverge"
    );
    assert_eq!(
        rows_of(&got_msgs),
        rows_of(&tables[1]),
        "chat_messages rows diverge"
    );
    eprintln!(
        "OK: image failover matched v4 ({} cases, {} bubbles).",
        cases.len(),
        rows_of(&tables[1]).len()
    );
}
