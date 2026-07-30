//! Port of v4's lib/embedding/float32-conversion.ts — the embedding BLOB codec.
//!
//! This module is the SINGLE SOURCE OF TRUTH for how embedding vectors are
//! (de)serialized to/from SQLite BLOBs. Every consumer (backend hydration,
//! schema transforms, the maintenance sweep) decodes through
//! [`blob_to_float32`] and encodes through [`float32_to_blob`], so the on-disk
//! format can evolve without touching search/scoring code.
//!
//! ## On-disk formats
//!
//! **Legacy (pre-quantization):** the raw bytes of a `Float32Array`. No header.
//! v4 aliases a `Float32Array`'s backing `ArrayBuffer` as a Node `Buffer`, which
//! is the platform byte order; every supported target (macOS/iOS/Android on
//! x86-64 / ARM) is little-endian and the on-disk corpora were all written LE,
//! so we encode/decode little-endian explicitly.
//!
//! **Quantized (current writes):** a self-describing, versioned layout so legacy
//! Float32, int8 and float16 blobs can coexist during and after v4's
//! `quantize-embeddings-v1` migration (little-endian):
//!
//! ```text
//!   [0]      magic   = 0xEB
//!   [1]      version = 0x01
//!   [2]      dtype   : 0x01 = int8-symmetric, 0x02 = float16
//!   [3..6]   dim     : uint32
//!   dtype==int8: [7..10] scale : float32   (dequant: f = int8 * scale)
//!                [11..11+dim)   int8 body            → total = 11 + dim
//!   dtype==f16 : [7..7+2*dim)   float16 body (no scale) → total = 7 + 2*dim
//! ```
//!
//! A blob is treated as quantized iff the magic+version match AND the declared
//! dim is self-consistent with the byte length; anything else decodes as legacy
//! raw Float32. The combined check makes a false positive on a real Float32
//! buffer astronomically unlikely.
//!
//! Stored embeddings are unit-normalized, so per-vector symmetric int8
//! quantization (`scale = max|v_i| / 127`) is well-conditioned: ~4× smaller than
//! Float32 with mean cosine similarity ≥ 0.999. New writes use int8
//! ([`EMBEDDING_STORAGE_DTYPE`]); the format and codec also support the f16
//! fallback (2× smaller, effectively lossless). v5 writes byte-identically to v4
//! because it is a peer writer of the same synced DB — a mixed-format corpus is
//! unacceptable.
//!
//! The "from blob" direction always returns a fresh `Vec<f32>` — v4's source
//! `Buffer` may be pooled, so callers must not alias it; the Rust equivalents
//! own their output.
//!
//! `parse_legacy_embedding_text` recovers pre-BLOB JSON-text embeddings. Its
//! index-keyed-object shape relies on JS integer-key iteration order
//! (`Object.values` visits canonical array-index keys in ascending numeric order
//! before any other key); that ordering is reproduced explicitly below.

/// First byte of every quantized-format blob.
pub const EMBEDDING_BLOB_MAGIC: u8 = 0xeb;
/// Quantized-format version this codec reads and writes.
pub const EMBEDDING_BLOB_VERSION: u8 = 0x01;
/// dtype byte: int8 symmetric quantization with a per-vector float32 scale.
pub const EMBEDDING_DTYPE_INT8: u8 = 0x01;
/// dtype byte: IEEE 754 half-precision floats, no scale.
pub const EMBEDDING_DTYPE_F16: u8 = 0x02;

/// The dtype all NEW embedding writes use. int8-symmetric is the ~4× win;
/// switch to [`EMBEDDING_DTYPE_F16`] if int8 recall regression ever proves
/// material — one constant, no other change (v4's `EMBEDDING_STORAGE_DTYPE`).
pub const EMBEDDING_STORAGE_DTYPE: u8 = EMBEDDING_DTYPE_INT8;

/// magic + version + dtype + dim(4) + scale(4).
const INT8_HEADER_BYTES: usize = 11;
/// magic + version + dtype + dim(4).
const F16_HEADER_BYTES: usize = 7;

