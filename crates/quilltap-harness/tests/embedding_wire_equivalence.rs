//! W4.7e `embedding_wire_equivalence`: drives v4's REAL plugin embedding
//! providers (jest oracle, fetch/SDK mocked) and rebuilds each request + parses
//! each canned body through `model::embedding_wire`, diffing the recorded request
//! bytes (method/url/body) and the parsed result / error message.
//!
//! Generate the oracle (jest, Node 24):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   TMPO=/tmp/qt-embedding-wire-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/harness/oracle/cases" "$TMPO/harness/oracle/fixtures"
//!   cp "$V5/harness/oracle/cases/embedding-wire.test.ts" "$TMPO/harness/oracle/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-embedding-wire.ndjson \
//!     $N/npx jest --silent --watchman=false \
//!       --roots "$PWD" --roots "$TMPO/harness/oracle/cases" -- embedding-wire
//! Run:
//!   QT_ORACLE_EMBEDDING_WIRE=/tmp/oracle-embedding-wire.ndjson \
//!     cargo test -p quilltap-harness --test embedding_wire_equivalence

use quilltap_core::model::embedding_wire as ew;
use quilltap_core::model::request_builder::BuiltRequest;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct FetchResult {
    status: i64,
    body: Value,
}
#[derive(Deserialize)]
struct Recorded {
    method: String,
    url: String,
    body: Option<Value>,
}
#[derive(Deserialize)]
struct Row {
    id: String,
    kind: String,
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
    model: String,
    input: String,
    dimensions: Option<i64>,
    #[serde(rename = "fetchScript")]
    fetch_script: std::collections::HashMap<String, FetchResult>,
    #[serde(rename = "sdkResponse")]
    sdk_response: Option<Value>,
    #[serde(rename = "sdkRequestBody")]
    sdk_request_body: Option<Value>,
    recorded: Vec<Recorded>,
    result: Option<Value>,
    error: Option<String>,
}

fn status_text(status: i64) -> &'static str {
    match status {
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

/// Render an f64 the way v4's `JSON.stringify` does (integer-valued -> bare) —
/// mirrors `quilltap_core::db::js_number_to_json` (crate-private).
fn num(v: f64) -> Value {
    if v.is_finite() && v.fract() == 0.0 && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
        Value::from(v as i64)
    } else {
        Value::from(v)
    }
}

fn assert_request(got: &BuiltRequest, rec: &Recorded, ctx: &str) {
    assert_eq!(got.method, rec.method, "{ctx}: method");
    assert_eq!(got.url, rec.url, "{ctx}: url");
    assert_eq!(Some(&got.body), rec.body.as_ref(), "{ctx}: body");
}

/// Build v4's result-usage JSON for OpenAI (`{promptTokens, totalTokens}`,
/// present sub-fields only, no cost).
fn openai_usage(u: &ew::WireUsage) -> Value {
    let mut m = serde_json::Map::new();
    if let Some(p) = u.prompt_tokens {
        m.insert("promptTokens".into(), num(p));
    }
    if let Some(t) = u.total_tokens {
        m.insert("totalTokens".into(), num(t));
    }
    Value::Object(m)
}

/// Build v4's result-usage JSON for OpenRouter (`{promptTokens, totalTokens,
/// cost?}`).
fn openrouter_usage(u: &ew::WireUsage) -> Value {
    let mut m = serde_json::Map::new();
    if let Some(p) = u.prompt_tokens {
        m.insert("promptTokens".into(), num(p));
    }
    if let Some(t) = u.total_tokens {
        m.insert("totalTokens".into(), num(t));
    }
    if let Some(c) = u.cost {
        m.insert("cost".into(), num(c));
    }
    Value::Object(m)
}

/// v4 result object `{embedding, model, dimensions, usage?}`.
fn result_value(e: &ew::WireEmbedding, model: &str, usage: Option<Value>) -> Value {
    let mut m = serde_json::Map::new();
    m.insert(
        "embedding".into(),
        Value::Array(e.embedding.iter().map(|&x| num(x)).collect()),
    );
    m.insert("model".into(), json!(model));
    m.insert("dimensions".into(), json!(e.dimensions));
    if let Some(u) = usage {
        m.insert("usage".into(), u);
    }
    Value::Object(m)
}

