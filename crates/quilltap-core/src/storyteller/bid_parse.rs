//! Storyteller bid replies — the forgiving parser (v4
//! `lib/storyteller/bid-parse.ts`, P4.D343).
//!
//! A bid is a short labelled text block:
//!
//! ```text
//! BID: speak | pass
//! TIER: simple | lookup | act | destroy
//! TOOLS: doc_read_file, run_custom:entanglement_scan
//! PLAN: One or two sentences of what they mean to say or do.
//! ```
//!
//! Parsed as forgivingly as the turn sentinels in `skip_signal`: labels in any
//! case, wrapped in markdown emphasis or a code fence, in any order, with `:`
//! or `-` or `=`; a plan that runs onto following lines. Anything unparseable
//! is a pass (the raw text stays in the LLM log).
//!
//! Every regex is v4's, run through `regress` (the workspace's ECMAScript
//! engine) in UCS-2 mode — v4's literals carry no `u` flag — over UTF-16 code
//! units, so `\s`, `\b`, `.` and the `i` fold are JavaScript's.

use std::sync::LazyLock;

use regress::Regex;

use super::types::{ParsedBid, StorytellerTier, STORYTELLER_TIERS};
use crate::jsstr::js_trim;

static LABEL_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::with_flags(
        r"^\s*(?:[-*>]\s*)?[*_`~]*\s*(BID|TIER|TOOLS?|PLAN)\s*[*_`~]*\s*[:=\-–—]\s*(.*)$",
        "i",
    )
    .expect("LABEL_LINE compiles")
});
static FENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags(r"```[a-z]*\n?", "i").expect("fence compiles"));
static LINE_SPLIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags(r"\r?\n", "").expect("split compiles"));
static WRAPPING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::with_flags(r#"^[\s*_`~"'[\]]+|[\s*_`~"'[\].]+$"#, "").expect("wrapping compiles")
});
static SPEAK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::with_flags(r"^(speak|yes|bid|respond|reply|act)\b", "").expect("speak compiles")
});
static PASSES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::with_flags(r"^(pass|no|none|decline|skip)\b", "").expect("passes compiles")
});
static NO_TOOLS: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags(r"^(none|n\/a|-|—)?$", "i").expect("no-tools compiles"));
static TOOL_SPLIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags(r"[,;]|\s+and\s+", "").expect("tool split compiles"));
static CALL_PARENS: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags(r"\(\)$", "").expect("parens compiles"));
static NONE_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags(r"^none$", "i").expect("none compiles"));
static PLAN_WRAPPING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::with_flags(r#"^[\s*_`~"]+|[\s*_`~"]+$"#, "").expect("plan wrapping compiles")
});

/// v4's `TIER_SYNONYMS`, in INSERTION order (the `startsWith` scan takes the
/// first hit).
const TIER_SYNONYMS: [(&str, StorytellerTier); 11] = [
    ("simple", StorytellerTier::Simple),
    ("reply", StorytellerTier::Simple),
    ("speak", StorytellerTier::Simple),
    ("lookup", StorytellerTier::Lookup),
    ("look up", StorytellerTier::Lookup),
    ("read", StorytellerTier::Lookup),
    ("act", StorytellerTier::Act),
    ("action", StorytellerTier::Act),
    ("write", StorytellerTier::Act),
    ("destroy", StorytellerTier::Destroy),
    ("delete", StorytellerTier::Destroy),
];

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn from_units(u: &[u16]) -> String {
    String::from_utf16_lossy(u)
}

/// JS `s.replace(re, '')` for a global regex (every match removed).
fn replace_all(re: &Regex, s: &str) -> String {
    let u = units(s);
    let mut out: Vec<u16> = Vec::with_capacity(u.len());
    let mut last = 0;
    for m in re.find_from_ucs2(&u, 0) {
        out.extend_from_slice(&u[last..m.start()]);
        last = m.end();
    }
    out.extend_from_slice(&u[last..]);
    from_units(&out)
}

/// JS `s.replace(re, '')` for a NON-global regex (the first match removed).
fn replace_first(re: &Regex, s: &str) -> String {
    let u = units(s);
    match re.find_from_ucs2(&u, 0).next() {
        Some(m) => {
            let mut out = u[..m.start()].to_vec();
            out.extend_from_slice(&u[m.end()..]);
            from_units(&out)
        }
        None => s.to_string(),
    }
}

