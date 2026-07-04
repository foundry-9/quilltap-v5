//! Blob doc-edit handlers — v4 `lib/tools/handlers/doc-edit/blob-handlers.ts`:
//! `doc_write_blob`, `doc_read_blob`, `doc_list_blobs`, `doc_delete_blob`.
//!
//! Blobs live only in **document stores** (character vaults, project/group
//! stores) — a `qtap://` URI naming any other scope is rejected. The bytes land
//! via the ported [`crate::db::doc_mount_blobs::DocMountBlobsRepository::create`]
//! facade over [`crate::db::doc_mount_file_links::DocMountFileLinksRepository::link_blob_content`]
//! (the binary content/link split).
//!
//! ## The transcode seam
//!
//! v4 runs uploaded images through `transcodeToWebP` (sharp) before storing. There
//! is no native image codec in `quilltap-core`, so the write handler treats the
//! transcode as a **PASSTHROUGH**: the raw decoded bytes are stored unchanged with
//! `storedMimeType = input.mime_type` and `sha256 = sha256(raw bytes)`. The
//! differential oracle jest.mocks `transcodeToWebP` to the SAME passthrough so both
//! sides store byte-identical blobs (`normaliseBlobRelativePath` stays real on both
//! sides — for a passthrough it is a no-op, since the stored mime is not
//! `image/webp`). Native WebP transcoding is the Phase-4 host's job.
//!
//! ## Seamed side effects
//!
//! The Librarian blob write/delete announcements (`postLibrarianBlobWriteAnnouncement`
//! / `postLibrarianDeleteAnnouncement`) are fire-and-forget no-op seams (see
//! [`super::shared`]); the differential oracle mocks them.
#![allow(clippy::too_many_arguments)]

use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::shared::{
    arg_bool, arg_str, arg_str_ref, assert_write_does_not_target_peer_vault,
    collect_peer_character_ids_for_reads, get_accessible_mount_points, DocEditToolContext,
};
use super::DocEditToolResult;
use crate::db::doc_mount_blobs::{CreateBlobInput, DocMountBlobsRepository};
use crate::doc_edit::path_resolver::resolve_mount_point_ref;
use crate::doc_edit::qtap_uri::parse_qtap_uri;
use crate::doc_edit::uri_producers::{doc_store_uri_for, DocStoreUriResolver};
use crate::doc_edit::DocEditScope;

/// A resolved `{ mountPoint, path }` pair after projecting an optional `uri`.
struct BlobTarget {
    mount_point: Option<String>,
    path: Option<String>,
}

/// v4 `resolveBlobTarget`: project a blob tool's optional `uri` onto a
/// `{ mountPoint, path }` pair. Blobs live only in document stores, so a
/// non-`document_store` scope is rejected. When no `uri`, the explicit
/// `mount_point`/`path` pass through. `Err(message)` on a project/general URI.
fn resolve_blob_target(args: &Value) -> Result<BlobTarget, String> {
    if let Some(uri) = arg_str_ref(args, "uri") {
        let parts = parse_qtap_uri(uri).map_err(|e| e.message)?;
        if parts.scope != DocEditScope::DocumentStore {
            return Err(
                "Blobs live only in document stores; the qtap:// URI must name a store (or \"self\")."
                    .to_string(),
            );
        }
        return Ok(BlobTarget {
            mount_point: parts.mount_point,
            path: Some(parts.path),
        });
    }
    Ok(BlobTarget {
        mount_point: arg_str(args, "mount_point"),
        path: arg_str(args, "path"),
    })
}

/// A resolved accessible mount point (v4's `{ id, name }`).
struct ResolvedMount {
    id: String,
    name: String,
}

/// v4 `resolveBlobMountPointForRead`: translate the self-token, then match the ref
/// (by lowercased name or exact id) among the accessible mount points, admitting
/// peer vaults when cross-character reads are enabled. `None` without a
/// project/character context or when nothing matches.
fn resolve_blob_mount_point_for_read(
    main: &Connection,
    mount: &Connection,
    mount_point_ref: &str,
    ctx: &DocEditToolContext,
) -> Option<ResolvedMount> {
    if ctx.project_id.is_none() && ctx.character_id.is_none() {
        return None;
    }
    let effective_ref = resolve_mount_point_ref(main, mount_point_ref, ctx.character_id.as_deref());
    let peers = collect_peer_character_ids_for_reads(main, ctx);
    let mounts = get_accessible_mount_points(
        main,
        mount,
        ctx.project_id.as_deref(),
        ctx.character_id.as_deref(),
        &peers,
    );
    let needle = effective_ref.to_lowercase();
    mounts
        .into_iter()
        .find(|mp| mp.name.to_lowercase() == needle || mp.id == effective_ref)
        .map(|mp| ResolvedMount {
            id: mp.id,
            name: mp.name,
        })
}

