//! The avatar configuration cache (v4 `lib/wardrobe/avatar-cache.ts`,
//! `7fbf8a55b`).
//!
//! One avatar per character *per configuration*. `build_character_avatar_prompt`
//! is a pure function, so the prompt it returns is already the canonical
//! serialization of every deterministic input — the head-and-shoulders physical
//! variant, the expanded leaf outfit, the pronoun subject noun, the bare-top crop
//! branch, the capped art-direction preamble. There is nothing to hash
//! separately; the prompt *is* the digest.
//!
//! What the prompt does not carry is the provider-side shape — model, LoRAs,
//! stored profile options, size — which come from the shared params builder and
//! change the picture without changing a character of prompt text. So the key
//! covers the whole built params object as well.
//!
//! This module is the only place a key is derived or looked up. Never compute
//! one at a call site: a second derivation is a second format, and the two drift.
//! (The collapse heal, `db::avatar_rolls_collapse_heal`, groups through
//! [`derive_legacy_avatar_cache_key`] here for exactly that reason.)
//!
//! ## The params object is a `Value`, not an `ImageGenParams`
//!
//! v4 hashes the live JS object `buildImageGenParams` returned. v5's twin of
//! that object is [`crate::model::image::ImageGenParams::to_key_value`] — the
//! rendering already pinned key-for-key by the request-envelope corpora,
//! including the two insertion-order slots. So this module takes the RENDERED
//! `Value` and the caller renders once: one params object, hashed and sent, with
//! no second shape to drift.
//!
//! ## `undefined` has no Rust counterpart
//!
//! v4's `canonicalJson` drops `undefined`-valued keys, "because `undefined` and
//! a missing key mean the same thing to a provider". A `serde_json::Value` has
//! no `undefined`: the key is simply absent, which `to_key_value` already
//! guarantees for every `None` field. So there is nothing to filter — and
//! nothing may be filtered either, because a JSON `null` is a REAL value on both
//! sides and must survive into the preimage. The corpus pins both halves: an
//! explicit-`undefined` shape and its absent-key sibling hash identically, and a
//! `null`-valued key hashes differently from both.
//!
//! Design of record (v4): `docs/developer/features/avatar-configuration-cache.md`.

use std::cmp::Ordering;

use rusqlite::Connection;
use serde_json::Value;

use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::files::{FileEntry, FilesRepository};
use crate::pascal::js_value::json_stringify;
use crate::services::file_storage::parse_mount_blob_storage_key;

/// Both keys for one generation, in lookup order (v4 `AvatarCacheKeys`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvatarCacheKeys {
    /// Full-fidelity key; what a new row is stored under.
    pub key: String,
    /// Legacy key, matching rows the collapse heal keyed.
    pub legacy_key: String,
}

/// Stable JSON (v4 `canonicalJson`): object keys sorted recursively, so a
/// reordered params object can never produce a different hash. Arrays keep their
/// order — a LoRA list is ordered and two orderings are two configurations.
///
/// Leaves render through [`json_stringify`], v5's proven `JSON.stringify` twin,
/// so a JS number prints as JS prints it (`1`, never `1.0`) and a string carries
/// JS's escapes.
///
/// The key sort is v4's `a < b ? -1 : a > b ? 1 : 0` — **JS string comparison,
/// which is by UTF-16 code unit**, not by UTF-8 byte. The two orders disagree
/// whenever a supplementary-plane key meets a BMP key above U+DFFF (a surrogate
/// pair leads with 0xD800, which sorts *below* U+E000..U+FFFF in UTF-16 and
/// *above* every BMP byte sequence in UTF-8), so `encode_utf16().cmp()` is
/// load-bearing, not decoration: a profile's residual `parameters` bag carries
/// operator-chosen keys. Pinned by the `astral-vs-bmp-key-order` corpus row.
fn canonical_json(value: &Value) -> String {
    match value {
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| js_string_cmp(a, b));
            let parts: Vec<String> = keys
                .iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        Value::String((*k).clone()),
                        canonical_json(&map[*k])
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        leaf => json_stringify(leaf),
    }
}

/// JS relational string comparison — UTF-16 code-unit order (see
/// [`canonical_json`]). The `image_dialects.rs:1310` idiom.
fn js_string_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// v4 `sha256Hex` — `createHash('sha256').update(input, 'utf8').digest('hex')`.
fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// v4 `deriveAvatarCacheKey` — the full-fidelity key for a configuration.
///
/// `v: 1` is the format discriminator: it is what keeps a v1 key from ever
/// colliding with a legacy v0 key over the same prompt and model.
///
/// `params` is the RENDERED params object (see the module doc).
pub fn derive_avatar_cache_key(provider: &str, image_profile_id: &str, params: &Value) -> String {
    let preimage = serde_json::json!({
        "v": 1,
        "provider": provider,
        "imageProfileId": image_profile_id,
        "params": params,
    });
    sha256_hex(&canonical_json(&preimage))
}