/// JS `s.split(re)` for a regex without captures (`limit` absent).
fn split(re: &Regex, s: &str) -> Vec<String> {
    let u = units(s);
    if u.is_empty() {
        // `''.split(re)` is `['']` unless the regex matches the empty string.
        return if re.find_from_ucs2(&u, 0).next().is_some() {
            Vec::new()
        } else {
            vec![String::new()]
        };
    }
    // ES `RegExp.prototype[@@split]`, simplified for a capture-free pattern
    // whose matches are never empty here (every v4 split pattern consumes ≥1
    // unit).
    let mut out = Vec::new();
    let mut p = 0;
    for m in re.find_from_ucs2(&u, 0) {
        if m.end() == m.start() {
            continue;
        }
        out.push(from_units(&u[p..m.start()]));
        p = m.end();
    }
    out.push(from_units(&u[p..]));
    out
}

fn is_match(re: &Regex, s: &str) -> bool {
    re.find_from_ucs2(&units(s), 0).next().is_some()
}

/// v4 `stripWrapping`: the leading `[\s*_`~"'[\]]` run and the trailing
/// `[\s*_`~"'[\].]` run removed, then JS `trim`.
fn strip_wrapping(value: &str) -> String {
    js_trim(&replace_all(&WRAPPING, value)).to_string()
}

/// JS `String.prototype.toLowerCase` — `str::to_lowercase` is byte-identical
/// to it (the Phase-1 ICU/Unicode cluster), except for the final-sigma rule,
/// which both apply.
fn js_lower(s: &str) -> String {
    s.to_lowercase()
}

/// v4 `parseBidReply(raw)` — `None` for an unreadable reply.
pub fn parse_bid_reply(raw: &str) -> Option<ParsedBid> {
    if raw.is_empty() || js_trim(raw).is_empty() {
        return None;
    }
    #[derive(Clone, Copy, PartialEq)]
    enum Label {
        Bid,
        Tier,
        Tools,
        Plan,
    }
    let mut bid: Option<String> = None;
    let mut tier: Option<String> = None;
    let mut tools: Option<String> = None;
    let mut plan: Option<String> = None;
    let mut current: Option<Label> = None;

    let unfenced = replace_all(&FENCE, raw);
    for line in split(&LINE_SPLIT, &unfenced) {
        let lu = units(&line);
        if let Some(m) = LABEL_LINE.find_from_ucs2(&lu, 0).next() {
            let g1 = m.group(1).map(|r| from_units(&lu[r])).unwrap_or_default();
            let g2 = m.group(2).map(|r| from_units(&lu[r])).unwrap_or_default();
            let label = match g1.to_ascii_uppercase().as_str() {
                "BID" => Label::Bid,
                "TIER" => Label::Tier,
                "PLAN" => Label::Plan,
                _ => Label::Tools, // TOOL | TOOLS
            };
            current = Some(label);
            let value = Some(js_trim(&g2).to_string());
            match label {
                Label::Bid => bid = value,
                Label::Tier => tier = value,
                Label::Tools => tools = value,
                Label::Plan => plan = value,
            }
            continue;
        }
        // Continuation lines belong to the plan; anything else before a label
        // is ignored.
        if current == Some(Label::Plan) && !js_trim(&line).is_empty() {
            let joined = format!("{} {}", plan.as_deref().unwrap_or(""), js_trim(&line));
            plan = Some(js_trim(&joined).to_string());
        }
    }

    let bid = bid?;
    let bid_word = js_lower(&strip_wrapping(&bid));
    let speak = is_match(&SPEAK, &bid_word);
    let passes = is_match(&PASSES, &bid_word);
    if !speak && !passes {
        return None;
    }

    let tier_word = tier
        .as_deref()
        .filter(|t| !t.is_empty())
        .map(|t| js_lower(&strip_wrapping(t)))
        .unwrap_or_default();
    let stated_tier = STORYTELLER_TIERS
        .iter()
        .copied()
        .find(|t| t.as_str() == tier_word)
        .or_else(|| {
            TIER_SYNONYMS
                .iter()
                .find(|(word, _)| tier_word.starts_with(word))
                .map(|(_, t)| *t)
        });

    let tools_text = tools
        .as_deref()
        .filter(|t| !t.is_empty())
        .map(strip_wrapping)
        .unwrap_or_default();
    let tools: Vec<String> = if is_match(&NO_TOOLS, &tools_text) {
        Vec::new()
    } else {
        split(&TOOL_SPLIT, &tools_text)
            .into_iter()
            .map(|tool| replace_first(&CALL_PARENS, &strip_wrapping(&tool)))
            .filter(|tool| !tool.is_empty() && !is_match(&NONE_WORD, tool))
            .collect()
    };
    // `[...new Set(tools)]` — first-seen order.
    let mut seen: Vec<String> = Vec::new();
    for t in tools {
        if !seen.contains(&t) {
            seen.push(t);
        }
    }

    let plan = plan.and_then(|p| {
        let stripped = js_trim(&replace_all(&PLAN_WRAPPING, &p)).to_string();
        (!stripped.is_empty()).then_some(stripped)
    });

    Some(ParsedBid {
        speak,
        stated_tier,
        tools: seen,
        plan,
    })
}