/// True iff the blob carries the self-describing quantized format: magic and
/// version match AND the declared dim is self-consistent with the byte length.
/// Anything else must be decoded as legacy raw Float32.
pub fn is_quantized_embedding_blob(blob: &[u8]) -> bool {
    if blob.len() < F16_HEADER_BYTES {
        return false;
    }
    if blob[0] != EMBEDDING_BLOB_MAGIC || blob[1] != EMBEDDING_BLOB_VERSION {
        return false;
    }
    let dtype = blob[2];
    let dim = u32::from_le_bytes([blob[3], blob[4], blob[5], blob[6]]) as usize;
    if dtype == EMBEDDING_DTYPE_INT8 {
        blob.len() == INT8_HEADER_BYTES + dim
    } else if dtype == EMBEDDING_DTYPE_F16 {
        blob.len() == F16_HEADER_BYTES + 2 * dim
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// float16 helpers (manual bit conversion — mirrors v4's float32ToHalfBits /
// halfBitsToFloat32, round-to-nearest-even, subnormals + Inf/NaN)
// ---------------------------------------------------------------------------

/// Encode one float32 as IEEE 754 half-precision bits (round-to-nearest-even).
fn float32_to_half_bits(value: f32) -> u16 {
    let x = value.to_bits();
    let sign = (x >> 16) & 0x8000;
    let mantissa_and_exp = x & 0x7fff_ffff;

    if mantissa_and_exp >= 0x4780_0000 {
        // Overflows half range → ±Infinity (or NaN preserved)
        let v = if mantissa_and_exp > 0x7f80_0000 {
            sign | 0x7e00
        } else {
            sign | 0x7c00
        };
        return v as u16;
    }
    if mantissa_and_exp < 0x3880_0000 {
        // Subnormal in half precision (or zero). A float32 value 1.m × 2^(e-127)
        // maps to the 10-bit subnormal mantissa h = mant24 × 2^(e-126), i.e.
        // mant24 >> (126 - e).
        let shift = 126i32 - (mantissa_and_exp >> 23) as i32;
        if shift > 24 {
            return sign as u16; // underflows to ±0
        }
        let mant = (mantissa_and_exp & 0x7fffff) | 0x800000;
        let half = mant >> shift;
        // round-to-nearest-even on the dropped bits
        let round_bit = (mant >> (shift - 1)) & 1;
        let sticky = (mant & ((1u32 << (shift - 1)) - 1)) != 0;
        let inc = if round_bit == 1 && (sticky || (half & 1) == 1) {
            1
        } else {
            0
        };
        return (sign | (half + inc)) as u16;
    }
    // Normal case: rebias exponent, round mantissa
    let bits = (mantissa_and_exp >> 13) - 0x1c000;
    let round_bit = (mantissa_and_exp >> 12) & 1;
    let sticky = (mantissa_and_exp & 0xfff) != 0;
    let inc = if round_bit == 1 && (sticky || (bits & 1) == 1) {
        1
    } else {
        0
    };
    (sign | (bits + inc)) as u16
}

/// Decode IEEE 754 half-precision bits to a float32 number.
fn half_bits_to_float32(h: u16) -> f32 {
    let h = h as u32;
    let sign = (h & 0x8000) << 16;
    let exp_mant = h & 0x7fff;
    let bits: u32 = if exp_mant >= 0x7c00 {
        // Inf / NaN
        sign | 0x7f80_0000 | ((exp_mant & 0x3ff) << 13)
    } else if exp_mant >= 0x0400 {
        // Normal
        sign | ((exp_mant + 0x1c000) << 13)
    } else if exp_mant == 0 {
        sign // ±0
    } else {
        // Subnormal half → normalized float32
        let mut mant = exp_mant & 0x3ff;
        let mut exp: u32 = 113;
        while (mant & 0x400) == 0 {
            mant <<= 1;
            exp -= 1;
        }
        mant &= 0x3ff;
        sign | (exp << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

// ---------------------------------------------------------------------------
// JS Math.round + Math.min/Math.max semantics (NaN-propagating clamp)
// ---------------------------------------------------------------------------

/// JS `Math.round(x)` = `floor(x + 0.5)` (half toward +∞ — differs from Rust's
/// `f64::round`, which is half-away-from-zero, at negative half-ties).
fn js_math_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// JS `Math.min(a, b)` — NaN propagates (Rust's `f64::min` ignores NaN, the
/// opposite). Only the two-arg form the codec needs.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else {
        b
    }
}

/// JS `Math.max(a, b)` — NaN propagates.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

// ---------------------------------------------------------------------------
// Quantized encode / decode
// ---------------------------------------------------------------------------

/// Encode an embedding into the self-describing quantized format.
/// int8-symmetric: `scale = max|v_i| / 127` (guarded to 1 for an all-zero or
/// non-finite-maxAbs vector), `q_i = clamp(round(v_i / scale), -127, 127)`.
///
/// The scale is written to the blob as float32 but the quantization loop divides
/// by the un-truncated float64 scale — v4's asymmetry (`writeFloatLE` truncates,
/// the `Math.round(src[i] / scale)` divide uses the JS-number scale). Reproduced
/// exactly so the byte output matches.
pub fn float32_to_quantized(embedding: &[f32], dtype: u8) -> Vec<u8> {
    let dim = embedding.len();

    if dtype == EMBEDDING_DTYPE_F16 {
        let mut buf = Vec::with_capacity(F16_HEADER_BYTES + 2 * dim);
        buf.push(EMBEDDING_BLOB_MAGIC);
        buf.push(EMBEDDING_BLOB_VERSION);
        buf.push(EMBEDDING_DTYPE_F16);
        buf.extend_from_slice(&(dim as u32).to_le_bytes());
        for &x in embedding {
            buf.extend_from_slice(&float32_to_half_bits(x).to_le_bytes());
        }
        return buf;
    }

    // int8-symmetric. maxAbs is a JS float64 (Math.abs of an f32-value promoted
    // to a JS number); the divide below uses this un-truncated scale.
    let mut max_abs: f64 = 0.0;
    for &x in embedding {
        let a = (x as f64).abs();
        if a > max_abs {
            max_abs = a;
        }
    }
    let scale: f64 = if max_abs > 0.0 && max_abs.is_finite() {
        max_abs / 127.0
    } else {
        1.0
    };

    let mut buf = Vec::with_capacity(INT8_HEADER_BYTES + dim);
    buf.push(EMBEDDING_BLOB_MAGIC);
    buf.push(EMBEDDING_BLOB_VERSION);
    buf.push(EMBEDDING_DTYPE_INT8);
    buf.extend_from_slice(&(dim as u32).to_le_bytes());
    buf.extend_from_slice(&(scale as f32).to_le_bytes());
    for &x in embedding {
        let r = js_math_round((x as f64) / scale);
        let clamped = js_max(-127.0, js_min(127.0, r));
        // Number.isFinite(q) ? q : 0 — a NaN (from v/scale = NaN) → 0.
        let q: i8 = if clamped.is_finite() {
            clamped as i8
        } else {
            0
        };
        buf.push(q as u8);
    }
    buf
}

/// Decode a quantized-format blob (int8 or f16, dispatched on the header) to a
/// fresh `Vec<f32>`. The caller must have confirmed
/// [`is_quantized_embedding_blob`]; [`blob_to_float32`] is the header-aware
/// entry point for callers that may hold legacy blobs.
pub fn quantized_to_float32(blob: &[u8]) -> Vec<f32> {
    debug_assert!(is_quantized_embedding_blob(blob), "not a quantized blob");
    let dtype = blob[2];
    let dim = u32::from_le_bytes([blob[3], blob[4], blob[5], blob[6]]) as usize;
    let mut out = Vec::with_capacity(dim);

    if dtype == EMBEDDING_DTYPE_INT8 {
        let scale = f32::from_le_bytes([blob[7], blob[8], blob[9], blob[10]]);
        for i in 0..dim {
            let b = blob[INT8_HEADER_BYTES + i] as i8;
            // v4: `out[i] = int8 * scale` computed as JS float64 then stored into
            // a Float32Array (truncated to f32).
            out.push(((b as f64) * (scale as f64)) as f32);
        }
        return out;
    }

    // EMBEDDING_DTYPE_F16 (is_quantized_embedding_blob admits nothing else)
    for i in 0..dim {
        let lo = blob[F16_HEADER_BYTES + 2 * i];
        let hi = blob[F16_HEADER_BYTES + 2 * i + 1];
        out.push(half_bits_to_float32(u16::from_le_bytes([lo, hi])));
    }
    out
}

// ---------------------------------------------------------------------------
// Public codec surface (header-aware read, quantized write)
// ---------------------------------------------------------------------------

/// Convert a stored embedding BLOB back to a vector of f32 (fresh copy).
/// Header-aware: quantized-format blobs are dequantized; anything else is
/// interpreted as legacy raw Float32 bytes. A trailing partial 4-byte chunk of a
/// legacy buffer is ignored (v4's `byteLength / BYTES_PER_ELEMENT` divide
/// assumes a whole number of elements).
pub fn blob_to_float32(blob: &[u8]) -> Vec<f32> {
    if is_quantized_embedding_blob(blob) {
        return quantized_to_float32(blob);
    }
    blob.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Convert a slice of f32 into the storage BLOB format. All new writes are
/// quantized ([`EMBEDDING_STORAGE_DTYPE`]); use [`float32_to_blob_raw`] for an
/// explicit legacy raw-Float32 buffer.
pub fn float32_to_blob(embedding: &[f32]) -> Vec<u8> {
    float32_to_quantized(embedding, EMBEDDING_STORAGE_DTYPE)
}

/// Encode raw Float32 bytes with no header — the legacy on-disk format. Kept for
/// round-trip tests and any caller that explicitly needs uncompressed Float32
/// storage.
pub fn float32_to_blob_raw(embedding: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(embedding.len() * 4);
    for &x in embedding {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// SQL expression yielding an embedding blob's dimension regardless of its
/// on-disk format (v4 `EMBEDDING_DIM_SQL`, which `7391404e` re-homed into
/// `float32-conversion.ts` "beside the blob layout" — so it lives beside the
/// codec here too, for the same reason: an SQL-side dimension check must never
/// drift from the byte layout above).
///
/// The expression names the bare column `embedding`, so it only composes into a
/// statement whose (single) table has that column — every embedding-bearing
/// table does.
///
/// ⚠ v5's own convention is to decode in Rust (`blob_to_float32(..).len()`), and
/// that stays the convention for row-by-row work: the two agree on well-formed
/// blobs but this expression is a byte-length heuristic, not a decode. It exists
/// for the boot-time dimension reconcile, where the point is (a) selecting rows
/// with SQL byte-identical to v4's and (b) keeping the conforming-corpus path a
/// COUNT that hydrates nothing. v4 itself measures in SQL here and in JS
/// (`embedding.length`) in the reindex's dim-match — do not "unify" them.
pub const EMBEDDING_DIM_SQL: &str = "CASE
  WHEN substr(embedding, 1, 3) = X'EB0101' THEN length(embedding) - 11
  WHEN substr(embedding, 1, 3) = X'EB0102' THEN (length(embedding) - 7) / 2
  ELSE length(embedding) / 4
END";

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

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's deterministic PRNG so accuracy failures reproduce identically.
    fn mulberry32(seed: u32) -> impl FnMut() -> f64 {
        let mut a = seed;
        move || {
            a = a.wrapping_add(0x6d2b_79f5);
            let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
            t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
            ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
        }
    }

    fn random_unit_vector(dim: usize, rand: &mut impl FnMut() -> f64) -> Vec<f32> {
        let mut v = vec![0f32; dim];
        let mut sum_sq = 0f64;
        for e in v.iter_mut() {
            let x = (rand() * 2.0 - 1.0) as f32;
            *e = x;
            sum_sq += (x as f64) * (x as f64);
        }
        let inv = (1.0 / sum_sq.sqrt()) as f32;
        for e in v.iter_mut() {
            *e = ((*e as f64) * (inv as f64)) as f32;
        }
        v
    }

    fn cosine(a: &[f32], b: &[f32]) -> f64 {
        let (mut dot, mut na, mut nb) = (0f64, 0f64, 0f64);
        for i in 0..a.len() {
            dot += (a[i] as f64) * (b[i] as f64);
            na += (a[i] as f64) * (a[i] as f64);
            nb += (b[i] as f64) * (b[i] as f64);
        }
        dot / (na.sqrt() * nb.sqrt())
    }

    // ---- format detection --------------------------------------------------

    #[test]
    fn recognizes_int8_and_f16_quantized_blobs() {
        let mut r = mulberry32(1);
        let v = random_unit_vector(64, &mut r);
        assert!(is_quantized_embedding_blob(&float32_to_quantized(
            &v,
            EMBEDDING_DTYPE_INT8
        )));
        assert!(is_quantized_embedding_blob(&float32_to_quantized(
            &v,
            EMBEDDING_DTYPE_F16
        )));
    }

    #[test]
    fn rejects_legacy_raw_float32_blobs() {
        let mut r = mulberry32(2);
        let v = random_unit_vector(64, &mut r);
        assert!(!is_quantized_embedding_blob(&float32_to_blob_raw(&v)));
    }

    #[test]
    fn rejects_magic_prefixed_blob_with_inconsistent_dim() {
        let mut r = mulberry32(3);
        let blob = float32_to_quantized(&random_unit_vector(64, &mut r), EMBEDDING_STORAGE_DTYPE);
        let truncated = &blob[..blob.len() - 3];
        assert!(!is_quantized_embedding_blob(truncated));
    }

    #[test]
    fn rejects_short_and_empty_buffers() {
        assert!(!is_quantized_embedding_blob(&[]));
        assert!(!is_quantized_embedding_blob(&[0xeb, 0x01]));
    }

    // ---- int8 round-trip ---------------------------------------------------

    #[test]
    fn int8_bounds_per_element_error_by_stored_scale() {
        let mut r = mulberry32(42);
        for _ in 0..10 {
            let v = random_unit_vector(1536, &mut r);
            let blob = float32_to_quantized(&v, EMBEDDING_DTYPE_INT8);
            let back = quantized_to_float32(&blob);
            let scale = f32::from_le_bytes([blob[7], blob[8], blob[9], blob[10]]) as f64;
            assert_eq!(back.len(), v.len());
            for i in 0..v.len() {
                assert!(
                    ((back[i] as f64) - (v[i] as f64)).abs() <= scale + 1e-12,
                    "element {i}: |{}-{}| > scale {scale}",
                    back[i],
                    v[i]
                );
            }
        }
    }

    #[test]
    fn int8_keeps_mean_cosine_above_0_999() {
        let mut r = mulberry32(7);
        let mut sum = 0f64;
        let trials = 50;
        for _ in 0..trials {
            let v = random_unit_vector(1024, &mut r);
            let back = quantized_to_float32(&float32_to_quantized(&v, EMBEDDING_DTYPE_INT8));
            sum += cosine(&v, &back);
        }
        assert!(
            sum / trials as f64 >= 0.999,
            "mean cosine {}",
            sum / trials as f64
        );
    }

    #[test]
    fn int8_is_about_4x_smaller_and_sized_11_plus_dim() {
        let mut r = mulberry32(8);
        let v = random_unit_vector(1536, &mut r);
        let raw = float32_to_blob_raw(&v).len();
        let quant = float32_to_quantized(&v, EMBEDDING_DTYPE_INT8).len();
        assert_eq!(quant, 11 + 1536);
        assert!((quant as f64) * 3.5 < raw as f64);
    }

    #[test]
    fn int8_all_zero_vector_scale_guard() {
        let back =
            quantized_to_float32(&float32_to_quantized(&[0f32; 16], EMBEDDING_STORAGE_DTYPE));
        assert_eq!(back, vec![0f32; 16]);
    }

    #[test]
    fn int8_empty_vector() {
        let blob = float32_to_quantized(&[], EMBEDDING_STORAGE_DTYPE);
        assert!(is_quantized_embedding_blob(&blob));
        assert_eq!(blob.len(), INT8_HEADER_BYTES);
        assert!(quantized_to_float32(&blob).is_empty());
    }

    // ---- f16 round-trip ----------------------------------------------------

    #[test]
    fn f16_keeps_mean_cosine_above_0_9999() {
        let mut r = mulberry32(11);
        let mut sum = 0f64;
        let trials = 50;
        for _ in 0..trials {
            let v = random_unit_vector(1024, &mut r);
            let back = quantized_to_float32(&float32_to_quantized(&v, EMBEDDING_DTYPE_F16));
            sum += cosine(&v, &back);
        }
        assert!(
            sum / trials as f64 >= 0.9999,
            "mean cosine {}",
            sum / trials as f64
        );
    }

    #[test]
    fn f16_round_trips_exact_half_values_losslessly() {
        let exact: Vec<f32> = vec![0.5, -0.25, 1.0, -1.0, 0.0, 0.125, 2048.0];
        let back = quantized_to_float32(&float32_to_quantized(&exact, EMBEDDING_DTYPE_F16));
        assert_eq!(back, exact);
    }

    #[test]
    fn f16_is_2x_plus_header() {
        let mut r = mulberry32(12);
        let v = random_unit_vector(1536, &mut r);
        assert_eq!(
            float32_to_quantized(&v, EMBEDDING_DTYPE_F16).len(),
            7 + 2 * 1536
        );
    }

    // ---- header-aware blob_to_float32 --------------------------------------

    #[test]
    fn decodes_legacy_raw_float32_bit_exactly() {
        let mut r = mulberry32(22);
        let v = random_unit_vector(256, &mut r);
        let back = blob_to_float32(&float32_to_blob_raw(&v));
        assert_eq!(back, v);
    }

    #[test]
    fn storage_writer_is_quantized_and_reads_both_formats() {
        let mut r = mulberry32(23);
        let v = random_unit_vector(128, &mut r);
        let stored = float32_to_blob(&v);
        assert!(is_quantized_embedding_blob(&stored));
        assert!(cosine(&v, &blob_to_float32(&stored)) >= 0.999);
        // legacy raw decodes bit-exact
        assert_eq!(blob_to_float32(&float32_to_blob_raw(&v)), v);
    }

    #[test]
    fn accepts_plain_slice_input() {
        let arr = [0.6f32, -0.8f32];
        let back = blob_to_float32(&float32_to_blob(&arr));
        assert_eq!(back.len(), 2);
        assert!(((back[0] as f64) - 0.6).abs() <= 0.8 / 127.0 + 1e-9);
    }

    // ---- retrieval quality (synthetic top-k overlap) -----------------------

    #[test]
    fn int8_keeps_top10_overlap_above_0_95() {
        fn nearby(center: &[f32], jitter: f32, rand: &mut impl FnMut() -> f64) -> Vec<f32> {
            let mut v = vec![0f32; center.len()];
            let mut sum_sq = 0f64;
            for i in 0..center.len() {
                let x = center[i] + ((rand() * 2.0 - 1.0) as f32) * jitter;
                v[i] = x;
                sum_sq += (x as f64) * (x as f64);
            }
            let inv = (1.0 / sum_sq.sqrt()) as f32;
            for e in v.iter_mut() {
                *e = ((*e as f64) * (inv as f64)) as f32;
            }
            v
        }
        fn top_k(query: &[f32], corpus: &[Vec<f32>], k: usize) -> Vec<usize> {
            let mut scored: Vec<(usize, f64)> = corpus
                .iter()
                .enumerate()
                .map(|(i, v)| (i, cosine(query, v)))
                .collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            scored.into_iter().take(k).map(|x| x.0).collect()
        }

        let mut r = mulberry32(1234);
        let (dim, clusters, per_cluster, queries, k) =
            (512usize, 40usize, 10usize, 20usize, 10usize);
        let centers: Vec<Vec<f32>> = (0..clusters)
            .map(|_| random_unit_vector(dim, &mut r))
            .collect();
        let mut corpus: Vec<Vec<f32>> = Vec::new();
        for center in &centers {
            for _ in 0..per_cluster {
                corpus.push(nearby(center, 0.03, &mut r));
            }
        }
        let quantized: Vec<Vec<f32>> = corpus
            .iter()
            .map(|v| blob_to_float32(&float32_to_quantized(v, EMBEDDING_STORAGE_DTYPE)))
            .collect();

        let mut overlap_sum = 0f64;
        for q in 0..queries {
            let query = nearby(&centers[q % clusters], 0.03, &mut r);
            let before: std::collections::HashSet<usize> =
                top_k(&query, &corpus, k).into_iter().collect();
            let after = top_k(&query, &quantized, k);
            overlap_sum += after.iter().filter(|i| before.contains(i)).count() as f64 / k as f64;
        }
        assert!(
            overlap_sum / queries as f64 >= 0.95,
            "overlap {}",
            overlap_sum / queries as f64
        );
    }

    // ---- EMBEDDING_DIM_SQL -------------------------------------------------

    /// The SQL dimension expression must agree with the Rust decoder on every
    /// format it can meet on disk — that agreement is the whole reason v4 homes
    /// the constant beside the codec, and the boot dimension reconcile selects
    /// rows for DELETE with it.
    #[test]
    fn dim_sql_agrees_with_the_decoder_on_all_three_formats() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (id TEXT, embedding BLOB);")
            .unwrap();

        let vec8: Vec<f32> = (0..8).map(|i| (i as f32) / 10.0).collect();
        let vec258: Vec<f32> = (0..258).map(|i| (i as f32) / 1000.0).collect();
        let cases: Vec<(&str, Vec<u8>, usize)> = vec![
            ("raw-8", float32_to_blob_raw(&vec8), 8),
            ("raw-258", float32_to_blob_raw(&vec258), 258),
            (
                "int8-8",
                float32_to_quantized(&vec8, EMBEDDING_DTYPE_INT8),
                8,
            ),
            (
                "int8-258",
                float32_to_quantized(&vec258, EMBEDDING_DTYPE_INT8),
                258,
            ),
            ("f16-8", float32_to_quantized(&vec8, EMBEDDING_DTYPE_F16), 8),
            (
                "f16-258",
                float32_to_quantized(&vec258, EMBEDDING_DTYPE_F16),
                258,
            ),
        ];
        for (id, blob, want) in &cases {
            conn.execute(
                "INSERT INTO t (id, embedding) VALUES (?1, ?2)",
                rusqlite::params![id, blob],
            )
            .unwrap();
            assert_eq!(
                blob_to_float32(blob).len(),
                *want,
                "{id}: the decoder disagrees with the case"
            );
        }

        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, {EMBEDDING_DIM_SQL} FROM t ORDER BY rowid"
            ))
            .unwrap();
        let got: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let want: Vec<(String, i64)> = cases
            .iter()
            .map(|(id, _, d)| (id.to_string(), *d as i64))
            .collect();
        assert_eq!(got, want, "EMBEDDING_DIM_SQL diverges from the codec");
    }

    // ---- legacy text -------------------------------------------------------

    #[test]
    fn parses_legacy_json_array_and_object() {
        assert_eq!(
            parse_legacy_embedding_text("[0.1, 0.2, 0.3]"),
            Some(vec![0.1, 0.2, 0.3])
        );
        assert_eq!(
            parse_legacy_embedding_text(r#"{"0":0.1,"1":0.2,"2":0.3}"#),
            Some(vec![0.1, 0.2, 0.3])
        );
        assert_eq!(parse_legacy_embedding_text("42"), None);
        assert_eq!(parse_legacy_embedding_text("not json"), None);
    }
}
