//! **P4.102 — the shared-decoder class closed for the tri-state REST edges.**
//!
//! Two deliverables in one file, because they are two views of the same
//! claim: `crate::request_envelope::request_envelope` (P4.98's
//! `images_generate_request`, generalized) is the ONLY way a
//! `crates/quilltap-web/src/*_routes.rs` edge may construct a `Request`
//! variant that carries an `Option<Option<Value>>` field.
//!
//! 1. **Decode-identity pins** — for every tri-state REST edge this port
//!    owns (`characterSubpromptCreate`/`Update`, `promptTemplateCreate`/
//!    `Update`), and for every tri-state key on it, the edge's decode and
//!    `POST /api/dispatch`'s decode of the SAME wire bytes must be the exact
//!    same `Request`, for all three states — absent / explicit `null` /
//!    value. Because both sides now call the same function, this is a
//!    regression pin rather than a coincidence check (the P4.98 shape;
//!    `images_generate_decoder_tests` in `images_routes.rs` is the
//!    precedent this file generalizes across the two newly-converted files).
//!    A distinctness test rides beside it: the decode-identity pin alone
//!    cannot catch a helper that silently maps two of the three states
//!    together (`a-decode-raw-test-cannot-catch-a-wrong-decode` — P4.98's
//!    own mutation M1 found exactly that class survive a weaker pin).
//!
//! 2. **The census guard** — mechanically derives, from
//!    `crates/quilltap-core/src/api/types.rs`, every `Request` variant
//!    carrying an `Option<Option<…>>` field (parsing the enum body
//!    brace-balanced, robust to the one multi-line `#[serde(...)]` attribute
//!    in the file — `ChatUpdate.concierge_state` — which
//!    `dispatch_wrong_type_census.rs`'s line-prefix stripper cannot survive;
//!    see `dispatch-census-strip-noise-and-multi-line-serde-attrs`), then
//!    scans every `crates/quilltap-web/src/*_routes.rs` file for a
//!    `CoreRequest::<V> { ` / `Request::<V> { ` construction of one of those
//!    variants. **The set is not quite empty.**
//!
//!    `characters_routes.rs:596`'s `answer_body_failure` builds a
//!    `Request::CharacterRename { character_id, primary_rename: None,
//!    additional_replacements: None, dry_run: None }` — but every tri-state
//!    field is a COMPILE-TIME LITERAL `None`, never read from a parsed body.
//!    It is not a second spelling of the presence/null/value decode (the
//!    defect class this helper exists to close); it is a fixed existence
//!    probe used only to distinguish v4's 404-before-body-validation
//!    ordering. `characters_routes.rs` is outside this order's ownership (a
//!    different lane's file this round — see the order's Ownership table),
//!    so it cannot be converted here; it is recorded as the one named,
//!    pinned exception below rather than silently excluded or left to redden
//!    the guard. Any OTHER hit — including a change to this probe that
//!    starts carrying a real value — moves the allow-list and must be
//!    argued for by name, exactly as a brand new `tri()` would be (mutation
//!    **M2**: reintroducing a hand-rolled `tri()` on one of THIS order's two
//!    edges adds an unlisted hit and reddens
//!    [`no_new_tri_state_variant_is_hand_built_outside_the_helper`]).
//!
//!    The remaining hits are constructions of TYPED-FIELD-ONLY variants —
//!    a different, already-adjudicated class (P4.60/P4.62's
//!    `web_edge_body_parse_guard.rs`, over in `quilltap-harness`, holds
//!    their wrong-type verdicts). They are enumerated, not converted, as a
//!    per-file count in [`TYPED_ONLY_HAND_BUILT_CONSTRUCTIONS_BY_FILE`] — the
//!    P4.98 Tier-3 enumeration deliverable.
//!
//! Run standalone:
//!   cargo test -p quilltap-web --test tri_state_edges_share_the_decoder

use serde_json::{json, Value};

use quilltap_core::api::Request as CoreRequest;
use quilltap_web::request_envelope::request_envelope;

// ===========================================================================
// Part 1 — decode-identity pins
// ===========================================================================

