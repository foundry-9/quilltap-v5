//! Differential harness library: shared corpus + equivalence helpers.
//! The actual assertions live in tests/ (run via `cargo test -p quilltap-harness`).

use quilltap_core::memory_weighting::MemoryInputs;

/// Fixed reference clock — MUST equal NOW in harness/oracle/cases/memory-weighting.ts
/// (2026-06-27T12:00:00.000Z). Expressed as epoch millis.
/// (Verified: `new Date("2026-06-27T12:00:00.000Z").getTime()` === this.)
pub const NOW_MS: f64 = 1_782_561_600_000.0;

/// Parse an ISO-8601 UTC instant (the exact subset the corpus uses:
/// `YYYY-MM-DDTHH:MM:SS.sssZ`) to epoch millis, matching JS `new Date(s).getTime()`.
/// Deliberately tiny and total over the corpus's well-formed inputs; the real
/// core will use a date crate. Panics on malformed input (a corpus bug, not a
/// runtime case).
pub fn iso_to_ms(s: &str) -> f64 {
    // Split "YYYY-MM-DDTHH:MM:SS.sssZ"
    let (date, rest) = s.split_once('T').expect("iso: missing T");
    let time = rest.strip_suffix('Z').expect("iso: missing Z");
    let mut dparts = date.split('-');
    let y: i64 = dparts.next().unwrap().parse().unwrap();
    let mo: i64 = dparts.next().unwrap().parse().unwrap();
    let d: i64 = dparts.next().unwrap().parse().unwrap();
    let (hms, millis) = match time.split_once('.') {
        Some((a, b)) => (a, b.parse::<i64>().unwrap()),
        None => (time, 0),
    };
    let mut tparts = hms.split(':');
    let h: i64 = tparts.next().unwrap().parse().unwrap();
    let mi: i64 = tparts.next().unwrap().parse().unwrap();
    let se: i64 = tparts.next().unwrap().parse().unwrap();

    // Days since Unix epoch via a civil-from-days algorithm (Howard Hinnant's),
    // matching the proleptic Gregorian calendar JS Date uses for UTC.
    let days = days_from_civil(y, mo, d);
    let secs = days * 86_400 + h * 3_600 + mi * 60 + se;
    (secs as f64) * 1000.0 + (millis as f64)
}