#[test]
fn embedding_wire_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_EMBEDDING_WIRE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_EMBEDDING_WIRE to the oracle NDJSON.");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let base = row.base_url.clone().unwrap_or_default();

        match row.kind.as_str() {
            "openai" => {
                let req =
                    ew::build_openai_request(&base, &row.model, &row.input, row.dimensions, "sk-x");
                assert_request(&req, &row.recorded[0], &row.id);
                let script = &row.fetch_script["/embeddings"];
                if (200..300).contains(&script.status) {
                    let parsed = ew::parse_openai_response(&script.body).unwrap();
                    let usage = parsed.usage.as_ref().map(openai_usage);
                    let got = result_value(&parsed, &row.model, usage);
                    assert_eq!(got, row.result.clone().unwrap(), "{}: result", row.id);
                } else {
                    let msg = ew::openai_error_message(&script.body, status_text(script.status));
                    assert_eq!(msg, row.error.clone().unwrap(), "{}: error", row.id);
                }
            }
            "ollama" => {
                if let Some(err) = ew::ollama_empty_input_error(&row.input) {
                    assert!(row.recorded.is_empty(), "{}: no requests on empty", row.id);
                    assert_eq!(err, row.error.clone().unwrap(), "{}: empty error", row.id);
                    count += 1;
                    continue;
                }
                // /api/show
                let show_req = ew::build_ollama_show_request(&base, &row.model);
                assert_request(&show_req, &row.recorded[0], &format!("{} show", row.id));
                let (num_ctx, _derived) = ew::derive_num_ctx(&row.fetch_script["/api/show"].body);
                // /api/embed
                let embed_req =
                    ew::build_ollama_embed_request(&base, &row.model, &row.input, num_ctx);
                assert_request(&embed_req, &row.recorded[1], &format!("{} embed", row.id));
                let embed = &row.fetch_script["/api/embed"];
                if embed.status == 404 {
                    let legacy_req = ew::build_ollama_legacy_request(&base, &row.model, &row.input);
                    assert_request(&legacy_req, &row.recorded[2], &format!("{} legacy", row.id));
                    let parsed =
                        ew::parse_ollama_legacy_response(&row.fetch_script["/api/embeddings"].body);
                    finalize_ollama(parsed, &row);
                } else if (200..300).contains(&embed.status) {
                    let parsed = ew::parse_ollama_embed_response(&embed.body);
                    finalize_ollama(parsed, &row);
                } else {
                    let msg = ew::ollama_error_message(&embed.body, status_text(embed.status));
                    assert_eq!(msg, row.error.clone().unwrap(), "{}: error", row.id);
                }
            }
            "openrouter" => {
                let body =
                    ew::build_openrouter_request_body(&row.model, &row.input, row.dimensions);
                assert_eq!(
                    Some(&body),
                    row.sdk_request_body.as_ref(),
                    "{}: sdk request body",
                    row.id
                );
                let response = row.sdk_response.clone().unwrap();
                match ew::parse_openrouter_response(&response) {
                    Ok(parsed) => {
                        let usage = parsed.usage.as_ref().map(openrouter_usage);
                        let got = result_value(&parsed, &parsed.model, usage);
                        assert_eq!(got, row.result.clone().unwrap(), "{}: result", row.id);
                    }
                    Err(msg) => {
                        assert_eq!(msg, row.error.clone().unwrap(), "{}: error", row.id);
                    }
                }
            }
            other => panic!("unknown kind {other}"),
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty");
    eprintln!("OK: embedding-wire matched oracle ({count} rows).");
}

fn finalize_ollama(parsed: Result<ew::WireEmbedding, String>, row: &Row) {
    match parsed {
        Ok(e) => {
            // Ollama result carries no usage.
            let got = result_value(&e, &row.model, None);
            assert_eq!(got, row.result.clone().unwrap(), "{}: result", row.id);
        }
        Err(msg) => {
            assert_eq!(msg, row.error.clone().unwrap(), "{}: error", row.id);
        }
    }
}
