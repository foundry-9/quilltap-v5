//! Tier-3 differential (P4.D356 items 11–12, bug 269 / v4 `4b0d1cb04`): v4's
//! REAL `NanoGPTImageProvider.generateImage` (SDK + `fetch` scripted, no model)
//! ↔ v5's [`RealImageProvider`] over a recording [`WireTransport`]. Per case,
//! compared exactly:
//!
//! - the POSTED bodies, in order (v4's snapshotted `client.images.generate`
//!   params ↔ v5's wire bodies, key order included) — the fit, the flat
//!   `aspect_ratio` before `seed`, and the ONE square retry;
//! - the catalog GETs (v4's `fetch` URLs ↔ v5's GET URLs) — the cold fetch,
//!   and none when warm;
//! - the outcome (`{ok, images}` or the error message);
//! - every log line: level + message + key NAMES in order (v4 camelCase ↔ v5
//!   snake_case), plus the `error` text — so every case is also a silence leg
//!   (a line v4 did not log must not appear).
//!
//! **The framing.** v4's SDK is mocked, so its "posted body" is the params
//! object; v5's is the wire body — the same JSON by construction of the
//! `84f33ce94` port (`image_dialects_equivalence` proves the unfitted body).
//! A scripted failure is v4's test's own `Error(`${status} ${text}`)` with
//! `.status`; v5 receives a NON-JSON body `text` with that status, which its
//! SDK reconstruction renders `${status} ${text}`. A key v4 logs as
//! `undefined` is absent from its record; v5 always names the field, so v5's
//! `Option` shape fields (`size`, `aspect_ratio`, `requested_*`, `square_*`)
//! that are `None` count as absent — and only those (`loraDialect: null` is a
//! present key on both sides).
//!
//! The catalog cache is process-global: every case holds
//! `CATALOG_TEST_LOCK`, resets it cold, and a `warm` case fills it through
//! `remember_detailed_catalog` (the listing's own fill) before capture starts.
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-nanogpt-image-provider-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/nanogpt-image-provider.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-nanogpt-image-provider.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "nanogpt-image-provider\.test\.ts$"
//!   cd "$V5W"
//!   QT_ORACLE_NANOGPT_IMAGE_PROVIDER=/tmp/oracle-nanogpt-image-provider.ndjson \
//!     cargo test -p quilltap-harness --test nanogpt_image_provider_equivalence -- --nocapture

use std::sync::{Arc, Mutex};

use quilltap_core::model::image::{ImageGenParams, ImageProvider};
use quilltap_core::model::image_dialects::RealImageProvider;
use quilltap_core::model::nanogpt_catalog::{
    remember_detailed_catalog, reset_catalog_for_tests, CATALOG_TEST_LOCK,
};
use quilltap_core::model::wire::{WireResponse, WireTransport};
use serde_json::{json, Value};

const LISTING_URL: &str = "https://nano-gpt.com/api/v1/image-models?detailed=true";

/// Answers the catalog GET from the case's `catalog`, and the i-th POST from
/// the case's `generate` script; records every request it is handed.
struct ScriptedTransport {
    catalog: Value,
    script: Mutex<Vec<Value>>,
    gets: Arc<Mutex<Vec<String>>>,
    posts: Arc<Mutex<Vec<String>>>,
}

impl WireTransport for ScriptedTransport {
    async fn send(
        &self,
        method: &str,
        url: &str,
        _headers: &[(String, String)],
        body: &str,
    ) -> Result<WireResponse, String> {
        if method == "GET" {
            self.gets.lock().unwrap().push(url.to_string());
            if url != LISTING_URL {
                return Err(format!("unscripted GET {url}"));
            }
            if let Some(msg) = self.catalog.get("throws").and_then(Value::as_str) {
                return Err(msg.to_string());
            }
            return match self.catalog.get("status").and_then(Value::as_u64) {
                Some(status) => Ok(WireResponse::new(
                    status as u16,
                    self.catalog["text"].as_str().unwrap_or(""),
                )),
                None => Err("unscripted catalog fetch".to_string()),
            };
        }
        self.posts.lock().unwrap().push(body.to_string());
        let mut script = self.script.lock().unwrap();
        if script.is_empty() {
            return Err("generate called past the script".to_string());
        }
        let next = script.remove(0);
        if next == json!("ok") {
            return Ok(WireResponse::new(
                200,
                r#"{"data":[{"b64_json":"aGVsbG8="}]}"#,
            ));
        }
        let text = next["text"].as_str().unwrap_or("").to_string();
        match next.get("status").and_then(Value::as_u64) {
            Some(status) => Ok(WireResponse::new(status as u16, &text)),
            None => Err(text),
        }
    }
}

/// One captured event: level, message, field names in order, field values.
#[derive(Debug)]
struct Event {
    level: String,
    message: String,
    fields: Vec<(String, String)>,
}

struct Visitor(Event);

impl tracing::field::Visit for Visitor {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.0.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        if f.name() == "message" {
            self.0.message = format!("{v:?}");
        } else {
            self.0.fields.push((f.name().to_string(), format!("{v:?}")));
        }
    }
}

