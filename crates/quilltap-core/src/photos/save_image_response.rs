//! The answers every save-image door gives (v4 `lib/photos/save-image-response.ts`,
//! `01a83539d`) — the Salon's message and gallery routes and the wardrobe
//! picture viewer — so the save dialog reads one shape whichever door it
//! posted to.
//!
//! - [`saved_image_body`] — v4 `savedImageResponse`: the 200 body with the
//!   saved picture's whereabouts.
//! - [`save_image_error_response`] — v4 `saveImageErrorResponse`: `ALREADY_SAVED`
//!   is a 409 carrying when the picture was filed; any other code is a 400.
//! - [`save_image_error_code`] — v4's wire string for a
//!   [`SaveImageErrorCode`], for the doors' log lines.

use serde_json::{json, Value};

use crate::api::types::{AlreadySavedRiders, ErrorKind, Response};

use super::save_image_to_album::{
    SaveImageErrorCode, SaveImageToAlbumError, SaveImageToAlbumOutput,
};

/// v4 `savedImageResponse(saved)` (`save-image-response.ts:14-24`) — `{saved:
/// true, mountPoint, relativePath, linkId, keptAt, fileId, sha256}`, in v4's
/// key order; the bytes both Salon doors built inline before `01a83539d`.
pub fn saved_image_body(saved: &SaveImageToAlbumOutput) -> Value {
    json!({
        "saved": true,
        "mountPoint": saved.mount_point_name,
        "relativePath": saved.relative_path,
        "linkId": saved.link_id,
        "keptAt": saved.kept_at,
        "fileId": saved.file_id,
        "sha256": saved.sha256,
    })
}

/// v4 `saveImageErrorResponse(error)` (`save-image-response.ts:32-45`). v4's
/// why, carried: already in that album is not a failure of the request — it
/// is the answer to it — so it is a 409 carrying when the picture was filed,
/// which the dialog says in those words. Anything else is a 400 with the
/// service's message.
///
/// v4's 409 body is `{error, code, relativePath, keptAt}` — four FLAT
/// siblings. `code` rides the existing flat carrier (its documented purpose:
/// "the SPA error translation keys on this first"); the two riders ride
/// `already_saved`, which every transport spreads beside `error` through
/// `CoreError::already_saved_wire_body` (an absent rider drops out, as v4's
/// `undefined` does).
pub fn save_image_error_response(err: SaveImageToAlbumError) -> Response {
    if err.code != SaveImageErrorCode::AlreadySaved {
        return Response::error(ErrorKind::BadRequest, err.message);
    }
    let mut resp = Response::error(ErrorKind::Conflict, err.message);
    if let Response::Error(e) = &mut resp {
        e.code = Some(save_image_error_code(SaveImageErrorCode::AlreadySaved).to_string());
        e.already_saved = Some(Box::new(AlreadySavedRiders {
            relative_path: err.existing_relative_path,
            kept_at: err.existing_created_at,
        }));
    }
    resp
}

/// v4's `SaveImageErrorCode` wire strings (`save-image-to-album.ts:96-101`).
pub fn save_image_error_code(code: SaveImageErrorCode) -> &'static str {
    match code {
        SaveImageErrorCode::ImageNotFound => "IMAGE_NOT_FOUND",
        SaveImageErrorCode::NotAnImage => "NOT_AN_IMAGE",
        SaveImageErrorCode::EmptyBytes => "EMPTY_BYTES",
        SaveImageErrorCode::MountNotFound => "MOUNT_NOT_FOUND",
        SaveImageErrorCode::AlreadySaved => "ALREADY_SAVED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(code: SaveImageErrorCode, riders: bool) -> SaveImageToAlbumError {
        SaveImageToAlbumError {
            code,
            message: "the sentence".into(),
            existing_relative_path: riders.then(|| "photos/a.webp".to_string()),
            existing_created_at: riders.then(|| "2026-10-09T00:00:00.000Z".to_string()),
        }
    }

    #[test]
    fn the_saved_body_keeps_v4s_key_order() {
        let body = saved_image_body(&SaveImageToAlbumOutput {
            mount_point_name: "Quilltap General".into(),
            relative_path: "photos/a.webp".into(),
            link_id: "l".into(),
            kept_at: "k".into(),
            file_id: "f".into(),
            sha256: "s".into(),
        });
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"saved":true,"mountPoint":"Quilltap General","relativePath":"photos/a.webp","linkId":"l","keptAt":"k","fileId":"f","sha256":"s"}"#
        );
    }

    #[test]
    fn already_saved_is_a_409_with_its_riders_and_the_rest_are_400s() {
        let Response::Error(e) =
            save_image_error_response(err(SaveImageErrorCode::AlreadySaved, true))
        else {
            panic!("an error")
        };
        assert_eq!(e.kind, ErrorKind::Conflict);
        assert_eq!(
            serde_json::to_string(&e.already_saved_wire_body()).unwrap(),
            r#"{"error":"the sentence","code":"ALREADY_SAVED","relativePath":"photos/a.webp","keptAt":"2026-10-09T00:00:00.000Z"}"#
        );
        for code in [
            SaveImageErrorCode::ImageNotFound,
            SaveImageErrorCode::NotAnImage,
            SaveImageErrorCode::EmptyBytes,
            SaveImageErrorCode::MountNotFound,
        ] {
            let Response::Error(e) = save_image_error_response(err(code, false)) else {
                panic!("an error")
            };
            assert_eq!(e.kind, ErrorKind::BadRequest, "{code:?}");
            assert_eq!(e.code, None);
            assert!(e.already_saved.is_none());
            assert_eq!(e.message, "the sentence");
        }
    }

    #[test]
    fn wire_strings() {
        assert_eq!(
            save_image_error_code(SaveImageErrorCode::ImageNotFound),
            "IMAGE_NOT_FOUND"
        );
        assert_eq!(
            save_image_error_code(SaveImageErrorCode::NotAnImage),
            "NOT_AN_IMAGE"
        );
        assert_eq!(
            save_image_error_code(SaveImageErrorCode::EmptyBytes),
            "EMPTY_BYTES"
        );
        assert_eq!(
            save_image_error_code(SaveImageErrorCode::MountNotFound),
            "MOUNT_NOT_FOUND"
        );
        assert_eq!(
            save_image_error_code(SaveImageErrorCode::AlreadySaved),
            "ALREADY_SAVED"
        );
    }
}
