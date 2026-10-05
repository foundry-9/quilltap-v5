//! Tier-1 differential: v4 `lib/services/file-content-extractor.ts`
//! `extractFileContent`, ported as
//! `quilltap_core::generators::file_content::extract_file_content` (P4.D253,
//! v4 `a434c715b` / bug 177).
//!
//! The FIRST family over that function. The wizard and ai-import tier-3
//! families reach it only through v4's jest storage stub (every `downloadFile`
//! answers 'mock file content'), so until this family the PDF arm had never
//! been compared at all — and the module header's "a real v4 extracts PDF text
//! through pdf-parse" was false for a whole release line (v4 called pdf-parse
//! 2.x's class as the 1.x function and failed EVERY PDF; the oracle at the
//! baseline `52d6e7ecd` records exactly that, `pdfParse is not a function` on
//! all thirteen PDF rows of the 25-case corpus — re-measured at the
//! `07b8f0209` unification).
//!
//! Both sides read the same 25-case corpus
//! (`harness/oracle/cases/file-content-extractor-corpus.json`): the text /
//! markdown / code / JSON arms, the 50,000-unit truncation, the image
//! description and placeholder arms, the binary placeholder, the 10 MB ceiling,
//! the no-storage-key and failed-download refusals, and thirteen PDF rows. The
//! storage read answers the case's bytes on both sides (v4's mocked
//! `downloadFile`; a [`CaseBackend`] here), and the converter seam is SCRIPTED
//! identically from the case's `pdfParse` field: v4's mocked `PDFParse.getText`
//! resolves the text or throws; v5's [`ScriptedTextExtractor`] answers the text,
//! or `''` for a throw (v4's `convertPdfBufferToText` swallows every throw into
//! `''`). Compared per case, exactly:
//!
//! - `result` — v4's `ExtractedContent` through `JSON.stringify` against v5's
//!   serialized struct (absent keys stay absent on both sides);
//! - the extractor's log lines, in order — level, message, and every field
//!   (`size` / `chars` bare integers; a `child` line carries v4's `fileId`
//!   binding as v5's `file_id`; the child's other binding, `module:
//!   'file-content-extractor'`, is what v5's target names, so it is not
//!   rendered as a field). One field is compared by PRESENCE only: the
//!   failed download's `error` (v4's is the mock's raw rejection; v5's is
//!   `download_file`'s v4-wrapped message over the backend's).
//!
//! Two further pins, named:
//!
//! - **The recorded divergence** (`production_seam_diverges_only_where_pdf_parse
//!   _finds_text`): over the PRODUCTION seam (`default_text_extractor()`, which
//!   refuses — P4.6y), v5 equals v4 on every row EXCEPT the four where
//!   pdf-parse finds text (`pdf_parsed_text`, `pdf_parsed_padded`,
//!   `pdf_parsed_multibyte`, `pdf_parsed_wins_over_fallback`,
//!   `pdf_parsed_js_whitespace`, `pdf_parsed_truncated`); there v4 answers the
//!   parsed text and v5 runs the regex fallback over the same bytes — which
//!   finds nothing, except on `pdf_parsed_wins_over_fallback`, whose bytes the
//!   scrape CAN read (the row that pins the converter's precedence on the
//!   scripted side). Asserted both ways, never masked.
//! - **The converter's own WARNs** (`PDF buffer is empty`, `Failed to extract
//!   text from PDF buffer`, under `MountIndex:PdfConverter`) are v4's and not
//!   v5's — the seam does not reproduce the converter (P4.D253 Tier 3 item
//!   10). The oracle records them in `converterLines`; this family pins exactly
//!   which rows carry them, so a future extractor port knows its list.
//!
//! Generate the oracle output (Node 24, from the v4 checkout — the jest case's
//! own header carries the staged-mirror steps):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TMPO=/tmp/qt-file-content-extractor-oracle; rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp $V5W/harness/oracle/cases/file-content-extractor.test.ts "$TMPO/cases/"
//!   cp $V5W/harness/oracle/cases/file-content-extractor-corpus.json "$TMPO/cases/"
//!   rm -f /tmp/oracle-file-content-extractor.ndjson
//!   QT_ORACLE_OUT=/tmp/oracle-file-content-extractor.ndjson \
//!     PATH=$N:$PATH $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- 'cases/file-content-extractor\.test\.ts$'
//! Run:
//!   QT_ORACLE_FILE_CONTENT_EXTRACTOR=/tmp/oracle-file-content-extractor.ndjson \
//!     cargo test -p quilltap-harness --test file_content_extractor_equivalence -- --nocapture

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use quilltap_core::db::files::FileFull;
use quilltap_core::db::runtime::Db;
use quilltap_core::generators::file_content::{
    extract_file_content, ExtractedContent, ScriptedTextExtractorGuard,
};
use quilltap_core::services::file_storage::StorageBackend;
use quilltap_core::services::mount_index::converters::DocumentTextExtractor;
use quilltap_core::test_support::global_capture;
use serde::Deserialize;
use serde_json::Value;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const TARGET: &str = "quilltap::file_content_extractor";