/// Days from 1970-01-01 for a civil (y, m, d), proleptic Gregorian. m in 1..=12.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Build the corpus — MUST mirror CORPUS in the TS oracle case exactly
/// (same ids, same fields, same timestamps).
pub fn corpus() -> Vec<(&'static str, MemoryInputs)> {
    let m = |importance: f64,
             reinforced: Option<f64>,
             created: &str,
             last_reinforced: Option<&str>,
             last_accessed: Option<&str>,
             reinforcement_count: Option<u64>,
             graph_degree: usize| MemoryInputs {
        importance,
        reinforced_importance: reinforced,
        created_at_ms: iso_to_ms(created),
        last_reinforced_at_ms: last_reinforced.map(iso_to_ms),
        last_accessed_at_ms: last_accessed.map(iso_to_ms),
        reinforcement_count,
        graph_degree,
        kind_episodic: false,
        occurred_at_ms: None,
    };

    vec![
        (
            "fresh-high",
            m(0.9, None, "2026-06-27T00:00:00.000Z", None, None, None, 0),
        ),
        (
            "old-high-floor",
            m(0.9, None, "2025-06-27T00:00:00.000Z", None, None, None, 0),
        ),
        (
            "reinforced-recent",
            m(
                0.5,
                Some(0.8),
                "2026-01-01T00:00:00.000Z",
                Some("2026-06-20T00:00:00.000Z"),
                None,
                None,
                0,
            ),
        ),
        (
            "retrieval-doesnt-reset",
            m(
                0.6,
                None,
                "2026-03-01T00:00:00.000Z",
                None,
                Some("2026-06-26T00:00:00.000Z"),
                None,
                0,
            ),
        ),
        (
            "graph-heavy",
            m(0.4, None, "2026-05-01T00:00:00.000Z", None, None, None, 8),
        ),
        (
            "reinforce-saturate",
            m(
                0.5,
                None,
                "2026-06-10T00:00:00.000Z",
                None,
                None,
                Some(64),
                0,
            ),
        ),
        (
            "recent-access-bonus",
            m(
                0.3,
                None,
                "2026-04-01T00:00:00.000Z",
                None,
                Some("2026-06-01T00:00:00.000Z"),
                None,
                0,
            ),
        ),
        (
            "stale-access",
            m(
                0.3,
                None,
                "2026-01-01T00:00:00.000Z",
                None,
                Some("2026-01-15T00:00:00.000Z"),
                None,
                0,
            ),
        ),
        (
            "zero-importance",
            m(0.0, None, "2026-06-01T00:00:00.000Z", None, None, None, 0),
        ),
        (
            "content-cap",
            m(1.0, None, "2026-06-27T06:00:00.000Z", None, None, None, 0),
        ),
        // ── Episodic spine (v4 8bf3cb5f) — mirrors the TS corpus additions ──
        (
            "episodic-old-low",
            MemoryInputs {
                kind_episodic: true,
                ..m(0.2, None, "2025-09-01T00:00:00.000Z", None, None, None, 0)
            },
        ),
        (
            "episodic-clamp",
            MemoryInputs {
                kind_episodic: true,
                ..m(
                    1.0,
                    None,
                    "2026-06-27T00:00:00.000Z",
                    None,
                    Some("2026-06-26T00:00:00.000Z"),
                    Some(64),
                    4,
                )
            },
        ),
        (
            "semantic-explicit",
            m(0.5, None, "2026-06-01T00:00:00.000Z", None, None, None, 0),
        ),
        (
            "event-clock-age",
            MemoryInputs {
                kind_episodic: true,
                occurred_at_ms: Some(iso_to_ms("2026-05-20T00:00:00.000Z")),
                ..m(0.5, None, "2026-06-25T00:00:00.000Z", None, None, None, 0)
            },
        ),
        (
            "event-clock-age-reinforced",
            MemoryInputs {
                kind_episodic: true,
                occurred_at_ms: Some(iso_to_ms("2025-11-02T00:00:00.000Z")),
                ..m(
                    0.5,
                    None,
                    "2026-01-10T00:00:00.000Z",
                    Some("2026-06-25T00:00:00.000Z"),
                    None,
                    None,
                    0,
                )
            },
        ),
        // occurredAt 'not-a-date' / '' → unparsable/falsy → the write clock
        // (`event_time_ms` yields None on both, matching v4's NaN fallback).
        (
            "event-clock-unparsable",
            m(0.5, None, "2026-06-25T00:00:00.000Z", None, None, None, 0),
        ),
        (
            "event-clock-empty",
            m(0.5, None, "2026-06-25T00:00:00.000Z", None, None, None, 0),
        ),
    ]
}

/// ============================================================================
/// A harness-only [`PixelCodec`] whose encode CHANGES the bytes deterministically
/// (P4.D152).
/// ============================================================================
///
/// Bug 117 is an ORDERING defect: `files.sha256` was computed before the bridge
/// transcoded, so it named bytes that were never stored. Nothing in the
/// production build can measure that, because every production chat-upload call
/// hands the bridges [`quilltap_core::services::file_storage::NotConfiguredPixelCodec`]
/// — every encode fails, the policy layer passes the ORIGINAL bytes through, and
/// the input hash and the stored hash are trivially equal whichever order they
/// are computed in. The comparand would be vacuously true pre-fix.
///
/// So the differential drives the upload with a codec that DOES change the
/// bytes: `encode_webp` returns a fixed prefix followed by the input. v4's real
/// sharp changes the bytes too — different bytes, which is exactly why the
/// comparand is the WITHIN-TREE boolean `files.sha256 == doc_mount_blobs.sha256`
/// and never the hash string itself. Pre-fix v5 yields `false` on a PNG upload
/// and post-fix `true`; v4 with real sharp yields `true` both times.
///
/// `measure` returns nothing, matching the not-configured codec — dimensions are
/// not part of this comparand and v4's sharp answers real ones.
pub struct PrefixingPixelCodec;

/// The prefix `encode_webp` prepends. Arbitrary, fixed, and deliberately not
/// valid WebP: nothing downstream of the bridge decodes these bytes, and a
/// changed-bytes result is the entire point.
pub const PREFIXING_CODEC_PREFIX: &[u8] = b"QTAP-HARNESS-WEBP:";

