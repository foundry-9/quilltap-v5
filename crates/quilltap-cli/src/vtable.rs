//! `console.table` reproduced byte-for-byte in its **non-TTY** form (the form
//! the differential diffs — no color; V8's box-drawing, the `(index)` column,
//! one-space cell padding, left alignment, column width = max rendered cell
//! width).
//!
//! Cells arrive pre-rendered (numbers via `js_num_string`, strings via
//! `inspect_quote`, `null` literal, absent keys empty) — the caller owns the
//! per-value `util.inspect` choice, this module owns the geometry. Widths are
//! counted in `char`s (Node's `getStringWidth` full-width table is a
//! documented seam; the corpus stays narrow-width).
//!
//! Interactive divergence (documented): on a TTY, Node colorizes cell values
//! (yellow numbers, green strings); this port emits the same uncolored bytes
//! on a TTY as when piped.

use serde_json::Value;

use crate::nodefmt::{inspect_quote, js_num_string};

/// Render a JS value into its `console.table` cell text.
pub fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => js_num_string(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => inspect_quote(s),
        // Nested objects/arrays are out of the corpus; render compactly rather
        // than reproducing V8's nested-inspect (documented seam).
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// `console.table(rows)` for an array of objects: returns the full multi-line
/// output (each line newline-terminated).
pub fn console_table(rows: &[serde_json::Map<String, Value>]) -> String {
    // Column set = union of keys in first-seen order (V8 walks the rows).
    let mut columns: Vec<String> = Vec::new();
    for row in rows {
        for key in row.keys() {
            if !columns.iter().any(|c| c == key) {
                columns.push(key.clone());
            }
        }
    }

    // Pre-render every cell.
    let index_header = "(index)";
    let mut cells: Vec<Vec<String>> = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let mut line = Vec::with_capacity(columns.len() + 1);
        line.push(i.to_string());
        for col in &columns {
            line.push(match row.get(col) {
                Some(v) => cell_text(v),
                None => String::new(),
            });
        }
        cells.push(line);
    }

    // Widths: header vs every cell, per column (index column first).
    let mut widths: Vec<usize> = Vec::with_capacity(columns.len() + 1);
    widths.push(index_header.chars().count());
    for col in &columns {
        widths.push(col.chars().count());
    }
    for line in &cells {
        for (i, cell) in line.iter().enumerate() {
            let w = cell.chars().count();
            if w > widths[i] {
                widths[i] = w;
            }
        }
    }

    let rule = |left: char, mid: char, right: char| -> String {
        let mut s = String::new();
        s.push(left);
        for (i, w) in widths.iter().enumerate() {
            if i > 0 {
                s.push(mid);
            }
            for _ in 0..w + 2 {
                s.push('─');
            }
        }
        s.push(right);
        s.push('\n');
        s
    };
    let data_line = |line: &[String]| -> String {
        let mut s = String::new();
        s.push('│');
        for (i, cell) in line.iter().enumerate() {
            s.push(' ');
            s.push_str(cell);
            for _ in 0..widths[i].saturating_sub(cell.chars().count()) {
                s.push(' ');
            }
            s.push(' ');
            s.push('│');
        }
        s.push('\n');
        s
    };

    let mut out = String::new();
    out.push_str(&rule('┌', '┬', '┐'));
    let mut header: Vec<String> = vec![index_header.to_string()];
    header.extend(columns.iter().cloned());
    out.push_str(&data_line(&header));
    out.push_str(&rule('├', '┼', '┤'));
    for line in &cells {
        out.push_str(&data_line(line));
    }
    out.push_str(&rule('└', '┴', '┘'));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rows(v: Value) -> Vec<serde_json::Map<String, Value>> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_object().unwrap().clone())
            .collect()
    }

    #[test]
    fn matches_probed_node_output() {
        // node -e 'console.table([{a:1,b:"x"},{a:22,b:null}])' piped:
        let out = console_table(&rows(json!([{"a": 1, "b": "x"}, {"a": 22, "b": null}])));
        let expected = "\
┌─────────┬────┬──────┐
│ (index) │ a  │ b    │
├─────────┼────┼──────┤
│ 0       │ 1  │ 'x'  │
│ 1       │ 22 │ null │
└─────────┴────┴──────┘
";
        assert_eq!(out, expected);
    }

    #[test]
    fn union_columns_and_absent_cells() {
        // node: console.table([{a:1},{b:2}]) — absent keys render empty.
        let out = console_table(&rows(json!([{"a": 1}, {"b": 2}])));
        let expected = "\
┌─────────┬───┬───┐
│ (index) │ a │ b │
├─────────┼───┼───┤
│ 0       │ 1 │   │
│ 1       │   │ 2 │
└─────────┴───┴───┘
";
        assert_eq!(out, expected);
    }
}
