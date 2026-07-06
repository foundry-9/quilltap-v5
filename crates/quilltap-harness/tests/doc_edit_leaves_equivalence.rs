//! Tier-1 differential (W4.1d batch 3a): the doc-edit pure leaves — exact
//! field-by-field equivalence vs v4's REAL exports from
//! lib/doc-edit/{diacritics, mime-registry, unified-diff, markdown-parser}.ts.
//!
//! Covers: NFD diacritics normalize + match (index/length UTF-16 remap), the MIME
//! registry (detect / predicates / parse / serialize / validate), the hand-rolled
//! unified diff, and the markdown heading ops + `serializeFrontmatter` /
//! `updateFrontmatterInContent`. The V8 `JSON.parse` message text is a documented
//! normalized seam (failure messages → `<ERR>` on both sides; structure + values
//! compared exactly). `findHeadingSection`'s thrown messages ARE reproducible and
//! compared byte-for-byte.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   LOG_LEVEL=error npx tsx \
//!     ~/source/quilltap-v5/harness/oracle/cases/doc-edit-leaves.ts \
//!     > /tmp/oracle-doc-edit-leaves.ndjson
//! Run:
//!   QT_ORACLE_DOC_EDIT_LEAVES=/tmp/oracle-doc-edit-leaves.ndjson \
//!     cargo test -p quilltap-harness --test doc_edit_leaves_equivalence

use quilltap_core::doc_edit::diacritics::{
    find_all_matches, find_unique_match, normalize_diacritics, DiacriticsMatchOptions, UniqueMatch,
};
use quilltap_core::doc_edit::line_diff::{changed_block_indices, diff_lines, LineOp, LineOpType};
use quilltap_core::doc_edit::markdown_parser::{
    find_heading_section, parse_heading_tree, read_heading_content, replace_heading_content,
    serialize_frontmatter, slugify_heading, update_frontmatter_in_content, HeadingInfo,
};
use quilltap_core::doc_edit::mime_registry::{
    detect_mime_from_extension, is_json_family, is_json_mime, is_jsonl_mime, parse_content,
    serialize_content, validate_json, ParseResult,
};
use quilltap_core::doc_edit::unified_diff::{format_autosave_notification, generate_unified_diff};
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Row {
    kind: String,
    id: String,
    #[serde(flatten)]
    rest: Value,
}

fn opts(norm: bool, cs: bool) -> DiacriticsMatchOptions {
    DiacriticsMatchOptions {
        normalize_diacritics: norm,
        case_sensitive: cs,
    }
}

fn matches_json(m: &[(usize, usize)]) -> Value {
    Value::Array(
        m.iter()
            .map(|(i, l)| json!({ "index": i, "length": l }))
            .collect(),
    )
}

fn unique_json(u: UniqueMatch) -> Value {
    match u {
        UniqueMatch::Found { index, length } => {
            json!({ "found": true, "index": index, "length": length })
        }
        UniqueMatch::NotFound { count } => json!({ "found": false, "count": count }),
    }
}

fn heading_json(h: &HeadingInfo) -> Value {
    json!({
        "text": h.text,
        "level": h.level,
        "slug": h.slug,
        "line": h.line,
        "offset": h.offset,
        "contentStart": h.content_start,
        "contentEnd": h.content_end,
    })
}

/// Mirror the oracle's `normResult`: failure → `{ok:false}`; JSONL-line arrays get
/// per-object `error` text replaced with `<ERR>`; other Ok values pass through.
fn norm_result(r: &ParseResult) -> Value {
    match r {
        ParseResult::Err { .. } => json!({ "ok": false }),
        ParseResult::Ok(v) => {
            let value = if let Value::Array(items) = v {
                Value::Array(
                    items
                        .iter()
                        .map(|item| match item {
                            Value::Object(m) if m.contains_key("error") => {
                                let mut m2 = m.clone();
                                m2.insert("error".into(), Value::from("<ERR>"));
                                Value::Object(m2)
                            }
                            other => other.clone(),
                        })
                        .collect(),
                )
            } else {
                v.clone()
            };
            json!({ "ok": true, "value": value })
        }
    }
}

fn expect_mime(mime: &str) -> &str {
    mime
}

