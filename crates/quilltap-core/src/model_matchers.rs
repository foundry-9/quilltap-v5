//! Model matchers for `ProviderOptionField.appliesToModels` (v4
//! `lib/plugins/model-matchers.ts`, `84f33ce94`).
//!
//! Pure string work with no imports, deliberately: in v4 the options panel is a
//! client component and cannot reach the server-side plugin registry, so the
//! matcher that decides whether a field applies has to be able to run in the
//! browser. The server-side `matchModel` in [`crate::image_gen`] answers a
//! different question (which declaration object applies) against the registry;
//! this one answers "does this matcher list cover this model?".
//!
//! A matcher is one of:
//!   - an exact model id (`flux-lora`)
//!   - a `*` glob (`wavespeed-ai/*`, `flux-2-*`, `*-lora`)
//!   - a family prefix (`flux-lora` also covers `flux-lora/inpainting`)
//!
//! P4.D139 transcribes a client TS twin of this module; the names are kept
//! identical on both sides.

/// Does one matcher cover this model id? (v4 `modelMatchesPattern`.)
///
/// The glob arm is v4's `new RegExp('^' + pattern.split('*').map(escape).join('.*') + '$')`.
/// Two JS-regex fidelity points, both handled here rather than left to luck:
///
///   - v4 escapes each literal part with `/[.*+?^${}()|[\]\\]/g` → `\$&`;
///     [`regex::escape`] escapes a SUPERSET of that set, which is semantically
///     identical (every extra escape is of a character that was already literal).
///   - JS's `.` (no `s` flag) excludes `\n`, `\r`, ` ` and ` `; Rust's
///     `.` excludes only `\n`. The join is therefore spelled as the explicit
///     negated class rather than `.*`, so a model id carrying a CR or a line
///     separator matches on both sides identically.
pub fn model_matches_pattern(model: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    if model == pattern {
        return true;
    }

    if pattern.contains('*') {
        // JS `.` semantics (see the doc note above).
        const ANY: &str = "[^\\n\\r\\u{2028}\\u{2029}]*";
        let escaped: Vec<String> = pattern.split('*').map(regex::escape).collect();
        let source = format!("^{}$", escaped.join(ANY));
        // Every part is escaped, so this cannot fail to compile; a failure would
        // be a Rust-side bug, and answering `false` matches nothing rather than
        // everything.
        return match regex::Regex::new(&source) {
            Ok(re) => re.is_match(model),
            Err(_) => false,
        };
    }

    // Plain prefix: a family entry covers the SKUs beneath it.
    model.starts_with(pattern)
}

/// Should a field with this `appliesToModels` list render for this model?
/// (v4 `fieldAppliesToModel`.)
///
/// Renders unconditionally when the list is absent or empty, and when the host
/// does not know which model is selected — a field the user cannot see is a
/// setting they cannot reach, so "unknown" resolves toward showing it rather
/// than hiding it.
pub fn field_applies_to_model(applies_to_models: Option<&[String]>, model: Option<&str>) -> bool {
    let Some(list) = applies_to_models else {
        return true;
    };
    if list.is_empty() {
        return true;
    }
    // v4 `if (!model) return true` — a JS falsy test, so an EMPTY model id is
    // "unknown", not "a model that matches nothing".
    let Some(model) = model.filter(|m| !m.is_empty()) else {
        return true;
    };
    list.iter().any(|p| model_matches_pattern(model, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pattern_never_matches() {
        assert!(!model_matches_pattern("flux-lora", ""));
        assert!(!model_matches_pattern("", ""));
    }

    #[test]
    fn regex_metacharacters_are_escaped() {
        // v4's own test pins this: `gpt-image-125` must NOT match `gpt-image-1.5*`.
        assert!(!model_matches_pattern("gpt-image-125", "gpt-image-1.5*"));
        assert!(model_matches_pattern(
            "gpt-image-1.5-mini",
            "gpt-image-1.5*"
        ));
    }

    #[test]
    fn prefix_and_glob() {
        assert!(model_matches_pattern("flux-lora/inpainting", "flux-lora"));
        assert!(model_matches_pattern("wavespeed-ai/krea", "wavespeed-ai/*"));
        assert!(model_matches_pattern("z-image-turbo-lora", "*-lora"));
        assert!(!model_matches_pattern("z-image-turbo", "*-lora"));
    }

    #[test]
    fn field_applies_defaults_to_showing() {
        assert!(field_applies_to_model(None, Some("x")));
        assert!(field_applies_to_model(Some(&[]), Some("x")));
        let list = vec!["flux-lora".to_string()];
        assert!(field_applies_to_model(Some(&list), None));
        assert!(field_applies_to_model(Some(&list), Some("")));
        assert!(!field_applies_to_model(Some(&list), Some("hidream")));
    }
}
