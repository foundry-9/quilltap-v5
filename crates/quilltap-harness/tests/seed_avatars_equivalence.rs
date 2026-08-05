//! P4.4u4 unit 2 tier-2 differential: v4's avatar seeding
//! (`reseedAvatarsForCharacters` → the private `seedAvatars`) vs the Rust port
//! (`quilltap_core::services::quilltap_import::seed::seed_avatars`).
//!
//! Both sides import the committed `.qtap` (create Lorian + Riya) on empty
//! fixtures, then seed the avatars. BOTH `Lorian.webp` and `Riya.webp` exist at
//! v4 `a7b1398d` and are already WebP, so the vault write stores them AS-IS — the
//! blob sha256 is DETERMINISTIC and diffed EXACTLY (no codec seam). We diff, per
//! character: the `images/avatar.webp` link + blob (relativePath / fileName /
//! originalMimeType / storedMimeType / sha256 / size), that `defaultImageId`
//! points at that link, and the idempotency of a second reseed (defaultImageId
//! stable, still one link).
//!
//! Generate the oracle (see the .ts header) then run:
//!   QT_ORACLE_SEED_AVATARS=/tmp/oracle-seed-avatars.ndjson \
//!   QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
//!   QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
//!     cargo test -p quilltap-harness --test seed_avatars_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::SINGLE_USER_ID;
use quilltap_core::services::quilltap_import::seed::{seed_avatars, SeedReport};
use quilltap_core::services::quilltap_import::{execute_import, parse_export_file, ImportOptions};
use quilltap_host::HostImageCodec;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/qtap-import-tier2.json")
}
fn qtap_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/first-startup/imports/lorian-and-riya.qtap")
}

/// Lorian's `images/avatar.webp` link + blob, joined by fileId; the character's
/// mount is passed in.
fn lorian_link(mount: &Writer, mount_point_id: &str) -> Option<Value> {
    mount
        .connection()
        .query_row(
            "SELECT l.id, l.relativePath, l.fileName, l.originalMimeType, \
                    b.storedMimeType, b.sha256, length(b.data) \
             FROM doc_mount_file_links l JOIN doc_mount_blobs b ON b.fileId = l.fileId \
             WHERE l.mountPointId = ?1 AND l.relativePath = 'images/avatar.webp' ORDER BY l.id",
            [mount_point_id],
            |r| {
                Ok(json!({
                    "linkId": r.get::<_, String>(0)?,
                    "relativePath": r.get::<_, String>(1)?,
                    "fileName": r.get::<_, String>(2)?,
                    "originalMimeType": r.get::<_, Option<String>>(3)?,
                    "storedMimeType": r.get::<_, String>(4)?,
                    "sha256": r.get::<_, String>(5)?,
                    "sizeBytes": r.get::<_, i64>(6)?,
                }))
            },
        )
        .ok()
}

