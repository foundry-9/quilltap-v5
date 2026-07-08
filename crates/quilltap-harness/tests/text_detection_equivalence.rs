//! Tier-1 differential test: the file text/MIME sniffing leaf (v4
//! `lib/files/text-detection.ts`).
//!
//! Exact-equality check against the v4 oracle across all five exported
//! functions (`isTextContent`, `getMimeTypeFromExtension`, `isTextMimeType`,
//! `detectTextContent`, `getBestMimeType`). Each oracle row carries a `kind`
//! discriminator; the Rust side dispatches to the matching
//! `quilltap_core::files::text_detection` function and asserts the port returns
//! the byte-identical result v4's real code returned.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/text-detection.ts \
//!     > /tmp/oracle-text-detection.ndjson
//! Run:
//!   QT_ORACLE_TEXT_DETECTION=/tmp/oracle-text-detection.ndjson \
//!     cargo test -p quilltap-harness --test text_detection_equivalence

use quilltap_core::files::text_detection::{
    detect_text_content, get_best_mime_type, get_mime_type_from_extension, is_text_content,
    is_text_mime_type, TextDetectionResult,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct DetectOut {
    #[serde(rename = "isPlainText")]
    is_plain_text: bool,
    #[serde(rename = "detectedMimeType")]
    detected_mime_type: Option<String>,
    encoding: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "isTextContent")]
    IsTextContent {
        id: String,
        bytes: Vec<u8>,
        out: bool,
    },
    #[serde(rename = "getMime")]
    GetMime {
        id: String,
        filename: String,
        out: Option<String>,
    },
    #[serde(rename = "isTextMime")]
    IsTextMime { id: String, mime: String, out: bool },
    #[serde(rename = "detect")]
    Detect {
        id: String,
        bytes: Vec<u8>,
        filename: String,
        provided: String,
        out: DetectOut,
    },
    #[serde(rename = "getBest")]
    GetBest {
        id: String,
        #[serde(rename = "detectedMimeType")]
        detected_mime_type: Option<String>,
        provided: String,
        out: String,
    },
}

#[test]
fn text_detection_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TEXT_DETECTION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TEXT_DETECTION to the oracle NDJSON (see header).");
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        match row {
            Row::IsTextContent { id, bytes, out } => {
                let got = is_text_content(&bytes);
                assert_eq!(got, out, "[{id}] is_text_content = {got}, oracle = {out}");
            }
            Row::GetMime { id, filename, out } => {
                let got = get_mime_type_from_extension(&filename);
                assert_eq!(
                    got, out,
                    "[{id}] get_mime_type_from_extension({filename:?}) = {got:?}, oracle = {out:?}"
                );
            }
            Row::IsTextMime { id, mime, out } => {
                let got = is_text_mime_type(&mime);
                assert_eq!(
                    got, out,
                    "[{id}] is_text_mime_type({mime:?}) = {got}, oracle = {out}"
                );
            }
            Row::Detect {
                id,
                bytes,
                filename,
                provided,
                out,
            } => {
                let got = detect_text_content(&bytes, &filename, &provided);
                assert_eq!(
                    got.is_plain_text, out.is_plain_text,
                    "[{id}] detect.is_plain_text mismatch"
                );
                assert_eq!(
                    got.detected_mime_type, out.detected_mime_type,
                    "[{id}] detect.detected_mime_type mismatch"
                );
                assert_eq!(
                    got.encoding, out.encoding,
                    "[{id}] detect.encoding mismatch"
                );
            }
            Row::GetBest {
                id,
                detected_mime_type,
                provided,
                out,
            } => {
                let result = TextDetectionResult {
                    is_plain_text: false,
                    detected_mime_type,
                    encoding: "utf-8".to_string(),
                };
                let got = get_best_mime_type(&result, &provided);
                assert_eq!(
                    got, out,
                    "[{id}] get_best_mime_type = {got:?}, oracle = {out:?}"
                );
            }
        }
        n += 1;
    }
    assert!(n >= 30, "expected the full corpus, saw {n} rows");
    eprintln!("OK: text_detection matched oracle on {n} inputs.");
}
