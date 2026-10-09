//! Dogfood #158 — a `.qtap` upload past 2 MB reaches the import loader.
//!
//! The import legs (`POST /api/v1/system/tools?action=import-preview|
//! import-execute`) buffer the body and then re-drive the multipart parser over
//! a rebuilt `Request`. That request carried the headers but not the original's
//! extensions — and the router's 10 GB `DefaultBodyLimit` rides the extensions —
//! so axum's `Multipart` fell back to its 2 MB default and EVERY upload past
//! 2 MB answered 400 `No file provided` (an 8 MB character export with its
//! memories, on the Friday copy). The SPA's Import dialog always uses these
//! multipart legs. v4 reads `await req.formData()` with no such cap.
//!
//! The payload is deliberately NOT a valid export: the loader's own sentence
//! proves the file part was parsed; `No file provided` is the parser failing.
//!
//! Run: `cargo test -p quilltap-web --test qtap_import_large_multipart`

mod common;

use serde_json::Value;

const LOADER_SENTENCE: &str = "Invalid export file format. Expected quilltap-export v1.0 format.";

async fn post_file(addr: std::net::SocketAddr, action: &str, bytes: Vec<u8>) -> (u16, Value) {
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name("big.qtap")
        .mime_str("application/octet-stream")
        .unwrap();
    let mut form = reqwest::multipart::Form::new().part("file", part);
    if action == "import-execute" {
        form = form.text("options", r#"{"conflictStrategy":"duplicate"}"#);
    }
    let res = reqwest::Client::new()
        .post(format!("http://{addr}/api/v1/system/tools?action={action}"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    let status = res.status().as_u16();
    (status, res.json().await.unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_qtap_upload_past_two_megabytes_reaches_the_loader() {
    let base = common::materialize_bare_instance();
    let (addr, _state) = common::serve_instance(base.path(), |c| c).await;

    // ~3 MB: one envelope line padded with trailing spaces past axum's default.
    let mut big = br#"{"kind":"__envelope__"}"#.to_vec();
    big.resize(3 * 1024 * 1024, b' ');
    big.push(b'\n');

    // Preview validates the manifest (the loader's 400); execute does not —
    // v4's malformed export reaches `executeImport`'s own catch (500). Either
    // way the 3 MB part was parsed: neither leg may answer `No file provided`.
    for (action, status_want, error_want) in [
        ("import-preview", 400, LOADER_SENTENCE),
        ("import-execute", 500, "Failed to execute import"),
    ] {
        let (status, body) = post_file(addr, action, big.clone()).await;
        assert_eq!(status, status_want, "{action}: {body}");
        assert_eq!(
            body.get("error").and_then(Value::as_str),
            Some(error_want),
            "{action}: the 3 MB file part must get past the multipart parse: {body}"
        );
    }
}
