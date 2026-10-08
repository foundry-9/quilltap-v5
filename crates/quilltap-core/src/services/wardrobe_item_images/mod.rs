//! The wardrobe item-images service — v4 `lib/wardrobe/item-images.ts`
//! (`7c8572869`, #82).
//!
//! P4.D255 (the round's keystone) lands only [`primitives`]: the history read,
//! the ownership check a PUT runs, and the delete-time cleanup — what the item
//! routes (P4.D256) and the carriers (P4.D264) call. Everything else (the
//! container resolver, add / set-current / delete / carry, the generation, the
//! job handler) is P4.D263's, which owns this directory from the `KEYSTONE`
//! commit on and changes no signature in `primitives.rs`.

pub mod primitives;