#[test]
fn doc_edit_leaves_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_DOC_EDIT_LEAVES") else {
        eprintln!("SKIP: set QT_ORACLE_DOC_EDIT_LEAVES to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    // The heading corpus doc + the replace-heading doc must mirror the oracle.
    let doc = "# Title\nintro text\n## Section A\naaa\n### Sub A1\nsub body\n## Section A\nbbb\n```\n# not a heading\n```\n# Title\ntail\n";
    let simple = "# A\nold body\n## B\nkeep\n";

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("oracle line parses");
        let want = |k: &str| row.rest.get(k).cloned().unwrap_or(Value::Null);
        let id = row.id.as_str();

        match row.kind.as_str() {
            "diacritics-normalize" => {
                let input = diacritics_input_for(id);
                assert_eq!(
                    json!(normalize_diacritics(input)),
                    want("result"),
                    "normalize {id}"
                );
            }
            "diacritics-match" => {
                let (h, n, norm, cs) = diacritics_match_for(id);
                let o = opts(norm, cs);
                assert_eq!(
                    matches_json(&find_all_matches(h, n, o)),
                    want("all"),
                    "findAll {id}"
                );
                assert_eq!(
                    unique_json(find_unique_match(h, n, o)),
                    want("unique"),
                    "findUnique {id}"
                );
            }
            "mime-detect" => {
                let p = mime_detect_for(id);
                let got = detect_mime_from_extension(p)
                    .map(Value::from)
                    .unwrap_or(Value::Null);
                assert_eq!(got, want("result"), "detectMime {id}");
            }
            "mime-predicate" => {
                let m = mime_predicate_for(id);
                assert_eq!(json!(is_json_mime(Some(m))), want("isJson"), "isJson {id}");
                assert_eq!(
                    json!(is_jsonl_mime(Some(m))),
                    want("isJsonl"),
                    "isJsonl {id}"
                );
                assert_eq!(
                    json!(is_json_family(Some(m))),
                    want("isFamily"),
                    "isFamily {id}"
                );
            }
            "mime-parse" => {
                let (content, mime) = mime_parse_for(id);
                assert_eq!(
                    norm_result(&parse_content(content, mime)),
                    want("result"),
                    "parse {id}"
                );
            }
            "mime-serialize" => {
                let got = mime_serialize_for(id);
                assert_eq!(norm_result(&got), want("result"), "serialize {id}");
            }
            "mime-validate" => {
                let (content, mime) = mime_validate_for(id);
                assert_eq!(
                    norm_result(&validate_json(content, mime)),
                    want("result"),
                    "validate {id}"
                );
            }
            "diff" => {
                let (o, n) = diff_for(id);
                assert_eq!(
                    json!(generate_unified_diff(&o, &n, "f.md")),
                    want("diff"),
                    "diff {id}"
                );
                let notify = format_autosave_notification(&o, &n, "f.md")
                    .map(Value::from)
                    .unwrap_or(Value::Null);
                assert_eq!(notify, want("notify"), "notify {id}");
            }
            "diff-lines" => {
                let (old_l, new_l) = diff_lines_for(id);
                let old_ref: Vec<&str> = old_l.iter().map(String::as_str).collect();
                let new_ref: Vec<&str> = new_l.iter().map(String::as_str).collect();
                let got = line_ops_json(&diff_lines(&old_ref, &new_ref));
                assert_eq!(got, want("result"), "diff-lines {id}");
            }
            "changed-blocks" => {
                let (base, cur) = diff_lines_for(id);
                let base_ref: Vec<&str> = base.iter().map(String::as_str).collect();
                let cur_ref: Vec<&str> = cur.iter().map(String::as_str).collect();
                let got: Value = Value::Array(
                    changed_block_indices(&base_ref, &cur_ref)
                        .into_iter()
                        .map(|i| json!(i))
                        .collect(),
                );
                assert_eq!(got, want("result"), "changed-blocks {id}");
            }
            "slug" => {
                let t = slug_for(id);
                assert_eq!(json!(slugify_heading(t)), want("result"), "slug {id}");
            }
            "heading-tree" => {
                let got: Value =
                    Value::Array(parse_heading_tree(doc).iter().map(heading_json).collect());
                assert_eq!(got, want("result"), "heading-tree {id}");
            }
            "find-heading" => {
                let (t, lvl) = find_heading_for(id);
                let got = match find_heading_section(doc, t, lvl) {
                    Ok(h) => {
                        json!({ "ok": true, "heading": heading_json(&h), "read": read_heading_content(doc, &h) })
                    }
                    Err(e) => json!({ "ok": false, "message": e.0 }),
                };
                assert_eq!(got, want("result"), "find-heading {id}");
            }
            "replace-heading" => {
                let h = find_heading_section(simple, "A", None).unwrap();
                let preserve = id == "preserve";
                let got = replace_heading_content(simple, &h, "NEW\n", preserve);
                assert_eq!(json!(got), want("result"), "replace-heading {id}");
            }
            "fm-serialize" => {
                let data = fm_serialize_for(id);
                assert_eq!(
                    json!(serialize_frontmatter(&data)),
                    want("result"),
                    "fm-serialize {id}"
                );
            }
            "fm-update" => {
                let (content, updates, replace_all) = fm_update_for(id);
                let got = update_frontmatter_in_content(content, &updates, replace_all);
                assert_eq!(json!(got), want("result"), "fm-update {id}");
            }
            other => panic!("unknown kind '{other}'"),
        }
        count += 1;
    }
    assert!(count >= 100, "expected the full corpus, saw {count}");
    eprintln!("doc_edit_leaves: {count} rows matched the oracle.");
}