impl quilltap_core::services::file_storage::PixelCodec for PrefixingPixelCodec {
    fn encode_webp(
        &self,
        bytes: &[u8],
        _quality: i64,
        _effort: Option<i64>,
        _animated: bool,
    ) -> Result<Vec<u8>, String> {
        let mut out = PREFIXING_CODEC_PREFIX.to_vec();
        out.extend_from_slice(bytes);
        Ok(out)
    }

    fn measure(&self, _bytes: &[u8]) -> (Option<i64>, Option<i64>) {
        (None, None)
    }
}

/// ============================================================================
/// A harness-only SCRIPTED [`quilltap_core::files::image_processing::ImageTranscoder`]
/// (P4.D198 — v4 `bcd7e4852`, bug 151).
/// ============================================================================
///
/// The bug-151 transport shrink is a DECISION over a pixel op, and the two
/// implementations' pixel ops cannot be byte-compared: v4 encodes through sharp,
/// v5 through libwebp, and D19 says the operation is ported while encoded byte
/// parity is neither required nor reachable. Comparing real encoders would
/// compare nothing but the encoders.
///
/// So both sides run the SAME per-case SCRIPT below a deterministic encoder —
/// v4 via a `jest.doMock('sharp')` built by `harness/oracle/lib/shrink-script.ts`,
/// v5 via this type — and the differential proves the decision: the arm order,
/// the ceiling, which rungs are asked for and in what order, which encode
/// becomes `best`, when a grown encode is discarded, and what a throw does to a
/// partial ladder. Because the scripts agree, the RESULT BYTES agree too, so
/// `buffer` is a real comparand rather than a length check.
///
/// Keep the byte patterns in step with the TS half: an input is
/// `(i * 7 + 13) & 0xff` and a scripted encode is `(quality + i) & 0xff`. Both
/// sides assert their own generators against fixed probes, so a drift in either
/// is loud rather than silently voiding every byte comparand.
pub mod scripted_transcoder {
    use std::sync::Mutex;

    use quilltap_core::files::image_processing::{ImageMetadata, ImageTranscoder, OutputFormat};

    /// What a metadata probe of the ORIGINAL buffer answers.
    #[derive(Clone, Debug)]
    pub enum ScriptMetadata {
        /// sharp resolved a bag; either dimension may be absent.
        Dims(Option<i64>, Option<i64>),
        /// sharp REJECTED. v5's `metadata` cannot, so this answers no
        /// dimensions and the script's first step carries the same message as
        /// an `Err` — the recorded mechanism difference (see
        /// `ImageTranscoder::shrink_to_webp`'s doc).
        Throws(String),
    }

    /// One case's (or one file's) scripted encoder behaviour.
    #[derive(Clone, Debug)]
    pub struct ShrinkScript {
        pub metadata: ScriptMetadata,
        /// What a probe of a NON-original buffer answers.
        pub final_dims: (Option<i64>, Option<i64>),
        /// One entry per ladder rung, in order: `Ok(len)` or `Err(message)`.
        pub steps: Vec<Result<usize, String>>,
    }

