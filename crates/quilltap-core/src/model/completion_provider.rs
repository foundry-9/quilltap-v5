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
use crate::model::provider_auth::apply_auth;
use crate::model::request_builder::{build_request, RequestInput};
use crate::model::response_parse::parse_for_provider;
use crate::model::stream::StreamMessage;
use crate::model::transport::{
    transport_headers, BoxFuture, ProviderTransport, TransportPolicy, TransportRequest,
};
use crate::provider_manifest::Registry;

/// Build the provider-agnostic [`RequestInput`] from a [`CompletionParams`] (the
/// non-streaming cheap-LLM shape). The cheap path only ever builds
/// `system`/`user` messages (v4 same); the role union's other arms map to their
/// plain-text equivalents — a `Tool`-role completion message has no id and no
/// representation on this path, so it is dropped exactly as v4's builders drop
/// an id-less tool message (unreachable from every cheap-LLM caller).
///
/// `params.attachments` are stamped onto the LAST user message (v4's
/// image-description call sends `messages: [{ role: 'user', content, attachments:
/// [attachmentForLLM] }]` — one user message carrying the vision payload), as
/// `FileAttachment` JSON bags for the request builders to emit (P4.21; before
/// this, the describe path built its params correctly and the wire dropped them
/// — dogfood #37).
pub(crate) fn request_input_from_params(params: &CompletionParams) -> RequestInput {
    use crate::model::completion::CompletionRole;
    let mut messages: Vec<StreamMessage> = params
        .messages
        .iter()
        .filter_map(|m| match m.role {
            CompletionRole::System => Some(StreamMessage::system(m.content.clone())),
            CompletionRole::User => Some(StreamMessage::user(m.content.clone())),
            CompletionRole::Assistant => Some(StreamMessage::assistant(m.content.clone())),
            CompletionRole::Tool => None,
        })
        .collect();
    if !params.attachments.is_empty() {
        let bags: Vec<serde_json::Value> = params
            .attachments
            .iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.id,
                    "filename": a.filename,
                    "mimeType": a.mime_type,
                    "data": a.data,
                })
            })
            .collect();
        if let Some(StreamMessage::User { attachments, .. }) = messages
            .iter_mut()
            .rev()
            .find(|m| matches!(m, StreamMessage::User { .. }))
        {
            *attachments = bags;
        }
    }
    RequestInput {
        model: params.model.clone(),
        messages,
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

/// Compose a full non-streaming completion: build → transport → parse → map to
/// the [`CompletionResponse`] the cheap-LLM path consumes. `user_agent` /
/// `base_url_env` feed [`transport_headers`] (the host injects the version +
/// `BASE_URL`); `base_url` is the connection profile's per-call base override
/// (v4 `createLLMProvider(provider, baseUrl)`), swapped for the manifest base
/// like the streaming composer's, localhost-rewritten. Errors carry the
/// transport message (higher layers classify via
/// [`handle_provider_error`](crate::services::llm_errors::handle_provider_error)).
#[allow(clippy::too_many_arguments)]
pub fn execute_completion<'a, T: ProviderTransport + ?Sized>(
    transport: &'a T,
    provider: &'a str,
    base_url: Option<&'a str>,
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

        // A profile baseUrl overrides the manifest base (the streaming
        // composer's swap, verbatim): strip the manifest base off the built
        // url and re-root on the override, localhost-rewritten.
        let mut url = match base_url.filter(|b| !b.is_empty()) {
            Some(base) => {
                let base = crate::provider_manifest::rewrite_localhost_url(base, None);
                match registry
                    .get_provider(provider)
                    .and_then(|m| built.url.strip_prefix(m.base_url.as_str()))
                {
                    Some(rest) => format!("{base}{rest}"),
                    None => built.url.clone(),
                }
            }
            None => built.url.clone(),
        };
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
            finish_reason: parsed.finish_reason,
            // The builder's format-time report (v4 attaches it to LLMResponse).
            attachment_results: Some(built.attachment_results.clone()),
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
            attachments: Vec::new(),
        }
    }

    /// P4.21 (dogfood #37): a describe-shaped call — one user message +
    /// `CompletionParams.attachments` — must reach the request builders with the
    /// attachment stamped on the last user message. The old
    /// `request_input_from_params` read `m.content` alone, so the describe path
    /// built its params correctly and the wire silently dropped the image; the
    /// canned tier-3 provider KEYS on attachments, which is why the differential
    /// stayed green through a total outage. This pins the conversion itself.
    #[test]
    fn attachments_are_stamped_onto_the_last_user_message() {
        use crate::model::completion::CompletionAttachment;
        use crate::model::stream::StreamMessage;
        let mut p = params("vision-model");
        p.messages = vec![
            CompletionMessage::system("sys"),
            CompletionMessage::user("first"),
            CompletionMessage::user("Describe this image."),
        ];
        p.attachments = vec![CompletionAttachment {
            id: "file-1".to_string(),
            filename: "photo.png".to_string(),
            mime_type: "image/png".to_string(),
            data: "aGVsbG8=".to_string(),
        }];
        let input = request_input_from_params(&p);
        // Only the LAST user message carries the bag (v4's describe call has one
        // user message; the rule generalizes as "last user" like the stamper's).
        let atts: Vec<&[serde_json::Value]> = input
            .messages
            .iter()
            .filter(|m| matches!(m, StreamMessage::User { .. }))
            .map(|m| m.attachments())
            .collect();
        assert_eq!(atts.len(), 2);
        assert!(atts[0].is_empty(), "earlier user message must stay bare");
        assert_eq!(atts[1].len(), 1);
        let bag = &atts[1][0];
        assert_eq!(bag["id"], "file-1");
        assert_eq!(bag["filename"], "photo.png");
        assert_eq!(bag["mimeType"], "image/png");
        assert_eq!(bag["data"], "aGVsbG8=");
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
            None,
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

    /// Dogfood finding #23's regression pin, AT THE CALL SITE. The differential
    /// corpus builds its own `RequestInput`s, so it could never have caught the
    /// real defect: this composition sets `stream: false` but every builder
    /// hard-coded `"stream": true`, so every cheap-LLM call in production asked
    /// for SSE and then failed to parse the frames as JSON. Assert on the bytes
    /// this function actually hands the transport.
    #[tokio::test]
    async fn cheap_llm_calls_ask_for_a_non_streaming_body() {
        let policy = TransportPolicy::default();
        let cases: &[(&str, &[u8])] = &[
            (
                "DEEPSEEK",
                br#"{"choices":[{"message":{"content":"a"},"finish_reason":"stop"}],"usage":{}}"#,
            ),
            (
                "ANTHROPIC",
                br#"{"content":[{"type":"text","text":"a"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#,
            ),
            (
                "OPENAI",
                br#"{"status":"completed","output":[],"output_text":"a","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}"#,
            ),
        ];
        for (provider, body) in cases {
            let transport = FakeTransport {
                body: body.to_vec(),
                seen: std::sync::Mutex::new(None),
            };
            execute_completion(
                &transport,
                provider,
                None,
                "synthetic-key",
                &params("some-model"),
                &policy,
                "Quilltap/test",
                None,
            )
            .await
            .unwrap_or_else(|e| panic!("{provider}: {}", e.message));
            let seen = transport.seen.lock().unwrap().clone().unwrap();
            let sent = String::from_utf8(seen.body).expect("utf8 body");
            assert!(
                !sent.contains(r#""stream":true"#),
                "{provider} asked the provider to STREAM on the non-streaming path: {sent}"
            );
            // Anthropic omits the key entirely on `sendMessage`; the others send
            // it false. Either way the wire must never say true here.
            assert!(
                sent.contains(r#""stream":false"#) || !sent.contains(r#""stream""#),
                "{provider} sent an unexpected stream flag: {sent}"
            );
        }
        // Google carries the distinction in the URL, not the body.
        let transport = FakeTransport {
            body: br#"{"candidates":[{"content":{"parts":[{"text":"ok"}]},"finishReason":"STOP"}],"usageMetadata":{}}"#.to_vec(),
            seen: std::sync::Mutex::new(None),
        };
        execute_completion(
            &transport,
            "GOOGLE",
            None,
            "synthetic-key",
            &params("gemini-2.5-flash"),
            &policy,
            "Quilltap/test",
            None,
        )
        .await
        .expect("google completion");
        let seen = transport.seen.lock().unwrap().clone().unwrap();
        assert!(
            seen.url.contains(":generateContent") && !seen.url.contains("streamGenerateContent"),
            "google used the streaming endpoint on the non-streaming path: {}",
            seen.url
        );
    }

    /// P4.21: the non-streaming composition carries the builder's
    /// `attachmentResults` on the response (v4 `LLMResponse.attachmentResults`)
    /// — an Anthropic describe-shaped call reports its image as sent, and the
    /// body carries the image source block.
    #[tokio::test]
    async fn response_carries_the_builders_attachment_results() {
        use crate::model::completion::CompletionAttachment;
        let body = br#"{"content":[{"type":"text","text":"a photo"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#.to_vec();
        let transport = FakeTransport {
            body,
            seen: std::sync::Mutex::new(None),
        };
        let mut p = params("claude-haiku-4-5-20251001");
        p.messages = vec![CompletionMessage::user("Describe this image.")];
        p.attachments = vec![CompletionAttachment {
            id: "file-9".to_string(),
            filename: "photo.png".to_string(),
            mime_type: "image/png".to_string(),
            data: "aGVsbG8h".to_string(),
        }];
        let resp = execute_completion(
            &transport,
            "ANTHROPIC",
            None,
            "synthetic-key",
            &p,
            &TransportPolicy::default(),
            "Quilltap/test",
            None,
        )
        .await
        .expect("completion");
        let results = resp
            .attachment_results
            .expect("response must carry attachment results");
        assert_eq!(results.sent, vec!["file-9".to_string()]);
        assert!(results.failed.is_empty());
        let seen = transport.seen.lock().unwrap().clone().unwrap();
        let sent = String::from_utf8(seen.body).unwrap();
        assert!(
            sent.contains(r#""type":"image""#) && sent.contains("aGVsbG8h"),
            "the describe-path bytes must carry the image block: {sent}"
        );
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
            None,
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