/// For one edge (`kind` + its full `body_keys`, plus any URL-sourced
/// `path_fields`) and one `varying_key` on it: for every one of the three
/// tri-state states, the edge's decode of a body carrying only that key must
/// equal what `POST /api/dispatch` decodes from the exact same wire bytes
/// (the envelope's `type` plus the path fields plus that one body key).
fn assert_edge_matches_dispatch(
    kind: &str,
    body_keys: &[&str],
    path_fields: &[(&str, Value)],
    varying_key: &str,
) {
    for state in [
        None,                              // absent
        Some(Value::Null),                 // explicit null
        Some(json!("x")),                  // a value
        Some(json!({ "nested": [1, 2] })), // a structured value
    ] {
        let mut body = serde_json::Map::new();
        if let Some(v) = state.clone() {
            body.insert(varying_key.to_string(), v);
        }
        let parsed = Value::Object(body.clone());
        let edge = request_envelope(kind, &parsed, body_keys, path_fields)
            .unwrap_or_else(|| panic!("the edge failed to decode {kind} {varying_key}={state:?}"));

        // What dispatch would decode from the very same keys.
        let mut envelope = body;
        envelope.insert("type".into(), Value::String(kind.to_string()));
        for (k, v) in path_fields {
            envelope.insert((*k).to_string(), v.clone());
        }
        let wire = serde_json::to_vec(&Value::Object(envelope)).unwrap();
        let dispatched = serde_json::from_slice::<CoreRequest>(&wire).unwrap_or_else(|e| {
            panic!("dispatch failed to decode {kind} {varying_key}={state:?}: {e}")
        });

        assert_eq!(
            edge, dispatched,
            "the REST edge and the dispatch decode disagree about `{varying_key}` \
             = {state:?} on `{kind}` — the tri-state has two spellings again"
        );
    }
}

