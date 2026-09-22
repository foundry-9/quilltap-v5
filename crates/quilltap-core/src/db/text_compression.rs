//! Port of v4's `lib/database/text-compression.ts` — the text BLOB codec — plus
//! `lib/database/backends/sqlite/text-codec-function.ts`, the `qt_text()` UDF.
//!
//! This module is the SINGLE SOURCE OF TRUTH for how large text columns are
//! (de)serialized to/from SQLite BLOBs. Every consumer (the repositories, the
//! backup collector, the boot heals, the CLI's raw-SQL path) decodes through
//! [`blob_to_text`] and encodes through [`text_to_blob`], so the on-disk format
//! can evolve without touching the code that reads the text.
//!
//! It is a deliberate sibling of [`crate::embedding_blob`] and follows the same
//! doctrine: a self-describing header, readers that accept both the new format
//! and the legacy plaintext, and a batched migration that only reclaims bytes
//! rather than being a correctness prerequisite.
//!
//! ## On-disk formats
//!
//! **Legacy (and still written for short values):** a plain TEXT string. No
//! header, byte-identical to what the column always held.
//!
//! **Compressed:** a BLOB with a 3-byte header.
//!
//! ```text
//!   [0]      magic   = 0x51   ('Q'; distinct from 0xEB, the embedding magic)
//!   [1]      version = 0x01
//!   [2]      codec   = 0x01   (brotli)
//!   [3..]    payload
//! ```
//!
//! A value is treated as compressed iff it is a BLOB whose magic, version and
//! codec byte are all recognised. Anything else — a string, or a BLOB that
//! fails the check — is read as UTF-8 text. That tolerance is the whole point:
//! a column can hold a mix of compressed and plaintext rows indefinitely, so a
//! backfill can run late, run partially, or never run.
//!
//! ## Byte parity with Node
//!
//! v5 is a PEER WRITER of the same synced database, so the compressed bytes
//! must match what v4 would have written. **Measured 2026-09-21 (P4.D203 tier-1
//! item 1) and re-measured 2026-09-22 with the two large-prose rows below: 35
//! encode rows, byte-identical on every one** (decision, stored length and the
//! stored bytes): the `brotli` crate 8.0.4's `CompressorWriter` at
//! `quality: 5`, `size_hint: raw.len()` and the default `lgwin` produces
//! exactly what Node 24.13.1's bundled brotli 1.2.0
//! `brotliCompressSync(raw, { BROTLI_PARAM_QUALITY: 5, BROTLI_PARAM_SIZE_HINT:
//! raw.length })` produces. The corpus is committed at
//! `harness/oracle/fixtures/text-compression.json` and the pin is the tier-1
//! family `text_compression_equivalence`.
//!
//! **The measured parity ceiling is 262,293 raw bytes** — the corpus row
//! `real-shaped prose ~256 KiB`, which stores as 51,589 bytes on both sides;
//! `real-shaped prose ~64 KiB` (65,551 → 13,224) sits below it. Until those
//! two rows landed the largest input ever measured was 5,464 bytes, so parity
//! was pinned only over inputs the encoder handles in one go. Both rows are
//! deliberately *compressible* prose rather than random bytes: an
//! incompressible input takes the `total >= raw.len()` arm and is stored as
//! plain text, so it never exercises the encoder at all.
//!
//! ⚠ **Never call `flush()` on the `CompressorWriter` before dropping it.** A
//! flush emits an empty meta-block, which adds 1–3 bytes to every payload and
//! silently breaks parity while still round-tripping perfectly. The writer
//! finishes its stream on `Drop`; that — and only that — is the parity-correct
//! shape. (This was measured the hard way: the first parity run showed all 34
//! blob rows LONGER than Node's by 1–3 bytes.)
//!
//! ## Why there is a size floor
//!
//! Below [`TEXT_COMPRESSION_MIN_BYTES`] compression is a net LOSS: the header
//! plus brotli's own framing outweighs the gain, and short values stop being
//! greppable by tools that read the file directly. v4's measurement on real
//! message rows, brotli quality 5 per row:
//!
//! ```text
//!   <512 B    ~63% of original   ← a loss once framing is counted
//!   512B–1K    43%
//!   1K–4K      31%
//!   4K–16K     27%
//!   >16K       23%
//! ```
//!
//! **The floor is measured in BYTES, not characters** (v4 has a test saying so:
//! 200 four-byte emoji are 800 bytes and compress, despite being 400 UTF-16
//! units and 200 code points).
//!
//! ## The three reclamation migrations v5 does NOT run this round
//!
//! v4 `186eb09cb`/`f45a517a9` ship three batched backfills that rewrite
//! existing plaintext rows as compressed BLOBs. **v5 runs none of them this
//! round, deliberately and by name:**
//!
//! * `compress-chat-message-text-v1` — `dependsOn: ['sqlite-initial-schema-v1',
//!   'create-chat-message-fts-v1']`, `BATCH_SIZE` 250, `SAMPLE_LIMIT` 50,
//!   `COLUMNS = ['content','opaqueContent','description','context']`.
//! * `compress-llm-log-payloads-v1` — `dependsOn:
//!   ['move-llm-logs-to-separate-db-v1']`.
//! * `compress-conversation-chunk-content-v1`.
//!
//! All three are, in v4's own words, "byte reclamation, not a correctness
//! prerequisite" — new writes already compress, and every reader on both sides
//! tolerates a mixed column forever. There is no gate for them to sit behind.
//!
//! ⚠ v4's ordering constraint matters if one is ever re-homed: `create-chat-
//! message-fts-v1` MUST run before `compress-chat-message-text-v1`, because the
//! `_au` trigger's comparison is DECODED (`qt_text(new."content") IS NOT
//! qt_text(old."content")`) and so a compress-in-place leaves the index
//! untouched. v5 honours it trivially by never compressing in a heal at all
//! this round. A future order may re-home `compress-chat-message-text-v1` as a
//! ledger-guarded boot heal — but only AFTER P4.D204's reconciler has put the
//! index in place.
//!
//! Seam 12 of the `186eb09cb` drift row (v4's `withTransaction` bypass) is
//! **N/A for v5**, recorded here against that sha: v4 needed it because its
//! collection abstraction wraps writes in a transaction the migration must
//! sidestep; v5 has no collection abstraction and its writers own their
//! transactions directly.
//!
//! @module db/text_compression (v4 `lib/database/text-compression.ts`)

