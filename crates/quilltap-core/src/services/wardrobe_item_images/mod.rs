//! The wardrobe item-images service — v4 `lib/wardrobe/item-images.ts`
//! (`7c8572869`, #82).
//!
//! P4.D255 (the round's keystone) landed [`primitives`]: the history read,
//! the ownership check a PUT runs, and the delete-time cleanup — what the item
//! routes (P4.D256) and the carriers (P4.D264) call. P4.D263 (which owns this
//! directory from the `KEYSTONE` commit on and changes no signature in
//! `primitives.rs`) adds [`service`] (the summary, add / set-current / delete)
//! and [`carry`] (pictures travelling with a transfer). The container resolver
//! is `services::wardrobe_container`; the bridge `services::
//! wardrobe_image_bridge`.

pub mod carry;
pub mod primitives;
pub mod service;
