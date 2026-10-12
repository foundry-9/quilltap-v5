//! Tier-2 differential: `resolve_avatar_image_profile` + `find_default` against
//! v4's REAL `resolveAvatarImageProfile` over the real `ImageProfilesRepository`
//! (P4.D279 §8, v4 `lib/image-gen/profile-resolution.ts:150-210` at `4a38222e6`).
//!
//! Both sides start from the SAME seed fixture (built through v4's own
//! `ensureCollection` + `ImageProfilesRepository.create`, then one row's `name`
//! made a BLOB — the P4.139 corrupt-cell idiom, the one way to reach the
//! repository fallback lines since a missing table is healed by v4's
//! `getCollection`) and run the SAME resolution matrix from the committed spec
//! (`harness/oracle/fixtures/avatar-image-profile-resolution-tier2.json`):
//! override hit / override miss → chat / override miss → default / chat hit /
//! chat miss → default / default only / none → null / unknown ids / falsy ids /
//! a second user / the corrupt row at each tier. Per row the resolved profile
//! id (or null) must match, and so must the captured lines — the three
//! `[ProfileResolution]` DEBUG lines and the repository fallback lines — by
//! level, message and FIELDS (v4's camelCase keys), never by transport prefix.
//!
//! The one deliberate field mask: a corrupt row's `error` is v4's ZodError text
//! (`[ { "expected": "string", … } ]`) against v5's driver sentence (`Invalid
//! column type Blob at index: 2, name: name`) — both sides must carry a
//! non-empty `error`, and v5's is pinned to its literal here (the P4.139
//! `corrupt_key` precedent). Everything else compares byte-equal.
//!
//! RED-FIRST: this family cannot be red on main — `resolve_avatar_image_profile`
//! and `find_default` did not exist before this lane (the red-first measurement
//! is the compile failure, recorded in the lane's report).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-avatar-profile-fixture.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-avatar-image-profile-resolution-fixture.ts
//!   QT_FIXTURE_AVATAR_PROFILE=/tmp/qt-avatar-profile-fixture.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/avatar-image-profile-resolution.ts \
//!     > /tmp/oracle-avatar-profile.ndjson
//! Run:
//!   QT_ORACLE_AVATAR_PROFILE=/tmp/oracle-avatar-profile.ndjson \
//!   QT_FIXTURE_AVATAR_PROFILE_MAIN=/tmp/qt-avatar-profile-fixture.db \
//!     cargo test -p quilltap-harness --test avatar_image_profile_resolution_equivalence

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::services::image_profile_resolution::resolve_avatar_image_profile;
use quilltap_core::test_support::captured_with;
use serde::Deserialize;
use serde_json::Value;

/// The committed fixture spec — the single source driving both sides.
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    matrix: Vec<MatrixRow>,
}

#[derive(Deserialize)]
struct MatrixRow {
    name: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "override")]
    override_id: Option<String>,
    chat: Option<Value>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/avatar-image-profile-resolution-tier2.json")
}

/// Every message either side may log while resolving (the oracle's `CAPTURED`
/// set, minus the two v4 lines the oracle PROVED absent — `Error finding
/// default image profile`, `SQLite findOne error` — which v5 never emits; a
/// v5 line outside this set is itself a diff).
const CAPTURED: &[&str] = &[
    "[ProfileResolution] Avatar override profile unusable; falling back",
    "[ProfileResolution] Chat image profile unusable for avatars; falling back",
    "[ProfileResolution] No usable avatar image profile",
    "Error finding entity by ID",
    "Error finding entity by filter",
];

/// The repository fallback lines whose `error` field is masked to presence.
const DB_FALLBACK_LINES: &[&str] = &[
    "Error finding entity by ID",
    "Error finding entity by filter",
];

/// v5's `error` on the corrupt row — pinned, so the mask hides nothing else.
const V5_CORRUPT_ERROR: &str = "Invalid column type Blob at index: 2, name: name";

type Line = (String, String, BTreeMap<String, String>);

