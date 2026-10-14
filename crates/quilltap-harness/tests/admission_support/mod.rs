//! Shared plumbing for the P4.D290 admission differentials
//! (`admission_tier3_equivalence`, `update_memory_tier2_equivalence`): the
//! recording canned provider built from an oracle's emitted map, the
//! thread-scoped capture of the lines admission owns rendered `LEVEL message
//! k=v …` (and v4's captured lines in the same rendering), and the ONE
//! normalisation both sides go through (minted ids tokenised by row content,
//! wall-clock stamps placeholdered, numbers within 1e-12).

#![allow(dead_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use quilltap_core::admission::AdmissionRow;
use quilltap_core::db::runtime::DbPaths;
use quilltap_core::model::embedding::{
    CannedEmbeddingProvider, EmbeddingCallOptions, EmbeddingError, EmbeddingPriority,
    EmbeddingProvider, EmbeddingResult,
};
use quilltap_core::pascal::js_value::number_to_string;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// The recording provider
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Recording {
    pub inner: CannedEmbeddingProvider,
    pub calls: Arc<Mutex<Vec<Value>>>,
}

impl EmbeddingProvider for Recording {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        user_id: &str,
        profile_id: Option<&str>,
        opts: EmbeddingCallOptions,
    ) -> impl std::future::Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        let priority = match opts.priority {
            EmbeddingPriority::Background => "background",
            EmbeddingPriority::Interactive => "interactive",
        };
        self.calls.lock().unwrap().push(json!({
            "text": text,
            "lexicalText": opts.lexical_text,
            "priority": priority,
        }));
        self.inner
            .generate_embedding_for_user(text, user_id, profile_id, opts)
    }
}

// ---------------------------------------------------------------------------
// The line capture (thread-scoped; every admission future is polled here)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Visitor {
    message: String,
    fields: Vec<(String, String)>,
}

impl tracing::field::Visit for Visitor {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_i64(&mut self, f: &tracing::field::Field, v: i64) {
        self.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        self.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_f64(&mut self, f: &tracing::field::Field, v: f64) {
        self.fields
            .push((f.name().to_string(), number_to_string(v)));
    }
    fn record_bool(&mut self, f: &tracing::field::Field, v: bool) {
        self.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        if f.name() == "message" {
            self.message = format!("{v:?}");
        } else {
            self.fields.push((f.name().to_string(), format!("{v:?}")));
        }
    }
}

/// Captures the lines whose `module` is one of `owned` (or whose message
/// starts with one of `prefixes`).
pub struct Capture {
    pub lines: Arc<Mutex<Vec<String>>>,
    pub owned: &'static [&'static str],
    pub prefixes: &'static [&'static str],
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let mut v = Visitor::default();
        event.record(&mut v);
        let module = v
            .fields
            .iter()
            .find(|(k, _)| k == "module")
            .map(|(_, m)| m.clone());
        let owned = module.as_deref().is_some_and(|m| self.owned.contains(&m))
            || self.prefixes.iter().any(|p| v.message.starts_with(p));
        if !owned {
            return;
        }
        let level = event.metadata().level().to_string();
        self.lines
            .lock()
            .unwrap()
            .push(render(&level, &v.message, &v.fields));
    }
}

pub fn render(level: &str, message: &str, fields: &[(String, String)]) -> String {
    let mut out = format!("{} {message}", level.to_uppercase());
    for (k, v) in fields {
        out.push_str(&format!(" {k}={v}"));
    }
    out
}

/// A v4 captured line (`{level, message, context}`) in the same rendering.
pub fn render_v4(line: &Value) -> String {
    let mut fields = Vec::new();
    if let Some(ctx) = line["context"].as_object() {
        for (k, v) in ctx {
            match v {
                Value::Array(_) | Value::Object(_) => {
                    fields.push((format!("{k}Json"), serde_json::to_string(v).unwrap()))
                }
                Value::String(s) => fields.push((k.clone(), s.clone())),
                Value::Null => fields.push((k.clone(), "null".into())),
                Value::Bool(b) => fields.push((k.clone(), b.to_string())),
                Value::Number(n) => fields.push((k.clone(), number_to_string(n.as_f64().unwrap()))),
            }
        }
    }
    render(
        line["level"].as_str().unwrap(),
        line["message"].as_str().unwrap(),
        &fields,
    )
}

// ---------------------------------------------------------------------------
// The spec
// ---------------------------------------------------------------------------

pub fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/admission-tier3.json")
}

pub fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

