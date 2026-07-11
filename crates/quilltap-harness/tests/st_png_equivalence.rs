//! Tier-1 differential test: the SillyTavern PNG codec (v4
//! `lib/sillytavern/character.ts` — `createSTCharacterPNG` / `parseSTCharacterPNG`,
//! which run `exportSTCharacter` + `generatePlaceholderPNG` + `calculateCRC32`).
//!
//! Three legs against the v4 oracle:
//!   - ENCODE / real-avatar: the produced PNG must be BYTE-IDENTICAL.
//!   - ENCODE / placeholder: the tEXt + IHDR chunks must be byte-identical and
//!     the IDAT must INFLATE to identical raw scanlines (the declared DEFLATE
//!     seam — v4 zlib-compresses, the port stores).
//!   - DECODE: the parsed card must match (or both null).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx <worktree>/harness/oracle/cases/st-png.ts > /tmp/oracle-st-png.ndjson
//! Run:
//!   QT_ORACLE_ST_PNG=/tmp/oracle-st-png.ndjson \
//!     cargo test -p quilltap-harness --test st_png_equivalence

use std::io::Read;

use flate2::read::ZlibDecoder;
use quilltap_core::services::sillytavern::{create_st_character_png, parse_st_character_png};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Row {
    id: String,
    kind: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    character: Option<Value>,
    #[serde(default)]
    avatar: Option<String>,
    #[serde(default)]
    input: Option<String>,
    out: Value,
}

fn b64_decode(s: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .expect("valid base64")
}

/// Walk PNG chunks, returning `(type, whole-chunk-bytes)` in order.
fn png_chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut out = Vec::new();
    let mut offset = 8usize; // skip signature
    while offset + 8 <= png.len() {
        let length = u32::from_be_bytes([
            png[offset],
            png[offset + 1],
            png[offset + 2],
            png[offset + 3],
        ]) as usize;
        let ty = [
            png[offset + 4],
            png[offset + 5],
            png[offset + 6],
            png[offset + 7],
        ];
        let end = (offset + 12 + length).min(png.len());
        out.push((ty, png[offset..end].to_vec()));
        offset += 12 + length;
    }
    out
}

fn chunk_by_type<'a>(chunks: &'a [([u8; 4], Vec<u8>)], ty: &[u8; 4]) -> Option<&'a Vec<u8>> {
    chunks.iter().find(|(t, _)| t == ty).map(|(_, b)| b)
}

/// The IDAT data bytes (between the 8-byte length+type prefix and 4-byte CRC).
fn idat_data(chunks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let chunk = chunk_by_type(chunks, b"IDAT").expect("IDAT present");
    chunk[8..chunk.len() - 4].to_vec()
}

fn inflate(zlib: &[u8]) -> Vec<u8> {
    let mut d = ZlibDecoder::new(zlib);
    let mut raw = Vec::new();
    d.read_to_end(&mut raw).expect("inflate IDAT");
    raw
}

#[test]
fn st_png_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_ST_PNG") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_ST_PNG to the oracle NDJSON (see header).");
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n_encode_exact = 0;
    let mut n_encode_placeholder = 0;
    let mut n_decode = 0;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        match row.kind.as_str() {
            "encode" => {
                let character = row.character.as_ref().expect("encode row has character");
                let expected = b64_decode(row.out.as_str().expect("encode out is base64"));
                let avatar = row.avatar.as_deref().map(b64_decode);
                let got = create_st_character_png(character, avatar.as_deref());
                match row.mode.as_deref() {
                    Some("exact") => {
                        assert_eq!(
                            got, expected,
                            "[{}] encode(real-avatar) not byte-identical",
                            row.id
                        );
                        n_encode_exact += 1;
                    }
                    Some("placeholder") => {
                        let gc = png_chunks(&got);
                        let ec = png_chunks(&expected);
                        // tEXt + IHDR chunks byte-identical (the ported card JSON
                        // + framing + header).
                        assert_eq!(
                            chunk_by_type(&gc, b"tEXt"),
                            chunk_by_type(&ec, b"tEXt"),
                            "[{}] placeholder tEXt chunk differs",
                            row.id
                        );
                        assert_eq!(
                            chunk_by_type(&gc, b"IHDR"),
                            chunk_by_type(&ec, b"IHDR"),
                            "[{}] placeholder IHDR chunk differs",
                            row.id
                        );
                        // IDAT decodes to identical raw scanlines (the DEFLATE seam).
                        assert_eq!(
                            inflate(&idat_data(&gc)),
                            inflate(&idat_data(&ec)),
                            "[{}] placeholder decoded pixels differ",
                            row.id
                        );
                        n_encode_placeholder += 1;
                    }
                    other => panic!("[{}] unknown encode mode {other:?}", row.id),
                }
            }
            "decode" => {
                let input = b64_decode(row.input.as_ref().expect("decode row has input"));
                let got = parse_st_character_png(&input);
                let expected = if row.out.is_null() {
                    None
                } else {
                    Some(row.out.clone())
                };
                assert_eq!(got, expected, "[{}] decode result differs", row.id);
                n_decode += 1;
            }
            other => panic!("[{}] unknown row kind {other}", row.id),
        }
    }

    assert!(
        n_encode_exact >= 3 && n_encode_placeholder >= 2 && n_decode >= 8,
        "expected the full corpus, saw exact={n_encode_exact} placeholder={n_encode_placeholder} decode={n_decode}"
    );
    eprintln!(
        "OK: ST-PNG matched oracle — {n_encode_exact} real-avatar, {n_encode_placeholder} placeholder, {n_decode} decode."
    );
}
