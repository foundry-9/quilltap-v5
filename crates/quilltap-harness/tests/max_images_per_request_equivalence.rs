//! P4.D362 — tier-1 exact: the per-profile image cap's accessors against v4's
//! REAL `resolveMaxImagesPerRequest` / `parseMaxImagesPerRequest`
//! (`lib/llm/connection-profile-utils.ts`) and the two constants of
//! `lib/schemas/profile.types.ts`.
//!
//! Generate the oracle (Node 24, from the v4 checkout so `@/` resolves):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/max-images-per-request.ts \
//!     > /tmp/oracle-max-images-per-request.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_MAX_IMAGES=/tmp/oracle-max-images-per-request.ndjson \
//!     cargo test -p quilltap-harness --test max_images_per_request_equivalence -- --nocapture
use quilltap_core::services::connection_profiles::{
    parse_max_images_per_request, resolve_max_images_per_request, DEFAULT_MAX_IMAGES_PER_REQUEST,
    MAX_IMAGES_PER_REQUEST_CEILING,
};
use serde_json::{json, Value};

#[test]
fn max_images_accessors_match_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_MAX_IMAGES") else {
        eprintln!("SKIP: set QT_ORACLE_MAX_IMAGES (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(rows.len() > 20, "corpus too small: {}", rows.len());
    let mut checked = 0;
    for row in &rows {
        let name = row["name"].as_str().unwrap();
        if name == "__constants" {
            assert_eq!(
                row["DEFAULT_MAX_IMAGES_PER_REQUEST"],
                json!(DEFAULT_MAX_IMAGES_PER_REQUEST)
            );
            assert_eq!(
                row["MAX_IMAGES_PER_REQUEST_CEILING"],
                json!(MAX_IMAGES_PER_REQUEST_CEILING)
            );
            continue;
        }
        let absent = row["absent"].as_bool().unwrap();
        let profile = if absent {
            json!({})
        } else {
            json!({ "maxImagesPerRequest": row["input"].clone() })
        };
        let resolved = resolve_max_images_per_request(&profile);
        assert_eq!(json!(resolved), row["resolved"], "{name}: resolve");
        // `undefined` and `null` both parse to null in v4.
        let parsed =
            parse_max_images_per_request(if absent { &Value::Null } else { &row["input"] });
        assert_eq!(
            parsed.map(Value::from).unwrap_or(Value::Null),
            row["parsed"],
            "{name}: parse"
        );
        checked += 1;
    }
    eprintln!("max images per request: {checked} rows matched");
}