/// Serialize a line-op script to the oracle's `[{type, line}]` shape.
fn line_ops_json(ops: &[LineOp]) -> Value {
    Value::Array(
        ops.iter()
            .map(|o| {
                let t = match o.op_type {
                    LineOpType::Equal => "equal",
                    LineOpType::Del => "del",
                    LineOpType::Ins => "ins",
                };
                json!({ "type": t, "line": o.line })
            })
            .collect(),
    )
}

// ---- corpus mirrors (MUST match harness/oracle/cases/doc-edit-leaves.ts) ----

fn diacritics_input_for(id: &str) -> &'static str {
    match id {
        "nfd-accent" => "Nimuë",
        "nfd-cafe" => "café",
        "nfd-plain" => "plain ascii",
        "nfd-hangul" => "가나다",
        "nfd-multi" => "Zoë Śląsk Ñoño",
        "nfd-empty" => "",
        o => panic!("unknown diacritics-normalize id {o}"),
    }
}

fn diacritics_match_for(id: &str) -> (&'static str, &'static str, bool, bool) {
    match id {
        "m-base-vs-accent" => ("say Nimuë now", "Nimue", true, true),
        "m-accent-vs-base" => ("Nimue speaks", "Nimuë", true, true),
        "m-multiple" => ("aXaXa", "a", true, true),
        "m-ci" => ("HELLO hello", "hello", true, false),
        "m-no-normalize" => ("café cafe", "café", false, true),
        "m-not-found" => ("abc", "xyz", true, true),
        "m-empty-needle" => ("abc", "", true, true),
        "m-cafe-ci-norm" => ("CAFÉ café", "cafe", true, false),
        o => panic!("unknown diacritics-match id {o}"),
    }
}

fn mime_detect_for(id: &str) -> &'static str {
    match id {
        "ext-json" => "a.json",
        "ext-jsonl" => "a.JSONL",
        "ext-ndjson" => "x.ndjson",
        "ext-md" => "dir/b.md",
        "ext-markdown" => "c.markdown",
        "ext-txt" => "d.TXT",
        "ext-yaml" => "e.yaml",
        "ext-yml" => "f.yml",
        "ext-none" => "noext",
        "ext-dotfile" => ".gitignore",
        "ext-unknown" => "g.xml",
        o => panic!("unknown mime-detect id {o}"),
    }
}

fn mime_predicate_for(id: &str) -> &'static str {
    match id {
        "is-appjson" => "application/json",
        "is-ndjson" => "application/x-ndjson",
        "is-md" => "text/markdown",
        o => panic!("unknown mime-predicate id {o}"),
    }
}

fn mime_parse_for(id: &str) -> (&'static str, &'static str) {
    match id {
        "parse-json-ok" => ("{\"a\":1,\"b\":[2,3]}", "application/json"),
        "parse-json-empty-obj" => ("{}", "application/json"),
        "parse-json-array" => ("[1,2,3]", "application/json"),
        "parse-json-bad" => ("{bad", "application/json"),
        "parse-jsonl-ok" => ("{\"a\":1}\n{\"b\":2}\n", "application/x-ndjson"),
        "parse-jsonl-blank-lines" => ("{\"a\":1}\n\n  \n{\"b\":2}", "application/x-ndjson"),
        "parse-jsonl-mixed" => ("{\"a\":1}\nnotjson\n{\"c\":3}", "application/x-ndjson"),
        "parse-md-unsupported" => ("text", "text/markdown"),
        o => panic!("unknown mime-parse id {o}"),
    }
}

