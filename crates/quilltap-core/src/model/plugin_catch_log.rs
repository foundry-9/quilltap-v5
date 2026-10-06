//! The ERROR (and one WARN) lines v4's text-provider PLUGINS log around their
//! provider calls (P4.122 / P4.128 / P4.141) — one home for every provider's
//! catch shape, serving both the streaming composer (`streaming_provider.rs`)
//! and the non-streaming one (`completion_provider.rs`).
//!
//! Per v4 plugin (`f6426e196`; recorded through the REAL plugins by
//! `harness/oracle/providers/record-text-errors.mjs` — the `pluginErrorLog` /
//! `pluginWarnLog` fields; the field ORDER is v4's `context` object's key order,
//! then the logger's third argument as `error`):
//!
//! | provider · method | line(s) | fields |
//! |---|---|---|
//! | OPENAI_COMPATIBLE / DEEPSEEK / NANOGPT · both | `<Name> API error in <method>` | `context, baseUrl` + error |
//! | GOOGLE · send | `Error calling Google Gemini API` | `context, model, error` (in the CONTEXT, no third argument) |
//! | GOOGLE · stream | `Error streaming from Google Gemini API` | `context, model, error` |
//! | OLLAMA · send | `Ollama API error response` on a non-2xx, then `Ollama sendMessage failed` | `context, status, error` (the raw body); `context, baseUrl` + error |
//! | OLLAMA · stream | `Ollama streaming API error` on a non-2xx, then `Ollama streamMessage failed` | as send |
//! | OPENROUTER · stream (raw path) | `OpenRouter API error` on a non-2xx, then `Error in streamViaChatCompletions` | `context, status, error`; `context` + error |
//! | OPENROUTER · send (vision) | `OpenRouter API error` on a non-2xx — its ONLY line; `Error in sendViaChatCompletions` on a `fetch` throw | `context, status, error`; `context` + error |
//!
//! Z.AI, OPENAI, GROK and ANTHROPIC carry no catch line. Each line fires once,
//! after the SDK's own retries, at every arm v4's `try` covers: a non-2xx, a
//! transport failure, a timeout, a 2xx body the plugin cannot read
//! ([`sdk_response_shape`](crate::model::sdk_response_shape)), and every
//! mid-stream throw — EXCEPT OpenRouter's vision send, whose `try` wraps the
//! `fetch` alone (`provider.ts:422-442`; its `!response.ok` branch and its
//! `response.json()` sit outside it). The request BUILD sits outside every
//! `try`, so a build failure is silent.
//!
//! OpenRouter logs only on its raw Chat Completions `fetch` path: v4 streams
//! there when the request carries tools or images (`provider.ts:495-498`) and
//! sends there when it carries an image (`:162-164`); every other OpenRouter
//! call runs `@openrouter/sdk`, which logs nothing. v5 always speaks the raw
//! wire (the deliberate divergence `streaming_provider.rs` records), so the
//! line follows v4's PATH choice — [`openrouter_streaming_takes_raw_path`] /
//! [`openrouter_non_streaming_is_vision`] — not v5's wire. (The streaming
//! predicate lives HERE rather than beside its non-streaming twin in
//! `request_builder/chat_completions.rs`: that module is private and its
//! re-export list in `request_builder/mod.rs` sits outside P4.141's
//! ownership; it is v4's `hasTools || hasImages` over the twin.)
//!
//! Every value renders through the `%` (Display) sigil, so a string field is
//! unquoted, exactly as the capture rig renders v4's recorded context
//! (`tracing-percent-field-renders-unquoted`).
//!
//! [`openrouter_non_streaming_is_vision`]: crate::model::request_builder::openrouter_non_streaming_is_vision

use crate::model::provider_io::ProviderKind;
use crate::model::stream::StreamMessage;
use crate::model::transport::TransportError;
use crate::provider_manifest::{rewrite_localhost_url, Registry};

