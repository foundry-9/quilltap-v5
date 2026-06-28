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
//! `parseLegacyEmbeddingText` (recovering pre-BLOB JSON-text embeddings) is
//! deferred: its index-keyed-object shape relies on JS integer-key iteration
//! order, a serialisation-fidelity seam handled with the other deferred
//! collation/ordering cases.

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