fn mime_serialize_for(id: &str) -> ParseResult {
    let j = "application/json";
    let l = "application/x-ndjson";
    match id {
        "ser-json-obj" => serialize_content(&json!({"a":1,"b":"two"}), expect_mime(j), true),
        "ser-json-compact" => serialize_content(&json!({"a":1}), j, false),
        "ser-json-nested" => serialize_content(&json!({"a":{"b":[1,2]},"c":"x"}), j, true),
        "ser-jsonl-arr" => serialize_content(&json!([{"a":1},{"b":2}]), l, true),
        "ser-jsonl-empty" => serialize_content(&json!([]), l, true),
        "ser-jsonl-notarray" => serialize_content(&json!({"a":1}), l, true),
        o => panic!("unknown mime-serialize id {o}"),
    }
}

fn mime_validate_for(id: &str) -> (&'static str, &'static str) {
    match id {
        "val-json-ok" => ("{\"a\":1}", "application/json"),
        "val-json-bad" => ("{bad", "application/json"),
        "val-jsonl-ok" => ("{\"a\":1}\n{\"b\":2}", "application/x-ndjson"),
        "val-jsonl-bad" => ("{\"a\":1}\nnope", "application/x-ndjson"),
        o => panic!("unknown mime-validate id {o}"),
    }
}

// ---- unified-diff corpus mirrors (MUST match the generated inputs in the oracle) ----

/// `numbered(count)` = `["line1", .., "line{count}"]` (mirrors the oracle helper).
fn numbered(count: usize) -> Vec<String> {
    (1..=count).map(|i| format!("line{i}")).collect()
}

/// Apply `[(index, value)]` edits to a copy of `base`.
fn with_change(base: &[String], edits: &[(usize, &str)]) -> Vec<String> {
    let mut out = base.to_vec();
    for (i, v) in edits {
        out[*i] = (*v).to_string();
    }
    out
}

fn diff_for(id: &str) -> (String, String) {
    let lit = |a: &str, b: &str| (a.to_string(), b.to_string());
    match id {
        // Legacy corpus (re-generated under the new Myers/hunk algorithm).
        "diff-identical" => lit("a\nb\nc\n", "a\nb\nc\n"),
        "diff-one-line" => lit("a\nb\nc\n", "a\nB\nc\n"),
        "diff-add" => lit("a\nc\n", "a\nb\nc\n"),
        "diff-remove" => lit("a\nb\nc\n", "a\nc\n"),
        "diff-append" => lit("a\n", "a\nb\nc\n"),
        "diff-multi-hunk" => lit("a\nb\nc\nd\ne\n", "a\nB\nc\nd\nE\n"),
        "diff-full-replace" => lit("x\ny\n", "p\nq\n"),
        "diff-empty-old" => lit("", "new\n"),
        // New behavior (v4 8617ce7a): Myers + git-style hunks.
        "diff-both-empty" => lit("", ""),
        "diff-shifted-insert" => lit("a\nb\nc", "a\nNEW\nb\nc"),
        "diff-single-line-file" => lit("only", "changed"),
        "diff-create-from-empty" => lit("", "hello\nworld"),
        "diff-empty-from-content" => lit("a\nb", ""),
        "diff-context-start" => lit("A\nb\nc\nd\ne", "X\nb\nc\nd\ne"),
        "diff-context-end" => lit("a\nb\nc\nd\nE", "a\nb\nc\nd\nZ"),
        "diff-unicode" => lit("café\nNimuë\n世界", "café\nNimué\n世界"),
        "diff-context-cap" => (
            numbered(12).join("\n"),
            with_change(&numbered(12), &[(6, "CHANGED")]).join("\n"),
        ),
        "diff-distant" => (
            numbered(20).join("\n"),
            with_change(&numbered(20), &[(1, "TOP"), (18, "BOTTOM")]).join("\n"),
        ),
        "diff-coalesce" => (
            numbered(10).join("\n"),
            with_change(&numbered(10), &[(3, "A"), (5, "B")]).join("\n"),
        ),
        "diff-huge-fallback" => (vec!["x"; 5001].join("\n"), vec!["y"; 5000].join("\n")),
        o => panic!("unknown diff id {o}"),
    }
}