fn char_row(main: &Writer, mount: &Writer, name: &str) -> Value {
    let all = quilltap_core::db::characters_read::find_by_user_id(
        main.connection(),
        mount.connection(),
        SINGLE_USER_ID,
    )
    .expect("find chars");
    all.into_iter()
        .find(|c| {
            c.get("name")
                .and_then(Value::as_str)
                .map(|n| n.eq_ignore_ascii_case(name))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("character {name} missing"))
}

#[test]
fn seed_avatars_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SEED_AVATARS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SEED_AVATARS to the oracle NDJSON (see header).");
            return;
        }
    };
    let (main_fixture, mount_fixture) = match (
        std::env::var("QT_FIXTURE_QTAPIMPORT_MAIN"),
        std::env::var("QT_FIXTURE_QTAPIMPORT_MOUNT"),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            eprintln!("SKIP: set QT_FIXTURE_QTAPIMPORT_MAIN/MOUNT (header).");
            return;
        }
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .expect("read oracle")
            .lines()
            .find(|l| !l.trim().is_empty())
            .expect("one oracle line"),
    )
    .expect("parse oracle");

    let qtap = std::fs::read_to_string(qtap_path()).expect("read committed .qtap");
    let export = parse_export_file(&qtap).expect("parse .qtap");

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-seed-avatars-main-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-seed-avatars-mount-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).expect("copy main");
    std::fs::copy(&mount_fixture, &mount_work).expect("copy mount");

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64).expect("open main");
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64).expect("open mount");

    // Create the seed characters.
    execute_import(
        main.connection(),
        mount.connection(),
        SINGLE_USER_ID,
        &export,
        &ImportOptions::seed_defaults(),
        None,
    )
    .expect("import");

    // Seed the avatars (the reset-builtins / startup code path). BOTH characters
    // have an avatar at v4 a7b1398d.
    let mut report = SeedReport::default();
    seed_avatars(
        main.connection(),
        mount.connection(),
        &HostImageCodec,
        Some(&["Lorian", "Riya"]),
        &mut report,
    );
    assert_eq!(
        report.avatars_written, 2,
        "two avatars written (Lorian + Riya)"
    );
    assert!(
        report.warnings.is_empty(),
        "no warnings: {:?}",
        report.warnings
    );

    // Per-character: the deterministic link/blob fields diff exactly (WebP
    // passthrough), and defaultImageId points at the link.
    let check = |name: &str, oracle_link_key: &str, oracle_flag_key: &str| -> String {
        let c = char_row(&main, &mount, name);
        let cmount = c
            .get("characterDocumentMountPointId")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{name} mount"));
        let link = lorian_link(&mount, cmount).unwrap_or_else(|| panic!("{name} avatar link"));
        let want = &oracle[oracle_link_key];
        for key in [
            "relativePath",
            "fileName",
            "originalMimeType",
            "storedMimeType",
            "sha256",
            "sizeBytes",
        ] {
            assert_eq!(link[key], want[key], "{name} link.{key} diverged");
        }
        let default_id = c.get("defaultImageId").and_then(Value::as_str);
        assert_eq!(
            default_id,
            link["linkId"].as_str(),
            "{name} defaultImageId should equal the avatar link id"
        );
        assert_eq!(oracle[oracle_flag_key], Value::Bool(true));
        cmount.to_string()
    };
    let lorian_mount = check("Lorian", "lorianLink", "lorianDefaultImageIdIsLink");
    let riya_mount = check("Riya", "riyaLink", "riyaDefaultImageIdIsLink");

    // The two avatars are different files (distinct sha) — a guard against seeding
    // the same bytes for both.
    assert_ne!(
        oracle["lorianLink"]["sha256"], oracle["riyaLink"]["sha256"],
        "Lorian + Riya avatars must be distinct files"
    );

    let lorian_default = char_row(&main, &mount, "Lorian")
        .get("defaultImageId")
        .and_then(Value::as_str)
        .map(str::to_string);

    // Idempotency: a 2nd reseed no-ops (defaultImageIds stable, still one link each).
    let mut report2 = SeedReport::default();
    seed_avatars(
        main.connection(),
        mount.connection(),
        &HostImageCodec,
        Some(&["Lorian", "Riya"]),
        &mut report2,
    );
    assert_eq!(
        report2.avatars_written, 0,
        "idempotent reseed writes nothing"
    );
    assert_eq!(
        char_row(&main, &mount, "Lorian")
            .get("defaultImageId")
            .and_then(Value::as_str)
            .map(str::to_string),
        lorian_default,
        "Lorian defaultImageId stable across reseed"
    );
    let link_count = |mp: &str| -> i64 {
        mount
            .connection()
            .query_row(
                "SELECT count(*) FROM doc_mount_file_links WHERE mountPointId = ?1 AND relativePath = 'images/avatar.webp'",
                [mp],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(link_count(&lorian_mount), 1, "Lorian: one avatar link");
    assert_eq!(link_count(&riya_mount), 1, "Riya: one avatar link");
    assert_eq!(oracle["secondRunLinkCounts"]["lorian"], Value::from(1));
    assert_eq!(oracle["secondRunLinkCounts"]["riya"], Value::from(1));
    assert_eq!(oracle["secondRunDefaultImageIdStable"], Value::Bool(true));

    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    eprintln!("OK: seed_avatars matched oracle (2 deterministic WebP blobs + idempotency).");
}
