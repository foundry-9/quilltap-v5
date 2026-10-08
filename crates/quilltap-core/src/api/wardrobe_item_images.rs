//! The wardrobe item-images route — v4 `app/api/v1/wardrobe/[itemId]/images/
//! route.ts` (`7c8572869`): list / generate / set-current / delete-image
//! (upload is a binary `quilltap-web` route with no dispatch verb).
//!
//! P4.D255 (the round's keystone) lands this module with the contract-C1 §11
//! signatures and the loud typed refusal bodies agreed in advance (R-E,
//! CLAUDE.md's no-stub rule); P4.D263 owns it from the `KEYSTONE` commit on,
//! replaces the bodies in this same round, and may widen THESE functions'
//! signatures (to take the image seams off the engine's existing
//! `ready_images_generate` accessor) — never the engine arms' call shape.

use super::types::{ErrorKind, Response, WardrobeContainerScope};
use crate::db::runtime::Db;

fn not_available(action: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{action}' action is recognized but not yet available (P4.D263)."),
    )
}

/// `GET …/images` → `Response::WardrobeItemImages({current, images})`.
pub fn list(
    _db: &Db,
    _scope: WardrobeContainerScope,
    _container_id: Option<&str>,
    _item_id: &str,
) -> Response {
    not_available("wardrobeItemImagesList")
}

/// `POST …/images?action=generate` → 201 `{image, current, prompt, subject,
/// profile, rerouted, trail}`.
pub async fn generate(
    _db: &Db,
    _user_id: &str,
    _scope: WardrobeContainerScope,
    _container_id: Option<&str>,
    _item_id: &str,
    _image_profile_id: Option<&str>,
) -> Response {
    not_available("wardrobeItemImageGenerate")
}

/// `POST …/images?action=set-current` → `{current}`.
pub fn set_current(
    _db: &Db,
    _scope: WardrobeContainerScope,
    _container_id: Option<&str>,
    _item_id: &str,
    _file_id: &str,
) -> Response {
    not_available("wardrobeItemImageSetCurrent")
}

/// `POST …/images?action=delete-image` → `{current}`.
pub fn delete(
    _db: &Db,
    _scope: WardrobeContainerScope,
    _container_id: Option<&str>,
    _item_id: &str,
    _file_id: &str,
) -> Response {
    not_available("wardrobeItemImageDelete")
}
