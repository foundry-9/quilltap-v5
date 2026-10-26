//! P4.D325 INVENTORY OBJECT-IMAGES ROUTE differential:
//! `api::inventory_item_images` + the `/api/v1/inventory/{itemId}/images` REST
//! edge vs v4's REAL `app/api/v1/inventory/[itemId]/images/route.ts` at the
//! `322a5488e` pin — the wardrobe images route's twin without `link-image` —
//! over a FRESH copy of the committed `inventory-routes-{main,mount}.db` pair
//! per request (the routes family's pair: two pictures on Vivienne's sword,
//! one on the General lantern its vault instance inherits).
//!
//! THE CASE LIST LIVES IN `inventory-item-images-routes.json#cases` (the
//! shared-corpus rule; the row count is asserted against the NDJSON).
//!
//! Drivers: `http` for EVERY case — the real router over
//! `common::serve_instance` (`on_lock_lost` no-op): the query parse, the
//! action dispatch before the query, v4's 400 → 404 → body order, the
//! `generate` text body, the multipart `upload` leg through the host codec,
//! `save-to-store`'s forgiving body; and `direct` for the cases a dispatch
//! verb can express (`direct: true`) — the case as its dispatch wire handed to
//! the core fns the engine arms call. Comparands: status (201 → 200 over
//! dispatch, R-11) and the body as BYTES after the shared normalization; a
//! `followGet` adds `${name}__get` (the list on the same copy — persistence).
//!
//! No provider is reached: the pair has no image profile, so a `generate` past
//! its gates answers v4's one no-profile 400. The 201 / 422 / 502 mapping is
//! pinned by `quilltap-web`'s `inventory_images_routes` unit tests (`render`'s
//! 502 restore) and the keystone's generation family — recorded in the lane.
//!
//! Generate the oracle (see the .ts header), then:
//!   QT_ORACLE_IIIR=/tmp/oracle-inventory-item-images-routes.ndjson \
//!     cargo test -p quilltap-web --test inventory_item_images_routes_equivalence -- --nocapture

mod common;
#[path = "support/inventory_norm.rs"]
mod inventory_norm;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::StatusCode;
use base64::Engine as _;
use inventory_norm::{known_of, normalize};
use quilltap_core::api::types::{InventoryContainer, InventoryScope, Request};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    method: String,
    item: String,
    query: Vec<(String, String)>,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default)]
    direct: bool,
    #[serde(default)]
    follow_get: bool,
    /// Dispatch only: a booted host provisions built-in stores the oracle's
    /// unbooted pair never has, so `save-targets`' album list is compared
    /// over dispatch (the wardrobe twin's rule).
    #[serde(default)]
    direct_only: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    png: String,
    cases: Vec<Case>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// `{ref: {item, nth}}` → the item's `nth` picture newest-first; `{vault: id}`
/// → that character's vault mount id (the oracle's `resolveRefs`).
fn resolve_refs(main: &rusqlite::Connection, v: &Value) -> Value {
    match v {
        Value::Object(o) if o.get("ref").is_some_and(Value::is_object) => {
            let item = o["ref"]["item"].as_str().unwrap();
            let nth = o["ref"].get("nth").and_then(Value::as_u64).unwrap_or(0) as usize;
            let ids: Vec<String> = main
                .prepare(
                    r#"SELECT id FROM files WHERE category = 'IMAGE' AND "linkedTo" LIKE ?1 ORDER BY "createdAt" DESC, rowid"#,
                )
                .unwrap()
                .query_map([format!("%{item}%")], |r| r.get(0))
                .unwrap()
                .filter_map(Result::ok)
                .collect();
            json!(ids[nth])
        }
        Value::Object(o) if o.get("vault").is_some_and(Value::is_string) => {
            let m: String = main
                .query_row(
                    r#"SELECT "characterDocumentMountPointId" FROM characters WHERE id = ?1"#,
                    [o["vault"].as_str().unwrap()],
                    |r| r.get(0),
                )
                .unwrap();
            json!(m)
        }
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, e)| (k.clone(), resolve_refs(main, e)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(|e| resolve_refs(main, e)).collect()),
        other => other.clone(),
    }
}