/// Line-array inputs for both `diff-lines` and `changed-blocks` rows.
fn diff_lines_for(id: &str) -> (Vec<String>, Vec<String>) {
    let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match id {
        // diffLines
        "dl-identical" => (v(&["a", "b"]), v(&["a", "b"])),
        "dl-insert" => (v(&["a", "b", "c"]), v(&["a", "NEW", "b", "c"])),
        "dl-modify" => (v(&["a", "b", "c"]), v(&["a", "B", "c"])),
        "dl-replace-all" => (v(&["a", "b"]), v(&["x", "y"])),
        "dl-empty-old" => (v(&[]), v(&["a", "b"])),
        "dl-empty-new" => (v(&["a", "b"]), v(&[])),
        "dl-both-empty" => (v(&[]), v(&[])),
        // changedBlockIndices
        "cb-unchanged" => (v(&["a", "b", "c"]), v(&["a", "b", "c"])),
        "cb-insert-top" => (
            v(&["intro", "section one", "body one"]),
            v(&["intro", "NEW SECTION", "section one", "body one"]),
        ),
        "cb-modify" => (v(&["a", "b", "c"]), v(&["a", "B", "c"])),
        "cb-pure-delete" => (v(&["a", "b", "c"]), v(&["a", "c"])),
        "cb-replace-all" => (v(&["a", "b"]), v(&["x", "y"])),
        "cb-empty-doc" => (v(&[]), v(&["a", "b"])),
        o => panic!("unknown line-pair id {o}"),
    }
}

fn slug_for(id: &str) -> &'static str {
    match id {
        "slug-basic" => "Character Backstory",
        "slug-punct" => "Hello, World! (v2)",
        "slug-dashes" => "A -- B",
        "slug-trim" => "  Spaced Out  ",
        "slug-underscore" => "snake_case name",
        "slug-only-punct" => "!!!",
        "slug-numbers" => "Chapter 3.1",
        o => panic!("unknown slug id {o}"),
    }
}

fn find_heading_for(id: &str) -> (&'static str, Option<usize>) {
    match id {
        "find-title" => ("title", None),
        "find-section-dup" => ("section a", None),
        "find-sub" => ("Sub A1", Some(3)),
        "find-missing" => ("nope", None),
        "find-wrong-level" => ("Sub A1", Some(2)),
        o => panic!("unknown find-heading id {o}"),
    }
}

fn obj(pairs: &[(&str, Value)]) -> Map<String, Value> {
    let mut m = Map::new();
    for (k, v) in pairs {
        m.insert((*k).to_string(), v.clone());
    }
    m
}

fn fm_serialize_for(id: &str) -> Map<String, Value> {
    match id {
        "fm-strings" => obj(&[("title", json!("My Doc")), ("author", json!("Ada"))]),
        "fm-mixed" => obj(&[
            ("title", json!("Doc")),
            ("count", json!(3)),
            ("active", json!(true)),
            ("embed", json!(false)),
            ("note", json!(null)),
        ]),
        "fm-colon" => obj(&[("summary", json!("has: a colon"))]),
        "fm-array" => obj(&[("tags", json!(["a", "b", "c"]))]),
        "fm-num-array" => obj(&[("nums", json!([1, 2, 3]))]),
        "fm-empty" => Map::new(),
        "fm-policy" => obj(&[("character_read", json!("no")), ("embed", json!(false))]),
        "fm-quoted-word" => obj(&[("flag", json!("true"))]),
        o => panic!("unknown fm-serialize id {o}"),
    }
}

fn fm_update_for(id: &str) -> (&'static str, Map<String, Value>, bool) {
    match id {
        "upd-merge" => (
            "---\ntitle: Old\nkeep: yes\n---\nbody here\n",
            obj(&[("title", json!("New"))]),
            false,
        ),
        "upd-delete" => (
            "---\ntitle: Doc\ndrop: gone\n---\nbody\n",
            obj(&[("drop", json!(null))]),
            false,
        ),
        "upd-add-new" => (
            "no frontmatter body\n",
            obj(&[("title", json!("Added"))]),
            false,
        ),
        "upd-replace" => (
            "---\na: 1\nb: 2\n---\nbody\n",
            obj(&[("only", json!("this"))]),
            true,
        ),
        o => panic!("unknown fm-update id {o}"),
    }
}
