//! P4.D222 tier-1 differential — `quilltap_core::embedding_vector::
//! average_embeddings` vs v4's REAL `averageEmbeddings`
//! (`lib/embedding/embedding-service.ts`, new at `492771aff`, bug 168).
//!
//! Every vector crosses as the little-endian hex of its f32 bytes, so both
//! sides start from the identical bits and the comparison is BIT-EXACT — the
//! f32 accumulator (`1e8 + 1 + 1` keeps losing its `+1`s), the zero-norm
//! pass-through, `-0` sums, subnormals, an overflow to infinity (whose NaN
//! output must be v4's canonical bits), and the differing-dimension refusal's
//! exact message.
//!
//! Generate the oracle output (from the v4 checkout, or the pinned worktree
//! while v4 HEAD is past the baseline):
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/average-embeddings.ts \
//!     > /tmp/oracle-average-embeddings.ndjson
//! Run:
//!   QT_ORACLE_AVERAGE_EMBEDDINGS=/tmp/oracle-average-embeddings.ndjson \
//!     cargo test -p quilltap-harness --test average_embeddings_equivalence

use quilltap_core::embedding_vector::average_embeddings;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    kind: String,
    id: String,
    inputs: Vec<String>,
    /// Absent on an error row; `null` for the empty set.
    #[serde(default, deserialize_with = "present")]
    out: Option<Option<String>>,
    #[serde(default)]
    error: Option<String>,
}

/// Keep a present `null` distinct from an absent key (`Some(None)` vs `None`).
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::deserialize(d)?))
}

fn from_hex(h: &str) -> Vec<f32> {
    let bytes = hex::decode(h).expect("hex");
    assert_eq!(bytes.len() % 4, 0, "f32 hex must be whole words");
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn to_hex(v: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for x in v {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    hex::encode(bytes)
}

#[test]
fn average_embeddings_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_AVERAGE_EMBEDDINGS") else {
        eprintln!("SKIP: QT_ORACLE_AVERAGE_EMBEDDINGS unset");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    // The case emits 41 rows; a truncated regen (the empty-file trap) must not
    // pass vacuously.
    assert_eq!(rows.len(), 41, "oracle row count");

    let (mut ok_rows, mut err_rows, mut none_rows) = (0, 0, 0);
    let mut failures = Vec::new();
    for row in &rows {
        assert_eq!(row.kind, "average", "{}: kind", row.id);
        let inputs: Vec<Vec<f32>> = row.inputs.iter().map(|h| from_hex(h)).collect();
        let got = average_embeddings(&inputs);
        match (&row.error, &row.out, got) {
            (Some(want), None, Err(e)) => {
                err_rows += 1;
                if &e.message() != want {
                    failures.push(format!("{}: error {:?} != {want:?}", row.id, e.message()));
                }
            }
            (None, Some(None), Ok(None)) => none_rows += 1,
            (None, Some(Some(want)), Ok(Some(v))) => {
                ok_rows += 1;
                let got = to_hex(&v);
                if &got != want {
                    failures.push(format!("{}: bits differ\n  v5 {got}\n  v4 {want}", row.id));
                }
            }
            (e, o, g) => failures.push(format!(
                "{}: shape mismatch — oracle error {e:?} out {:?}, v5 {:?}",
                row.id,
                o.as_ref().map(|x| x.as_ref().map(|s| s.len())),
                g.map(|x| x.map(|v| v.len()))
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // Every arm exercised: the empty set, the refusal, and the averaged rows.
    assert_eq!(none_rows, 1, "the empty-set row");
    assert_eq!(err_rows, 3, "the differing-dimension rows");
    assert_eq!(ok_rows, 37, "the averaged rows");
}
