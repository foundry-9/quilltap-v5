//! The Almanack — phase manifest (v4 `lib/tools/almanack/phases.ts`).
//!
//! The seven named phases the report pipeline walks, in order. Pure data,
//! shared by the server (which publishes a `phase` progress frame on entering
//! each) and — mirrored byte-for-byte in TypeScript by P4.38 — the card that
//! draws them as progress-bar segments, so the two can never drift.
//!
//! `estimated_share` is the fraction of a typical run each phase occupies (v4
//! eyeballed them from a warm-cache run); they sum to 1.

/// One phase of the report pipeline (v4 `AlmanackPhase`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlmanackPhase {
    /// The stable key (v4 `AlmanackPhaseKey`).
    pub key: &'static str,
    /// User-facing, present-continuous, house voice.
    pub label: &'static str,
    /// Fraction of a typical run. Sums to 1 across the manifest.
    pub estimated_share: f64,
}

/// v4 `ALMANACK_PHASES` — labels and shares transcribed byte-for-byte.
pub const ALMANACK_PHASES: [AlmanackPhase; 7] = [
    AlmanackPhase {
        key: "premises",
        label: "Taking the measure of the premises…",
        estimated_share: 0.05,
    },
    AlmanackPhase {
        key: "machinery",
        label: "Cataloguing the machinery…",
        estimated_share: 0.25,
    },
    AlmanackPhase {
        key: "ledgers",
        label: "Auditing the ledgers…",
        estimated_share: 0.2,
    },
    AlmanackPhase {
        key: "scriptorium",
        label: "Touring the Scriptorium…",
        estimated_share: 0.2,
    },
    AlmanackPhase {
        key: "personae",
        label: "Assembling the dramatis personae…",
        estimated_share: 0.12,
    },
    AlmanackPhase {
        key: "wire-records",
        label: "Reading the wire records…",
        estimated_share: 0.13,
    },
    AlmanackPhase {
        key: "binding",
        label: "Binding the volume…",
        estimated_share: 0.05,
    },
];

/// v4 `ALMANACK_PHASE_COUNT`.
pub const ALMANACK_PHASE_COUNT: usize = ALMANACK_PHASES.len();

/// v4 `ALMANACK_NAME` — the feature's user-facing name.
pub const ALMANACK_NAME: &str = "The Almanack";

/// v4 `ALMANACK_TITLE` — the name as it appears in headings and card titles.
/// The parenthetical is deliberate: it keeps the feature findable for anyone
/// searching the old "capabilities report" wording.
pub const ALMANACK_TITLE: &str = "The Almanack (System Report)";

/// The 1-based index of a phase key in the manifest, or `None` if unknown
/// (v4 `announce`'s `findIndex(...) < 0` guard).
pub fn phase_index(key: &str) -> Option<usize> {
    ALMANACK_PHASES
        .iter()
        .position(|p| p.key == key)
        .map(|i| i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_sum_to_one() {
        let sum: f64 = ALMANACK_PHASES.iter().map(|p| p.estimated_share).sum();
        assert!((sum - 1.0).abs() < 1e-9, "shares sum to {sum}");
    }

    #[test]
    fn keys_in_v4_order() {
        let keys: Vec<&str> = ALMANACK_PHASES.iter().map(|p| p.key).collect();
        assert_eq!(
            keys,
            vec![
                "premises",
                "machinery",
                "ledgers",
                "scriptorium",
                "personae",
                "wire-records",
                "binding",
            ]
        );
        assert_eq!(phase_index("premises"), Some(1));
        assert_eq!(phase_index("binding"), Some(7));
        assert_eq!(phase_index("nope"), None);
    }
}