fn materialize(dir: &Path, main_name: &str, mount_name: &str, pepper: &str) -> Option<Value> {
    std::fs::copy(
        fixtures_dir().join("inventory-routes-main.db"),
        dir.join(main_name),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("inventory-routes-mount.db"),
        dir.join(mount_name),
    )
    .unwrap();
    let w = Writer::open_writable(&dir.join(main_name), pepper).unwrap();
    quilltap_core::test_support::ensure_chat_wardrobe_history_on(w.connection());
    None
}

fn resolved_json_body(dir: &Path, main_name: &str, case: &Case, pepper: &str) -> Option<Value> {
    let body = case.body.as_ref()?;
    if body["type"] != "json" {
        return None;
    }
    let w = Writer::open_writable(&dir.join(main_name), pepper).unwrap();
    Some(resolve_refs(w.connection(), &body["value"]))
}

fn query<'a>(case: &'a Case, key: &str) -> Option<&'a str> {
    case.query
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn container_of(case: &Case) -> InventoryContainer {
    let scope = match query(case, "scope") {
        Some("character") => InventoryScope::Character,
        Some("project") => InventoryScope::Project,
        Some("group") => InventoryScope::Group,
        _ => InventoryScope::General,
    };
    InventoryContainer {
        scope,
        id: query(case, "id").map(str::to_string),
    }
}

/// The case as its dispatch wire.
fn dispatch_wire(case: &Case, body: Option<&Value>) -> Value {
    let mut w = Map::new();
    w.insert(
        "container".into(),
        serde_json::to_value(container_of(case)).unwrap(),
    );
    w.insert("itemId".into(), json!(case.item));
    let ty = match (case.method.as_str(), query(case, "action")) {
        ("GET", None) => "inventoryItemImagesList",
        ("GET", Some("save-targets")) => "inventoryItemImageSaveTargets",
        ("POST", Some("generate")) => {
            if let Some(Value::Object(o)) = body {
                if let Some(id) = o.get("imageProfileId") {
                    w.insert("imageProfileId".into(), id.clone());
                }
            }
            "inventoryItemImageGenerate"
        }
        ("POST", Some(action)) => {
            if let Some(Value::Object(o)) = body {
                for (k, v) in o {
                    w.insert(k.clone(), v.clone());
                }
            }
            match action {
                "set-current" => "inventoryItemImageSetCurrent",
                "delete-image" => "inventoryItemImageDelete",
                "save-to-store" => "inventoryItemImageSaveToStore",
                other => panic!("{}: no verb for {other}", case.name),
            }
        }
        (m, a) => panic!("{}: no verb for {m} {a:?}", case.name),
    };
    w.insert("type".into(), json!(ty));
    Value::Object(w)
}

async fn run_direct(db: &Db, req: Request) -> quilltap_core::api::types::Response {
    use quilltap_core::api::inventory_item_images as iii;
    match req {
        Request::InventoryItemImagesList { container, item_id } => {
            iii::list(db, &container, &item_id).await
        }
        Request::InventoryItemImageSaveTargets { container, item_id } => {
            iii::save_targets(db, &container, &item_id).await
        }
        Request::InventoryItemImageGenerate {
            container,
            item_id,
            image_profile_id,
        } => {
            // No provider is reached on this pair (no image profile).
            let seams = quilltap_core::api::images::ImagesGenerateSeams {
                provider: quilltap_core::model::image::ErasedImageGenerate::new(Unreached),
                classifier: quilltap_core::api::images::ErasedImagePromptClassifier::new(Unreached),
                codec: Arc::new(quilltap_host::HostImageCodec),
            };
            iii::generate(
                db,
                &seams,
                quilltap_core::api::SINGLE_USER_ID,
                &container,
                &item_id,
                image_profile_id.as_deref(),
            )
            .await
        }
        Request::InventoryItemImageSetCurrent {
            container,
            item_id,
            body,
        } => iii::set_current(db, &container, &item_id, &body).await,
        Request::InventoryItemImageDelete {
            container,
            item_id,
            body,
        } => iii::delete(db, &container, &item_id, &body).await,
        Request::InventoryItemImageSaveToStore {
            container,
            item_id,
            body,
        } => {
            // The engine arm's seams; every directly-driven save refuses
            // before it writes (the corpus's success-free rows).
            let files = tempfile::tempdir().unwrap();
            let bytes: Arc<dyn quilltap_core::photos::save_image_to_album::FileBytesStore> =
                Arc::new(quilltap_core::services::file_storage::ProductionFileBytes {
                    db: db.clone(),
                    backend: Arc::new(quilltap_host::LocalStorageBackend::new(
                        files.path().to_path_buf(),
                    )),
                    codec: Arc::new(quilltap_host::HostImageCodec),
                });
            let effects: Arc<
                dyn quilltap_core::photos::save_image_to_album::SaveImageSideEffects + Send + Sync,
            > = Arc::new(
                quilltap_core::photos::mount_embedding_effects::MountEmbeddingSideEffects::new(
                    db.clone(),
                ),
            );
            iii::save_to_store(
                db,
                bytes,
                effects,
                quilltap_core::api::SINGLE_USER_ID,
                &container,
                &item_id,
                &body,
                "2026-02-01T00:00:00.000Z",
            )
            .await
        }
        other => panic!("not driven directly: {other:?}"),
    }
}

