//! The Scenario Builder request body — v4 `lib/scenario-builder/
//! request-schema.ts` (`d1c06cd9d`), `scenarioBuildRequestSchema`, as a
//! hand-written Zod twin over the ONE issue home ([`crate::api::zod_issues`]).
//!
//! ```text
//! mode:                z.enum(['real', 'in-world'])
//! location:            z.string().trim().min(1).max(500)
//! time:                z.string().trim().min(1).max(200)
//! details:             z.string().trim().max(4000).default('')
//! connectionProfileId: UUIDSchema                         (z.uuid())
//! projectId:           UUIDSchema.nullish()
//! characterIds:        z.array(UUIDSchema).max(32).default([])
//! chatId:              UUIDSchema.nullish()
//! priorDraft:          z.string().max(20_000).nullish()   (NOT trimmed)
//! revision:            z.string().trim().max(2000).nullish()
//! .refine((r) => (r.priorDraft == null) === (r.revision == null),
//!         { message: 'priorDraft and revision travel together' })
//! ```
//!
//! The dispatch verb carries the body as ONE raw `serde_json::Value`
//! (`Request::ScenarioBuilderBuild { body }`), so absent / `null` / wrong-typed
//! keys all reach this twin exactly as v4's `safeParse(raw)` sees them — the
//! tri-state rule at object granularity. Unknown keys are stripped (Zod's
//! default).
//!
//! ## What the oracle measured that the schema text does not say
//!
//! `scenario_build_request_schema_equivalence` diffs this twin against v4's
//! REAL schema (zod 4.6.5 at the `d1c06cd9d` pin), and three behaviours are
//! Zod's, not the schema's:
//!
//! - **The refine runs on a dirty object — unless an issue ABORTED.** Zod 4
//!   skips a refinement only when the payload carries a non-continuable
//!   issue. Measured per code: `invalid_type` and `invalid_value` abort it;
//!   `too_small`, `too_big` and `invalid_format` do not (so a too-long `time`
//!   plus a lone `priorDraft` answers BOTH issues, the `custom` one last).
//!   And the refine reads the RAW value of a field that itself failed a
//!   continuable check (`priorDraft` 20,001 chars + no `revision` → the size
//!   issue AND the refine).
//! - **Array elements are checked before the array's size**
//!   (`characterIds` of 33 with a bad 33rd entry → the element's
//!   `invalid_format` at `["characterIds", 32]`, then `too_big`).
//! - **Lengths are Zod ≥ 4.5's**: UTF-16 units, re-counted in code points only
//!   when the unit count is in doubt ([`crate::jsstr::zod_len_max_ok`]); trims
//!   are JS `String.prototype.trim` ([`crate::jsstr::js_trim`] — strips
//!   U+FEFF, keeps U+0085).
//!
//! The PARSED OUTPUT's key order is the shape's, with an absent optional
//! OMITTED and an explicit `null` KEPT — [`ScenarioBuildRequest::to_value`]
//! reproduces it, because the tier-1 family diffs the output bytes too.

use serde_json::{Map, Value};

use crate::api::zod_issues::{key, zod_uuid_ok, ZodIssue};
use crate::jsstr::{js_trim, zod_len_max_ok, zod_len_min_ok};

/// The refine's message, verbatim.
pub const TRAVEL_TOGETHER: &str = "priorDraft and revision travel together";

/// `mode: z.enum(['real', 'in-world'])`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioBuilderMode {
    Real,
    InWorld,
}

impl ScenarioBuilderMode {
    /// The wire spelling (and the `Mode:` line of the user message).
    pub fn as_str(self) -> &'static str {
        match self {
            ScenarioBuilderMode::Real => "real",
            ScenarioBuilderMode::InWorld => "in-world",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "real" => Some(ScenarioBuilderMode::Real),
            "in-world" => Some(ScenarioBuilderMode::InWorld),
            _ => None,
        }
    }
}

/// A `.nullish()` key as Zod's OUTPUT keeps it: absent (`None`), an explicit
/// `null` (`Some(None)`), or a value.
pub type Nullish<T> = Option<Option<T>>;

/// v4 `ScenarioBuildRequest` — the schema's OUTPUT (trims and defaults
/// applied).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioBuildRequest {
    pub mode: ScenarioBuilderMode,
    pub location: String,
    pub time: String,
    pub details: String,
    pub connection_profile_id: String,
    pub project_id: Nullish<String>,
    pub character_ids: Vec<String>,
    pub chat_id: Nullish<String>,
    pub prior_draft: Nullish<String>,
    pub revision: Nullish<String>,
}

fn flat(v: &Nullish<String>) -> Option<&str> {
    v.as_ref().and_then(|o| o.as_deref())
}

