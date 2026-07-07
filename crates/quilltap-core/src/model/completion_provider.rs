//! The production [`CompletionProvider`] composition (W4.7d) — the real adapter
//! the canned responder stands in for. Composes the frozen W4.7c/d surfaces:
//! [`request_builder`](crate::model::request_builder) (the wire body) →
//! [`transport`](crate::model::transport) (headers + IO) →
//! [`response_parse`](crate::model::response_parse) (the LLMResponse) → the
//! [`CompletionResponse`] subset the cheap-LLM path consumes.
//!
//! [`execute_completion`] is generic over any [`ProviderTransport`] (the trait is
//! IO-free; the concrete `reqwest` impl is the feature-gated
//! [`ReqwestTransport`](crate::model::transport::ReqwestTransport)), so this
//! composition is always compiled and unit-testable with a fake transport.
//!
//! API-key acquisition stays host-side (v4 resolves `getApiKeyForCheapLLMSelection`
//! *before* the provider call — see [`crate::services::api_key_service`]); the
//! resolved plaintext key is passed in and injected per the manifest `auth`
//! scheme. `base_url` overrides (the `OPENAI_COMPATIBLE` custom host) are applied
//! at construction via `rewrite_localhost_url` (W4.7a) before the manifest is
//! consulted; a per-call override is a host concern.

use crate::model::completion::{
    CompletionError, CompletionParams, CompletionResponse, CompletionUsage,
};
use crate::model::request_builder::{build_request, RequestInput, RequestMessage};
use crate::model::response_parse::parse_for_provider;
use crate::model::transport::{
    transport_headers, BoxFuture, ProviderTransport, TransportPolicy, TransportRequest,
};
use crate::provider_manifest::Registry;

/// Build the provider-agnostic [`RequestInput`] from a [`CompletionParams`] (the
/// non-streaming cheap-LLM shape).
fn request_input_from_params(params: &CompletionParams) -> RequestInput {
    RequestInput {
        model: params.model.clone(),
        messages: params
            .messages
            .iter()
            .map(|m| RequestMessage::text(m.role.as_str(), &m.content))
            .collect(),
        temperature: params.temperature,
        max_tokens: Some(params.max_tokens),
        top_p: None,
        stop: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        web_search_enabled: false,
        profile_parameters: params.profile_parameters.clone(),
        cache_key: params.cache_key.clone(),
        previous_response_id: None,
        strict_max_tokens: params.strict_max_tokens,
        stream: false,
    }
}

/// Inject the api key per the manifest `auth` scheme onto (headers, url).
fn apply_auth(
    registry: &Registry,
    provider: &str,
    api_key: &str,
    headers: &mut Vec<(String, String)>,
    url: &mut String,
) {
    let Some(manifest) = registry.get_provider(provider) else {
        return;
    };
    match manifest.auth.kind.as_str() {
        "bearer" => headers.push(("Authorization".to_string(), format!("Bearer {api_key}"))),
        "header" => {
            if let Some(h) = &manifest.auth.header {
                headers.push((h.clone(), api_key.to_string()));
            }
        }
        "query" => {
            if let Some(param) = &manifest.auth.param {
                let sep = if url.contains('?') { '&' } else { '?' };
                url.push(sep);
                url.push_str(param);
                url.push('=');
                url.push_str(api_key);
            }
        }
        // "none" (ollama) and unknown schemes inject nothing.
        _ => {}
    }
}