struct Layer(Arc<Mutex<Vec<Event>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Layer {
    fn on_event(&self, e: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let mut v = Visitor(Event {
            level: e.metadata().level().to_string().to_lowercase(),
            message: String::new(),
            fields: Vec::new(),
        });
        e.record(&mut v);
        self.0.lock().unwrap().push(v.0);
    }
}

fn snake(camel: &str) -> String {
    let mut out = String::new();
    for c in camel.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The `Option` shape fields whose `None` is v4's `undefined` (absent key).
const SHAPE_FIELDS: [&str; 6] = [
    "size",
    "aspect_ratio",
    "requested_size",
    "requested_aspect_ratio",
    "square_size",
    "square_aspect_ratio",
];

fn params_of(p: &Value) -> ImageGenParams {
    let s = |k: &str| p.get(k).and_then(Value::as_str).map(str::to_string);
    ImageGenParams {
        prompt: s("prompt").unwrap_or_default(),
        model: s("model").unwrap_or_default(),
        size: s("size"),
        aspect_ratio: s("aspectRatio"),
        seed: p.get("seed").and_then(Value::as_f64),
        guidance_scale: p.get("guidanceScale").and_then(Value::as_f64),
        steps: p.get("steps").and_then(Value::as_f64),
        negative_prompt: s("negativePrompt"),
        ..Default::default()
    }
}

#[test]
fn nanogpt_image_provider_matches_v4() {
    let path = match std::env::var("QT_ORACLE_NANOGPT_IMAGE_PROVIDER") {
        Ok(p) => p,
        Err(_) => {
            println!(
                "SKIP: set QT_ORACLE_NANOGPT_IMAGE_PROVIDER to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let _lock = CATALOG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut rows, mut retried, mut fetched_rows) = (0usize, 0usize, 0usize);
    let mut failures = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let case: Value = serde_json::from_str(line).expect("oracle row");
        let id = case["id"].as_str().unwrap().to_string();
        rows += 1;

        reset_catalog_for_tests();
        if let Some(warm) = case["warm"].as_str() {
            remember_detailed_catalog(warm).expect("warm listing");
        }
        let gets = Arc::new(Mutex::new(Vec::new()));
        let posts = Arc::new(Mutex::new(Vec::new()));
        let transport = ScriptedTransport {
            catalog: case["catalog"].clone(),
            script: Mutex::new(case["generate"].as_array().cloned().unwrap_or_default()),
            gets: gets.clone(),
            posts: posts.clone(),
        };
        let provider = RealImageProvider::new(transport);
        let params = params_of(&case["params"]);
        let events = Arc::new(Mutex::new(Vec::<Event>::new()));
        let outcome = {
            use tracing_subscriber::layer::SubscriberExt;
            let subscriber = tracing_subscriber::registry().with(Layer(events.clone()));
            let _guard = tracing::subscriber::set_default(subscriber);
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            match rt.block_on(provider.generate_image("NANOGPT", "test-key", &params)) {
                Ok(r) => json!({"ok": true, "images": r.images.len()}),
                Err(e) => json!({"error": e.message}),
            }
        };

        let v4_calls: Vec<String> = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| serde_json::to_string(c).unwrap())
            .collect();
        let v5_posts = posts.lock().unwrap().clone();
        if v4_calls != v5_posts {
            failures.push(format!(
                "{id}: bodies\n  v4 {v4_calls:?}\n  v5 {v5_posts:?}"
            ));
        }
        if v4_calls.len() == 2 {
            retried += 1;
        }
        let v4_fetched: Vec<String> = case["fetched"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u.as_str().unwrap().to_string())
            .collect();
        let v5_gets = gets.lock().unwrap().clone();
        if v4_fetched != v5_gets {
            failures.push(format!("{id}: fetched v4 {v4_fetched:?} / v5 {v5_gets:?}"));
        }
        if !v4_fetched.is_empty() {
            fetched_rows += 1;
        }
        if outcome != case["outcome"] {
            failures.push(format!(
                "{id}: outcome v4 {} / v5 {outcome}",
                case["outcome"]
            ));
        }

        let v4_lines: Vec<Value> = case["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                let keys: Vec<String> = l["keys"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|k| snake(k.as_str().unwrap()))
                    .collect();
                json!({"level": l["level"], "message": l["message"], "keys": keys,
                       "error": l.get("error").cloned().unwrap_or(Value::Null)})
            })
            .collect();
        let v5_lines: Vec<Value> = events
            .lock()
            .unwrap()
            .iter()
            .map(|e| {
                let keys: Vec<&str> = e
                    .fields
                    .iter()
                    .filter(|(k, v)| !(SHAPE_FIELDS.contains(&k.as_str()) && v == "None"))
                    .map(|(k, _)| k.as_str())
                    .collect();
                let error = e
                    .fields
                    .iter()
                    .find(|(k, _)| k == "error")
                    .map_or(Value::Null, |(_, v)| Value::String(v.clone()));
                json!({"level": e.level, "message": e.message, "keys": keys, "error": error})
            })
            .collect();
        if v4_lines != v5_lines {
            failures.push(format!(
                "{id}: lines\n  v4 {}\n  v5 {}",
                serde_json::to_string_pretty(&v4_lines).unwrap(),
                serde_json::to_string_pretty(&v5_lines).unwrap()
            ));
        }
    }
    reset_catalog_for_tests();
    println!("nanogpt_image_provider: {rows} cases ({retried} retried, {fetched_rows} fetched)");
    assert!(
        failures.is_empty(),
        "{} differences:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(rows >= 20, "a short corpus: {rows}");
    assert!(retried >= 7, "the retry cases went missing: {retried}");
    assert!(
        fetched_rows >= 7,
        "the cold-fetch cases went missing: {fetched_rows}"
    );
}
