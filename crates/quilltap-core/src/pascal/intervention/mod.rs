//! Operator intervention — Pascal the Croupier holds a run for the operator
//! (v4 `839575d89` / `8ae8a127c` / `b02d902f3` at the `236c04e35` pin; P4.D345).
//!
//! An intervenable tool's run, reached by a model's `run_custom` or a trigger
//! phrase, is DEALT and then HELD between the deal and its effects: nothing is
//! applied, nothing posted, no bytes drawn again, no model called — until the
//! operator previews, re-asks, deals, lets it stand or sweeps it away. The
//! module split mirrors v4's `lib/pascal/intervention/` (ruling R-E): each file
//! is a unit the pure family names.
//!
//! Design of record (v4): `docs/developer/features/complete/
//! pascal-operator-intervention.md`.

pub mod eligibility;
pub mod overrides;
pub mod recompute;
pub mod spans;
pub mod veto_result;
pub mod view;