impl ScenarioBuildRequest {
    /// `body.projectId ?? null` as the route reads it.
    pub fn project_id(&self) -> Option<&str> {
        flat(&self.project_id)
    }
    /// `body.chatId` (truthy iff present and non-null — a uuid is never empty).
    pub fn chat_id(&self) -> Option<&str> {
        flat(&self.chat_id)
    }
    /// `body.priorDraft ?? null`.
    pub fn prior_draft(&self) -> Option<&str> {
        flat(&self.prior_draft)
    }
    /// `body.revision ?? null` (the route's `revising` is `revision != null`).
    pub fn revision(&self) -> Option<&str> {
        flat(&self.revision)
    }

    /// The parsed output as Zod renders it: shape key order, absent optionals
    /// omitted, explicit nulls kept.
    pub fn to_value(&self) -> Value {
        fn put(m: &mut Map<String, Value>, k: &str, v: &Nullish<String>) {
            if let Some(inner) = v {
                m.insert(
                    k.to_string(),
                    inner.clone().map(Value::from).unwrap_or(Value::Null),
                );
            }
        }
        let mut m = Map::new();
        m.insert("mode".into(), Value::from(self.mode.as_str()));
        m.insert("location".into(), Value::from(self.location.clone()));
        m.insert("time".into(), Value::from(self.time.clone()));
        m.insert("details".into(), Value::from(self.details.clone()));
        m.insert(
            "connectionProfileId".into(),
            Value::from(self.connection_profile_id.clone()),
        );
        put(&mut m, "projectId", &self.project_id);
        m.insert(
            "characterIds".into(),
            Value::Array(
                self.character_ids
                    .iter()
                    .cloned()
                    .map(Value::from)
                    .collect(),
            ),
        );
        put(&mut m, "chatId", &self.chat_id);
        put(&mut m, "priorDraft", &self.prior_draft);
        put(&mut m, "revision", &self.revision);
        Value::Object(m)
    }
}

/// The issue collector. `aborted` is Zod 4's "a non-continuable issue is
/// present" — the gate on the root refine (see the module header).
struct Issues {
    list: Vec<ZodIssue>,
    aborted: bool,
}

impl Issues {
    fn push(&mut self, issue: ZodIssue) {
        if matches!(
            issue,
            ZodIssue::InvalidType { .. } | ZodIssue::InvalidValue { .. }
        ) {
            self.aborted = true;
        }
        self.list.push(issue);
    }
}

/// `z.string()[.trim()][.min(min)].max(max)` over a PRESENT value. Returns the
/// (trimmed) output when the type check passed, whether or not a size check
/// failed — a size failure is continuable and the refine still reads the key.
fn string_field(
    issues: &mut Issues,
    name: &str,
    v: &Value,
    trim: bool,
    min: Option<usize>,
    max: usize,
) -> Option<String> {
    let Value::String(s) = v else {
        issues.push(ZodIssue::invalid_type("string", vec![key(name)], Some(v)));
        return None;
    };
    let out = if trim { js_trim(s) } else { s.as_str() };
    if let Some(min) = min {
        if !zod_len_min_ok(out, min) {
            issues.push(ZodIssue::too_small_string(
                Value::from(min),
                vec![key(name)],
            ));
        }
    }
    if !zod_len_max_ok(out, max) {
        issues.push(ZodIssue::too_big_string(Value::from(max), vec![key(name)]));
    }
    Some(out.to_string())
}

/// `UUIDSchema` (`z.uuid()`) at `path`.
fn uuid_at(issues: &mut Issues, path: Vec<Value>, v: &Value) -> Option<String> {
    match v {
        Value::String(s) if zod_uuid_ok(s) => Some(s.clone()),
        Value::String(_) => {
            issues.push(ZodIssue::invalid_uuid(path));
            None
        }
        other => {
            issues.push(ZodIssue::invalid_type("string", path, Some(other)));
            None
        }
    }
}

/// `.nullish()` around a field parser: absent → `None`, `null` → `Some(None)`.
fn nullish<F>(obj: &Map<String, Value>, name: &str, parse: F) -> Nullish<String>
where
    F: FnOnce(&Value) -> Option<String>,
{
    match obj.get(name) {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(parse(v)),
    }
}