use std::io::Write;

use rusqlite::functions::FunctionFlags;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::Connection;

use crate::pascal::js_value::number_to_string;

/// First byte of every compressed-text blob.
pub const TEXT_BLOB_MAGIC: u8 = 0x51;
/// Compressed-format version this codec reads and writes.
pub const TEXT_BLOB_VERSION: u8 = 0x01;
/// Codec byte: brotli.
pub const TEXT_CODEC_BROTLI: u8 = 0x01;
/// Bytes of header before the payload.
pub const TEXT_BLOB_HEADER_BYTES: usize = 3;

/// Values shorter than this stay plaintext — see the module doc. Changing it
/// means re-running the measurement, not guessing.
pub const TEXT_COMPRESSION_MIN_BYTES: usize = 512;

/// Brotli quality for stored text. 5 is the knee of the curve: quality 11 buys
/// a few more percent for roughly an order of magnitude more CPU, which is a
/// bad trade on a write path that runs per message.
pub const TEXT_COMPRESSION_QUALITY: i32 = 5;

/// The SQL-visible name. Referenced in raw SQL across the repositories.
pub const TEXT_CODEC_FUNCTION_NAME: &str = "qt_text";

/// What [`text_to_blob`] decided to store — v4's `Buffer | string` union.
///
/// Binds itself: `Text` binds as a SQLite TEXT value and `Blob` as a BLOB, so a
/// caller can always store the return value directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextCell {
    /// Below the floor, or compression did not pay — stored as plain TEXT.
    Text(String),
    /// The 3-byte header plus the brotli payload.
    Blob(Vec<u8>),
}

