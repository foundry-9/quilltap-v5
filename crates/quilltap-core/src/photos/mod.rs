//! Photo album subsystem — port of v4 `lib/photos/*`.
//!
//! Photos are documents in a character vault: a `photos/` subfolder in the
//! character's mount point. `keep_image` hard-links an existing image binary
//! (dedup by sha256) and writes a Markdown context document (prompt + scene
//! snapshot + caption + tags) as the link's `extractedText`, so the standard
//! character-vault search picks it up; `list_images` / `attach_image` resurface a
//! kept image. The three `tools::photo` handlers sit on top of this module.
//!
//! Modules:
//! - [`keep_image_markdown`] — the pure Markdown builder + parser
//!   (v4 `keep-image-markdown.ts`).
//! - [`photos_paths`] — the `photos/` folder path helpers (v4 `photos-paths.ts`).
//! - [`save_image_to_album`] — the stateful save service (v4
//!   `save-image-to-album.ts`), with image bytes behind an injected
//!   [`save_image_to_album::FileBytesStore`] seam.
//! - [`auto_describe_attachment`] — the upload-time vision describe pipeline
//!   (v4 `auto-describe-attachment.ts`), the `describe_image` tool's vision
//!   tier (P4.D108).
//! - [`save_attribution`] — the shared save-image body schema + the one
//!   attribution rule both save doors use (v4 `save-attribution.ts`, P4.D174).
//! - [`chat_gallery`] — the chat gallery enumerator (v4 `chat-gallery.ts`), the
//!   nine-source roll behind `chatGallery`, and the message-attachment walk
//!   `chatFilesList` shares with it (P4.D174).

pub mod auto_describe_attachment;
pub mod character_gallery_service;
// === P4.D174 ===
pub mod chat_gallery;
// === /P4.D174 ===
pub mod keep_image_markdown;
pub mod photo_link_summary;
pub mod photos_paths;
pub mod resolve_character_avatar;
// === P4.D174 ===
pub mod save_attribution;
// === /P4.D174 ===
pub mod save_image_to_album;
pub mod user_gallery_service;