/// A captured v5 line → `(level, message, fields)`. The rig renders
/// `LEVEL target message k=v k=v…`; the message must be one of [`CAPTURED`].
fn parse_line(line: &str) -> Result<Line, String> {
    let (level, rest) = line
        .split_once(' ')
        .ok_or_else(|| format!("no level: {line}"))?;
    let (_target, rest) = rest
        .split_once(' ')
        .ok_or_else(|| format!("no target: {line}"))?;
    let message = CAPTURED
        .iter()
        .find(|m| rest.starts_with(**m))
        .ok_or_else(|| format!("a v5 line outside the capture set: {line}"))?;
    let tail = rest[message.len()..].trim_start();
    let mut fields = BTreeMap::new();
    // Split on ` <ident>=` boundaries; a value may carry spaces (an error).
    let mut keys: Vec<(usize, usize)> = Vec::new();
    let bytes = tail.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let at_start = i == 0 || bytes[i - 1] == b' ';
        if at_start {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i && j < bytes.len() && bytes[j] == b'=' {
                keys.push((i, j));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    for (n, (start, eq)) in keys.iter().enumerate() {
        let end = keys
            .get(n + 1)
            .map(|(next, _)| next - 1)
            .unwrap_or(tail.len());
        fields.insert(tail[*start..*eq].to_string(), tail[eq + 1..end].to_string());
    }
    Ok((level.to_lowercase(), message.to_string(), fields))
}

/// v4's captured line → the same shape (field values as the rig renders them:
/// strings bare, booleans `true`/`false`).
fn oracle_line(v: &Value) -> Line {
    let fields = v["fields"]
        .as_object()
        .expect("oracle line fields")
        .iter()
        .map(|(k, val)| {
            let rendered = match val {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), rendered)
        })
        .collect();
    (
        v["level"].as_str().expect("level").to_string(),
        v["message"].as_str().expect("message").to_string(),
        fields,
    )
}

/// Apply the one mask: on a repository fallback line both sides' `error` must
/// be present and non-empty; v5's must be the pinned driver sentence. Returns
/// the masked pair.
fn mask_error(v5: Line, v4: Line, name: &str, failed: &mut Vec<String>) -> (Line, Line) {
    let (mut v5, mut v4) = (v5, v4);
    if DB_FALLBACK_LINES.contains(&v5.1.as_str()) {
        match v5.2.get("error") {
            Some(e) if e == V5_CORRUPT_ERROR => {
                v5.2.insert("error".into(), "<present>".into());
            }
            other => {
                eprintln!("[{name}] v5 fallback line's error is not the pinned literal: {other:?}");
                failed.push(format!("{name}_v5_error_literal"));
            }
        }
    }
    if DB_FALLBACK_LINES.contains(&v4.1.as_str()) {
        match v4.2.get("error") {
            Some(e) if !e.is_empty() => {
                v4.2.insert("error".into(), "<present>".into());
            }
            other => {
                eprintln!("[{name}] v4 fallback line carries no error text: {other:?}");
                failed.push(format!("{name}_v4_error_present"));
            }
        }
    }
    (v5, v4)
}

#[test]
fn avatar_image_profile_resolution_matches_v4() {
    let oracle_path = match std::env::var("QT_ORACLE_AVATAR_PROFILE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_AVATAR_PROFILE to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_AVATAR_PROFILE_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_AVATAR_PROFILE_MAIN to the seed fixture .db (see test header)."
            );
            return;
        }
    };

    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let oracle: BTreeMap<String, Value> = oracle_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("parse oracle row");
            (v["case"].as_str().expect("case").to_string(), v)
        })
        .collect();
    assert_eq!(
        oracle.len(),
        spec.matrix.len(),
        "the oracle must carry one row per matrix entry"
    );

    // A fresh copy of the seed fixture so the shared file stays pristine. Opened
    // BEFORE any capture: the open's own lines are not a resolution's.
    let work_scratch = tempfile::Builder::new()
        .prefix("qt-avatar-profile-rust-")
        .tempdir()
        .expect("tempdir");
    let work = work_scratch.path().join("work.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    let conn = writer.connection();

    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;
    for row in &spec.matrix {
        let name = row.name.as_str();
        let expected = &oracle[name];

        let (got, lines) = captured_with(|| {
            resolve_avatar_image_profile(
                conn,
                &row.user_id,
                row.override_id.as_deref(),
                row.chat.as_ref(),
            )
        });
        ran += 1;

        // 1. The resolved profile id (or null).
        let got_id = got
            .as_ref()
            .and_then(|p| p.get("id"))
            .and_then(Value::as_str);
        let want_id = expected["result"].as_str();
        if got_id != want_id {
            eprintln!("[{name}] result MISMATCH: v5={got_id:?} v4={want_id:?}");
            failed.push(format!("{name}_result"));
        }
        // A resolved profile is the WHOLE row — it must carry the key the job reads.
        if let Some(p) = &got {
            if p.get("apiKeyId")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                eprintln!("[{name}] v5 handed back a keyless profile: {p}");
                failed.push(format!("{name}_keyless"));
            }
        }

        // 2. The lines — level, message, fields.
        let v4_lines: Vec<Line> = expected["lines"]
            .as_array()
            .expect("oracle lines")
            .iter()
            .map(oracle_line)
            .collect();
        let v5_lines: Vec<Line> = match lines.iter().map(|l| parse_line(l)).collect() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[{name}] {e}");
                failed.push(format!("{name}_v5_line_shape"));
                continue;
            }
        };
        if v5_lines.len() != v4_lines.len() {
            eprintln!(
                "[{name}] line COUNT mismatch: v5={} v4={}\n  v5: {v5_lines:?}\n  v4: {v4_lines:?}",
                v5_lines.len(),
                v4_lines.len()
            );
            failed.push(format!("{name}_line_count"));
            continue;
        }
        for (i, (v5, v4)) in v5_lines.into_iter().zip(v4_lines).enumerate() {
            let (v5, v4) = mask_error(v5, v4, name, &mut failed);
            if v5 != v4 {
                eprintln!("[{name}] line {i} MISMATCH:\n  v5: {v5:?}\n  v4: {v4:?}");
                failed.push(format!("{name}_line_{i}"));
            }
        }
    }

    eprintln!("avatar_image_profile_resolution_equivalence: {ran} matrix rows compared");
    assert!(ran >= 20, "the matrix must carry the whole corpus");
    assert!(
        failed.is_empty(),
        "{} v4/v5 divergences: {failed:?}",
        failed.len()
    );
}