type Row = (u16, Value);

/// The seams `generate` is handed and must never reach (the pair has no image
/// profile — every generate stops at the no-profile 400 or an earlier gate).
struct Unreached;

impl quilltap_core::model::image::ImageProvider for Unreached {
    async fn generate_image(
        &self,
        _provider: &str,
        _api_key: &str,
        _params: &quilltap_core::model::image::ImageGenParams,
    ) -> Result<
        quilltap_core::model::image::ImageGenResponse,
        quilltap_core::model::image::ImageGenError,
    > {
        panic!("the image provider must not be reached")
    }
}

impl quilltap_core::api::images::ImagePromptClassifier for Unreached {
    fn classify<'a>(
        &'a self,
        _db: &'a Db,
        _content: &'a str,
        _selection: &'a quilltap_core::cheap_llm::CheapLlmSelection,
        _user_id: &'a str,
        _policy: &'a quilltap_core::services::dangerous_content::resolver::ResolvedConciergePolicy,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = quilltap_core::services::dangerous_content::gatekeeper::DangerClassificationResult,
                > + Send
                + 'a,
        >,
    >{
        panic!("the classifier must not be reached")
    }
}

async fn direct_rows(case: &Case, pepper: &str) -> Vec<(String, Row)> {
    let scratch = tempfile::tempdir().unwrap();
    materialize(scratch.path(), "main.db", "mount.db", pepper);
    let body = resolved_json_body(scratch.path(), "main.db", case, pepper);
    let wire = dispatch_wire(case, body.as_ref());
    let req: Request = serde_json::from_value(wire.clone())
        .unwrap_or_else(|e| panic!("{}: the wire {wire} does not parse: {e}", case.name));
    let db = Db::open(
        DbPaths {
            main: scratch.path().join("main.db"),
            mount_index: Some(scratch.path().join("mount.db")),
            llm_logs: None,
        },
        pepper,
    )
    .unwrap();
    let (status, out) = quilltap_web::inventory_images_routes::render(
        false,
        run_direct(&db, req).await,
        StatusCode::OK,
    );
    let mut rows = vec![(case.name.clone(), (status.as_u16(), out))];
    if case.follow_get {
        let (s, b) = quilltap_web::inventory_images_routes::render(
            false,
            quilltap_core::api::inventory_item_images::list(&db, &container_of(case), &case.item)
                .await,
            StatusCode::OK,
        );
        rows.push((format!("{}__get", case.name), (s.as_u16(), b)));
    }
    rows
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|c| match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => {
                (c as char).to_string()
            }
            _ => format!("%{c:02X}"),
        })
        .collect()
}

fn qs(pairs: &[(String, String)]) -> String {
    let s = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    if s.is_empty() {
        s
    } else {
        format!("?{s}")
    }
}