pub fn opt_strings(v: &Value, key: &str) -> Option<Vec<String>> {
    v.get(key).and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

/// A corpus row as v4's `AdmissionRow` object (the `_`-prefixed keys are the
/// oracle's own).
pub fn row_of(v: &Value) -> AdmissionRow {
    AdmissionRow {
        character_id: opt_str(v, "characterId").unwrap(),
        content: opt_str(v, "content").unwrap_or_default(),
        summary: opt_str(v, "summary").unwrap_or_default(),
        keywords: opt_strings(v, "keywords"),
        tags: opt_strings(v, "tags"),
        importance: v.get("importance").and_then(Value::as_f64),
        about_character_id: opt_str(v, "aboutCharacterId"),
        chat_id: opt_str(v, "chatId"),
        project_id: opt_str(v, "projectId"),
        source: opt_str(v, "source"),
        source_message_id: opt_str(v, "sourceMessageId"),
        source_message_timestamp: opt_str(v, "sourceMessageTimestamp"),
        witnessed_context: opt_str(v, "witnessedContext"),
        occurred_at: opt_str(v, "occurredAt"),
        narrative_time: opt_str(v, "narrativeTime"),
        entities: opt_strings(v, "entities"),
        kind: opt_str(v, "kind"),
    }
}

// ---------------------------------------------------------------------------
// Normalisation
// ---------------------------------------------------------------------------

pub const WALL_CLOCK: &[&str] = &[
    "createdAt",
    "updatedAt",
    "lastReinforcedAt",
    "lastAccessedAt",
];

pub struct Norm {
    /// raw minted id → token
    pub minted: Vec<(String, String)>,
    /// ISO stamps the corpus planted (never placeholdered)
    pub planted: HashSet<String>,
}

impl Norm {
    pub fn ids(&self, s: &str) -> String {
        let mut out = s.to_string();
        for (raw, token) in &self.minted {
            out = out.replace(raw, token);
        }
        out
    }

    pub fn value(&self, v: &Value, key: Option<&str>) -> Value {
        match v {
            Value::String(s) => {
                if key.is_some_and(|k| WALL_CLOCK.contains(&k)) && !self.planted.contains(s) {
                    Value::String("<ts>".into())
                } else {
                    Value::String(self.ids(s))
                }
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| self.value(x, None)).collect()),
            Value::Object(m) => Value::Object(
                m.iter()
                    .map(|(k, x)| (k.clone(), self.value(x, Some(k))))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    pub fn row(&self, row: &Value) -> Value {
        let mut out = serde_json::Map::new();
        for (k, v) in row.as_object().unwrap() {
            let v = match (k.as_str(), v) {
                // JSON-array text columns: remap their elements.
                ("relatedMemoryIds" | "consolidatedFrom", Value::String(s)) => {
                    Value::String(self.ids(s))
                }
                _ => self.value(v, Some(k)),
            };
            out.insert(k.clone(), v);
        }
        Value::Object(out)
    }
}

/// Minted ids of a dump: rows whose id the corpus did not seed, tokenised by
/// content (unique in the corpus's final state per character).
pub fn minted_ids(dump: &Value, seeded: &HashSet<String>) -> Vec<(String, String)> {
    dump["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| {
            let id = r["id"].as_str()?;
            (!seeded.contains(id)).then(|| {
                (
                    id.to_string(),
                    format!(
                        "MINTED<{}|{}>",
                        r["characterId"].as_str().unwrap_or_default(),
                        r["content"].as_str().unwrap_or_default()
                    ),
                )
            })
        })
        .collect()
}

/// Structural equality, numbers within 1e-12 (v4's `Math.log2` and Rust's
/// `f64::log2` may round a non-power-of-two count apart by an ulp).
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            (x - y).abs() <= 1e-12 * x.abs().max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

pub fn oracle_lines(path: &str) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect()
}

pub fn provider_from(oracle: &[Value]) -> Recording {
    let mut inner = CannedEmbeddingProvider::new();
    for line in oracle {
        match line["kind"].as_str() {
            Some("canned") => {
                let embedding: Vec<f32> = line["embedding"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_f64().unwrap() as f32)
                    .collect();
                inner = inner.with_result(
                    line["text"].as_str().unwrap(),
                    EmbeddingResult {
                        dimensions: embedding.len(),
                        embedding,
                        model: "canned".into(),
                        provider: line["provider"].as_str().unwrap().into(),
                    },
                );
            }
            Some("failure") => {
                inner = inner.with_failure_for(
                    line["text"].as_str().unwrap(),
                    line["message"].as_str().unwrap(),
                );
            }
            _ => {}
        }
    }
    Recording {
        inner,
        calls: Arc::new(Mutex::new(Vec::new())),
    }
}

pub fn copy_pair(main: &str, mount: &str, dir: &Path) -> DbPaths {
    let (m, n) = (dir.join("main.db"), dir.join("mount.db"));
    std::fs::copy(main, &m).expect("copy main fixture");
    std::fs::copy(mount, &n).expect("copy mount fixture");
    DbPaths {
        main: m,
        mount_index: Some(n),
        llm_logs: None,
    }
}

pub fn planted_stamps(spec: &Value, out: &mut HashSet<String>) {
    match spec {
        Value::String(s) if s.len() == 24 && s.ends_with('Z') && s.as_bytes()[10] == b'T' => {
            out.insert(s.clone());
        }
        Value::Array(a) => a.iter().for_each(|x| planted_stamps(x, out)),
        Value::Object(m) => m.values().for_each(|x| planted_stamps(x, out)),
        _ => {}
    }
}
