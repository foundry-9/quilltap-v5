//! `validateApiKey` / `getAvailableModels` request builders + response parsers
//! (W4.7d) — the sans-IO half of the model-discovery surface. The transport
//! (W4.7d) issues the request and maps the HTTP status to the `validateApiKey`
//! boolean; this module builds the request (method + path) and parses the
//! models-list response body.
//!
//! ## Endpoints (v4 plugin `getAvailableModels` / `validateApiKey`)
//!
//!   - OpenAI-family (`openai-compatible` base, `deepseek`, `z-ai`, `openrouter`,
//!     `openai`, `grok`): `GET {baseUrl}/models` → `{ data: [{ id }] }`. `validate`
//!     = a successful `models.list()`.
//!   - `anthropic`: `GET {baseUrl}/models` → `{ data: [{ id }] }` (NO sort; a
//!     static fallback list on failure). `validate` = a minimal `messages.create`
//!     (a POST — a transport concern, not a models list).
//!   - `google`: `GET {baseUrl}/models` → `{ models: [{ name, supportedActions }] }`
//!     — keep `generateContent`-capable, strip the `models/` prefix. `validate` = a
//!     minimal `generateContent` (POST).
//!   - `ollama`: `GET {baseUrl}/api/tags` → `{ models: [{ name }] }` (both
//!     `validate` and `models`; no API key).
//!
//! Per-provider list shaping (byte-faithful): `openai` filters chat prefixes
//! (`gpt-4`/`gpt-5`/`o1`/`o3`/`o4`) + sorts; the base + `grok` + `deepseek` map +
//! sort; `openrouter` maps with NO sort; `anthropic` maps with NO sort; `ollama`
//! maps names. **Tracked seam:** `z-ai`'s dynamic path (map ids, drop
//! image-gen-pattern ids, sort) is ported; its merge of a hardcoded STATIC model
//! list is config data left to the manifest/host (the corpus exercises the
//! dynamic path).

use serde_json::Value;

/// The models-list probe request (method + path relative to `baseUrl`). The
/// transport prepends `baseUrl` and adds auth + User-Agent headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelsListRequest {
    pub method: &'static str,
    /// Path appended to the provider `baseUrl` (e.g. `/models`, `/api/tags`).
    pub path: &'static str,
}

/// The models-list endpoint for a canonical provider id.
pub fn models_list_request(provider: &str) -> ModelsListRequest {
    let path = match provider {
        "OLLAMA" => "/api/tags",
        _ => "/models",
    };
    ModelsListRequest {
        method: "GET",
        path,
    }
}

/// z-ai drops model ids that look like image-generation models (v4
/// `IMAGE_GEN_MODEL_PATTERN = /^(cogview|glm-image)/i` — PREFIX-anchored,
/// case-insensitive).
fn is_zai_image_model(id: &str) -> bool {
    let lower = id.to_lowercase();
    lower.starts_with("cogview") || lower.starts_with("glm-image")
}

/// Parse a models-list response body into the model-id list, per the provider's
/// `getAvailableModels` shaping.
pub fn parse_models_list(provider: &str, body: &Value) -> Vec<String> {
    match provider {
        "OLLAMA" => body
            .get("models")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("name").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        "GOOGLE" => body
            .get("models")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter(|m| {
                        m.get("supportedActions")
                            .and_then(Value::as_array)
                            .map(|a| a.iter().any(|v| v.as_str() == Some("generateContent")))
                            .unwrap_or(false)
                    })
                    .filter_map(|m| m.get("name").and_then(Value::as_str))
                    .map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())
                    .collect()
            })
            .unwrap_or_default(),
        _ => {
            // OpenAI-family / anthropic: `{ data: [{ id }] }`.
            let mut ids: Vec<String> = body
                .get("data")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            match provider {
                // OpenAI: filter Responses-API chat models, then sort.
                "OPENAI" => {
                    const PREFIXES: &[&str] = &["gpt-4", "gpt-5", "o1", "o3", "o4"];
                    ids.retain(|id| PREFIXES.iter().any(|p| id.starts_with(p)));
                    ids.sort();
                    ids
                }
                // Z.AI: drop image-gen ids, sort (static-list merge is a seam).
                "Z_AI" => {
                    ids.retain(|id| !is_zai_image_model(id));
                    ids.sort();
                    ids.dedup();
                    ids
                }
                // OpenRouter + Anthropic: map with NO sort.
                "OPENROUTER" | "ANTHROPIC" => ids,
                // Base openai-compatible / deepseek / grok: map + sort.
                _ => {
                    ids.sort();
                    ids
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn endpoints() {
        assert_eq!(models_list_request("OLLAMA").path, "/api/tags");
        assert_eq!(models_list_request("OPENAI").path, "/models");
        assert_eq!(models_list_request("ANTHROPIC").method, "GET");
    }

    #[test]
    fn openai_filters_and_sorts() {
        let body = json!({ "data": [
            { "id": "gpt-5" }, { "id": "text-embedding-3" }, { "id": "gpt-4o" }, { "id": "o3-mini" }, { "id": "dall-e-3" }
        ]});
        assert_eq!(
            parse_models_list("OPENAI", &body),
            vec!["gpt-4o", "gpt-5", "o3-mini"]
        );
    }

    #[test]
    fn base_maps_and_sorts_openrouter_does_not() {
        let body = json!({ "data": [{ "id": "z" }, { "id": "a" }] });
        assert_eq!(parse_models_list("DEEPSEEK", &body), vec!["a", "z"]);
        assert_eq!(parse_models_list("OPENROUTER", &body), vec!["z", "a"]);
        assert_eq!(parse_models_list("ANTHROPIC", &body), vec!["z", "a"]);
    }

    #[test]
    fn zai_drops_image_models() {
        let body =
            json!({ "data": [{ "id": "glm-4.6" }, { "id": "cogview-3" }, { "id": "glm-4v" }] });
        // cogview dropped; "glm-4v" kept (no image/vision/cogview substring); sorted.
        assert_eq!(parse_models_list("Z_AI", &body), vec!["glm-4.6", "glm-4v"]);
    }

    #[test]
    fn google_filters_capability_and_strips_prefix() {
        let body = json!({ "models": [
            { "name": "models/gemini-2.5-flash", "supportedActions": ["generateContent"] },
            { "name": "models/embedding-001", "supportedActions": ["embedContent"] }
        ]});
        assert_eq!(parse_models_list("GOOGLE", &body), vec!["gemini-2.5-flash"]);
    }

    #[test]
    fn ollama_maps_names() {
        let body = json!({ "models": [{ "name": "llama3" }, { "name": "qwen2" }] });
        assert_eq!(parse_models_list("OLLAMA", &body), vec!["llama3", "qwen2"]);
    }
}