/// v4 `resolveBlobMountPointForWrite`: translate the self-token, raise the
/// dedicated read-only error if the ref names a peer's vault, then match among the
/// accessible mount points (WITHOUT peer vaults). `Err(message)` is the thrown
/// peer-vault error; `Ok(None)` is "not found".
fn resolve_blob_mount_point_for_write(
    main: &Connection,
    mount: &Connection,
    mount_point_ref: &str,
    ctx: &DocEditToolContext,
) -> Result<Option<ResolvedMount>, String> {
    if ctx.project_id.is_none() && ctx.character_id.is_none() {
        return Ok(None);
    }
    let effective_ref = resolve_mount_point_ref(main, mount_point_ref, ctx.character_id.as_deref());
    let peers = collect_peer_character_ids_for_reads(main, ctx);
    // Writes must not land in a peer's vault — raise the read-only error before
    // enumerating accessible mounts (v4 passes the ORIGINAL ref, not the resolved).
    assert_write_does_not_target_peer_vault(main, mount, Some(mount_point_ref), &peers)?;
    let mounts = get_accessible_mount_points(
        main,
        mount,
        ctx.project_id.as_deref(),
        ctx.character_id.as_deref(),
        &[],
    );
    let needle = effective_ref.to_lowercase();
    Ok(mounts
        .into_iter()
        .find(|mp| mp.name.to_lowercase() == needle || mp.id == effective_ref)
        .map(|mp| ResolvedMount {
            id: mp.id,
            name: mp.name,
        }))
}

/// v4 `normaliseBlobRelativePath` (`blob-transcode.ts`): rewrite a blob's
/// relativePath so the extension matches the storedMimeType. Only rewrites when
/// the stored type is `image/webp` and the path doesn't already end `.webp`. For
/// the transcode PASSTHROUGH (stored mime == input mime, never `image/webp` unless
/// the input already was) this is a no-op except for a genuine WebP upload.
pub fn normalise_blob_relative_path(relative_path: &str, stored_mime_type: &str) -> String {
    if stored_mime_type != "image/webp" {
        return relative_path.to_string();
    }
    if relative_path.to_lowercase().ends_with(".webp") {
        return relative_path.to_string();
    }
    // JS lastIndexOf over UTF-16 units; ASCII '.'/'/' are single-unit, so byte
    // indices coincide with UTF-16 indices for these characters.
    let last_dot = relative_path.rfind('.');
    let last_slash = relative_path.rfind('/');
    match last_dot {
        None => format!("{relative_path}.webp"),
        Some(dot) if last_slash.map(|s| dot < s).unwrap_or(false) => {
            format!("{relative_path}.webp")
        }
        Some(dot) => format!("{}.webp", &relative_path[..dot]),
    }
}

pub fn handle_write_blob(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let target = match resolve_blob_target(args) {
        Ok(t) => t,
        Err(msg) => return Ok(DocEditToolResult::fail(msg)),
    };
    let (Some(mount_point_ref), Some(path)) = (target.mount_point, target.path) else {
        return Ok(DocEditToolResult::fail(
            "A `mount_point` + `path` (or a `uri`) is required.",
        ));
    };

    let mp = match resolve_blob_mount_point_for_write(main, mount, &mount_point_ref, ctx)? {
        Some(mp) => mp,
        None => {
            return Ok(DocEditToolResult::fail(format!(
                "Mount point not found or not linked to this project: {mount_point_ref}"
            )));
        }
    };

    let data_base64 = arg_str_ref(args, "data_base64").unwrap_or("");
    // v4 `Buffer.from(str, 'base64')` is lenient (ignores non-base64 chars); the
    // corpus supplies well-formed or empty payloads. A genuine decode error maps
    // to v4's "Invalid base64 payload: ..." shape.
    let raw_bytes = match decode_base64_lenient(data_base64) {
        Ok(b) => b,
        Err(msg) => {
            return Ok(DocEditToolResult::fail(format!(
                "Invalid base64 payload: {msg}"
            )));
        }
    };
    if raw_bytes.is_empty() {
        return Ok(DocEditToolResult::fail("Empty blob payload"));
    }

    let mime_type = arg_str(args, "mime_type").unwrap_or_default();
    // The transcode seam: PASSTHROUGH. Stored bytes/mime/sha == the raw upload.
    let stored_mime_type = mime_type.clone();
    let transcoded_sha = hex::encode(Sha256::digest(&raw_bytes));
    let final_path = normalise_blob_relative_path(&path, &stored_mime_type);

    let original_filename = arg_str(args, "original_filename").unwrap_or_default();
    let description = arg_str(args, "description").unwrap_or_default();

    let stored = DocMountBlobsRepository::new(mount)
        .create(&CreateBlobInput {
            mount_point_id: mp.id.clone(),
            relative_path: final_path,
            original_file_name: original_filename,
            original_mime_type: mime_type,
            stored_mime_type,
            sha256: transcoded_sha,
            data: raw_bytes,
            description: Some(description),
            file_name: None,
            file_type: None,
        })
        .map_err(|e| e.to_string())?;

    // Librarian blob-write announcement + resolveActorOrigin: no-op seams.

    let stored_uri = doc_store_uri_for(
        main,
        mount,
        &mp.id,
        &mp.name,
        &stored.relative_path,
        ctx.character_id.as_deref(),
        None,
        None,
    );

    let result = json!({
        "success": true,
        "mount_point": mp.name,
        "relative_path": stored.relative_path,
        "uri": stored_uri,
        "size_bytes": stored.size_bytes,
        "stored_mime_type": stored.stored_mime_type,
        "sha256": stored.sha256,
    });
    let formatted = format!(
        "Uploaded blob to [{}] {} ({} bytes, {})",
        mp.name, stored.relative_path, stored.size_bytes, stored.stored_mime_type
    );
    Ok(DocEditToolResult::ok(result, formatted))
}

