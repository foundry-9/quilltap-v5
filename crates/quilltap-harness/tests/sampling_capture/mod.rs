//! Shared sampling-at-the-wire capture for the P4.D83 call-site differentials.
//!
//! v4's `d89babc4` routes four call sites through `resolveSamplingParams`, and
//! every one of them is measured by a tier-3 family whose seam is BELOW the
//! resolution: the oracle's mocked provider receives the resolved
//! `temperature` / `maxTokens` / `topP` and records them per call key. The Rust
//! side must produce the identical three numbers from its own port of the same
//! call site.
//!
//! Before this existed the corpora were blind — the canned key carries only the
//! temperature, so a dropped Max Tokens or an unread Top P changed nothing any
//! assertion could see. (Two of the four fixtures also carried no `parameters`
//! bag at all, which is the `p4.d79-multichar-prefill-lane` lesson: a bag left
//! at `{}` measures nothing downstream of it.)

#![allow(dead_code)] // each family uses the parts it needs

use std::collections::HashMap;
use std::sync::Mutex;

use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
};
use quilltap_core::model::stream::{StreamParams, StreamingCompletionProvider};
use serde_json::{json, Value};

/// The key function a capture uses — whatever key the family's oracle recorded.
type CompletionKeyFn = Box<dyn Fn(&str, &CompletionParams) -> String + Send + Sync>;
type StreamKeyFn = Box<dyn Fn(&str, &StreamParams) -> String + Send + Sync>;

/// The three knobs as `{temperature?, maxTokens?, topP?}` with absent knobs
/// OMITTED — the shape `JSON.stringify` gives v4's `SamplingParams`, and the
/// shape the oracle cases record.
pub fn sampling_object(
    temperature: Option<f64>,
    max_tokens: Option<i64>,
    top_p: Option<f64>,
) -> Value {
    let mut o = serde_json::Map::new();
    if let Some(t) = temperature {
        o.insert("temperature".into(), json!(t));
    }
    if let Some(m) = max_tokens {
        o.insert("maxTokens".into(), json!(m));
    }
    if let Some(p) = top_p {
        o.insert("topP".into(), json!(p));
    }
    Value::Object(o)
}

/// Wraps a canned [`CompletionProvider`] and records the sampling knobs each
/// call carried, keyed by whatever key function the family's oracle used.
pub struct SamplingCapture<P: CompletionProvider> {
    inner: P,
    key_of: CompletionKeyFn,
    recorded: Mutex<HashMap<String, Value>>,
}

impl<P: CompletionProvider> SamplingCapture<P> {
    pub fn new(
        inner: P,
        key_of: impl Fn(&str, &CompletionParams) -> String + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner,
            key_of: Box::new(key_of),
            recorded: Mutex::new(HashMap::new()),
        }
    }

    /// Diff every recorded call against the oracle's `sampling` objects.
    /// Panics with the offending key on the first mismatch, and refuses to pass
    /// vacuously when nothing was recorded.
    pub fn assert_matches(&self, expected: &HashMap<String, Value>, what: &str) {
        let recorded = self.recorded.lock().unwrap();
        assert!(
            !recorded.is_empty(),
            "{what}: no completion call recorded — the sampling assertion would be vacuous"
        );
        for (key, got) in recorded.iter() {
            let want = expected.get(key).unwrap_or_else(|| {
                panic!("{what}: a call with no oracle-recorded sampling for key:\n{key}")
            });
            assert_eq!(
                got, want,
                "{what}: sampling at wire mismatch for key:\n{key}"
            );
        }
        // The symmetric direction (§3 of the 93ed8abf round): every call the
        // ORACLE recorded must have been made here too — a v5 path that
        // silently makes fewer calls (≥1) must not pass on the subset.
        for key in expected.keys() {
            assert!(
                recorded.contains_key(key),
                "{what}: the oracle recorded a call v5 never made:\n{key}"
            );
        }
    }
}

impl<P: CompletionProvider> CompletionProvider for SamplingCapture<P> {
    fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        let key = (self.key_of)(provider, params);
        self.recorded.lock().unwrap().entry(key).or_insert_with(|| {
            sampling_object(params.temperature, params.max_tokens, params.top_p)
        });
        self.inner.send_message(provider, base_url, params)
    }

    // Forward the anchored entry point so a wrapped provider's own override
    // survives the wrapper (the hazard `completion.rs`'s Arc impl names; found
    // by the a14a1811 §3 review). Today every wrapped inner is canned — this is
    // insurance for the first placement-sensitive wrap, not a live fix.
    fn send_message_with_anchor(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
        attachment_anchor_index: Option<usize>,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        let key = (self.key_of)(provider, params);
        self.recorded.lock().unwrap().entry(key).or_insert_with(|| {
            sampling_object(params.temperature, params.max_tokens, params.top_p)
        });
        self.inner
            .send_message_with_anchor(provider, base_url, params, attachment_anchor_index)
    }
}

/// The streaming twin of [`SamplingCapture`] — same contract, for families whose
/// call site builds a [`StreamParams`].
pub struct SamplingStreamCapture<P: StreamingCompletionProvider> {
    inner: P,
    key_of: StreamKeyFn,
    recorded: Mutex<HashMap<String, Value>>,
}

impl<P: StreamingCompletionProvider> SamplingStreamCapture<P> {
    pub fn new(
        inner: P,
        key_of: impl Fn(&str, &StreamParams) -> String + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner,
            key_of: Box::new(key_of),
            recorded: Mutex::new(HashMap::new()),
        }
    }

    pub fn assert_matches(&self, expected: &HashMap<String, Value>, what: &str) {
        let recorded = self.recorded.lock().unwrap();
        assert!(
            !recorded.is_empty(),
            "{what}: no stream call recorded — the sampling assertion would be vacuous"
        );
        for (key, got) in recorded.iter() {
            let want = expected.get(key).unwrap_or_else(|| {
                panic!("{what}: a call with no oracle-recorded sampling for key:\n{key}")
            });
            assert_eq!(
                got, want,
                "{what}: sampling at wire mismatch for key:\n{key}"
            );
        }
        // The symmetric direction (§3 of the 93ed8abf round): every call the
        // ORACLE recorded must have been made here too — a v5 path that
        // silently makes fewer calls (≥1) must not pass on the subset.
        for key in expected.keys() {
            assert!(
                recorded.contains_key(key),
                "{what}: the oracle recorded a call v5 never made:\n{key}"
            );
        }
    }
}

impl<P: StreamingCompletionProvider> StreamingCompletionProvider for SamplingStreamCapture<P> {
    fn stream_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &StreamParams,
    ) -> impl std::future::Future<
        Output = tokio::sync::mpsc::Receiver<quilltap_core::model::stream::StreamChunkResult>,
    > + Send {
        let key = (self.key_of)(provider, params);
        self.recorded.lock().unwrap().entry(key).or_insert_with(|| {
            sampling_object(params.temperature, params.max_tokens, params.top_p)
        });
        self.inner.stream_message(provider, base_url, params)
    }
}