impl TextCell {
    /// The stored byte length — what the column actually costs.
    pub fn stored_len(&self) -> usize {
        match self {
            TextCell::Text(s) => s.len(),
            TextCell::Blob(b) => b.len(),
        }
    }

    /// True when this cell was compressed.
    pub fn is_blob(&self) -> bool {
        matches!(self, TextCell::Blob(_))
    }
}

impl ToSql for TextCell {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            TextCell::Text(s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
            TextCell::Blob(b) => ToSqlOutput::Borrowed(ValueRef::Blob(b)),
        })
    }
}

/// Report whether a value carries the compressed-text header.
///
/// Checks magic, version AND codec, so a future version byte is treated as
/// "not mine" rather than being mis-decoded by an older build.
pub fn is_compressed_text_blob(value: &[u8]) -> bool {
    value.len() >= TEXT_BLOB_HEADER_BYTES
        && value[0] == TEXT_BLOB_MAGIC
        && value[1] == TEXT_BLOB_VERSION
        && value[2] == TEXT_CODEC_BROTLI
}

/// Encode a string for storage — v4 `textToBlob`.
///
/// Returns [`TextCell::Text`] (the ORIGINAL string) when the value is below the
/// size floor or when compression fails to make it smaller, so a caller can
/// always bind the return value directly and short rows keep their plain TEXT
/// representation.
pub fn text_to_blob(value: &str) -> TextCell {
    let raw = value.as_bytes();
    if raw.len() < TEXT_COMPRESSION_MIN_BYTES {
        return TextCell::Text(value.to_string());
    }

    let compressed = brotli_compress(raw);

    if !compressed_is_worth_storing(raw.len(), compressed.len()) {
        return TextCell::Text(value.to_string());
    }

    let total = compressed.len() + TEXT_BLOB_HEADER_BYTES;
    let mut out = Vec::with_capacity(total);
    out.push(TEXT_BLOB_MAGIC);
    out.push(TEXT_BLOB_VERSION);
    out.push(TEXT_CODEC_BROTLI);
    out.extend_from_slice(&compressed);
    TextCell::Blob(out)
}

/// v4's `total >= raw.length` decision: incompressible input (already-
/// compressed payloads, base64, random text) can come out LARGER, and storing
/// that would be strictly worse than the string.
///
/// ⚠ **MEASURED UNREACHABLE for any real ≥512-byte UTF-8 input** (P4.D203,
/// 2026-09-21). The mutation proof "drop the guard" SURVIVED the whole tier-1
/// corpus, so the arm was probed directly across seven adversarial shapes at
/// and above the floor — base64 of random bytes, printable-ASCII-95, random
/// 2-byte codepoints (U+0080–U+07FF), random 3-byte BMP, random 4-byte astral,
/// and base64 of an already-brotli'd payload, at sizes 512 B to 16 KB. The
/// WORST compression ratio brotli q5 produced was **0.863** (printable ASCII
/// 95 at exactly 512 bytes); the corpus's own worst is 0.802. Brotli's framing
/// overhead is only a few bytes at ≥512 B, so `compressed + 3 >= raw` needs a
/// ratio at 1.0, which no UTF-8 text reaches.
///
/// The guard therefore stays because **v4 has it** — a port that drops it
/// diverges from v4's source even where no input distinguishes them, and a
/// future codec or quality change could make it bite. It is pinned HERE, at
/// its own altitude, rather than through the corpus: that is what makes the
/// mutation proof bite (`a-guard-whose-other-conjuncts-are-false-is-untested`).
fn compressed_is_worth_storing(raw_len: usize, compressed_len: usize) -> bool {
    compressed_len + TEXT_BLOB_HEADER_BYTES < raw_len
}