pub fn handle_read_blob(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let target = match resolve_blob_target(args) {
        Ok(t) => t,
        Err(msg) => return Ok(DocEditToolResult::fail(msg)),
    };
    let (Some(mount_point_ref), Some(path)) = (target.mount_point, target.path) else {
        return Ok(DocEditToolResult::fail(
            "A `mount_point` + `path` (or a `uri`) is required.",
        ));
    };

    let mp = match resolve_blob_mount_point_for_read(main, mount, &mount_point_ref, ctx) {
        Some(mp) => mp,
        None => {
            return Ok(DocEditToolResult::fail(format!(
                "Mount point not found or not linked to this project: {mount_point_ref}"
            )));
        }
    };

    let repo = DocMountBlobsRepository::new(mount);
    let meta = match repo
        .find_by_mount_point_and_path(&mp.id, &path)
        .map_err(|e| e.to_string())?
    {
        Some(m) => m,
        None => return Ok(DocEditToolResult::fail(format!("Blob not found: {path}"))),
    };

    let uri = doc_store_uri_for(
        main,
        mount,
        &mp.id,
        &mp.name,
        &meta.relative_path,
        ctx.character_id.as_deref(),
        None,
        None,
    );

    let mut result = json!({
        "mount_point": mp.name,
        "relative_path": meta.relative_path,
        "uri": uri,
        "original_filename": meta.original_file_name,
        "original_mime_type": meta.original_mime_type,
        "stored_mime_type": meta.stored_mime_type,
        "size_bytes": meta.size_bytes,
        "sha256": meta.sha256,
        "description": meta.description,
    });

    if arg_bool(args, "include_bytes") == Some(true) {
        if let Some(data) = repo.read_data(&meta.id).map_err(|e| e.to_string())? {
            let b64 = base64_encode(&data);
            if let Value::Object(o) = &mut result {
                o.insert("data_base64".into(), Value::String(b64));
            }
        }
    }

    let desc_suffix = if meta.description.is_empty() {
        String::new()
    } else {
        format!("\nDescription: {}", meta.description)
    };
    let formatted = format!(
        "Blob [{}] {} — {}, {} bytes{}",
        mp.name, meta.relative_path, meta.stored_mime_type, meta.size_bytes, desc_suffix
    );
    Ok(DocEditToolResult::ok(result, formatted))
}