/// The URL id wins over a body key that spells the same name. v4's route
/// schemas are `z.object`s over the body keys alone — a body `characterId`
/// is STRIPPED and the path param is what the handler reads — so the helper
/// must never let a body key redirect the write. Pinned with the path key
/// deliberately listed among `body_keys`, which is the only way the two can
/// collide (the `baa85e19b` round's §3 review: path fields used to be
/// inserted FIRST, so a same-named body key would have overwritten them).
#[test]
fn a_body_key_spelling_a_path_id_cannot_overwrite_the_url() {
    let parsed = json!({
        "characterId": "from-the-body",
        "subpromptId": "also-from-the-body",
        "title": "t",
    });
    let req = request_envelope(
        "characterSubpromptUpdate",
        &parsed,
        &["title", "content", "characterId", "subpromptId"],
        &[
            ("characterId", json!("from-the-url")),
            ("subpromptId", json!("url-subprompt")),
        ],
    )
    .expect("decodes");
    match req {
        CoreRequest::CharacterSubpromptUpdate {
            character_id,
            subprompt_id,
            title,
            ..
        } => {
            assert_eq!(character_id, "from-the-url");
            assert_eq!(subprompt_id, "url-subprompt");
            assert_eq!(title, Some(Some(json!("t"))));
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

const SUBPROMPT_KEYS: [&str; 2] = ["title", "content"];
const PROMPT_TEMPLATE_KEYS: [&str; 5] = ["name", "content", "description", "category", "modelHint"];

#[test]
fn subprompt_create_edge_and_dispatch_decode_identically() {
    let path_fields = [("characterId", json!("char-a"))];
    for key in SUBPROMPT_KEYS {
        assert_edge_matches_dispatch(
            "characterSubpromptCreate",
            &SUBPROMPT_KEYS,
            &path_fields,
            key,
        );
    }
}

#[test]
fn subprompt_update_edge_and_dispatch_decode_identically() {
    let path_fields = [
        ("characterId", json!("char-a")),
        ("subpromptId", json!("terse")),
    ];
    for key in SUBPROMPT_KEYS {
        assert_edge_matches_dispatch(
            "characterSubpromptUpdate",
            &SUBPROMPT_KEYS,
            &path_fields,
            key,
        );
    }
}

#[test]
fn prompt_template_create_edge_and_dispatch_decode_identically() {
    for key in PROMPT_TEMPLATE_KEYS {
        assert_edge_matches_dispatch("promptTemplateCreate", &PROMPT_TEMPLATE_KEYS, &[], key);
    }
}

#[test]
fn prompt_template_update_edge_and_dispatch_decode_identically() {
    let path_fields = [("id", json!("tpl-1"))];
    for key in PROMPT_TEMPLATE_KEYS {
        assert_edge_matches_dispatch(
            "promptTemplateUpdate",
            &PROMPT_TEMPLATE_KEYS,
            &path_fields,
            key,
        );
    }
}

/// The three states must stay DISTINGUISHABLE after the decode — the whole
/// property the decode-identity pins above assume rather than prove on their
/// own (`a-decode-raw-test-cannot-catch-a-wrong-decode`): a helper that
/// mapped two of the three together would still satisfy `assert_eq!(edge,
/// dispatched)` above if BOTH sides made the same mistake, so long as the
/// dispatch comparand were built the same wrong way — which it is not here
/// (it is built straight from the wire bytes), but the distinctness check
/// pins the property directly rather than through that indirection.
#[test]
fn absent_null_and_value_stay_three_distinct_requests_for_every_edge() {
    let of = |kind: &str, keys: &[&str], path: &[(&str, Value)], body: Value| {
        request_envelope(kind, &body, keys, path).expect("decodes")
    };

    let char_a = [("characterId", json!("char-a"))];
    for key in SUBPROMPT_KEYS {
        let other = if key == "title" { "content" } else { "title" };
        let absent = of(
            "characterSubpromptCreate",
            &SUBPROMPT_KEYS,
            &char_a,
            json!({ other: "x" }),
        );
        let null = of(
            "characterSubpromptCreate",
            &SUBPROMPT_KEYS,
            &char_a,
            json!({ other: "x", key: Value::Null }),
        );
        let value = of(
            "characterSubpromptCreate",
            &SUBPROMPT_KEYS,
            &char_a,
            json!({ other: "x", key: "v" }),
        );
        assert_ne!(
            absent, null,
            "characterSubpromptCreate.{key}: absent == null"
        );
        assert_ne!(
            absent, value,
            "characterSubpromptCreate.{key}: absent == value"
        );
        assert_ne!(null, value, "characterSubpromptCreate.{key}: null == value");
    }

    for key in PROMPT_TEMPLATE_KEYS {
        let absent = of(
            "promptTemplateCreate",
            &PROMPT_TEMPLATE_KEYS,
            &[],
            json!({}),
        );
        let null = of(
            "promptTemplateCreate",
            &PROMPT_TEMPLATE_KEYS,
            &[],
            json!({ key: Value::Null }),
        );
        let value = of(
            "promptTemplateCreate",
            &PROMPT_TEMPLATE_KEYS,
            &[],
            json!({ key: "v" }),
        );
        assert_ne!(absent, null, "promptTemplateCreate.{key}: absent == null");
        assert_ne!(absent, value, "promptTemplateCreate.{key}: absent == value");
        assert_ne!(null, value, "promptTemplateCreate.{key}: null == value");
    }
}

/// v4's route schemas are `z.object`, which STRIPS undeclared keys rather
/// than refusing them, and a body that parses but is not an object still
/// reaches v4's Zod parse (which the HANDLER refuses) — so both fold to
/// all-absent at the decoder, for every edge.
#[test]
fn unknown_keys_are_stripped_and_a_non_object_folds_to_absent() {
    let char_a = [("characterId", json!("char-a"))];
    assert_eq!(
        request_envelope(
            "characterSubpromptCreate",
            &json!({ "content": "x", "bogus": 1, "type": "nope" }),
            &SUBPROMPT_KEYS,
            &char_a,
        ),
        request_envelope(
            "characterSubpromptCreate",
            &json!({ "content": "x" }),
            &SUBPROMPT_KEYS,
            &char_a,
        ),
    );
    for non_object in [json!([1, 2, 3]), json!("nope"), json!(7), Value::Null] {
        assert_eq!(
            request_envelope(
                "characterSubpromptCreate",
                &non_object,
                &SUBPROMPT_KEYS,
                &char_a,
            ),
            request_envelope(
                "characterSubpromptCreate",
                &json!({}),
                &SUBPROMPT_KEYS,
                &char_a,
            ),
            "a non-object body must fold to all-absent: {non_object}"
        );
    }
}

// ===========================================================================
// Part 2 — the census guard
// ===========================================================================

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the web crate sits two levels under the repo root")
        .to_path_buf()
}

fn types_rs() -> String {
    let p = repo_root().join("crates/quilltap-core/src/api/types.rs");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Remove `//` line comments and bracket-balanced `#[...]` attribute spans.
///
/// `dispatch_wrong_type_census.rs`'s `strip_noise` drops only a LINE that
/// starts with `#[`, which leaves a continuation line's text (and its stray
/// top-level commas) in the stream for a multi-line attribute — hit at
/// P4.D163 (`dispatch-census-strip-noise-and-multi-line-serde-attrs`).
/// `types.rs` has exactly ONE such attribute today
/// (`ChatUpdate.concierge_state`'s `#[serde(\n  default,\n  …\n)]`), and this
/// census's whole job is finding `Option<Option<` fields, so it cannot use
/// that stripper. Operating on `char`s (not bytes) keeps this UTF-8 safe.
fn strip_comments_and_attrs(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if chars[i] == '#' && chars.get(i + 1) == Some(&'[') {
            let mut depth = 0i32;
            while i < chars.len() {
                match chars[i] {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        i += 1;
                        if depth == 0 {
                            break;
                        }
                        continue;
                    }
                    _ => {}
                }
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Split a body on top-level commas, counting only `{`/`}` — the
/// `dispatch_wrong_type_census.rs` `split_top_level(body, "{", "}")` shape,
/// sufficient once attributes (which could otherwise unbalance on a stray
/// `[`) are already stripped.
fn split_top_level_braces(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in body.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if ch == ',' && depth == 0 {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    out.push(cur);
    out
}

/// Every `Request` variant whose body carries an `Option<Option<…>>` field —
/// mechanically, from `api/types.rs`'s enum body.
fn tri_state_variants() -> Vec<String> {
    let src = strip_comments_and_attrs(&types_rs());
    let start = src.find("pub enum Request").expect("the Request enum");
    let open = src[start..].find('{').expect("enum body") + start;
    let mut depth = 0i32;
    let mut end = open;
    for (i, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &src[open + 1..end];
    let mut out = Vec::new();
    for variant in split_top_level_braces(body) {
        let v = variant.trim();
        if v.is_empty() {
            continue;
        }
        let head = v.split('{').next().unwrap_or("").trim();
        let Some(name) = head.trim_end_matches(',').split_whitespace().next_back() else {
            continue;
        };
        // Collapse whitespace so a field type wrapped across a line (rustfmt
        // never does this for `Option<Option<…>>`, but nothing here assumes
        // it won't) still matches as one string.
        let collapsed: String = v.split_whitespace().collect::<Vec<_>>().join("");
        if collapsed.contains("Option<Option<") {
            out.push(name.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// A floor so a parser that silently found nothing cannot pass.
#[test]
fn tri_state_variants_parser_finds_a_plausible_set() {
    let variants = tri_state_variants();
    assert!(
        variants.len() >= 20,
        "the mechanical walk found only {} tri-state variants — the parser broke \
         (or types.rs genuinely shrank; re-measure): {variants:?}",
        variants.len()
    );
    for must_have in [
        "CharacterSubpromptCreate",
        "CharacterSubpromptUpdate",
        "PromptTemplateCreate",
        "PromptTemplateUpdate",
        "ImagesGenerate",
        "ImageProfileGenerate",
        "CharacterRename",
        "ChatUpdate",
    ] {
        assert!(
            variants.iter().any(|v| v == must_have),
            "{must_have} should carry an Option<Option<…>> field — the parser missed it: \
             {variants:?}"
        );
    }
}

/// Every `(file, variant, line)` where a `crates/quilltap-web/src/*_routes.rs`
/// file constructs `CoreRequest::<variant> {` or `Request::<variant> {`
/// (either spelling — `"Request::"` is a substring of `"CoreRequest::"`, so
/// one search catches both; nothing else named `*Request` is constructed
/// this way anywhere in this glob today, measured).
fn all_variant_constructions() -> Vec<(String, String, usize)> {
    let dir = repo_root().join("crates/quilltap-web/src");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with("_routes.rs"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();

    let mut out = Vec::new();
    for path in paths {
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let rel = path
            .strip_prefix(repo_root())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        for (line_no, line) in src.lines().enumerate() {
            let mut rest = line;
            while let Some(pos) = rest.find("Request::") {
                let after = &rest[pos + "Request::".len()..];
                let variant: String = after
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                let tail = &after[variant.len()..];
                if !variant.is_empty() && tail.trim_start().starts_with('{') {
                    out.push((rel.clone(), variant.clone(), line_no + 1));
                }
                rest = after;
            }
        }
    }
    out
}

/// The named, measured, pinned exception to "the helper is the only
/// constructor" — see the module doc's discussion of
/// `characters_routes.rs:596`. `(file, variant)`; the line number is NOT
/// pinned here so a harmless shift inside `characters_routes.rs` (a lane
/// this order does not own) cannot redden this test — only a NEW or MOVED
/// hit (a different file, or a different variant) can.
const ALLOWED_TRI_STATE_HAND_BUILDS: &[(&str, &str)] = &[(
    "crates/quilltap-web/src/characters_routes.rs",
    "CharacterRename",
)];

/// **Tier 1 — the class held shut.** Every tri-state `Request` variant
/// construction across `*_routes.rs` must be either the ONE named exception
/// above or absent entirely. Before this port, `subprompts_routes.rs` and
/// `prompt_templates_routes.rs` built `CharacterSubpromptCreate`/`Update` and
/// `PromptTemplateCreate`/`Update` by hand (through the retired `tri()`
/// helpers) — the RED-FIRST state this guard is the instrument for (§R.5).
/// **Mutation M2:** reintroduce a hand-rolled `tri()` on either edge (or
/// construct one of these variants anywhere else by hand) and this test
/// reds on the new, un-allow-listed hit.
#[test]
fn no_new_tri_state_variant_is_hand_built_outside_the_helper() {
    let tri_state: std::collections::HashSet<String> = tri_state_variants().into_iter().collect();
    let hits: Vec<(String, String, usize)> = all_variant_constructions()
        .into_iter()
        .filter(|(_, variant, _)| tri_state.contains(variant))
        .collect();

    let unexpected: Vec<_> = hits
        .iter()
        .filter(|(file, variant, _)| {
            !ALLOWED_TRI_STATE_HAND_BUILDS
                .iter()
                .any(|(af, av)| af == file && av == variant)
        })
        .collect();

    assert!(
        unexpected.is_empty(),
        "a `*_routes.rs` file hand-builds a tri-state `Request` variant outside \
         `request_envelope` — either route it through the helper or, if it is a \
         fixed-fields probe like the one exception, add it to \
         ALLOWED_TRI_STATE_HAND_BUILDS with the same justification: {unexpected:#?}"
    );

    // The allow-list itself must still be a REAL hit (a stale entry for a
    // probe that got converted, or renamed, would silently widen the guard).
    for (allowed_file, allowed_variant) in ALLOWED_TRI_STATE_HAND_BUILDS {
        assert!(
            hits.iter()
                .any(|(f, v, _)| f == allowed_file && v == allowed_variant),
            "ALLOWED_TRI_STATE_HAND_BUILDS names {allowed_file}::{allowed_variant}, but no \
             such construction exists any more — retire the allow-list entry"
        );
    }
}

/// **Tier 2 — the enumeration deliverable (P4.98's Tier 3).** Every
/// `*_routes.rs` construction of a variant that carries ONLY typed fields
/// (not `Option<Option<…>>`) — a different class, out of THIS order's scope
/// (Tier 3: "Converting typed-field hand-built edges"). Their wrong-type
/// verdicts are `web_edge_body_parse_guard.rs`'s (`quilltap-harness`, P4.60/
/// P4.62) — this table only counts them, so a new hand-built edge has to be
/// argued into the count rather than typed silently.
///
/// Measured 2026-09-18 at this port's `main` tip (post-conversion: this
/// order's four edges no longer appear here — `subprompts_routes.rs` fell
/// from 5 hand-built constructions to 3, `prompt_templates_routes.rs` from 5
/// to 2; `images_routes.rs`'s `images_generate` leg was never a struct
/// literal, so its count (4: `ImagesList`, `ImageDelete`,
/// `ImageImportFromUrl`, `ImageUpload`) is unmoved by this port).
const TYPED_ONLY_HAND_BUILT_CONSTRUCTIONS_BY_FILE: &[(&str, usize)] = &[
    ("backup_routes.rs", 3),
    ("brahma_routes.rs", 7),
    ("characters_routes.rs", 10), // 11 hits minus the 1 CharacterRename exception
    ("chats_routes.rs", 1),
    ("custom_tools_routes.rs", 4),
    ("embedding_profiles_routes.rs", 9),
    ("files_routes.rs", 9),
    ("help_routes.rs", 9),
    ("images_routes.rs", 4),
    ("llm_logs_routes.rs", 5),
    ("messages_routes.rs", 2),
    ("photos_routes.rs", 4),
    ("prompt_templates_routes.rs", 2),
    ("subprompts_routes.rs", 3),
    ("system_data_routes.rs", 15),
    ("text_replacements_routes.rs", 6),
    ("tools_routes.rs", 1),
    ("ui_search_routes.rs", 1),
    // P4.D205 (v4 `e7d77bb60`): 14 → 15. The `?action=informs` GET hand-builds
    // `CoreRequest::ChatInformsList { chat_id }`, which is typed-only (one
    // `String`) and so is exactly what this table is for. The two POST arms
    // (`inform` / `cancel-inform`) do NOT appear here: their variants carry
    // `Option<Option<Value>>` tri-states, so they go through
    // `request_envelope::request_envelope` — which is the rule this census
    // exists to enforce.
    ("wardrobe_routes.rs", 15),
];

#[test]
fn typed_only_hand_built_construction_count_matches_the_recorded_table() {
    let tri_state: std::collections::HashSet<String> = tri_state_variants().into_iter().collect();
    let all = all_variant_constructions();

    let mut by_file: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (file, variant, _line) in &all {
        if tri_state.contains(variant) {
            continue; // Part 1's set — asserted separately above.
        }
        let base = file.rsplit('/').next().unwrap_or(file).to_string();
        *by_file.entry(base).or_insert(0) += 1;
    }

    let recorded: std::collections::BTreeMap<&str, usize> =
        TYPED_ONLY_HAND_BUILT_CONSTRUCTIONS_BY_FILE
            .iter()
            .copied()
            .collect();
    let recorded_owned: std::collections::BTreeMap<String, usize> =
        recorded.iter().map(|(k, v)| (k.to_string(), *v)).collect();

    assert_eq!(
        by_file, recorded_owned,
        "the typed-only hand-built construction count moved — a new `*_routes.rs` edge \
         hand-builds a `Request` variant (or one was removed/converted); re-measure with \
         `grep -noE '(CoreRequest|Request)::[A-Za-z0-9_]+ *\\{{' crates/quilltap-web/src/*_routes.rs \
         | sed 's/:.*//' | sort | uniq -c` and update TYPED_ONLY_HAND_BUILT_CONSTRUCTIONS_BY_FILE \
         (their wrong-type verdicts are `web_edge_body_parse_guard.rs`'s, not this file's, to \
         classify)"
    );

    let total: usize = by_file.values().sum();
    assert_eq!(
        total, 110,
        "P4.D205: 110 = 111 total `*_routes.rs` variant constructions minus the 1 \
         CharacterRename tri-state exception. 109 = 110 - 1 before the Inform verbs; the \
         `?action=informs` GET adds ONE typed-only hand-built construction \
         (`ChatInformsList`, whose only field is the path id), while the two POST arms go \
         through \
         `request_envelope` and add none — which is the rule this census enforces. \
         Re-measure both numbers together if this moves"
    );
}
