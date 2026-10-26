//! The Inventory route families' shared normalization (P4.D325): a UUID the
//! committed `inventory-routes-{main,mount}.db` pair does not contain was
//! minted by the request (first-seen tokens, shared across one case's rows);
//! a timestamp the pair does not contain is `<now>`. Applied to v4's and v5's
//! serialized bodies alike, so neither side's minting order matters.

use std::collections::HashSet;
use std::path::PathBuf;

use quilltap_core::db::Writer;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// A UUID at `s[i..i+36]`?
fn uuid_at(s: &[u8], i: usize) -> bool {
    if i + 36 > s.len() {
        return false;
    }
    (0..36).all(|k| match k {
        8 | 13 | 18 | 23 => s[i + k] == b'-',
        _ => is_hex(s[i + k]),
    })
}

/// A `YYYY-MM-DDTHH:MM:SS.mmmZ` timestamp at `s[i..i+24]`?
fn ts_at(s: &[u8], i: usize) -> bool {
    if i + 24 > s.len() {
        return false;
    }
    let pat = b"dddd-dd-ddTdd:dd:dd.dddZ";
    (0..24).all(|k| match pat[k] {
        b'd' => s[i + k].is_ascii_digit(),
        c => s[i + k] == c,
    })
}

/// Every UUID and timestamp a string carries.
fn scan(s: &str, ids: &mut HashSet<String>, stamps: &mut HashSet<String>) {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if uuid_at(b, i) {
            ids.insert(s[i..i + 36].to_ascii_lowercase());
            i += 36;
        } else if ts_at(b, i) {
            stamps.insert(s[i..i + 24].to_string());
            i += 24;
        } else {
            i += 1;
        }
    }
}

/// The ids and timestamps the committed pair carries (every text cell of every
/// table, both partitions) — anything else a response shows was minted.
pub struct Known {
    pub ids: HashSet<String>,
    pub stamps: HashSet<String>,
}

pub fn known_of(pepper: &str) -> Known {
    let mut ids = HashSet::new();
    let mut stamps = HashSet::new();
    for file in ["inventory-routes-main.db", "inventory-routes-mount.db"] {
        let scratch = tempfile::tempdir().unwrap();
        let copy = scratch.path().join(file);
        std::fs::copy(fixtures_dir().join(file), &copy).unwrap();
        let w = Writer::open_writable(&copy, pepper).unwrap();
        let conn = w.connection();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for t in tables {
            let mut stmt = conn.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
            let n = stmt.column_count();
            let mut rows = stmt.query([]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                for k in 0..n {
                    if let Ok(rusqlite::types::ValueRef::Text(bytes)) = row.get_ref(k) {
                        scan(&String::from_utf8_lossy(bytes), &mut ids, &mut stamps);
                    }
                }
            }
        }
    }
    Known { ids, stamps }
}

/// Replace minted UUIDs (first-seen tokens, shared across one case's rows) and
/// fresh timestamps in a serialized body.
pub fn normalize(s: &str, known: &Known, minted: &mut Vec<String>) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut last = 0;
    while i < b.len() {
        if uuid_at(b, i) {
            let id = s[i..i + 36].to_ascii_lowercase();
            if !known.ids.contains(&id) {
                out.push_str(&s[last..i]);
                let n = match minted.iter().position(|m| *m == id) {
                    Some(n) => n,
                    None => {
                        minted.push(id);
                        minted.len() - 1
                    }
                };
                out.push_str(&format!("<minted-{n}>"));
                last = i + 36;
            }
            i += 36;
        } else if ts_at(b, i) {
            if !known.stamps.contains(&s[i..i + 24]) {
                out.push_str(&s[last..i]);
                out.push_str("<now>");
                last = i + 24;
            }
            i += 24;
        } else {
            i += 1;
        }
    }
    out.push_str(&s[last..]);
    out
}