pub fn handle_list_blobs(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    // A qtap:// URI may address a store root or a folder; its path becomes the
    // folder filter.
    let mut mount_point_ref = arg_str(args, "mount_point");
    let mut folder = arg_str(args, "folder");
    if arg_str_ref(args, "uri").is_some() {
        let parts = match resolve_blob_target(args) {
            Ok(t) => t,
            Err(msg) => return Ok(DocEditToolResult::fail(msg)),
        };
        mount_point_ref = parts.mount_point;
        // v4: `parts.path ? parts.path : input.folder` — a non-empty path wins.
        folder = match parts.path {
            Some(p) if !p.is_empty() => Some(p),
            _ => folder,
        };
    }
    let Some(mount_point_ref) = mount_point_ref else {
        return Ok(DocEditToolResult::fail(
            "A `mount_point` or a `uri` is required.",
        ));
    };

    let mp = match resolve_blob_mount_point_for_read(main, mount, &mount_point_ref, ctx) {
        Some(mp) => mp,
        None => {
            return Ok(DocEditToolResult::fail(format!(
                "Mount point not found or not linked to this project: {mount_point_ref}"
            )));
        }
    };

    let metas = DocMountBlobsRepository::new(mount)
        .list_by_mount_point(&mp.id, folder.as_deref())
        .map_err(|e| e.to_string())?;

    let resolver = DocStoreUriResolver::build(main, mount, ctx.character_id.as_deref());
    let blobs: Vec<Value> = metas
        .iter()
        .map(|m| {
            json!({
                "relative_path": m.relative_path,
                "uri": resolver.uri_for_mount(&mp.name, &mp.id, &m.relative_path),
                "original_filename": m.original_file_name,
                "original_mime_type": m.original_mime_type,
                "stored_mime_type": m.stored_mime_type,
                "size_bytes": m.size_bytes,
                "description": m.description,
            })
        })
        .collect();

    let result = json!({
        "mount_point": mp.name,
        "blobs": blobs,
        "total": metas.len(),
    });

    let formatted = if metas.is_empty() {
        let under = folder
            .as_deref()
            .map(|f| format!(" under {f}"))
            .unwrap_or_default();
        format!("No blobs in [{}]{}.", mp.name, under)
    } else {
        let lines: Vec<String> = metas
            .iter()
            .map(|m| {
                let desc = if m.description.is_empty() {
                    String::new()
                } else {
                    format!("  — {}", m.description)
                };
                format!(
                    "  {}  ({}, {} bytes){}",
                    m.relative_path, m.stored_mime_type, m.size_bytes, desc
                )
            })
            .collect();
        format!(
            "{} blobs in [{}]:\n{}",
            metas.len(),
            mp.name,
            lines.join("\n")
        )
    };
    Ok(DocEditToolResult::ok(result, formatted))
}

pub fn handle_delete_blob(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let target = match resolve_blob_target(args) {
        Ok(t) => t,
        Err(msg) => return Ok(DocEditToolResult::fail(msg)),
    };
    let (Some(mount_point_ref), Some(path)) = (target.mount_point, target.path) else {
        return Ok(DocEditToolResult::fail(
            "A `mount_point` + `path` (or a `uri`) is required.",
        ));
    };

    let mp = match resolve_blob_mount_point_for_write(main, mount, &mount_point_ref, ctx)? {
        Some(mp) => mp,
        None => {
            return Ok(DocEditToolResult::fail(format!(
                "Mount point not found or not linked to this project: {mount_point_ref}"
            )));
        }
    };

    let deleted = DocMountBlobsRepository::new(mount)
        .delete_by_mount_point_and_path(&mp.id, &path)
        .map_err(|e| e.to_string())?;
    if !deleted {
        return Ok(DocEditToolResult::fail(format!("Blob not found: {path}")));
    }

    // Librarian delete announcement + resolveActorOrigin: no-op seams.

    let deleted_uri = doc_store_uri_for(
        main,
        mount,
        &mp.id,
        &mp.name,
        &path,
        ctx.character_id.as_deref(),
        None,
        None,
    );

    let result = json!({
        "success": true,
        "mount_point": mp.name,
        "relative_path": path,
        "uri": deleted_uri,
    });
    let formatted = format!("Deleted blob [{}] {}", mp.name, path);
    Ok(DocEditToolResult::ok(result, formatted))
}

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Decode base64 the way v4's `Buffer.from(str, 'base64')` does for our corpus:
/// standard alphabet, tolerant of missing padding, and **ignoring any non-alphabet
/// characters** (whitespace, newlines, stray bytes) — Node's decoder silently
/// skips them and stops at `=`. Hand-rolled (no `base64` crate in the core;
/// mirrors `dbkey::base64_decode` but leniently). Never fails on our inputs;
/// returns `Err` only in the theoretical un-decodable case (kept for the v4
/// "Invalid base64 payload:" error shape).
fn decode_base64_lenient(s: &str) -> Result<Vec<u8>, String> {
    let mut bits = 0u32;
    let mut nbits = 0u8;
    let mut out = Vec::new();
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let Some(idx) = B64_ALPHABET.iter().position(|&x| x == c) else {
            // Node ignores non-alphabet characters — skip.
            continue;
        };
        bits = (bits << 6) | (idx as u32);
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    Ok(out)
}

/// Standard-alphabet base64 encode with `=` padding — matches Node
/// `Buffer.toString('base64')`. Hand-rolled (no `base64` crate in the core).
fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}