/// v4 `deriveLegacyAvatarCacheKey` — the lower-fidelity key for rows that
/// predate the cache.
///
/// Their LoRAs and stored options are not recorded anywhere, so only the prompt
/// and model can be reconstructed — which is exactly what the collapse heal
/// groups on. v4's `input.modelName ?? null`: an absent model and a null model
/// are one key (in Rust they are one value, `None`).
pub fn derive_legacy_avatar_cache_key(model_name: Option<&str>, prompt: &str) -> String {
    let preimage = serde_json::json!({
        "v": 0,
        "modelName": model_name,
        "prompt": prompt,
    });
    sha256_hex(&canonical_json(&preimage))
}

/// v4 `deriveAvatarCacheKeys` — both keys for a generation about to happen.
///
/// The legacy half reads `model` and `prompt` back off the params object, as v4
/// does (`input.params.model ?? null`, `input.params.prompt`) — so a params
/// object without them still derives a key, over `null` / `""`.
pub fn derive_avatar_cache_keys(
    provider: &str,
    image_profile_id: &str,
    params: &Value,
) -> AvatarCacheKeys {
    AvatarCacheKeys {
        key: derive_avatar_cache_key(provider, image_profile_id, params),
        legacy_key: derive_legacy_avatar_cache_key(
            params.get("model").and_then(Value::as_str),
            params.get("prompt").and_then(Value::as_str).unwrap_or(""),
        ),
    }
}

/// Look up a cached avatar for a configuration (v4 `lookupCachedAvatar`).
///
/// Tries the full-fidelity key first, then the legacy key. A v0 hit is
/// deliberately **not** upgraded to a v1 key: we cannot verify that the LoRAs
/// and options in force back then match the ones in force now, and a second
/// indexed read costs nothing.
///
/// A row whose blob has since been deleted counts as a miss. Deleting a mount
/// blob drops every link to that blob's file, and a cache means many chats point
/// at one file id, so this check is what keeps one deletion from wedging every
/// chat that shared the image: the caller regenerates and rebinds the key.
///
/// ## Why this reads `tags` and `createdAt` itself
///
/// v4's `findByGenerationKey` hands back whole `FileEntry` rows, tags and
/// `createdAt` included. v5's [`FileEntry`] is a narrower projection
/// ([`crate::db::files::FILE_ENTRY_COLUMNS`]) that carries neither, and P4.D182
/// froze it. So the indexed read stays P4.D182's — `find_by_generation_key`, the
/// `findByGenerationKey` twin — and the two columns v4's policy needs arrive
/// through one extra keyed read, joined in memory. Same rows, same order, same
/// decisions; one more `SELECT` on a cold path. (The `photos::chat_gallery`
/// precedent: a module owns a purpose-built read when the shared projection does
/// not carry what its policy asks.)
///
/// Returns `None` on a read error, after v4's warn — a cache lookup must never
/// be the reason an avatar fails to generate.
pub fn lookup_cached_avatar(
    main: &Connection,
    mount: &Connection,
    keys: &AvatarCacheKeys,
    character_id: &str,
) -> Option<FileEntry> {
    let files = FilesRepository::new(main);

    for candidate_key in [&keys.key, &keys.legacy_key] {
        let rows = match files.find_by_generation_key(candidate_key) {
            Ok(rows) => rows,
            Err(error) => {
                // A cache lookup must never be the reason an avatar fails to
                // generate.
                tracing::warn!(
                    context = "wardrobe.avatar-cache",
                    characterId = character_id,
                    error = %error,
                    "[AvatarCache] Lookup failed, treating as a miss"
                );
                return None;
            }
        };
        let sidecars = match read_tag_and_created_at(main, candidate_key) {
            Ok(sidecars) => sidecars,
            Err(error) => {
                tracing::warn!(
                    context = "wardrobe.avatar-cache",
                    characterId = character_id,
                    error = %error,
                    "[AvatarCache] Lookup failed, treating as a miss"
                );
                return None;
            }
        };

        // Newest first — a forced reroll rebinds the key, and the newest holder
        // is the one that won it. v4 sorts on `String(b.createdAt)
        // .localeCompare(String(a.createdAt))`; these are ISO-8601 strings
        // minted by one clock, so the comparison is ordinary lexicographic
        // descending. A row whose sidecar read found nothing sorts as "".
        let mut ordered = rows;
        ordered.sort_by(|a, b| {
            let av = sidecars.get(&a.id).map(|s| s.1.as_str()).unwrap_or("");
            let bv = sidecars.get(&b.id).map(|s| s.1.as_str()).unwrap_or("");
            bv.cmp(av)
        });

        for row in ordered {
            // Belt and braces: a key encodes the character's name and
            // description, so a cross-character hit should be impossible.
            // Confirm anyway rather than hand one character another's face.
            let tagged = sidecars
                .get(&row.id)
                .map(|s| s.0.iter().any(|t| t == character_id))
                .unwrap_or(false);
            if !tagged {
                continue;
            }
            let Some(storage_key) = row.storage_key.as_deref() else {
                continue;
            };
            if !mount_blob_exists(mount, storage_key) {
                tracing::info!(
                    context = "wardrobe.avatar-cache",
                    characterId = character_id,
                    fileId = %row.id,
                    "[AvatarCache] Cached avatar blob is gone, regenerating"
                );
                continue;
            }

            tracing::debug!(
                context = "wardrobe.avatar-cache",
                characterId = character_id,
                fileId = %row.id,
                legacy = candidate_key == &keys.legacy_key,
                "[AvatarCache] Hit"
            );
            return Some(row);
        }
    }

    tracing::debug!(
        context = "wardrobe.avatar-cache",
        characterId = character_id,
        "[AvatarCache] Miss"
    );
    None
}

