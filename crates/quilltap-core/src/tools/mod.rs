//! The tool-handler family (v4 `lib/tools/`). Wave-4 (W4.1) begins here with the
//! RNG tool — the first handler ported. The tool loops, the registry, and the
//! provider-facing `buildTools` slate are later W4.1 sub-units; this module holds
//! the executable handlers themselves.

pub mod rng;
