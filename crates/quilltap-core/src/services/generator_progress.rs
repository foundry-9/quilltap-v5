//! The character-generator progress emitter (`p4.9k`, P4.9K0, §B.5).
//!
//! v4 hands each generator runner an `onProgress(event)` callback that its
//! route's `ReadableStream` turns into one `data: <JSON>\n\n` SSE frame. v5's
//! boundary streams only on the Event channel, so this emitter is that callback:
//! every `onProgress` becomes ONE [`Event::generator_progress`] on the engine's
//! EXISTING broadcast, scope-tagged by the client-minted `progressId`.
//!
//! ## Why this is NOT a second `CreationProgressBus`
//!
//! [`crate::services::creation_progress`] pairs its emitter with a replay buffer
//! because the Green Room's dialog opens its reader around the instant the
//! create POST fires, and a frame emitted in that gap would be lost forever.
//! A generator run has no such gap: the client mints the `progressId`, opens its
//! subscription, and only THEN dispatches — and the REST SSE edge
//! (`quilltap-web::generator_sse`) subscribes before it polls the dispatch future
//! at all. So the shape is mirrored (an emitter built from an `Option<&str>`
//! progress id plus the engine's event sender; `None` is entirely inert) while
//! the buffer is deliberately absent. No new bus, no scheduler — the core has
//! neither (`core-has-no-tokio-scheduler`).
//!
//! The terminal payload does NOT ride this channel alone: the dispatch call
//! resolves with `{ "terminal": <the last event> }` as well (§B.1), so a client
//! that missed the last frame still learns the outcome from the response.

use serde_json::Value;
use tokio::sync::broadcast;

use crate::api::types::{Event, GeneratorKind};

/// A per-run emitter handed to a generator (v4's `onProgress`). Inert when the
/// caller supplied no `progressId`, so a runner never has to branch on whether
/// progress is being tracked.
#[derive(Clone)]
pub struct GeneratorProgressEmitter {
    inner: Option<Active>,
}

#[derive(Clone)]
struct Active {
    progress_id: String,
    generator: GeneratorKind,
    events: broadcast::Sender<Event>,
}

impl GeneratorProgressEmitter {
    /// The no-op emitter — `progressId` absent or empty.
    pub fn inert() -> Self {
        GeneratorProgressEmitter { inner: None }
    }

    /// An active emitter for a `progress_id`.
    pub fn active(
        progress_id: impl Into<String>,
        generator: GeneratorKind,
        events: broadcast::Sender<Event>,
    ) -> Self {
        GeneratorProgressEmitter {
            inner: Some(Active {
                progress_id: progress_id.into(),
                generator,
                events,
            }),
        }
    }

    /// Build from an optional `progress_id`: `None` or empty becomes inert (the
    /// `CreationProgressEmitter::from_id` precedent, same empty-string rule).
    pub fn from_id(
        progress_id: Option<&str>,
        generator: GeneratorKind,
        events: broadcast::Sender<Event>,
    ) -> Self {
        match progress_id {
            Some(id) if !id.is_empty() => Self::active(id, generator, events),
            _ => Self::inert(),
        }
    }

    /// Publish one v4 progress event VERBATIM. Best-effort: a lagging or absent
    /// subscriber is fine (the frame is a narration, not the outcome).
    pub fn emit(&self, event: Value) {
        let Some(active) = &self.inner else {
            return;
        };
        let _ = active.events.send(Event::generator_progress(
            active.progress_id.clone(),
            active.generator,
            event,
        ));
    }

    /// Whether this emitter is active (has a `progress_id`).
    pub fn is_active(&self) -> bool {
        self.inner.is_some()
    }

    /// The generator this emitter narrates, for the runners' log context bags.
    pub fn generator(&self) -> Option<GeneratorKind> {
        self.inner.as_ref().map(|a| a.generator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::EventPayload;
    use serde_json::json;

    fn channel() -> (broadcast::Sender<Event>, broadcast::Receiver<Event>) {
        broadcast::channel(16)
    }

    /// The `None`-progress guard: an inert emitter publishes NOTHING. Dropping
    /// the `let Some(active) = … else { return }` in `emit` reddens this.
    #[test]
    fn an_absent_progress_id_emits_nothing() {
        let (tx, mut rx) = channel();
        let e = GeneratorProgressEmitter::from_id(None, GeneratorKind::Wizard, tx.clone());
        assert!(!e.is_active());
        e.emit(json!({"type": "status"}));
        assert!(rx.try_recv().is_err(), "an inert emitter published a frame");
    }

    /// v4's own emptiness rule (the `CreationProgressEmitter` precedent): an
    /// empty string is not a progress id.
    #[test]
    fn an_empty_progress_id_is_inert_too() {
        let (tx, mut rx) = channel();
        let e = GeneratorProgressEmitter::from_id(Some(""), GeneratorKind::Optimizer, tx);
        e.emit(json!({"type": "status"}));
        assert!(rx.try_recv().is_err());
    }

    /// The positive arm — and the WIRE bytes, §B.5 verbatim. The `event` object
    /// passes through untouched, key order included.
    #[test]
    fn an_active_emitter_publishes_the_contract_bytes() {
        let (tx, mut rx) = channel();
        let e = GeneratorProgressEmitter::from_id(Some("p-1"), GeneratorKind::AiImport, tx);
        assert!(e.is_active());
        assert_eq!(e.generator(), Some(GeneratorKind::AiImport));
        // Deliberately NOT alphabetical: the round's contract is that the v4
        // object-literal order survives.
        e.emit(json!({"type": "step_complete", "step": "basics", "index": 2}));
        let ev = rx.try_recv().expect("a frame");
        assert!(matches!(ev.payload, EventPayload::GeneratorProgress(_)));
        assert_eq!(
            serde_json::to_string(&ev).unwrap(),
            r#"{"progressId":"p-1","type":"generatorProgress","generator":"aiImport","event":{"type":"step_complete","step":"basics","index":2}}"#
        );
    }

    /// The three wire spellings (§B.5), pinned in both directions.
    #[test]
    fn the_three_generator_spellings_are_the_contract() {
        for (kind, wire) in [
            (GeneratorKind::Optimizer, "optimizer"),
            (GeneratorKind::Wizard, "wizard"),
            (GeneratorKind::AiImport, "aiImport"),
        ] {
            assert_eq!(kind.as_wire(), wire);
            assert_eq!(kind.to_string(), wire);
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<GeneratorKind>(json!(wire)).unwrap(),
                kind
            );
        }
    }
}