/// v4 `scenarioBuildRequestSchema.safeParse(raw)`: `Ok` with the output, or
/// `Err` with the issue list in Zod's order (the shape's key order, then the
/// root refine).
pub fn parse_scenario_build_request(raw: &Value) -> Result<ScenarioBuildRequest, Vec<ZodIssue>> {
    let Value::Object(obj) = raw else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(raw))]);
    };
    let mut issues = Issues {
        list: Vec::new(),
        aborted: false,
    };

    // mode — an enum reports `invalid_value` for a wrong TYPE and an absent
    // key too (it compares values; it has no separate type gate).
    let mode = match obj
        .get("mode")
        .and_then(Value::as_str)
        .and_then(ScenarioBuilderMode::parse)
    {
        Some(m) => Some(m),
        None => {
            issues.push(ZodIssue::invalid_value(
                &["real", "in-world"],
                vec![key("mode")],
            ));
            None
        }
    };

    let required_string =
        |issues: &mut Issues, name: &str, min: usize, max: usize| match obj.get(name) {
            Some(v) => string_field(issues, name, v, true, Some(min), max),
            None => {
                issues.push(ZodIssue::invalid_type("string", vec![key(name)], None));
                None
            }
        };
    let location = required_string(&mut issues, "location", 1, 500);
    let time = required_string(&mut issues, "time", 1, 200);

    // details — `.default('')` substitutes ONLY for undefined; `null` is a
    // type miss.
    let details = match obj.get("details") {
        None => Some(String::new()),
        Some(v) => string_field(&mut issues, "details", v, true, None, 4000),
    };

    let connection_profile_id = match obj.get("connectionProfileId") {
        Some(v) => uuid_at(&mut issues, vec![key("connectionProfileId")], v),
        None => {
            issues.push(ZodIssue::invalid_type(
                "string",
                vec![key("connectionProfileId")],
                None,
            ));
            None
        }
    };

    let project_id = nullish(obj, "projectId", |v| {
        uuid_at(&mut issues, vec![key("projectId")], v)
    });

    // characterIds — elements first, then the array's own size check.
    let character_ids = match obj.get("characterIds") {
        None => Some(Vec::new()),
        Some(Value::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            let mut ok = true;
            for (i, item) in items.iter().enumerate() {
                match uuid_at(&mut issues, vec![key("characterIds"), Value::from(i)], item) {
                    Some(id) => out.push(id),
                    None => ok = false,
                }
            }
            if items.len() > 32 {
                issues.push(ZodIssue::too_big_array(
                    Value::from(32),
                    vec![key("characterIds")],
                ));
            }
            ok.then_some(out)
        }
        Some(other) => {
            issues.push(ZodIssue::invalid_type(
                "array",
                vec![key("characterIds")],
                Some(other),
            ));
            None
        }
    };

    let chat_id = nullish(obj, "chatId", |v| {
        uuid_at(&mut issues, vec![key("chatId")], v)
    });
    let prior_draft = nullish(obj, "priorDraft", |v| {
        string_field(&mut issues, "priorDraft", v, false, None, 20_000)
    });
    let revision = nullish(obj, "revision", |v| {
        string_field(&mut issues, "revision", v, true, None, 2000)
    });

    // The root refine, over the RAW nullness — a field that failed a
    // continuable check still carries its value into the refine.
    if !issues.aborted {
        let draft_null = matches!(obj.get("priorDraft"), None | Some(Value::Null));
        let revision_null = matches!(obj.get("revision"), None | Some(Value::Null));
        if draft_null != revision_null {
            issues.push(ZodIssue::custom(vec![], TRAVEL_TOGETHER));
        }
    }

    if !issues.list.is_empty() {
        return Err(issues.list);
    }
    // Every field parsed (an issue-free run leaves no `None` behind).
    Ok(ScenarioBuildRequest {
        mode: mode.expect("issue-free"),
        location: location.expect("issue-free"),
        time: time.expect("issue-free"),
        details: details.expect("issue-free"),
        connection_profile_id: connection_profile_id.expect("issue-free"),
        project_id,
        character_ids: character_ids.expect("issue-free"),
        chat_id,
        prior_draft,
        revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Value {
        json!({
            "mode": "in-world",
            "location": " The Lantern ",
            "time": "dusk",
            "connectionProfileId": "11111111-1111-4111-8111-111111111111",
        })
    }

    #[test]
    fn minimal_body_applies_trims_and_defaults() {
        let r = parse_scenario_build_request(&base()).unwrap();
        assert_eq!(
            r.to_value().to_string(),
            r#"{"mode":"in-world","location":"The Lantern","time":"dusk","details":"","connectionProfileId":"11111111-1111-4111-8111-111111111111","characterIds":[]}"#
        );
    }

    #[test]
    fn a_lone_prior_draft_is_the_root_custom_issue() {
        let mut b = base();
        b["priorDraft"] = json!("Draft.");
        let issues = parse_scenario_build_request(&b).unwrap_err();
        assert_eq!(
            serde_json::to_string(&issues).unwrap(),
            r#"[{"code":"custom","path":[],"message":"priorDraft and revision travel together"}]"#
        );
    }

    #[test]
    fn an_aborting_issue_skips_the_refine_and_a_continuable_one_does_not() {
        let mut b = base();
        b["priorDraft"] = json!("Draft.");
        b["location"] = json!(5);
        assert_eq!(parse_scenario_build_request(&b).unwrap_err().len(), 1);
        b["location"] = json!("");
        assert_eq!(parse_scenario_build_request(&b).unwrap_err().len(), 2);
    }
}