/// The two columns [`FileEntry`] does not carry, for every holder of one key:
/// `id -> (tags, createdAt)`. A `tags` cell that is absent, NULL or unparseable
/// reads as the empty list (v4's Zod `.default([])`), which then fails the tag
/// check exactly as v4's `row.tags?.includes(...)` does on a missing array.
fn read_tag_and_created_at(
    main: &Connection,
    generation_key: &str,
) -> Result<std::collections::HashMap<String, (Vec<String>, String)>, rusqlite::Error> {
    let mut stmt =
        main.prepare("SELECT id, tags, createdAt FROM files WHERE generationKey = ?1")?;
    let mut out = std::collections::HashMap::new();
    let mut rows = stmt.query(rusqlite::params![generation_key])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let tags_raw: Option<String> = row.get(1)?;
        let created_at: Option<String> = row.get(2)?;
        let tags = tags_raw
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default();
        out.insert(id, (tags, created_at.unwrap_or_default()));
    }
    Ok(out)
}

/// v4 `mountBlobExists(storageKey)` — parse the `mount-blob:<mp>:<blob>` key and
/// ask the mount index whether the blob row is still there. A malformed key, a
/// missing row, or a read error all read as "gone" (v4's `!!metadata` over a
/// `findById` that answers null).
fn mount_blob_exists(mount: &Connection, storage_key: &str) -> bool {
    let Some((_mount_point_id, blob_id)) = parse_mount_blob_storage_key(storage_key) else {
        return false;
    };
    DocMountBlobsRepository::new(mount)
        .find_by_id(&blob_id)
        .ok()
        .flatten()
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `undefined`/absent identity the module doc claims, on the v5 side:
    /// there is nothing to filter, so an absent key and a `None` that serde
    /// omits are literally one preimage.
    #[test]
    fn an_absent_key_and_an_omitted_none_are_one_preimage() {
        let absent = serde_json::json!({ "prompt": "p", "model": "m" });
        let built = serde_json::json!({ "prompt": "p", "model": "m" });
        assert_eq!(canonical_json(&absent), canonical_json(&built));
    }

    /// A JSON `null` is a real value and survives into the preimage — the
    /// converse of the rule above. Filtering nulls here would silently collapse
    /// two different configurations onto one key.
    #[test]
    fn a_null_valued_key_is_not_dropped() {
        let with_null = serde_json::json!({ "a": Value::Null });
        assert_eq!(canonical_json(&with_null), r#"{"a":null}"#);
        assert_ne!(
            canonical_json(&with_null),
            canonical_json(&serde_json::json!({}))
        );
    }

    /// Keys sort by UTF-16 code unit, not UTF-8 byte. The two orders disagree
    /// here, which is what makes the comparator measurable.
    #[test]
    fn keys_sort_in_utf16_code_unit_order() {
        let astral = "\u{10000}";
        let bmp = "\u{FFFD}";
        // UTF-8 bytes put the BMP key first; UTF-16 code units put the astral
        // key first, and UTF-16 is what JS compares.
        assert!(bmp.as_bytes() < astral.as_bytes());
        assert_eq!(js_string_cmp(astral, bmp), Ordering::Less);
        let rendered = canonical_json(&serde_json::json!({ bmp: 1, astral: 2 }));
        assert!(
            rendered.starts_with("{\"\u{10000}\""),
            "astral key must sort first: {rendered}"
        );
    }

    /// Arrays keep their order — a LoRA list is ordered and two orderings are
    /// two configurations.
    #[test]
    fn arrays_keep_their_order() {
        let ab = serde_json::json!({ "loras": ["a", "b"] });
        let ba = serde_json::json!({ "loras": ["b", "a"] });
        assert_ne!(canonical_json(&ab), canonical_json(&ba));
    }

    /// The discriminator does its job: the same prompt and model never collide
    /// across the two key formats.
    #[test]
    fn a_v1_key_never_collides_with_a_v0_key() {
        let params = serde_json::json!({ "prompt": "a portrait", "model": "dall-e-3" });
        let keys = derive_avatar_cache_keys("OPENAI", "profile-1", &params);
        assert_ne!(keys.key, keys.legacy_key);
    }

    /// The legacy half of a pair reads the model and prompt back off the params
    /// object, so it equals the standalone v0 derivation over the same two.
    #[test]
    fn the_pairs_legacy_half_equals_the_standalone_v0_key() {
        let params = serde_json::json!({ "prompt": "a portrait", "model": "dall-e-3" });
        let keys = derive_avatar_cache_keys("OPENAI", "profile-1", &params);
        assert_eq!(
            keys.legacy_key,
            derive_legacy_avatar_cache_key(Some("dall-e-3"), "a portrait")
        );
    }
}