    /// One resize+encode the ladder asked for.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct RecordedCall {
        pub max_edge: i64,
        pub quality: i64,
    }

    /// The scripted encoder. Scripts are keyed by the ORIGINAL bytes they
    /// describe, so ONE instance can serve several files in a single load (the
    /// `file_attachment_tier3` shape) as well as one case at a time.
    pub struct ScriptedTranscoder {
        scripts: Vec<(Vec<u8>, ShrinkScript)>,
        calls: Mutex<Vec<(usize, RecordedCall)>>,
        /// The script whose original was probed most recently. A probe of a
        /// NON-original buffer is v4's `sharp(best).metadata()`, which can only
        /// belong to the shrink currently in flight — the JS mock knows it from
        /// its closure, and this is the same fact tracked explicitly.
        current: Mutex<Option<usize>>,
    }

    impl ScriptedTranscoder {
        pub fn new() -> Self {
            ScriptedTranscoder {
                scripts: Vec::new(),
                calls: Mutex::new(Vec::new()),
                current: Mutex::new(None),
            }
        }

        /// Register a script for one original buffer.
        pub fn with_script(mut self, original: Vec<u8>, script: ShrinkScript) -> Self {
            self.scripts.push((original, script));
            self
        }

        /// Every `(max_edge, quality)` asked for, in order, across all scripts.
        pub fn calls(&self) -> Vec<RecordedCall> {
            self.calls.lock().unwrap().iter().map(|(_, c)| *c).collect()
        }

        /// The calls asked for on ONE script's behalf, in order.
        pub fn calls_for(&self, original: &[u8]) -> Vec<RecordedCall> {
            let Some(idx) = self.index_of(original) else {
                return Vec::new();
            };
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(i, _)| *i == idx)
                .map(|(_, c)| *c)
                .collect()
        }

        /// Forget every recorded call (between cases sharing one instance).
        pub fn reset_calls(&self) {
            self.calls.lock().unwrap().clear();
            *self.current.lock().unwrap() = None;
        }

        fn index_of(&self, buffer: &[u8]) -> Option<usize> {
            self.scripts.iter().position(|(orig, _)| orig == buffer)
        }
    }

    impl Default for ScriptedTranscoder {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ImageTranscoder for ScriptedTranscoder {
        fn metadata(&self, buffer: &[u8]) -> ImageMetadata {
            if let Some(idx) = self.index_of(buffer) {
                *self.current.lock().unwrap() = Some(idx);
                return match &self.scripts[idx].1.metadata {
                    // v5's probe is infallible: a rejection maps to "no
                    // dimensions", which is what takes the ladder rather than
                    // the early return.
                    ScriptMetadata::Throws(_) => ImageMetadata::default(),
                    ScriptMetadata::Dims(w, h) => ImageMetadata {
                        width: *w,
                        height: *h,
                        has_alpha: false,
                    },
                };
            }
            // Not an original: this is the post-shrink probe of the shrink in
            // flight.
            let idx = self
                .current
                .lock()
                .unwrap()
                .expect("a metadata probe of non-original bytes with no shrink in flight");
            let (w, h) = self.scripts[idx].1.final_dims;
            ImageMetadata {
                width: w,
                height: h,
                has_alpha: false,
            }
        }

        fn resize_step(
            &self,
            buffer: &[u8],
            _target_width: i64,
            _format: OutputFormat,
            _quality: i64,
        ) -> Vec<u8> {
            // The provider-ceiling BACKSTOP's seam. Unused by the bug-151
            // families (every corpus image is under the 4 MB per-image cap, so
            // `resize_image_for_provider` early-returns), and required by the
            // trait — so it returns the input rather than pretending to encode.
            // If a corpus row ever reaches it, the recorded calls will show no
            // shrink rung for bytes that changed, which is the loud version.
            buffer.to_vec()
        }

        fn shrink_to_webp(
            &self,
            buffer: &[u8],
            max_edge: i64,
            quality: i64,
        ) -> Result<Vec<u8>, String> {
            let idx = self
                .index_of(buffer)
                .unwrap_or_else(|| panic!("no script for a {}-byte shrink input", buffer.len()));
            let mut calls = self.calls.lock().unwrap();
            let rung = calls.iter().filter(|(i, _)| *i == idx).count();
            calls.push((idx, RecordedCall { max_edge, quality }));
            drop(calls);
            match self.scripts[idx].1.steps.get(rung) {
                Some(Ok(len)) => Ok(encoded_bytes(quality, *len)),
                Some(Err(m)) => Err(m.clone()),
                None => panic!(
                    "the ladder asked for rung {} (quality {quality}); the script carries {} \
                     — a CORPUS bug, not a port one",
                    rung + 1,
                    self.scripts[idx].1.steps.len()
                ),
            }
        }
    }

    /// The input buffer a corpus `originalLen` denotes: `(i * 7 + 13) & 0xff`.
    /// Mirrors `originalBuffer` in `harness/oracle/lib/shrink-script.ts`.
    pub fn original_bytes(len: usize) -> Vec<u8> {
        (0..len).map(|i| ((i * 7 + 13) & 0xff) as u8).collect()
    }

    /// A scripted encode's bytes: `(quality + i) & 0xff`. Mirrors
    /// `encodedBuffer` in `harness/oracle/lib/shrink-script.ts`. Distinct from
    /// `original_bytes` at i=0 for every ladder quality (78/65/55/45 vs 13),
    /// which is what lets a metadata probe tell an encode from the input.
    pub fn encoded_bytes(quality: i64, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| ((quality as usize + i) & 0xff) as u8)
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The two generators, against the same fixed probes the TS half
        /// asserts. If either side drifts, every `buffer` comparand in both
        /// bug-151 families silently stops meaning anything — so both sides
        /// pin them.
        #[test]
        fn the_byte_patterns_match_the_typescript_half() {
            assert_eq!(original_bytes(5), vec![13u8, 20, 27, 34, 41]);
            assert_eq!(encoded_bytes(78, 4), vec![78u8, 79, 80, 81]);
            // The wrap at 256 (`& 0xff`) on both.
            assert_eq!(original_bytes(40)[36], ((36 * 7 + 13) & 0xff) as u8);
            assert_eq!(encoded_bytes(250, 10)[9], 3);
        }

        #[test]
        fn a_script_answers_its_own_rungs_and_records_them() {
            let orig = original_bytes(32);
            let t = ScriptedTranscoder::new().with_script(
                orig.clone(),
                ShrinkScript {
                    metadata: ScriptMetadata::Dims(Some(2048), Some(1536)),
                    final_dims: (Some(683), Some(1024)),
                    steps: vec![Ok(10), Err("boom".to_string())],
                },
            );
            let m = t.metadata(&orig);
            assert_eq!((m.width, m.height), (Some(2048), Some(1536)));
            assert_eq!(
                t.shrink_to_webp(&orig, 1024, 78).unwrap(),
                encoded_bytes(78, 10)
            );
            // The post-shrink probe answers the FINAL dims.
            let fm = t.metadata(&encoded_bytes(78, 10));
            assert_eq!((fm.width, fm.height), (Some(683), Some(1024)));
            assert_eq!(t.shrink_to_webp(&orig, 1024, 65), Err("boom".to_string()));
            assert_eq!(
                t.calls(),
                vec![
                    RecordedCall {
                        max_edge: 1024,
                        quality: 78
                    },
                    RecordedCall {
                        max_edge: 1024,
                        quality: 65
                    },
                ]
            );
        }

        #[test]
        fn a_throwing_probe_answers_no_dimensions() {
            let orig = original_bytes(8);
            let t = ScriptedTranscoder::new().with_script(
                orig.clone(),
                ShrinkScript {
                    metadata: ScriptMetadata::Throws("nope".to_string()),
                    final_dims: (None, None),
                    steps: vec![Err("nope".to_string())],
                },
            );
            assert_eq!(t.metadata(&orig), ImageMetadata::default());
        }

        #[test]
        fn two_scripts_keep_their_own_rung_counters() {
            let a = original_bytes(16);
            let b = original_bytes(24);
            let t = ScriptedTranscoder::new()
                .with_script(
                    a.clone(),
                    ShrinkScript {
                        metadata: ScriptMetadata::Dims(Some(2048), Some(2048)),
                        final_dims: (Some(1024), Some(1024)),
                        steps: vec![Ok(1), Ok(2)],
                    },
                )
                .with_script(
                    b.clone(),
                    ShrinkScript {
                        metadata: ScriptMetadata::Dims(Some(4096), Some(4096)),
                        final_dims: (Some(1024), Some(1024)),
                        steps: vec![Ok(3)],
                    },
                );
            assert_eq!(t.shrink_to_webp(&a, 1024, 78).unwrap().len(), 1);
            assert_eq!(t.shrink_to_webp(&b, 1024, 78).unwrap().len(), 3);
            assert_eq!(t.shrink_to_webp(&a, 1024, 65).unwrap().len(), 2);
            assert_eq!(
                t.calls_for(&b),
                vec![RecordedCall {
                    max_edge: 1024,
                    quality: 78
                }]
            );
            assert_eq!(t.calls_for(&a).len(), 2);
        }
    }
}

#[cfg(test)]
mod self_tests {
    use super::*;
    #[test]
    fn now_constant_matches_iso() {
        // The NOW_MS constant must equal the parsed oracle NOW.
        assert_eq!(NOW_MS, iso_to_ms("2026-06-27T12:00:00.000Z"));
    }
}