/// The rows where pdf-parse FINDS text — the only rows the production seam
/// (which refuses) may differ from v4 on.
const PARSED_TEXT_ROWS: &[&str] = &[
    "pdf_parsed_text",
    "pdf_parsed_padded",
    "pdf_parsed_multibyte",
    "pdf_parsed_wins_over_fallback",
    "pdf_parsed_js_whitespace",
    "pdf_parsed_truncated",
];

/// The parsed-text rows whose bytes the regex fallback CAN read (the §3 review
/// of the `07b8f0209` unification): v4 answers pdf-parse's text — the
/// converter WINS, bug 177's whole point — while v5's refusing production seam
/// falls through to the scrape and answers it. Every other parsed-text row's
/// bytes are a junk header the fallback finds nothing in.
const FALLBACK_READABLE_PARSED_ROWS: &[(&str, &str)] =
    &[("pdf_parsed_wins_over_fallback", "Fallback would say this")];

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CaseFile {
    original_filename: String,
    mime_type: String,
    size: i64,
    storage_key: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    description: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repeat {
    unit_base64: String,
    count: usize,
}

#[derive(Deserialize)]
struct PdfScript {
    text: Option<String>,
    throw: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    file: CaseFile,
    bytes_base64: Option<String>,
    repeat: Option<Repeat>,
    #[serde(default)]
    missing: bool,
    pdf_parse: Option<PdfScript>,
}

fn b64(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .expect("corpus base64")
}

impl Case {
    fn bytes(&self) -> Vec<u8> {
        if let Some(r) = &self.repeat {
            return b64(&r.unit_base64).repeat(r.count);
        }
        b64(self.bytes_base64.as_deref().unwrap_or(""))
    }

    fn file_full(&self) -> FileFull {
        let f = &self.file;
        FileFull {
            id: format!("file-{}", self.name),
            user_id: "user-1".into(),
            original_filename: f.original_filename.clone(),
            mime_type: f.mime_type.clone(),
            sha256: "0".repeat(64),
            size: f.size,
            width: f.width,
            height: f.height,
            category: "DOCUMENT".into(),
            description: f.description.clone(),
            generation_key: None,
            linked_to: Vec::new(),
            project_id: None,
            folder_path: None,
            storage_key: f.storage_key.clone(),
            file_status: None,
            created_at: "2026-10-04T00:00:00.000Z".into(),
            updated_at: "2026-10-04T00:00:00.000Z".into(),
        }
    }

    /// The scripted converter answer: v4's `convertPdfBufferToText` returns
    /// `getText().text` (untrimmed), or `''` on a throw / an empty buffer.
    fn scripted_text(&self) -> String {
        match &self.pdf_parse {
            // `getText` threw: the converter's catch answers `''`.
            Some(PdfScript { throw: Some(_), .. }) => String::new(),
            // An empty buffer never reaches the parser (`PDF buffer is empty`).
            Some(PdfScript { text: Some(t), .. }) if !self.bytes().is_empty() => t.clone(),
            _ => String::new(),
        }
    }
}

/// The case's storage: the bytes v4's mocked `downloadFile` resolves, or the
/// rejection a `missing` case poses. Never a mount-blob key, so `download_file`
/// consults this backend and never the (empty) DB.
struct CaseBackend {
    bytes: Option<Vec<u8>>,
}

impl StorageBackend for CaseBackend {
    fn upload(&self, _: &str, _: &[u8], _: &str) -> Result<(), String> {
        Err("read-only case backend".into())
    }
    fn download(&self, key: &str) -> Result<Vec<u8>, String> {
        self.bytes
            .clone()
            .ok_or_else(|| format!("no object at {key}"))
    }
    fn delete(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn exists(&self, _: &str) -> Result<bool, String> {
        Ok(self.bytes.is_some())
    }
}

/// v5's twin of the oracle's mocked `pdf-parse`, behind the thread-scoped seam.
struct ScriptedTextExtractor(String);

impl DocumentTextExtractor for ScriptedTextExtractor {
    fn extract(&self, _bytes: &[u8], file_type: &str) -> String {
        assert_eq!(file_type, "pdf", "the PDF arm reads the seam as `pdf`");
        self.0.clone()
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_corpus() -> Corpus {
    let p = repo_root().join("harness/oracle/cases/file-content-extractor-corpus.json");
    serde_json::from_str(&std::fs::read_to_string(&p).expect("read corpus")).expect("parse corpus")
}

/// The oracle rows by case name, or `None` (with the SKIP notice) when the
/// env var is unset.
fn load_oracle() -> Option<Vec<Value>> {
    let Ok(path) = std::env::var("QT_ORACLE_FILE_CONTENT_EXTRACTOR") else {
        eprintln!("SKIP: QT_ORACLE_FILE_CONTENT_EXTRACTOR unset");
        return None;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert!(!rows.is_empty(), "empty oracle NDJSON at {path}");
    Some(rows)
}

fn oracle_row<'a>(rows: &'a [Value], name: &str) -> &'a Value {
    rows.iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("oracle has no row {name}"))
}

/// Run v5's extractor over one case, capturing the extractor's own lines.
fn run_v5(case: &Case, scripted: bool) -> (Value, Vec<String>) {
    let dir = tempfile::tempdir().expect("scratch dir");
    let db = Db::open_main(dir.path().join("quilltap.db"), PEPPER).expect("open scratch db");
    let backend = CaseBackend {
        bytes: (!case.missing).then(|| case.bytes()),
    };
    let file = case.file_full();
    let _guard = scripted.then(|| {
        ScriptedTextExtractorGuard::install(Arc::new(ScriptedTextExtractor(case.scripted_text())))
    });
    let (result, lines): (ExtractedContent, Vec<String>) =
        global_capture::capture(|| extract_file_content(&db, &backend, &file));
    let lines = lines
        .into_iter()
        .filter(|l| l.split(' ').nth(1) == Some(TARGET))
        .collect();
    (
        serde_json::to_value(&result).expect("serialize result"),
        lines,
    )
}

/// v4's camelCase field names → v5's tracing field names.
fn v5_field(k: &str) -> &str {
    match k {
        "storageKey" => "storage_key",
        other => other,
    }
}

fn render_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Render one oracle log record as the capture rig's line (`"<LEVEL> <target>
/// <message> k=v …"`), cutting at ` error=` (compared by presence — module doc).
fn render_v4_line(line: &Value) -> String {
    let level = line["level"].as_str().expect("level").to_uppercase();
    let mut out = format!(
        "{level} {TARGET} {}",
        line["message"].as_str().expect("message")
    );
    if line["via"] == "child" {
        out.push_str(&format!(
            " file_id={}",
            render_value(&line["bindings"]["fileId"])
        ));
    }
    if let Some(fields) = line["fields"].as_object() {
        for (k, v) in fields {
            if k == "error" {
                out.push_str(" error=");
                break;
            }
            out.push_str(&format!(" {}={}", v5_field(k), render_value(v)));
        }
    }
    out
}

/// A captured v5 line, cut the same way as [`render_v4_line`].
fn cut_v5_line(line: &str) -> String {
    match line.find(" error=") {
        Some(i) => line[..i + " error=".len()].to_string(),
        None => line.to_string(),
    }
}

fn compare_case(case: &Case, row: &Value, v5_result: &Value, v5_lines: &[String]) -> Vec<String> {
    let mut reds = Vec::new();
    if &row["result"] != v5_result {
        reds.push(format!(
            "{}: result\n    v4 {}\n    v5 {}",
            case.name,
            clip(&row["result"].to_string()),
            clip(&v5_result.to_string())
        ));
    }
    let v4_lines: Vec<String> = row["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .map(render_v4_line)
        .collect();
    let v5_lines: Vec<String> = v5_lines.iter().map(|l| cut_v5_line(l)).collect();
    if v4_lines != v5_lines {
        reds.push(format!(
            "{}: lines\n    v4 {v4_lines:?}\n    v5 {v5_lines:?}",
            case.name
        ));
    }
    reds
}

fn clip(s: &str) -> String {
    if s.len() > 300 {
        // Cut on a char boundary — a red report may carry multibyte text.
        let cut = (0..=300)
            .rev()
            .find(|&i| s.is_char_boundary(i))
            .unwrap_or(0);
        format!("{}…({} bytes)", &s[..cut], s.len())
    } else {
        s.to_string()
    }
}

#[test]
fn scripted_seam_matches_v4_on_every_row() {
    global_capture::install();
    let Some(rows) = load_oracle() else { return };
    let corpus = load_corpus();
    assert_eq!(
        rows.len(),
        corpus.cases.len(),
        "oracle rows vs corpus cases"
    );
    let mut reds = Vec::new();
    for case in &corpus.cases {
        let (result, lines) = run_v5(case, true);
        reds.extend(compare_case(
            case,
            oracle_row(&rows, &case.name),
            &result,
            &lines,
        ));
    }
    assert!(
        reds.is_empty(),
        "{} red comparand(s) over {} cases:\n{}",
        reds.len(),
        corpus.cases.len(),
        reds.join("\n")
    );
}

#[test]
fn production_seam_diverges_only_where_pdf_parse_finds_text() {
    global_capture::install();
    let Some(rows) = load_oracle() else { return };
    let corpus = load_corpus();
    let mut reds = Vec::new();
    for case in &corpus.cases {
        let row = oracle_row(&rows, &case.name);
        let (result, lines) = run_v5(case, false);
        if PARSED_TEXT_ROWS.contains(&case.name.as_str()) {
            // v4: pdf-parse's text. v5's refusing seam: the regex fallback over
            // the same bytes — which finds nothing in the corpus's junk header,
            // or (the fallback-readable rows) the scrape v4 never reached.
            assert_eq!(row["result"]["success"], true, "{}: v4 parses", case.name);
            let scrape = FALLBACK_READABLE_PARSED_ROWS
                .iter()
                .find(|(name, _)| *name == case.name)
                .map(|(_, text)| *text);
            let expected = match scrape {
                Some(text) => serde_json::json!({
                    "success": true,
                    "content": text,
                    "contentType": "text",
                    "truncated": false,
                }),
                None => serde_json::json!({
                    "success": false,
                    "contentType": "error",
                    "error": "Failed to extract PDF content (no text found)",
                }),
            };
            assert_eq!(
                result, expected,
                "{}: the recorded divergence — v5's production seam refuses, so the fallback runs",
                case.name
            );
            assert_ne!(
                row["result"], result,
                "{}: the divergence is real",
                case.name
            );
            let mut expected_lines = vec![format!(
                "WARN {TARGET} pdf-parse found no text, using native fallback extraction size={}",
                case.bytes().len()
            )];
            if let Some(text) = scrape {
                expected_lines.push(format!(
                    "DEBUG {TARGET} Extracted PDF content size={} chars={}",
                    case.bytes().len(),
                    text.encode_utf16().count()
                ));
            }
            assert_eq!(
                lines.iter().map(|l| cut_v5_line(l)).collect::<Vec<_>>(),
                expected_lines,
                "{}: v5 announces the fallback v4 never reached",
                case.name
            );
        } else {
            reds.extend(compare_case(case, row, &result, &lines));
        }
    }
    assert!(
        reds.is_empty(),
        "production seam reds:\n{}",
        reds.join("\n")
    );
}

#[test]
fn converter_warns_are_v4_only_on_exactly_the_named_rows() {
    let Some(rows) = load_oracle() else { return };
    let corpus = load_corpus();
    for case in &corpus.cases {
        let row = oracle_row(&rows, &case.name);
        let messages: Vec<&str> = row["converterLines"]
            .as_array()
            .expect("converterLines")
            .iter()
            .map(|l| l["message"].as_str().expect("message"))
            .collect();
        let expected: &[&str] = match case.name.as_str() {
            "pdf_throw_then_fallback" | "pdf_fallback_mixed_dedup" => {
                &["Failed to extract text from PDF buffer"]
            }
            "pdf_empty_buffer" => &["PDF buffer is empty"],
            _ => &[],
        };
        assert_eq!(messages, expected, "{}: v4's converter WARNs", case.name);
        // The mocked parser fired on every PDF row with bytes — a mock that never
        // fired would have run the REAL pdf-parse over the corpus's junk.
        let is_pdf = case.file.mime_type == "application/pdf";
        let want_calls = u64::from(is_pdf && !case.bytes().is_empty());
        assert_eq!(
            row["parserCalls"], want_calls,
            "{}: parser calls",
            case.name
        );
    }
}