/// v4 OpenRouter `streamMessage`'s path choice (`provider.ts:495-498`):
/// `hasTools || hasImages` takes the raw Chat Completions `fetch`
/// (`streamViaChatCompletions`, which logs); anything else runs
/// `@openrouter/sdk` (which does not). `hasTools` is `params.tools &&
/// params.tools.length > 0`; `hasImages` is v4's `hasImageAttachments`, the
/// predicate [`openrouter_non_streaming_is_vision`](crate::model::request_builder::openrouter_non_streaming_is_vision)
/// already carries for the send.
pub fn openrouter_streaming_takes_raw_path(
    messages: &[StreamMessage],
    tools: Option<&serde_json::Value>,
) -> bool {
    let has_tools = tools
        .and_then(serde_json::Value::as_array)
        .is_some_and(|t| !t.is_empty());
    has_tools || crate::model::request_builder::openrouter_non_streaming_is_vision(messages)
}

/// Which plugin method the line belongs to — and so which tracing target it
/// logs on (`streaming_provider` / `completion_provider`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatchMethod {
    StreamMessage,
    SendMessage,
}

/// `tracing::error!` / `warn!` at the method's target (a target is a static,
/// so one callsite per target).
macro_rules! at_target {
    ($level:ident, $method:expr, $($rest:tt)*) => {
        match $method {
            CatchMethod::StreamMessage => tracing::$level!(
                target: "quilltap::model::streaming_provider",
                $($rest)*
            ),
            CatchMethod::SendMessage => tracing::$level!(
                target: "quilltap::model::completion_provider",
                $($rest)*
            ),
        }
    };
}

/// One provider's catch shape for one call.
#[derive(Clone, Debug)]
enum Shape {
    /// The three openai-SDK plugins: `<Name> API error in <method>`, `{context,
    /// baseUrl}` + error.
    SdkCatch {
        message: &'static str,
        context: &'static str,
        base_url: String,
    },
    /// Google: `{context, model, error}` with `error` in the context.
    Google {
        message: &'static str,
        context: &'static str,
        model: String,
    },
    /// Ollama: the status line on a non-2xx, then the catch line `{context,
    /// baseUrl}` + error.
    Ollama {
        status_message: &'static str,
        catch_message: &'static str,
        context: &'static str,
        base_url: String,
    },
    /// OpenRouter's raw `fetch` path: the status line on a non-2xx, then (the
    /// stream) the catch line `{context}` + error. `fetch_only_try` is the
    /// vision send's shape (no catch after the status line, none on a 2xx).
    OpenRouterRaw {
        catch_message: &'static str,
        context: &'static str,
        fetch_only_try: bool,
    },
}

/// The plugin lines for one provider call (see the module doc).
#[derive(Clone, Debug)]
pub struct PluginCatchLog {
    provider: String,
    method: CatchMethod,
    raw_path: bool,
    shape: Shape,
}

