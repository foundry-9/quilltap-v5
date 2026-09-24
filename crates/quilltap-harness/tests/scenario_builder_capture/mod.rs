//! A STRUCTURAL tracing capture for the Scenario Builder families (P4.D217):
//! each event as `(level, message, fields by name)`, so a v4 service-logger
//! line `{ level, message, context }` compares field for field.
//!
//! The shape is P4.D216's `brahma_console_tier3_equivalence` layer, lifted into
//! a module the three P4.D217 families share rather than copied three times.
//! Field ORDER is not part of the comparand (v4's is object-insertion order,
//! v5's is the macro's); the SET of `(name, rendered value)` pairs is.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub target: String,
    pub level: String,
    pub message: String,
    pub fields: Vec<(String, String)>,
}

struct LineVisitor {
    message: String,
    fields: Vec<(String, String)>,
}
impl tracing::field::Visit for LineVisitor {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_i64(&mut self, f: &tracing::field::Field, v: i64) {
        self.fields.push((f.name().to_string(), v.to_string()));
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        self.fields.push((f.name().to_string(), v.to_string()));
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

/// Captures every event whose target starts with one of `targets`.
pub struct StructuralCapture {
    pub lines: Arc<Mutex<Vec<Line>>>,
    targets: &'static [&'static str],
}

impl StructuralCapture {
    pub fn new(targets: &'static [&'static str]) -> (Self, Arc<Mutex<Vec<Line>>>) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        (
            StructuralCapture {
                lines: lines.clone(),
                targets,
            },
            lines,
        )
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for StructuralCapture {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        if !self.targets.iter().any(|t| meta.target() == *t) {
            return;
        }
        let mut v = LineVisitor {
            message: String::new(),
            fields: Vec::new(),
        };
        event.record(&mut v);
        self.lines.lock().unwrap().push(Line {
            target: meta.target().to_string(),
            level: meta.level().to_string().to_lowercase(),
            message: v.message,
            fields: v.fields,
        });
    }
}

/// A v4 context value rendered the way the v5 capture renders the same field:
/// strings bare, numbers/bools as text, a nested object as its compact JSON
/// (v5 logs a nested bag as a `%`-rendered JSON string), a string array as
/// Rust's `Debug` of a `Vec<&str>`.
pub fn render_v4(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) if items.iter().all(Value::is_string) => {
            let strs: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            format!("{strs:?}")
        }
        other => other.to_string(),
    }
}

/// v4's recorded `[{ level, message, context }]` as [`Line`]s (target blank).
pub fn v4_lines(rows: &Value) -> Vec<Line> {
    rows.as_array()
        .map(|a| {
            a.iter()
                .map(|l| Line {
                    target: String::new(),
                    level: l["level"].as_str().unwrap_or_default().to_string(),
                    message: l["message"].as_str().unwrap_or_default().to_string(),
                    fields: l["context"]
                        .as_object()
                        .map(|o| o.iter().map(|(k, v)| (k.clone(), render_v4(v))).collect())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `(level, message, sorted fields)`.
pub type NormLine = (String, String, Vec<(String, String)>);

/// Each line as a [`NormLine`]; `presence_only` names fields compared by
/// presence alone (an error's text — v4's `error.message` vs v5's `Display`).
pub fn normalize(lines: &[Line], presence_only: &[&str]) -> Vec<NormLine> {
    lines
        .iter()
        .map(|l| {
            let mut f: Vec<(String, String)> = l
                .fields
                .iter()
                .map(|(k, v)| {
                    if presence_only.contains(&k.as_str()) {
                        (k.clone(), "<present>".to_string())
                    } else {
                        (k.clone(), v.clone())
                    }
                })
                .collect();
            f.sort();
            (l.level.clone(), l.message.clone(), f)
        })
        .collect()
}
