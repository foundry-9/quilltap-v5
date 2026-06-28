//! Port of v4's lib/embedding/float32-conversion.ts — the Float32 ↔ byte-buffer
//! conversion used for embedding BLOBs on disk.
//!
//! Embeddings are stored as raw Float32 byte buffers. v4 reads/writes them by
//! aliasing a `Float32Array`'s backing `ArrayBuffer` as a Node `Buffer`, which
//! is the platform byte order. Every supported target (macOS/iOS/Android on
//! x86-64 / ARM) is little-endian, and the on-disk corpora were all written LE,
//! so we encode/decode little-endian explicitly. v4 always returns a fresh copy
//! to avoid aliasing a pooled buffer; the Rust equivalents own their output.
//!
//! `parse_legacy_embedding_text` recovers pre-BLOB JSON-text embeddings. Its
//! index-keyed-object shape relies on JS integer-key iteration order
//! (`Object.values` visits canonical array-index keys in ascending numeric
//! order before any other key); that ordering is reproduced explicitly below.

/// Convert a Float32-coded little-endian byte buffer back to a vector of f32.
/// The buffer length is assumed to be a multiple of 4 (one f32 per 4 bytes), as
/// v4's `byteLength / BYTES_PER_ELEMENT` divide assumes; a trailing partial
/// chunk is ignored.
pub fn blob_to_float32(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Convert a slice of f32 into a Float32-coded little-endian byte buffer
/// suitable for storage.
pub fn float32_to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(embedding.len() * 4);
    for &x in embedding {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// True when `key` is a JS *canonical array index*: the decimal string of a
/// `u32` in `[0, 2^32 - 2]`, with no leading zeros (`"0"` excepted) and no sign.
/// These are exactly the property keys that `Object.values` iterates first, in
/// ascending numeric order, ahead of all other (string) keys.
fn array_index(key: &str) -> Option<u32> {
    let n: u32 = key.parse().ok()?;
    // ToString(ToUint32(key)) === key rejects leading zeros, '+1', etc.;
    // 2^32 - 1 is excluded (it is `length`, never an index).
    if n != u32::MAX && key == n.to_string() {
        Some(n)
    } else {
        None
    }
}

/// Recover an embedding persisted as legacy JSON text (before the Float32-BLOB
/// format). Two historical shapes exist, both yielding the dense vector in
/// index order:
///
///   * a JSON array — `"[0.1, 0.2, ...]"`
///   * a JSON object from `JSON.stringify(someFloat32Array)`, which serialises a
///     typed array as an index-keyed object — `'{"0":0.1,"1":0.2,...}'`
///
/// Returns `None` when the text is not a usable embedding (scalar, `null`,
/// boolean, or unparseable) — mirroring v4 returning `undefined`. For the object
/// shape, v4 relies on `Object.values` visiting integer-index keys in ascending
/// numeric order; we reproduce that by sorting the array-index keys numerically.
///
/// Residual seam: the legacy writer only ever produced numeric arrays / purely
/// integer-keyed objects. A non-numeric element (outside that domain) yields
/// `None` here rather than v4's blind pass-through, and the relative order of
/// any *non*-index object keys follows `serde_json`'s key order rather than JS
/// insertion order — neither case occurs in real legacy data.
pub fn parse_legacy_embedding_text(value: &str) -> Option<Vec<f64>> {
    let parsed: serde_json::Value = serde_json::from_str(value).ok()?;
    match parsed {
        serde_json::Value::Array(arr) => arr.iter().map(|v| v.as_f64()).collect(),
        serde_json::Value::Object(map) => {
            // JS `Object.values`: array-index keys first, ascending numerically;
            // then any other keys. Real legacy objects are purely index-keyed.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| match (array_index(a), array_index(b)) {
                (Some(ia), Some(ib)) => ia.cmp(&ib),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.cmp(b),
            });
            keys.iter().map(|k| map[*k].as_f64()).collect()
        }
        // null, number, string, boolean → JS `undefined`.
        _ => None,
    }
}