async fn http_rows(case: &Case, spec: &Spec) -> Vec<(String, Row)> {
    let pepper = spec.test_pepper_base64.clone();
    let base = tempfile::tempdir().unwrap();
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    materialize(&data, "quilltap.db", "quilltap-mount-index.db", &pepper);
    let body = resolved_json_body(&data, "quilltap.db", case, &pepper);
    let p = pepper.clone();
    let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.env_pepper = Some(p);
        c.on_lock_lost = Some(Arc::new(|| {}));
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = format!(
        "http://{addr}/api/v1/inventory/{}/images{}",
        case.item,
        qs(&case.query)
    );
    let mut rb = if case.method == "GET" {
        client.get(&url)
    } else {
        client.post(&url)
    };
    match case.body.as_ref().map(|b| b["type"].as_str().unwrap()) {
        Some("json") => {
            rb = rb
                .header("content-type", "application/json")
                .body(body.unwrap().to_string());
        }
        Some("text") => {
            rb = rb.header("content-type", "application/json").body(
                case.body.as_ref().unwrap()["value"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            );
        }
        Some("multipart") => {
            let mut form = reqwest::multipart::Form::new();
            for part in case.body.as_ref().unwrap()["parts"].as_array().unwrap() {
                let name = part["name"].as_str().unwrap().to_string();
                if part.get("file") == Some(&json!(true)) {
                    let bytes = if part.get("png") == Some(&json!(true)) {
                        base64::engine::general_purpose::STANDARD
                            .decode(&spec.png)
                            .unwrap()
                    } else {
                        vec![0u8; part["size"].as_u64().unwrap_or(0) as usize]
                    };
                    let p = reqwest::multipart::Part::bytes(bytes)
                        .file_name(part["filename"].as_str().unwrap().to_string())
                        .mime_str(part["contentType"].as_str().unwrap())
                        .unwrap();
                    form = form.part(name, p);
                } else {
                    form = form.text(name, part["value"].as_str().unwrap().to_string());
                }
            }
            rb = rb.multipart(form);
        }
        _ => {}
    }
    let resp = rb.send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    let out: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{}: non-JSON body {text:?}: {e}", case.name));
    let mut rows = vec![(case.name.clone(), (status, out))];
    if case.follow_get {
        let get_pairs: Vec<(String, String)> = case
            .query
            .iter()
            .filter(|(k, _)| k != "action")
            .cloned()
            .collect();
        let g = client
            .get(format!(
                "http://{addr}/api/v1/inventory/{}/images{}",
                case.item,
                qs(&get_pairs)
            ))
            .send()
            .await
            .unwrap();
        let s = g.status().as_u16();
        let b: Value = serde_json::from_str(&g.text().await.unwrap()).unwrap();
        rows.push((format!("{}__get", case.name), (s, b)));
    }
    rows
}

#[tokio::test(flavor = "multi_thread")]
async fn inventory_item_images_routes_equivalence() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_IIIR") else {
        eprintln!("SKIP: set QT_ORACLE_IIIR (see test header).");
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/inventory-item-images-routes.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path).unwrap().lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    let expected: usize = spec
        .cases
        .iter()
        .map(|c| 1 + usize::from(c.follow_get))
        .sum();
    assert_eq!(
        oracle.len(),
        expected,
        "oracle row count — regenerate the NDJSON"
    );
    let known = known_of(&spec.test_pepper_base64);
    let mut failed: Vec<String> = Vec::new();
    let (mut direct_ran, mut http_ran) = (0usize, 0usize);
    for case in &spec.cases {
        for driver in ["http", "direct"] {
            let rows = if driver == "http" {
                if case.direct_only {
                    continue;
                }
                http_ran += 1;
                http_rows(case, &spec).await
            } else if case.direct {
                direct_ran += 1;
                direct_rows(case, &spec.test_pepper_base64).await
            } else {
                continue;
            };
            let (mut v4m, mut v5m) = (Vec::new(), Vec::new());
            for (name, (status, body)) in &rows {
                let want = &oracle[name];
                let ws = want["status"].as_u64().unwrap() as u16;
                let status_ok = if driver == "direct" && ws == 201 {
                    *status == 200
                } else {
                    ws == *status
                };
                let w = normalize(&want["body"].to_string(), &known, &mut v4m);
                let g = normalize(&body.to_string(), &known, &mut v5m);
                if !status_ok || w != g {
                    eprintln!("[{driver}:{name}] v4 {ws} {w}\n  v5 {status} {g}");
                    failed.push(format!("{driver}:{name}"));
                }
            }
        }
    }
    eprintln!(
        "inventory_item_images_routes_equivalence: {} cases, {http_ran} http + {direct_ran} direct, {} failed",
        spec.cases.len(),
        failed.len()
    );
    assert!(failed.is_empty(), "{failed:#?}");
    assert_eq!(
        http_ran,
        spec.cases.iter().filter(|c| !c.direct_only).count()
    );
    assert!(direct_ran > 20);
}