impl PluginCatchLog {
    /// The shape for `provider`'s `method`, or `None` for a plugin that logs
    /// nothing there. `base_url` is the profile's override (localhost-
    /// rewritten against `gateway`, as v4's registry hands it to the plugin)
    /// or the manifest default; `model` is the call's (Google's field);
    /// `raw_path` is v4's OpenRouter path choice (ignored elsewhere).
    pub fn for_call(
        provider: &str,
        base_url: Option<&str>,
        gateway: Option<&str>,
        method: CatchMethod,
        model: &str,
        raw_path: bool,
    ) -> Option<Self> {
        let resolved_base = || -> Option<String> {
            match base_url.filter(|b| !b.is_empty()) {
                Some(base) => Some(rewrite_localhost_url(base, gateway)),
                None => Some(
                    Registry::built_in()
                        .get_provider(provider)?
                        .base_url
                        .clone(),
                ),
            }
        };
        let stream = method == CatchMethod::StreamMessage;
        let sdk = |message: &'static str, context: &'static str| -> Option<Shape> {
            Some(Shape::SdkCatch {
                message,
                context,
                base_url: resolved_base()?,
            })
        };
        let shape = match ProviderKind::of(provider)? {
            ProviderKind::OpenAiCompatible if stream => sdk(
                "OpenAICompatible API error in streamMessage",
                "OpenAICompatibleProvider.streamMessage",
            )?,
            ProviderKind::OpenAiCompatible => sdk(
                "OpenAICompatible API error in sendMessage",
                "OpenAICompatibleProvider.sendMessage",
            )?,
            ProviderKind::DeepSeek if stream => sdk(
                "DeepSeek API error in streamMessage",
                "DeepSeekProvider.streamMessage",
            )?,
            ProviderKind::DeepSeek => sdk(
                "DeepSeek API error in sendMessage",
                "DeepSeekProvider.sendMessage",
            )?,
            ProviderKind::NanoGpt if stream => sdk(
                "NanoGPT API error in streamMessage",
                "NanoGPTProvider.streamMessage",
            )?,
            ProviderKind::NanoGpt => sdk(
                "NanoGPT API error in sendMessage",
                "NanoGPTProvider.sendMessage",
            )?,
            ProviderKind::Google => Shape::Google {
                message: if stream {
                    "Error streaming from Google Gemini API"
                } else {
                    "Error calling Google Gemini API"
                },
                context: if stream {
                    "GoogleProvider.streamMessage"
                } else {
                    "GoogleProvider.sendMessage"
                },
                model: model.to_string(),
            },
            ProviderKind::Ollama => Shape::Ollama {
                status_message: if stream {
                    "Ollama streaming API error"
                } else {
                    "Ollama API error response"
                },
                catch_message: if stream {
                    "Ollama streamMessage failed"
                } else {
                    "Ollama sendMessage failed"
                },
                context: if stream {
                    "OllamaProvider.streamMessage"
                } else {
                    "OllamaProvider.sendMessage"
                },
                // v4's constructor strips trailing slashes (`ollama/provider.ts:
                // 74`, `baseUrl.replace(/\/+$/, '')`) — the field logs that.
                base_url: resolved_base()?.trim_end_matches('/').to_string(),
            },
            // The SDK path logs nothing.
            ProviderKind::OpenRouter if !raw_path => return None,
            ProviderKind::OpenRouter if stream => Shape::OpenRouterRaw {
                catch_message: "Error in streamViaChatCompletions",
                context: "OpenRouterProvider.streamViaChatCompletions",
                fetch_only_try: false,
            },
            ProviderKind::OpenRouter => Shape::OpenRouterRaw {
                catch_message: "Error in sendViaChatCompletions",
                context: "OpenRouterProvider.sendViaChatCompletions",
                fetch_only_try: true,
            },
            ProviderKind::OpenAi
            | ProviderKind::Grok
            | ProviderKind::ZAi
            | ProviderKind::Anthropic => return None,
        };
        Some(Self {
            provider: provider.to_string(),
            method,
            raw_path,
            shape,
        })
    }

    /// A transport failure inside v4's `try`: on a non-2xx the status line
    /// (Ollama / OpenRouter) with the raw body, then the catch line with v4's
    /// thrown text ([`v4_thrown_message`](crate::model::provider_error::v4_thrown_message)).
    pub fn emit_transport(&self, error: &TransportError) {
        if let Some(status) = error.status {
            let body = error.http_body().unwrap_or_default();
            self.emit_status(status, body);
            if matches!(
                self.shape,
                Shape::OpenRouterRaw {
                    fetch_only_try: true,
                    ..
                }
            ) {
                // `if (!response.ok)` sits outside the vision send's `try`:
                // the status line is its only line.
                return;
            }
        }
        let text = crate::model::provider_error::v4_thrown_message(
            &self.provider,
            self.method,
            self.raw_path,
            error,
        );
        self.emit_catch(&text);
    }

    /// A throw with v4's text that is NOT a transport failure — a mid-stream
    /// decode or transport error, the finish arm, a 2xx body the plugin cannot
    /// read. OpenRouter's vision send logs none of these (its `try` wraps the
    /// `fetch` alone).
    pub fn emit_thrown(&self, text: &str) {
        if matches!(
            self.shape,
            Shape::OpenRouterRaw {
                fetch_only_try: true,
                ..
            }
        ) {
            return;
        }
        self.emit_catch(text);
    }

    fn emit_status(&self, status: u16, body: &str) {
        match &self.shape {
            Shape::Ollama {
                status_message,
                context,
                ..
            } => at_target!(
                error,
                self.method,
                context = context,
                status = status,
                error = %body,
                "{}",
                status_message
            ),
            Shape::OpenRouterRaw { context, .. } => at_target!(
                error,
                self.method,
                context = context,
                status = status,
                error = %body,
                "OpenRouter API error"
            ),
            Shape::SdkCatch { .. } | Shape::Google { .. } => {}
        }
    }

