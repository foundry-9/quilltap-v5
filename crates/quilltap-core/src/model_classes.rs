//! Port of v4's lib/llm/model-classes.ts — the built-in LLM capability tiers
//! and the two lookups over them. Pure constant data + table search.

/// A model class: a named capability tier a connection profile can reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelClass {
    /// Unique identifier / display name.
    pub name: &'static str,
    /// Single-letter tier designation (A = smallest, D = largest).
    pub tier: &'static str,
    /// Maximum context window size in tokens.
    pub max_context: i64,
    /// Maximum output/completion size in tokens.
    pub max_output: i64,
    /// Capability tags for categorization and filtering.
    pub tags: &'static [&'static str],
    /// Quality ranking (0 = lowest, higher = better).
    pub quality: i64,
}

/// Built-in model classes defining standard LLM capability tiers. Order and
/// values mirror v4's `MODEL_CLASSES` exactly.
pub const MODEL_CLASSES: &[ModelClass] = &[
    ModelClass {
        name: "Compact",
        tier: "A",
        max_context: 32000,
        max_output: 4000,
        tags: &["SMALL", "CHEAP", "LOCAL"],
        quality: 0,
    },
    ModelClass {
        name: "Standard",
        tier: "B",
        max_context: 128000,
        max_output: 16000,
        tags: &["BUDGET"],
        quality: 1,
    },
    ModelClass {
        name: "Extended",
        tier: "C",
        max_context: 200000,
        max_output: 128000,
        tags: &["CREATIVE", "THINKING"],
        quality: 2,
    },
    ModelClass {
        name: "Deep",
        tier: "D",
        max_context: 1000000,
        max_output: 128000,
        tags: &["MAX"],
        quality: 3,
    },
];

/// Look up a model class by exact name, or `None` if unknown.
pub fn get_model_class(name: &str) -> Option<&'static ModelClass> {
    MODEL_CLASSES.iter().find(|mc| mc.name == name)
}

/// Whether `name` matches a known model class (exact, case-sensitive).
pub fn is_valid_model_class_name(name: &str) -> bool {
    MODEL_CLASSES.iter().any(|mc| mc.name == name)
}
