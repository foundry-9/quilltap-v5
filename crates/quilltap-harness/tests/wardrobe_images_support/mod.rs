//! Shared helpers for the P4.D263 wardrobe-image families
//! (`wardrobe_item_images_tier2_equivalence`,
//! `wardrobe_item_image_generation_tier3_equivalence`): the table dump, the
//! log-line renderings, and the scenario normalizer (see the tier-2 header for
//! its rules).

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use regex::Regex;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// The six mount-index tables both families dump (`doc_mount_chunks`
/// excluded, as the transfers family does).
pub const MOUNT_TABLES: [(&str, &str); 6] = [
    ("points", "doc_mount_points"),
    ("folders", "doc_mount_folders"),
    ("links", "doc_mount_file_links"),
    ("mountFiles", "doc_mount_files"),
    ("documents", "doc_mount_documents"),
    ("blobs", "doc_mount_blobs"),
];

/// `LEVEL target message k=v…` → `LEVEL message k=v…` for the lines this
/// family pins.
pub fn rust_lines(lines: &[String], prefixes: &[&str]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next()?;
            let _target = parts.next()?;
            let rest = parts.next()?;
            prefixes
                .iter()
                .any(|p| rest.starts_with(p))
                .then(|| format!("{level} {rest}"))
        })
        .collect()
}

/// v4's `{level, message, meta}` → the same rendering (meta keys in order,
/// `null` for null, numbers/booleans bare).
pub fn oracle_lines(logs: &Value) -> Vec<String> {
    logs.as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let mut s = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            if let Some(meta) = l["meta"].as_object() {
                for (k, v) in meta {
                    // §R.5: an array/object field renders through the `…Json`
                    // file-layer convention.
                    match v {
                        Value::String(t) => s.push_str(&format!(" {k}={t}")),
                        Value::Null => s.push_str(&format!(" {k}=null")),
                        Value::Array(_) | Value::Object(_) => s.push_str(&format!(" {k}Json={v}")),
                        other => s.push_str(&format!(" {k}={other}")),
                    }
                }
            }
            s
        })
        .collect()
}

pub fn dump(conn: &Connection, table: &str) -> Value {
    let columns: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            let mut m = Map::new();
            for (i, c) in columns.iter().enumerate() {
                use rusqlite::types::ValueRef;
                let v = match r.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    // JS has one number type: a whole REAL reads as an integer.
                    ValueRef::Real(f) if f.fract() == 0.0 && f.abs() < 9e15 => json!(f as i64),
                    ValueRef::Real(f) => json!(f),
                    ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                    ValueRef::Blob(b) => {
                        json!(b.iter().map(|x| format!("{x:02x}")).collect::<String>())
                    }
                };
                m.insert(c.clone(), v);
            }
            Ok(Value::Object(m))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    json!({ "table": table, "columns": columns, "rows": rows })
}

pub fn dump_all(main: &Connection, mount: &Connection) -> Value {
    let mut tables = Map::new();
    tables.insert("files".into(), dump(main, "files"));
    for (key, table) in MOUNT_TABLES {
        tables.insert(key.into(), dump(mount, table));
    }
    Value::Object(tables)
}

/// The scenario-wide normalizer (see the header).
pub struct Normalizer {
    baked: HashSet<String>,
    ids: HashMap<String, String>,
    leaves: HashMap<String, String>,
    uuid: Regex,
    iso: Regex,
    leaf: Regex,
    sha: Regex,
}

impl Normalizer {
    pub fn new(baked: HashSet<String>) -> Self {
        Normalizer {
            baked,
            ids: HashMap::new(),
            leaves: HashMap::new(),
            uuid: Regex::new(
                r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
            )
            .unwrap(),
            iso: Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap(),
            leaf: Regex::new(r"\d{8}-\d{6}-(generated|uploaded|imported)-[0-9a-f]{8}").unwrap(),
            sha: Regex::new(r"\b[0-9a-f]{64}\b").unwrap(),
        }
    }

    pub fn string(&mut self, s: &str) -> String {
        let s = self.iso.replace_all(s, "<ts>").into_owned();
        let mut out = String::new();
        let mut last = 0;
        let leaves: Vec<(usize, usize, String, String)> = self
            .leaf
            .captures_iter(&s)
            .map(|c| {
                let m = c.get(0).unwrap();
                (m.start(), m.end(), m.as_str().to_string(), c[1].to_string())
            })
            .collect();
        for (start, end, whole, kind) in leaves {
            out.push_str(&s[last..start]);
            if self.baked.contains(&whole) {
                out.push_str(&whole);
            } else {
                let n = self.leaves.len();
                let token = self
                    .leaves
                    .entry(whole)
                    .or_insert_with(|| format!("<leaf:{kind}:{n}>"))
                    .clone();
                out.push_str(&token);
            }
            last = end;
        }
        out.push_str(&s[last..]);
        let s = out;
        let ids: Vec<String> = self
            .uuid
            .find_iter(&s)
            .map(|m| m.as_str().to_string())
            .collect();
        let mut s = s;
        for id in ids {
            if self.baked.contains(&id) {
                continue;
            }
            let n = self.ids.len();
            let token = self
                .ids
                .entry(id.clone())
                .or_insert_with(|| format!("<id:{n}>"))
                .clone();
            s = s.replace(&id, &token);
        }
        let shas: Vec<String> = self
            .sha
            .find_iter(&s)
            .map(|m| m.as_str().to_string())
            .collect();
        for sha in shas {
            if !self.baked.contains(&sha) {
                s = s.replace(&sha, "<sha>");
            }
        }
        s
    }

    pub fn value(&mut self, v: &Value) -> Value {
        match v {
            Value::String(s) => Value::String(self.string(s)),
            Value::Array(a) => Value::Array(a.iter().map(|e| self.value(e)).collect()),
            Value::Object(o) => {
                let mut m = Map::new();
                for (k, e) in o {
                    m.insert(k.clone(), self.value(e));
                }
                Value::Object(m)
            }
            other => other.clone(),
        }
    }
}

/// Every uuid / leaf / 64-hex token in the baked fixture's dump, plus the
/// picture shas — the values that compare literally.
pub fn baked_tokens(tables: &Value, spec: &Value) -> HashSet<String> {
    let text = tables.to_string();
    let mut out = HashSet::new();
    for re in [
        r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
        r"\d{8}-\d{6}-(generated|uploaded|imported)-[0-9a-f]{8}",
        r"\b[0-9a-f]{64}\b",
    ] {
        for m in Regex::new(re).unwrap().find_iter(&text) {
            out.insert(m.as_str().to_string());
        }
    }
    for m in
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap()
            .find_iter(&spec.to_string())
    {
        out.insert(m.as_str().to_string());
    }
    let webp: Vec<Value> = match &spec["webp"] {
        Value::Array(a) => a.clone(),
        one => vec![one.clone()],
    };
    for b in &webp {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b.as_str().unwrap())
            .unwrap();
        out.insert(
            Sha256::digest(&bytes)
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect(),
        );
    }
    out
}
