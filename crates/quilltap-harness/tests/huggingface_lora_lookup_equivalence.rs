//! P4.D138 unit 7 — the HuggingFace LoRA lookup (v4 `2ece98c90`) vs v4's REAL
//! `lib/image-gen/huggingface-repo-id.ts` + `huggingface-lookup.ts`.
//!
//! Tier 1 for the pure half (repo-id extraction + the card URL) and a canned
//! transport for the network half: the corpus carries the canned wire WITH each
//! row, so both sides drive the identical response, and it carries the REQUEST
//! v4 made, so v5's URL and `Authorization` header are comparands rather than
//! assumptions.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the case header):
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-hf-lora.ndjson \
//!     npx tsx <V5>/harness/oracle/cases/huggingface-lora-lookup.ts
//! Run:
//!   QT_ORACLE_HF_LORA=/tmp/oracle-hf-lora.ndjson \
//!     cargo test -p quilltap-harness --test huggingface_lora_lookup_equivalence

use std::sync::Mutex;

use quilltap_core::image_gen::huggingface_lookup::{
    lookup_huggingface_lora, LoraMetadataTransport, ThrownError,
};
use quilltap_core::image_gen::huggingface_repo_id::{
    extract_huggingface_repo_id, huggingface_card_url,
};
use quilltap_core::model::wire::WireResponse;
use serde_json::Value;

/// ⚠ RECORDED DIVERGENCE — the non-JSON body's `detail`.
///
/// v4's detail on that one arm is the message V8 puts on the `SyntaxError` that
/// `response.json()` throws (`Unexpected token 'h', "this is not json" is not
/// valid JSON`). It is the JSON parser's own wording, not v4's, and no Rust
/// parser produces it. Everything else about the arm — the reason, the ids, the
/// URL — is compared whole; only this string is exempt, and BOTH spellings are
/// asserted so the exemption cannot quietly widen.
const V8_JSON_ERROR_PREFIX: &str = "Unexpected token";

/// The corpus's canned wire, recorded for exactly one call.
struct CannedTransport {
    answer: Result<WireResponse, ThrownError>,
    seen: Mutex<Vec<(String, Vec<String>)>>,
}

impl CannedTransport {
    fn new(answer: Result<WireResponse, ThrownError>) -> Self {
        Self {
            answer,
            seen: Mutex::new(Vec::new()),
        }
    }
    /// The requests actually made, in v4's recorded `{url, headers}` shape with
    /// the headers sorted (v4 sorts them too — the order is the transport's).
    fn recorded(&self) -> Value {
        Value::Array(
            self.seen
                .lock()
                .unwrap()
                .iter()
                .map(|(url, headers)| {
                    let mut h = headers.clone();
                    h.sort();
                    serde_json::json!({ "url": url, "headers": h })
                })
                .collect(),
        )
    }
}

impl LoraMetadataTransport for CannedTransport {
    async fn get(
        &self,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<WireResponse, ThrownError> {
        self.seen.lock().unwrap().push((
            url.to_string(),
            headers.iter().map(|(k, v)| format!("{k}: {v}")).collect(),
        ));
        self.answer.clone()
    }
}

fn load() -> Option<Vec<Value>> {
    let path = std::env::var("QT_ORACLE_HF_LORA").ok()?;
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("QT_ORACLE_HF_LORA={path}: {e}"));
    Some(
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("oracle row"))
            .collect(),
    )
}

#[test]
fn huggingface_lora_lookup_matches_v4() {
    let Some(rows) = load() else {
        eprintln!("SKIP: QT_ORACLE_HF_LORA unset");
        return;
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let (mut repo_rows, mut lookup_rows) = (0usize, 0usize);

    for row in &rows {
        let name = row["name"].as_str().unwrap().to_string();
        match row["kind"].as_str().unwrap() {
            "repo_id" => {
                repo_rows += 1;
                let source = row["source"].as_str().unwrap();
                let got = extract_huggingface_repo_id(source);
                let want = row["repoId"].as_str().map(str::to_string);
                if got != want {
                    eprintln!("[{name}] repoId: got {got:?}, want {want:?} (source {source:?})");
                    failed.push(name.clone());
                    continue;
                }
                let got_url = got.as_deref().map(huggingface_card_url);
                let want_url = row["cardUrl"].as_str().map(str::to_string);
                if got_url != want_url {
                    eprintln!("[{name}] cardUrl: got {got_url:?}, want {want_url:?}");
                    failed.push(name);
                }
            }
            "lookup" => {
                lookup_rows += 1;
                let wire = &row["wire"];
                let answer = if let Some(thrown) = wire.get("thrown") {
                    Err(ThrownError {
                        name: thrown["name"].as_str().unwrap().to_string(),
                        message: thrown["message"].as_str().unwrap().to_string(),
                    })
                } else {
                    Ok(WireResponse::new(
                        wire["status"].as_u64().unwrap() as u16,
                        wire["body"].as_str().unwrap(),
                    ))
                };
                let transport = CannedTransport::new(answer);
                let token = row["token"].as_str();
                let result = rt.block_on(lookup_huggingface_lora(
                    &transport,
                    row["source"].as_str().unwrap(),
                    token,
                ));
                let mut got = serde_json::to_value(&result).unwrap();
                let mut want = row["result"].clone();

                // The one exempt string, asserted in BOTH spellings so the
                // exemption can never widen unnoticed.
                if name == "a_body_that_is_not_json_is_http" {
                    let g = got["detail"].as_str().unwrap_or_default().to_string();
                    let w = want["detail"].as_str().unwrap_or_default().to_string();
                    assert!(
                        w.starts_with(V8_JSON_ERROR_PREFIX),
                        "[{name}] v4's detail is no longer V8's SyntaxError wording ({w:?}) — \
                         re-measure the divergence before keeping the exemption"
                    );
                    assert!(
                        !g.is_empty() && !g.starts_with(V8_JSON_ERROR_PREFIX),
                        "[{name}] v5's detail now reads like V8's ({g:?}) — the divergence is \
                         gone; delete this exemption and compare whole"
                    );
                    got.as_object_mut().unwrap().remove("detail");
                    want.as_object_mut().unwrap().remove("detail");
                }

                if got != want {
                    eprintln!(
                        "[{name}] result MISMATCH:\n  got  {}\n  want {}",
                        serde_json::to_string(&got).unwrap(),
                        serde_json::to_string(&want).unwrap()
                    );
                    failed.push(name.clone());
                }
                let seen = transport.recorded();
                if seen != row["seen"] {
                    eprintln!(
                        "[{name}] REQUEST MISMATCH:\n  got  {}\n  want {}",
                        serde_json::to_string(&seen).unwrap(),
                        serde_json::to_string(&row["seen"]).unwrap()
                    );
                    failed.push(name);
                }
            }
            other => panic!("unknown oracle row kind {other}"),
        }
    }

    // Shape guards: a corpus that silently loses a tier stops proving anything.
    assert!(
        repo_rows >= 38,
        "the oracle is stale: only {repo_rows} repo-id rows (regenerate it)"
    );
    assert!(
        lookup_rows >= 25,
        "the oracle is stale: only {lookup_rows} lookup rows (regenerate it)"
    );
    assert!(
        failed.is_empty(),
        "{} case(s) failed: {failed:?}",
        failed.len()
    );
    eprintln!("huggingface lora lookup: {repo_rows} repo-id + {lookup_rows} lookup rows OK");
}