/// R-10: the `[Inventory Images v1]` lines core logs — DEBUG `Listed object
/// images` `{itemId, count, current}` and INFO `Uploaded object image`
/// `{itemId, fileId}` (`images/route.ts:69,151`) — capture-pinned on the
/// calling thread (the served host logs off it), with the silence leg: a
/// refused upload (an archived character's object) logs no upload line.
/// `Saved object picture to a store` has no success row on this pair and the
/// edge's DEBUG `Generate body is not valid JSON` logs inside the served
/// router — both recorded in the lane record as not capture-pinned here.
#[test]
fn the_route_lines_are_v4s() {
    use quilltap_core::api::inventory_item_images as iii;
    use quilltap_core::test_support::global_capture::capture_async;
    quilltap_core::test_support::global_capture::install();
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/inventory-item-images-routes.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let pepper = spec.test_pepper_base64.clone();
    let scratch = tempfile::tempdir().unwrap();
    materialize(scratch.path(), "main.db", "mount.db", &pepper);
    let db = Db::open(
        DbPaths {
            main: scratch.path().join("main.db"),
            mount_index: Some(scratch.path().join("mount.db")),
            llm_logs: None,
        },
        &pepper,
    )
    .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    const V: &str = "c1000000-0000-4000-8000-000000000001";
    const Z: &str = "c3000000-0000-4000-8000-000000000003";
    const SWORD: &str = "a0000010-0000-4000-8000-000000000000";
    const ZLANTERN: &str = "a0000020-0000-4000-8000-000000000000";
    let vc = InventoryContainer {
        scope: InventoryScope::Character,
        id: Some(V.into()),
    };
    let (r, lines) = rt.block_on(capture_async(iii::list(&db, &vc, SWORD)));
    let quilltap_core::api::types::Response::Inventory(body) = &r else {
        panic!("{r:?}")
    };
    let (n, current) = (
        body["images"].as_array().unwrap().len(),
        body["current"].as_str().unwrap().to_string(),
    );
    let want = format!(
        "DEBUG quilltap_core::api::inventory_item_images [Inventory Images v1] Listed object images itemId={SWORD} count={n} current={current} context=inventory"
    );
    assert!(lines.contains(&want), "{want}\n{lines:#?}");

    let png = base64::engine::general_purpose::STANDARD
        .decode(&spec.png)
        .unwrap();
    let blob_webp: Arc<dyn quilltap_core::services::mount_index::blob_transcode::WebpTranscoder> =
        Arc::new(
            quilltap_core::services::mount_index::normalize_blob_image::PixelCodecWebp(Arc::new(
                quilltap_host::HostImageCodec,
            )),
        );
    let upload = |container: InventoryContainer, item: &'static str| {
        let (db, blob_webp, png) = (db.clone(), blob_webp.clone(), png.clone());
        async move {
            let home = iii::resolve_home(&db, &container, item).await.unwrap();
            iii::upload_on_home(
                &db,
                blob_webp,
                &home,
                quilltap_core::services::wardrobe_image_bridge::WardrobeImageKind::Uploaded,
                png,
                "image/png".into(),
                None,
                None,
            )
            .await
        }
    };
    let (r, lines) = rt.block_on(capture_async(upload(vc.clone(), SWORD)));
    let quilltap_core::api::types::Response::Inventory(body) = &r else {
        panic!("{r:?}")
    };
    let file_id = body["current"].as_str().unwrap().to_string();
    let want = format!(
        "INFO quilltap_core::api::inventory_item_images [Inventory Images v1] Uploaded object image itemId={SWORD} fileId={file_id} context=inventory"
    );
    assert!(lines.contains(&want), "{want}\n{lines:#?}");
    let zc = InventoryContainer {
        scope: InventoryScope::Character,
        id: Some(Z.into()),
    };
    let (r, lines) = rt.block_on(capture_async(upload(zc, ZLANTERN)));
    assert!(
        matches!(r, quilltap_core::api::types::Response::Error(_)),
        "{r:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("Uploaded object image")),
        "{lines:#?}"
    );
}