/// Compose a full non-streaming completion: build → transport → parse → map to
/// the [`CompletionResponse`] the cheap-LLM path consumes. `user_agent` /
/// `base_url_env` feed [`transport_headers`] (the host injects the version +
/// `BASE_URL`). Errors carry the transport message (higher layers classify via
/// [`handle_provider_error`](crate::services::llm_errors::handle_provider_error)).
pub fn execute_completion<'a, T: ProviderTransport + ?Sized>(
    transport: &'a T,
    provider: &'a str,
    api_key: &'a str,
    params: &'a CompletionParams,
    policy: &'a TransportPolicy,
    user_agent: &'a str,
    base_url_env: Option<&'a str>,
) -> BoxFuture<'a, Result<CompletionResponse, CompletionError>> {
    Box::pin(async move {
        let registry = Registry::built_in();
        let input = request_input_from_params(params);
        let built = build_request(provider, &input)
            .map_err(|e| CompletionError::new(format!("request build: {e}")))?;

        let mut url = built.url.clone();
        let mut headers = transport_headers(provider, &built.headers, user_agent, base_url_env);
        apply_auth(registry, provider, api_key, &mut headers, &mut url);

        let request = TransportRequest {
            provider: provider.to_string(),
            method: built.method.clone(),
            url,
            headers,
            body: built.body_string().into_bytes(),
            api_key: api_key.to_string(),
        };

        let resp = transport
            .execute(&request, policy)
            .await
            .map_err(|e| CompletionError::new(e.message))?;
        let json = resp
            .json()
            .map_err(|e| CompletionError::new(format!("response parse: {e}")))?;

        let parsed = parse_for_provider(provider, &json);
        Ok(CompletionResponse {
            content: parsed.content,
            usage: Some(CompletionUsage {
                prompt_tokens: parsed.usage.prompt_tokens,
                completion_tokens: parsed.usage.completion_tokens,
                total_tokens: parsed.usage.total_tokens,
            }),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::completion::CompletionMessage;
    use crate::model::transport::{StreamBytes, TransportError, TransportResponse};

    /// A transport that returns one canned body and records the request it saw.
    struct FakeTransport {
        body: Vec<u8>,
        seen: std::sync::Mutex<Option<TransportRequest>>,
    }

    impl ProviderTransport for FakeTransport {
        fn execute<'a>(
            &'a self,
            request: &'a TransportRequest,
            _policy: &'a TransportPolicy,
        ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
            *self.seen.lock().unwrap() = Some(request.clone());
            let body = self.body.clone();
            Box::pin(async move { Ok(TransportResponse { status: 200, body }) })
        }
        fn execute_stream<'a>(
            &'a self,
            _request: &'a TransportRequest,
            _policy: &'a TransportPolicy,
        ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>>
        {
            Box::pin(async move {
                Err(TransportError {
                    message: "no stream".to_string(),
                    status: None,
                })
            })
        }
    }

    fn params(model: &str) -> CompletionParams {
        CompletionParams {
            messages: vec![CompletionMessage::user("hi")],
            model: model.to_string(),
            temperature: Some(0.3),
            max_tokens: 2048,
            strict_max_tokens: true,
            cache_key: None,
            profile_parameters: None,
        }
    }

    #[tokio::test]
    async fn composes_deepseek_completion_and_injects_bearer() {
        let body = br#"{"choices":[{"message":{"content":"answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":3,"total_tokens":13}}"#.to_vec();
        let transport = FakeTransport {
            body,
            seen: std::sync::Mutex::new(None),
        };
        let policy = TransportPolicy::default();
        let resp = execute_completion(
            &transport,
            "DEEPSEEK",
            "synthetic-key",
            &params("deepseek-chat"),
            &policy,
            "Quilltap/test",
            None,
        )
        .await
        .expect("completion");
        assert_eq!(resp.content, "answer");
        assert_eq!(resp.usage.unwrap().total_tokens, 13);

        // The request carried the Bearer auth + User-Agent.
        let seen = transport.seen.lock().unwrap().clone().unwrap();
        assert!(seen
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer synthetic-key"));
        assert!(seen
            .headers
            .iter()
            .any(|(k, v)| k == "User-Agent" && v == "Quilltap/test"));
    }

    #[tokio::test]
    async fn google_injects_key_as_query_param() {
        // A minimal google generateContent body.
        let body = br#"{"candidates":[{"content":{"parts":[{"text":"ok"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":1,"totalTokenCount":6}}"#.to_vec();
        let transport = FakeTransport {
            body,
            seen: std::sync::Mutex::new(None),
        };
        let policy = TransportPolicy::default();
        let resp = execute_completion(
            &transport,
            "GOOGLE",
            "synthetic-key",
            &params("gemini-2.5-flash"),
            &policy,
            "Quilltap/test",
            None,
        )
        .await
        .expect("completion");
        assert_eq!(resp.content, "ok");
        let seen = transport.seen.lock().unwrap().clone().unwrap();
        assert!(
            seen.url.contains("key=synthetic-key"),
            "url was {}",
            seen.url
        );
        assert!(seen.url.contains(":generateContent"));
    }
}