/// Node `zlib.brotliCompressSync(raw, { QUALITY: 5, SIZE_HINT: raw.length })`,
/// byte-for-byte. See the module doc's parity paragraph — and its warning about
/// `flush()`.
fn brotli_compress(raw: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    {
        let params = brotli::enc::BrotliEncoderParams {
            quality: TEXT_COMPRESSION_QUALITY,
            size_hint: raw.len(),
            ..Default::default()
        };
        let mut writer = brotli::CompressorWriter::with_params(&mut out, 4096, &params);
        // `write_all` into a `Vec` cannot fail; the writer finishes on drop.
        writer
            .write_all(raw)
            .expect("writing to an in-memory Vec cannot fail");
    }
    out
}

/// Decode a stored value back to a string — v4 `blobToText`.
///
/// TOTAL: never fails, never panics. v4's six arms in order — `NULL` →
/// `None`; TEXT → itself; INTEGER/REAL → JS `String(value)`; a BLOB without the
/// header → lossy UTF-8; else decompress, with the payload-as-UTF-8 fallback a
/// truncated or corrupt payload takes.
pub fn blob_to_text(value: ValueRef<'_>) -> Option<String> {
    match value {
        ValueRef::Null => None,
        ValueRef::Text(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        // v4's `String(value)` arm: better-sqlite3 hands a numeric cell back as
        // a JS number, and `blobToText` stringifies it. `safeIntegers` is off,
        // so an INTEGER arrives as a double — which is why both arms render
        // through the same JS `Number::toString`.
        ValueRef::Integer(i) => Some(number_to_string(i as f64)),
        ValueRef::Real(f) => Some(number_to_string(f)),
        ValueRef::Blob(bytes) => Some(decode_blob(bytes)),
    }
}

/// The BLOB arm of [`blob_to_text`], also reachable from a `&[u8]` a caller
/// already holds (the sanitizer, the CLI formatter).
pub fn decode_blob(bytes: &[u8]) -> String {
    if !is_compressed_text_blob(bytes) {
        // Exactly what a legacy plaintext row that SQLite happened to hand back
        // as a BLOB should become.
        return String::from_utf8_lossy(bytes).into_owned();
    }

    let payload = &bytes[TEXT_BLOB_HEADER_BYTES..];
    let mut decoded: Vec<u8> = Vec::new();
    match brotli::BrotliDecompress(&mut std::io::Cursor::new(payload), &mut decoded) {
        Ok(()) => String::from_utf8_lossy(&decoded).into_owned(),
        // A truncated or corrupt payload must not take down the read path; the
        // caller gets the best available reading of the bytes.
        Err(_) => String::from_utf8_lossy(payload).into_owned(),
    }
}

/// A text column that may hold a compressed BLOB.
///
/// Converting a read site is a ONE-TYPE-NAME change: `row.get::<_, String>(i)`
/// becomes `row.get::<_, CompressedText>(i)?.0`, and `Option<String>` becomes
/// `Option<CompressedText>`. Every such site previously rejected a BLOB with
/// rusqlite's `InvalidColumnType` — which is precisely the failure the drift
/// ledger measured on a v4-4.10 instance.
///
/// ⚠ The decode arm must run BEFORE any JSON or embedding handling at a site
/// that has both (v4 carries the same rule, in `rowToDocument`): `llm_logs.
/// request` is both a compressed column and a JSON column, and an embedding
/// BLOB (magic `0xEB`) must never be mistaken for compressed text nor the
/// reverse. [`is_compressed_text_blob`] checks all three header bytes, so the
/// two magics cannot collide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedText(pub String);

impl CompressedText {
    /// Consume the newtype, yielding the decoded text.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<CompressedText> for String {
    fn from(v: CompressedText) -> Self {
        v.0
    }
}

impl FromSql for CompressedText {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match blob_to_text(value) {
            Some(s) => Ok(CompressedText(s)),
            // `Option<CompressedText>` handles NULL itself; a bare
            // `CompressedText` over a NULL cell is the same error a bare
            // `String` would have raised.
            None => Err(FromSqlError::InvalidType),
        }
    }
}