    fn emit_catch(&self, text: &str) {
        match &self.shape {
            Shape::SdkCatch {
                message,
                context,
                base_url,
            } => at_target!(
                error,
                self.method,
                context = context,
                baseUrl = %base_url,
                error = %text,
                "{}",
                message
            ),
            Shape::Google {
                message,
                context,
                model,
            } => at_target!(
                error,
                self.method,
                context = context,
                model = %model,
                error = %text,
                "{}",
                message
            ),
            Shape::Ollama {
                catch_message,
                context,
                base_url,
                ..
            } => at_target!(
                error,
                self.method,
                context = context,
                baseUrl = %base_url,
                error = %text,
                "{}",
                catch_message
            ),
            Shape::OpenRouterRaw {
                catch_message,
                context,
                ..
            } => at_target!(
                error,
                self.method,
                context = context,
                error = %text,
                "{}",
                catch_message
            ),
        }
    }
}

/// Which v4 call reached `extractTextFromResponse` — the line's TARGET (v4
/// logs both through the one plugin logger; v5 files each under the composer
/// that ran it, the family's two model targets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoogleExtractSite {
    /// `sendMessage` (`provider.ts:618`) — every non-streaming send.
    Send,
    /// `streamMessage` (`provider.ts:847-850`) — ONLY a thinking model that
    /// streamed no visible text, over the last chunk.
    Stream,
}

macro_rules! google_extract_warn {
    ($site:expr, $($rest:tt)*) => {
        match $site {
            GoogleExtractSite::Send => tracing::warn!(
                target: "quilltap::model::completion_provider", $($rest)*
            ),
            GoogleExtractSite::Stream => tracing::warn!(
                target: "quilltap::model::streaming_provider", $($rest)*
            ),
        }
    };
}