/// Register `qt_text(value)` on a connection — v4
/// `registerTextCodecFunction`.
///
/// Compressed text columns hold a BLOB. The repository layer decodes them
/// transparently, but **raw SQL does not go through the repository layer**, and
/// SQLite has no idea how to read brotli. Any statement that looks *inside*
/// such a column — `json_extract`, `LIKE`, `LENGTH` on the text, an FTS trigger
/// — must wrap it:
///
/// ```sql
/// SELECT json_extract(qt_text("response"), '$.error') FROM llm_logs;
/// ```
///
/// `qt_text` is deliberately total: it accepts a compressed BLOB, a plain
/// string, an uncompressed BLOB or NULL and always returns text or NULL. That
/// means a statement can wrap a column that is only PARTIALLY migrated — which
/// is the normal state, since reads tolerate plaintext and the backfill may run
/// late or not at all.
///
/// **Every connection v5 opens must register it**, with the same "before
/// anything else touches the file" discipline as the `PRAGMA key`. Three facts
/// make this non-negotiable rather than a nicety:
///
/// 1. v4 `f45a517a9` puts three triggers over `chat_messages` and TWO of them
///    call `qt_text` — `_ai`'s INSERT body and `_au`'s **`WHEN` clause**.
/// 2. **SQLite resolves a trigger's functions when it COMPILES the trigger
///    program, before any row is tested.** So the blast radius is not "indexed
///    messages": it is EVERY `chat_messages` INSERT, including the SYSTEM,
///    staff and content-NULL rows the `WHEN` clause would have excluded.
/// 3. A connection that misses it therefore fails loudly with "no such
///    function: qt_text" rather than silently returning wrong answers — which
///    is the behaviour we want, and why a registration failure here is an open
///    ERROR, not a warn.
pub fn register_qt_text(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        TEXT_CODEC_FUNCTION_NAME,
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(blob_to_text(ctx.get_raw(0))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's test helper: repeat `s` until it is comfortably over the floor.
    fn long(s: &str) -> String {
        let reps = (TEXT_COMPRESSION_MIN_BYTES * 4).div_ceil(s.len());
        s.repeat(reps)
    }

    fn decode(cell: &TextCell) -> Option<String> {
        match cell {
            TextCell::Text(s) => blob_to_text(ValueRef::Text(s.as_bytes())),
            TextCell::Blob(b) => blob_to_text(ValueRef::Blob(b)),
        }
    }

    #[test]
    fn round_trips_every_shape_byte_exactly() {
        for text in [
            long("The djinn of Istanbul waited by the bosphorus. "),
            long("café — naïve — 日本語 — 🜁🜂🜃🜄 — Ω≈ç√∫ "),
            long("{\"role\":\"assistant\",\"content\":\"well, quite.\"} "),
            long("## Interchange 4\n\n*He bowed.*\n\n"),
            long("   \t  \n   "),
        ] {
            let stored = text_to_blob(&text);
            assert!(stored.is_blob(), "expected a blob for {} bytes", text.len());
            assert_eq!(decode(&stored).as_deref(), Some(text.as_str()));
        }
    }

    #[test]
    fn shrinks_compressible_text_substantially() {
        let text = long("the same clause over and over again, endlessly. ");
        let stored = text_to_blob(&text);
        assert!(stored.stored_len() < text.len() / 2);
    }

    #[test]
    fn leaves_short_values_as_plain_strings() {
        let text = "x".repeat(TEXT_COMPRESSION_MIN_BYTES - 1);
        let stored = text_to_blob(&text);
        assert_eq!(stored, TextCell::Text(text.clone()));
        assert_eq!(decode(&stored).as_deref(), Some(text.as_str()));
    }

    #[test]
    fn compresses_at_exactly_the_floor() {
        let text = "x".repeat(TEXT_COMPRESSION_MIN_BYTES);
        assert!(text_to_blob(&text).is_blob());
    }

    #[test]
    fn measures_the_floor_in_bytes_not_characters() {
        // 200 four-byte emoji = 800 bytes, over the floor despite being 200
        // code points (and 400 UTF-16 units, which is what v4's `.length` sees).
        let text = "🜁".repeat(200);
        assert!(text.chars().count() < TEXT_COMPRESSION_MIN_BYTES);
        assert!(text.len() > TEXT_COMPRESSION_MIN_BYTES);
        let stored = text_to_blob(&text);
        assert!(stored.is_blob());
        assert_eq!(decode(&stored).as_deref(), Some(text.as_str()));
    }

    #[test]
    fn never_stores_more_bytes_than_the_plain_string_would() {
        // High-entropy inputs are where a naive codec loses. Whatever it
        // decides, the stored form must never be larger than the text it
        // replaces — and it must still round-trip.
        let mut seed: u32 = 0x1a2b_3c4d;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let alphabet: Vec<char> =
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
                .chars()
                .collect();
        let pseudo_base64: String = (0..5464).map(|_| alphabet[(rnd() % 64) as usize]).collect();
        let printable: String = (0..2000u32)
            .map(|i| char::from(32 + ((i * 7919) % 95) as u8))
            .collect();
        for text in [pseudo_base64, printable] {
            let stored = text_to_blob(&text);
            assert!(stored.stored_len() <= text.len());
            assert_eq!(decode(&stored).as_deref(), Some(text.as_str()));
        }
    }

    /// v4's `total >= raw.length` boundary, pinned at its own altitude — see
    /// the function's doc for why the corpus cannot reach it.
    #[test]
    fn the_incompressible_guard_refuses_a_stored_form_that_is_not_smaller() {
        // 3 bytes of header + the payload must come to LESS than the raw text.
        assert!(compressed_is_worth_storing(512, 508)); // 511 < 512 — store it
        assert!(!compressed_is_worth_storing(512, 509)); // 512 >= 512 — refuse
        assert!(!compressed_is_worth_storing(512, 510)); // 513 >= 512 — refuse
        assert!(!compressed_is_worth_storing(512, 600)); // grew — refuse
                                                         // The degenerate edges: a payload can never be worth storing when the
                                                         // header alone matches or exceeds the raw length.
        assert!(!compressed_is_worth_storing(3, 0));
        assert!(!compressed_is_worth_storing(0, 0));
    }

    #[test]
    fn blob_to_text_tolerates_every_legacy_shape() {
        assert_eq!(
            blob_to_text(ValueRef::Text(b"already text")).as_deref(),
            Some("already text")
        );
        assert_eq!(
            blob_to_text(ValueRef::Blob(b"legacy plaintext row")).as_deref(),
            Some("legacy plaintext row")
        );
        assert_eq!(blob_to_text(ValueRef::Null), None);
        // v4's `String(value)` arm for a numeric cell.
        assert_eq!(blob_to_text(ValueRef::Integer(42)).as_deref(), Some("42"));
        assert_eq!(blob_to_text(ValueRef::Real(1.5)).as_deref(), Some("1.5"));
    }

    #[test]
    fn does_not_throw_on_a_truncated_compressed_payload() {
        let TextCell::Blob(stored) = text_to_blob(&long("some compressible text ")) else {
            panic!("expected a blob");
        };
        // Total, so this just returns the best available reading of the bytes.
        let got = blob_to_text(ValueRef::Blob(&stored[..8]));
        assert!(got.is_some());
    }

    #[test]
    fn treats_an_unknown_version_or_codec_byte_as_not_mine() {
        let TextCell::Blob(stored) = text_to_blob(&long("abc ")) else {
            panic!("expected a blob");
        };
        let mut future_version = stored.clone();
        future_version[1] = TEXT_BLOB_VERSION + 1;
        assert!(!is_compressed_text_blob(&future_version));

        let mut future_codec = stored.clone();
        future_codec[2] = TEXT_CODEC_BROTLI + 1;
        assert!(!is_compressed_text_blob(&future_codec));
    }

    #[test]
    fn does_not_mistake_an_embedding_blob_for_compressed_text() {
        // The embedding magic is 0xEB (crate::embedding_blob); ours is 0x51.
        let embedding = [0xeb_u8, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00];
        assert!(!is_compressed_text_blob(&embedding));
    }

    /// The four SQLite-side facts the whole raw-SQL rewrite rests on, measured
    /// in ONE test over the real amalgamation (P4.D203 tier-1 item 1; the drift
    /// ledger left all four as inferences). Each is why a specific expression
    /// had to gain a `qt_text()` wrapper — or why a specific read bind had to
    /// change type.
    #[test]
    fn the_four_sqlite_facts_that_force_the_qt_text_wrappers() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, c)")
            .unwrap();
        // A real `llm_logs.response` shape: valid JSON, over the floor, and
        // compressible — so the cell is genuinely a brotli BLOB.
        let payload = format!(
            "{{\"error\":\"boom\",\"detail\":\"{}\"}}",
            "the same clause over and over again, endlessly. ".repeat(20)
        );
        let TextCell::Blob(blob) = text_to_blob(&payload) else {
            panic!("expected a blob");
        };
        conn.execute("INSERT INTO t (id, c) VALUES (1, ?1)", [&blob])
            .unwrap();

        // (1) `json_extract` over a BLOB raises — BLOB arguments are JSONB
        // since SQLite 3.45, so a brotli payload is read as malformed JSONB.
        // This is why `almanack/phase6_wire_records.rs` and v4's usage
        // aggregate had to wrap the column.
        let err = conn
            .query_row(
                "SELECT json_extract(c, '$.error') FROM t WHERE id = 1",
                [],
                |r| r.get::<_, Option<String>>(0),
            )
            .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("json"),
            "expected a malformed-JSON error, got {err}"
        );
        // Wrapped, the same expression reads the text inside.
        register_qt_text(&conn).unwrap();
        let got: Option<String> = conn
            .query_row(
                "SELECT json_extract(qt_text(c), '$.error') FROM t WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(got.as_deref(), Some("boom"));

        // (2) `trim(<blob>) != ''` is ALWAYS true — `trim` casts the BLOB to
        // text, and a brotli payload never trims to empty. So
        // `conversation_chunks.rs`'s emptiness guard was silent-wrong (it let
        // every compressed row through), not erroring.
        let unwrapped_nonempty: i64 = conn
            .query_row("SELECT trim(c) != '' FROM t WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(unwrapped_nonempty, 1);
        conn.execute(
            "INSERT INTO t (id, c) VALUES (2, ?1)",
            [&text_to_blob("   ")],
        )
        .unwrap();
        let wrapped_blank: i64 = conn
            .query_row(
                "SELECT trim(qt_text(c)) != '' FROM t WHERE id = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            wrapped_blank, 0,
            "a whitespace-only cell must trim to empty"
        );

        // (3) `LENGTH(<blob>)` counts COMPRESSED bytes, not characters — so
        // `conversation_render_reconcile.rs`'s three `LENGTH(cc."content")`
        // were measuring the wrong quantity, also silently.
        let (raw_len, decoded_len): (i64, i64) = conn
            .query_row(
                "SELECT LENGTH(c), LENGTH(qt_text(c)) FROM t WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(raw_len as usize, blob.len());
        assert!(
            decoded_len > raw_len,
            "the decoded text must be longer than its compressed form ({decoded_len} vs {raw_len})"
        );

        // (4) rusqlite's `FromSql for String` REJECTS a BLOB with
        // `InvalidColumnType` — the failure the drift ledger measured on a
        // v4-4.10 instance, and the reason every read site on the seven
        // registered columns had to become `CompressedText`.
        let as_string = conn.query_row("SELECT c FROM t WHERE id = 1", [], |r| {
            r.get::<_, String>(0)
        });
        assert!(
            matches!(as_string, Err(rusqlite::Error::InvalidColumnType(..))),
            "expected InvalidColumnType, got {as_string:?}"
        );
        // The newtype reads the same cell.
        let via_newtype: CompressedText = conn
            .query_row("SELECT c FROM t WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(via_newtype.0, payload);
    }

    #[test]
    fn rejects_a_buffer_too_short_to_carry_a_header() {
        assert!(!is_compressed_text_blob(&[
            TEXT_BLOB_MAGIC,
            TEXT_BLOB_VERSION
        ]));
        assert!(!is_compressed_text_blob(&[]));
    }
}