/// v4 Google `extractTextFromResponse` (`provider.ts:255-305`) — its two
/// WARNs, in v4's order, and the text it answers:
///
/// 1. the SDK's `.text` getter first: a first candidate with a NON-EMPTY
///    `content.parts` array answers without a line (the parts concat, or the
///    parts loop — neither logs);
/// 2. no non-empty `candidates` array → WARN `No candidates found in Google
///    response` `{context, modelName, blockReason}` (`:265-269`) → `''`;
/// 3. `candidates[0].content.parts` missing / non-array / empty → WARN `No
///    parts found in Google response candidate` `{context, modelName,
///    finishReason}` (`:276-280`, P4.150 D2), then `content.text` when truthy
///    (`:283-285` — MEASURED reachable through the real `@google/genai`
///    1.52.0: the SDK keeps the unknown `content.text` key), else `''`.
///
/// An `undefined` context value never reaches v4's line, so `blockReason` /
/// `finishReason` are OMITTED when absent. Returns the step-3 fallback text
/// (`Some` only there) for the caller's content.
pub fn emit_google_extract_text_warns(
    site: GoogleExtractSite,
    model: &str,
    body: &serde_json::Value,
) -> Option<String> {
    let first = body
        .get("candidates")
        .and_then(serde_json::Value::as_array)
        .and_then(|c| c.first());
    let Some(first) = first else {
        match crate::model::response_parse::google_block_reason(body) {
            Some(reason) => google_extract_warn!(
                site,
                context = "GoogleProvider.extractTextFromResponse",
                modelName = %model,
                blockReason = %reason,
                "No candidates found in Google response"
            ),
            None => google_extract_warn!(
                site,
                context = "GoogleProvider.extractTextFromResponse",
                modelName = %model,
                "No candidates found in Google response"
            ),
        }
        return None;
    };
    let content = first.get("content");
    let has_parts = content
        .and_then(|c| c.get("parts"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|p| !p.is_empty());
    if has_parts {
        return None;
    }
    match first.get("finishReason") {
        Some(reason) => {
            let reason = crate::pascal::js_value::to_js_string(reason);
            google_extract_warn!(
                site,
                context = "GoogleProvider.extractTextFromResponse",
                modelName = %model,
                finishReason = %reason,
                "No parts found in Google response candidate"
            )
        }
        None => google_extract_warn!(
            site,
            context = "GoogleProvider.extractTextFromResponse",
            modelName = %model,
            "No parts found in Google response candidate"
        ),
    }
    content
        .and_then(|c| c.get("text"))
        .and_then(serde_json::Value::as_str)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    //! Exact bytes, through the thread-scoped capture, against the shapes
    //! v4's plugins recorded (`text-http-errors.recorded.ndjson`'s
    //! `pluginErrorLog` / `pluginWarnLog`); the family diffs every corpus row.
    use super::*;
    use crate::test_support::captured;

    const SEND: CatchMethod = CatchMethod::SendMessage;
    const STREAM: CatchMethod = CatchMethod::StreamMessage;

    fn log(provider: &str, method: CatchMethod, raw_path: bool) -> PluginCatchLog {
        PluginCatchLog::for_call(provider, None, None, method, "gemini-2.5-flash", raw_path)
            .unwrap_or_else(|| panic!("{provider} logs"))
    }

    fn timeout() -> TransportError {
        TransportError::timeout("error sending request for url (http://x/)")
    }

    fn connect() -> TransportError {
        TransportError::connect("error sending request for url (http://x/)")
    }

    /// P4.122 item 6 (moved here by P4.141): the `baseUrl` the line names —
    /// the profile's (localhost-rewritten) over the manifest default; an
    /// empty profile base is no override.
    #[test]
    fn base_url_is_the_profiles_rewritten_or_the_manifests() {
        let lines = captured(|| {
            log("DEEPSEEK", STREAM, false).emit_thrown("x");
            PluginCatchLog::for_call(
                "OPENAI_COMPATIBLE",
                Some("http://localhost:1234/v1"),
                Some("host.docker.internal"),
                STREAM,
                "m",
                false,
            )
            .unwrap()
            .emit_thrown("x");
            PluginCatchLog::for_call("NANOGPT", Some(""), None, SEND, "m", false)
                .unwrap()
                .emit_thrown("x");
            PluginCatchLog::for_call(
                "OLLAMA",
                Some("http://localhost:11434"),
                Some("gw"),
                SEND,
                "m",
                false,
            )
            .unwrap()
            .emit_thrown("x");
        });
        assert_eq!(
            lines,
            [
                "ERROR quilltap::model::streaming_provider DeepSeek API error in streamMessage context=DeepSeekProvider.streamMessage baseUrl=https://api.deepseek.com error=x",
                &format!(
                    "ERROR quilltap::model::streaming_provider OpenAICompatible API error in streamMessage context=OpenAICompatibleProvider.streamMessage baseUrl={} error=x",
                    rewrite_localhost_url("http://localhost:1234/v1", Some("host.docker.internal"))
                ),
                "ERROR quilltap::model::completion_provider NanoGPT API error in sendMessage context=NanoGPTProvider.sendMessage baseUrl=https://nano-gpt.com/api/v1 error=x",
                &format!(
                    "ERROR quilltap::model::completion_provider Ollama sendMessage failed context=OllamaProvider.sendMessage baseUrl={} error=x",
                    // v4's registry rewrites, then the constructor strips the
                    // trailing slash the URL normalisation added.
                    rewrite_localhost_url("http://localhost:11434", Some("gw")).trim_end_matches('/')
                ),
            ]
        );
    }

    /// Ollama's `baseUrl` is the CONSTRUCTOR's: trailing slashes stripped
    /// (`ollama/provider.ts:74`) — a §3 finding at the `f6426e196`
    /// recorded-divergences unification (the corpus always used the manifest
    /// default). The SDK shapes keep the profile's bytes.
    #[test]
    fn ollamas_base_url_drops_trailing_slashes() {
        let lines = captured(|| {
            PluginCatchLog::for_call(
                "OLLAMA",
                Some("http://ollama.lan:11434//"),
                None,
                SEND,
                "m",
                false,
            )
            .unwrap()
            .emit_thrown("x");
        });
        assert_eq!(
            lines,
            ["ERROR quilltap::model::completion_provider Ollama sendMessage failed context=OllamaProvider.sendMessage baseUrl=http://ollama.lan:11434 error=x"]
        );
    }

    /// The silence legs: the plugins with no catch line, and OpenRouter's
    /// SDK path.
    #[test]
    fn silent_plugins_have_no_shape() {
        for p in ["Z_AI", "OPENAI", "GROK", "ANTHROPIC", "NOT_A_PROVIDER"] {
            for m in [STREAM, SEND] {
                for raw in [false, true] {
                    assert!(
                        PluginCatchLog::for_call(p, None, None, m, "m", raw).is_none(),
                        "{p}"
                    );
                }
            }
        }
        for m in [STREAM, SEND] {
            assert!(
                PluginCatchLog::for_call("OPENROUTER", None, None, m, "m", false).is_none(),
                "the SDK path logs nothing"
            );
        }
    }

    /// The three openai-SDK plugins: one catch line, v4's thrown text per
    /// failure kind (`v4_thrown_message`).
    #[test]
    fn sdk_catch_line_per_kind() {
        let lines = captured(|| {
            let l = log("OPENAI_COMPATIBLE", SEND, false);
            l.emit_transport(&TransportError::http(
                400,
                r#"{"error":{"message":"Filtered."}}"#,
            ));
            l.emit_transport(&connect());
            l.emit_transport(&timeout());
        });
        let head = "ERROR quilltap::model::completion_provider OpenAICompatible API error in sendMessage context=OpenAICompatibleProvider.sendMessage baseUrl=http://localhost:8080/v1";
        assert_eq!(
            lines,
            [
                format!("{head} error=400 Filtered."),
                format!("{head} error=Connection error."),
                format!("{head} error=Request timed out."),
            ]
        );
    }

    /// Google (#1 / #2): `{context, model, error}`, the error in the context
    /// — the field ORDER is the context object's.
    #[test]
    fn google_lines_carry_model_then_error() {
        let lines = captured(|| {
            log("GOOGLE", SEND, false).emit_transport(&connect());
            log("GOOGLE", STREAM, false).emit_transport(&timeout());
            log("GOOGLE", STREAM, false).emit_thrown("Incomplete JSON segment at the end");
            log("GOOGLE", SEND, false)
                .emit_transport(&TransportError::http(400, r#"{"error":{"code":400}}"#));
        });
        assert_eq!(
            lines,
            [
                "ERROR quilltap::model::completion_provider Error calling Google Gemini API context=GoogleProvider.sendMessage model=gemini-2.5-flash error=fetch failed",
                "ERROR quilltap::model::streaming_provider Error streaming from Google Gemini API context=GoogleProvider.streamMessage model=gemini-2.5-flash error=This operation was aborted",
                "ERROR quilltap::model::streaming_provider Error streaming from Google Gemini API context=GoogleProvider.streamMessage model=gemini-2.5-flash error=Incomplete JSON segment at the end",
                r#"ERROR quilltap::model::completion_provider Error calling Google Gemini API context=GoogleProvider.sendMessage model=gemini-2.5-flash error={"error":{"code":400}}"#,
            ]
        );
    }

    /// Ollama (#3–#6): the status line BEFORE the catch line on a non-2xx;
    /// the catch line alone otherwise, with the send/stream abort texts.
    #[test]
    fn ollama_status_line_then_catch_line() {
        let lines = captured(|| {
            log("OLLAMA", SEND, false)
                .emit_transport(&TransportError::http(400, r#"{"error":"bad"}"#));
            log("OLLAMA", STREAM, false).emit_transport(&TransportError::http(500, ""));
            log("OLLAMA", SEND, false).emit_transport(&timeout());
            log("OLLAMA", STREAM, false).emit_transport(&timeout());
            log("OLLAMA", STREAM, false).emit_transport(&connect());
            log("OLLAMA", STREAM, false).emit_thrown("mid-stream");
        });
        assert_eq!(
            lines,
            [
                r#"ERROR quilltap::model::completion_provider Ollama API error response context=OllamaProvider.sendMessage status=400 error={"error":"bad"}"#,
                r#"ERROR quilltap::model::completion_provider Ollama sendMessage failed context=OllamaProvider.sendMessage baseUrl=http://localhost:11434 error=Ollama API error: 400 {"error":"bad"}"#,
                "ERROR quilltap::model::streaming_provider Ollama streaming API error context=OllamaProvider.streamMessage status=500 error=",
                "ERROR quilltap::model::streaming_provider Ollama streamMessage failed context=OllamaProvider.streamMessage baseUrl=http://localhost:11434 error=Ollama API error: 500 ",
                "ERROR quilltap::model::completion_provider Ollama sendMessage failed context=OllamaProvider.sendMessage baseUrl=http://localhost:11434 error=The operation was aborted due to timeout",
                "ERROR quilltap::model::streaming_provider Ollama streamMessage failed context=OllamaProvider.streamMessage baseUrl=http://localhost:11434 error=This operation was aborted",
                "ERROR quilltap::model::streaming_provider Ollama streamMessage failed context=OllamaProvider.streamMessage baseUrl=http://localhost:11434 error=fetch failed",
                "ERROR quilltap::model::streaming_provider Ollama streamMessage failed context=OllamaProvider.streamMessage baseUrl=http://localhost:11434 error=mid-stream",
            ]
        );
    }

    /// OpenRouter's raw path (#7–#10): the stream logs the status line then
    /// its catch; the vision send's `try` wraps the `fetch` alone, so its
    /// non-2xx logs the status line ONLY, a `fetch` throw logs #10, and a
    /// thrown 2xx parse logs nothing.
    #[test]
    fn openrouter_raw_path_lines() {
        let lines = captured(|| {
            let stream = log("OPENROUTER", STREAM, true);
            stream.emit_transport(&TransportError::http(403, "no"));
            stream.emit_transport(&timeout());
            stream.emit_thrown("mid-stream");
            let send = log("OPENROUTER", SEND, true);
            send.emit_transport(&TransportError::http(403, "no"));
            send.emit_transport(&connect());
            send.emit_transport(&timeout());
            send.emit_thrown("Unexpected end of JSON input");
        });
        assert_eq!(
            lines,
            [
                "ERROR quilltap::model::streaming_provider OpenRouter API error context=OpenRouterProvider.streamViaChatCompletions status=403 error=no",
                "ERROR quilltap::model::streaming_provider Error in streamViaChatCompletions context=OpenRouterProvider.streamViaChatCompletions error=OpenRouter API error: 403 - no",
                "ERROR quilltap::model::streaming_provider Error in streamViaChatCompletions context=OpenRouterProvider.streamViaChatCompletions error=This operation was aborted",
                "ERROR quilltap::model::streaming_provider Error in streamViaChatCompletions context=OpenRouterProvider.streamViaChatCompletions error=mid-stream",
                "ERROR quilltap::model::completion_provider OpenRouter API error context=OpenRouterProvider.sendViaChatCompletions status=403 error=no",
                "ERROR quilltap::model::completion_provider Error in sendViaChatCompletions context=OpenRouterProvider.sendViaChatCompletions error=fetch failed",
                "ERROR quilltap::model::completion_provider Error in sendViaChatCompletions context=OpenRouterProvider.sendViaChatCompletions error=The operation was aborted due to timeout",
            ]
        );
    }

    /// The FIELD ORDER of each shape is v4's `context` key order, then the
    /// third argument (`a-sorted-field-capture-cannot-see-v4-field-order`):
    /// the capture keeps declaration order, so the rendered key sequence is
    /// pinned directly.
    #[test]
    fn field_order_is_v4s_context_key_order() {
        let keys = |line: &str| -> Vec<String> {
            line.split(' ')
                .filter_map(|w| w.split_once('=').map(|(k, _)| k.to_string()))
                .collect()
        };
        let lines = captured(|| {
            log("DEEPSEEK", SEND, false).emit_thrown("e");
            log("GOOGLE", SEND, false).emit_thrown("e");
            log("OLLAMA", SEND, false).emit_transport(&TransportError::http(400, "e"));
            log("OPENROUTER", STREAM, true).emit_thrown("e");
        });
        let got: Vec<Vec<String>> = lines.iter().map(|l| keys(l)).collect();
        assert_eq!(
            got,
            [
                vec!["context", "baseUrl", "error"],
                vec!["context", "model", "error"],
                vec!["context", "status", "error"],
                vec!["context", "baseUrl", "error"],
                vec!["context", "error"],
            ]
        );
    }

    /// v4 Google `extractTextFromResponse`'s two WARNs, in its order: `No
    /// candidates…` with no non-empty `candidates` (`blockReason` only when the
    /// body carries one); `No parts…` when the first candidate has no
    /// non-empty `content.parts` (`finishReason` only when present), answering
    /// `content.text` when truthy (P4.150 D2); silence once parts exist; and
    /// the target follows the site.
    #[test]
    fn google_extract_text_warns() {
        use GoogleExtractSite::{Send, Stream};
        let m = "gemini-2.5-flash";
        let (texts, lines) = crate::test_support::captured_with(|| {
            vec![
                emit_google_extract_text_warns(Send, m, &serde_json::json!({})),
                emit_google_extract_text_warns(
                    Send,
                    m,
                    &serde_json::json!({"candidates": [], "promptFeedback": {"blockReason": "SAFETY"}}),
                ),
                emit_google_extract_text_warns(Send, m, &serde_json::json!({"candidates": {}})),
                emit_google_extract_text_warns(Send, m, &serde_json::json!({"candidates": [{}]})),
                emit_google_extract_text_warns(
                    Send,
                    m,
                    &serde_json::json!({"candidates": [{"finishReason": "MAX_TOKENS", "content": {"parts": []}}]}),
                ),
                emit_google_extract_text_warns(
                    Stream,
                    m,
                    &serde_json::json!({"candidates": [{"finishReason": "STOP", "content": {"role": "model", "text": "On content."}}]}),
                ),
                // Silence: the first candidate carries parts (even thought-only).
                emit_google_extract_text_warns(
                    Send,
                    m,
                    &serde_json::json!({"candidates": [{"content": {"parts": [{"text": "x", "thought": true}]}}]}),
                ),
            ]
        });
        assert_eq!(
            texts,
            [
                None,
                None,
                None,
                None,
                None,
                Some("On content.".to_string()),
                None
            ]
        );
        assert_eq!(
            lines,
            [
                "WARN quilltap::model::completion_provider No candidates found in Google response context=GoogleProvider.extractTextFromResponse modelName=gemini-2.5-flash",
                "WARN quilltap::model::completion_provider No candidates found in Google response context=GoogleProvider.extractTextFromResponse modelName=gemini-2.5-flash blockReason=SAFETY",
                "WARN quilltap::model::completion_provider No candidates found in Google response context=GoogleProvider.extractTextFromResponse modelName=gemini-2.5-flash",
                "WARN quilltap::model::completion_provider No parts found in Google response candidate context=GoogleProvider.extractTextFromResponse modelName=gemini-2.5-flash",
                "WARN quilltap::model::completion_provider No parts found in Google response candidate context=GoogleProvider.extractTextFromResponse modelName=gemini-2.5-flash finishReason=MAX_TOKENS",
                "WARN quilltap::model::streaming_provider No parts found in Google response candidate context=GoogleProvider.extractTextFromResponse modelName=gemini-2.5-flash finishReason=STOP",
            ]
        );
    }

    /// v4 `hasTools || hasImages`.
    #[test]
    fn openrouter_streaming_raw_path_predicate() {
        let hi = [StreamMessage::user("hi")];
        assert!(!openrouter_streaming_takes_raw_path(&hi, None));
        assert!(!openrouter_streaming_takes_raw_path(
            &hi,
            Some(&serde_json::json!([]))
        ));
        assert!(!openrouter_streaming_takes_raw_path(
            &hi,
            Some(&serde_json::json!({"a": 1}))
        ));
        assert!(openrouter_streaming_takes_raw_path(
            &hi,
            Some(&serde_json::json!([{"type": "function"}]))
        ));
        let mut img = StreamMessage::user("what is this");
        if let StreamMessage::User { attachments, .. } = &mut img {
            attachments.push(serde_json::json!({
                "id": "a1", "filename": "x.png", "mimeType": "image/png", "data": "AAAA"
            }));
        }
        assert!(openrouter_streaming_takes_raw_path(&[img], None));
    }
}
