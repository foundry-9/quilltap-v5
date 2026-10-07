//! P4.162 — dogfood #151: v4's three context-middleware ERRORs for a
//! store-unavailable 503, at v5's transport seams.
//!
//! v4's `handleRouteError` (`lib/api/middleware/context.ts:159-207` at
//! `94fbb1ae3`) — reached through the two `createContext*Handler` wrappers that
//! 117 of 137 route files use — logs, before answering the contextful 503:
//!
//!   ERROR `[${method} ${pathname}] Project document store unavailable`
//!         `{projectId, officialMountPointId}`
//!   ERROR `[${method} ${pathname}] Group document store unavailable`
//!         `{groupId, officialMountPointId}`
//!   ERROR `[${method} ${pathname}] Character vault unavailable`
//!         `{characterId, characterDocumentMountPointId}`
//!
//! on the `api-context-middleware` child logger. Before P4.162 v5 answered the
//! same 503 bodies and logged NOTHING. This binary drives the live server over
//! a doctored copy of the committed `groups-projects-{main,mount}.db` pair
//! (each picked entity's `properties.json` keystone DELETED — the plant that
//! yields the overlay's `Unavailable`, not `unparseable`) and pins:
//!
//!   1. the dispatch seam over HTTP — the prefix is the request's REAL method
//!      and pathname (`[POST /api/dispatch]`; ruling R-A: v4's
//!      `/api/v1/projects/<id>` is a path v5 never serves for a dispatch verb,
//!      so the prefix is compared by SHAPE against v4 and the sentence and
//!      fields by BYTES), for project, group AND character;
//!   2. the dispatch seam over Tauri IPC (`dispatch_body` driven directly, no
//!      HTTP request in scope) — the prefix is `[<verb>]`;
//!   3. a REST seam (`POST /api/v1/characters/{id}?action=rename` →
//!      `core_error_status_body`) — the prefix is that route's method + path,
//!      with NO query string (v4's `new URL(request.url).pathname`);
//!   4. the silence leg — a sound store answers 200 with no such line.
//!
//! The project arm drives `projectUpdate` (v4's PUT): v4's project GET
//! (`project-crud.ts:72-89`) wraps its read in its own try/catch and answers
//! 500 `Failed to fetch project` — the middleware never sees the error, and v5
//! answers the same 500. The group GET and the character GET do not catch.
//!
//! The mount id rides the error (`DbError::StoreUnavailable.mount_point_id`,
//! ruling R-B); the wire BODY stays v4's `{error, <entity>Id}` (v4 never puts
//! the mount id in the body — `context.ts:182`), which is asserted too.
//!
//! The v4 bytes these pins copy are the survey's (`context.ts:177-200`);
//! recording them through v4's REAL route with a `Logger.prototype` spy is a
//! `HANDOFF(P4.163)` (P4.163 owns `projects-routes.test.ts`).
//!
//! Run:
//!   cargo test -p quilltap-web --test store_unavailable_errors

mod common;

use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository;
use quilltap_core::db::Writer;
use quilltap_core::test_support::FieldVisitor;
use serde_json::{json, Value};

/// v4's `contextLogger` (`logger.child({ module: 'api-context-middleware' })`).
const TARGET: &str = "quilltap::api_context_middleware";

const IOTA: &str = "a3000000-0000-4000-8000-000000000001";
const KAPPA: &str = "a3000000-0000-4000-8000-000000000003";
const GAMMA: &str = "a2000000-0000-4000-8000-000000000001";
const DELTA: &str = "a2000000-0000-4000-8000-000000000002";
const ARIA: &str = "a1000000-0000-4000-8000-000000000001";

/// Every event the process emits, on any thread (the server's handlers run on
/// the runtime's workers, which a thread-scoped capture cannot see). ONE test
/// in this binary, so nothing else writes here.
fn sink() -> &'static Arc<Mutex<Vec<String>>> {
    static SINK: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    SINK.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let lines = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(Layer(lines.clone())),
        )
        .expect("this binary must own the global subscriber");
        lines
    })
}

struct Layer(Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Layer {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let meta = event.metadata();
        let mut v = FieldVisitor(format!("{} {}", meta.level(), meta.target()));
        event.record(&mut v);
        self.0.lock().unwrap().push(v.0);
    }
}

/// Drain the sink, keeping only the middleware target's lines.
fn take_lines() -> Vec<String> {
    let mut all = sink().lock().unwrap();
    let out = all
        .iter()
        .filter(|l| l.split(' ').nth(1) == Some(TARGET))
        .cloned()
        .collect();
    all.clear();
    out
}

fn materialize() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        common::fixtures_dir().join("groups-projects-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        common::fixtures_dir().join("groups-projects-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
        common::rewrite_fixture_user_ids(w.connection());
    }
    base
}

/// The store mount an entity's row points at.
fn mount_of(main: &std::path::Path, table: &str, column: &str, id: &str) -> String {
    let w = Writer::open_writable(main, common::TEST_PEPPER).unwrap();
    w.connection()
        .query_row(
            &format!("SELECT {column} FROM {table} WHERE id = ?1"),
            [id],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_else(|e| panic!("{table} {id}: no {column}: {e}"))
}

/// The ERROR line v4's middleware logs, as the capture renders it.
fn want_line(
    route: &str,
    sentence: &str,
    id_key: &str,
    id: &str,
    mount_key: &str,
    mount: &str,
) -> String {
    format!("ERROR {TARGET} [{route}] {sentence} {id_key}={id} {mount_key}={mount}")
}

#[tokio::test(flavor = "multi_thread")]
async fn store_unavailable_errors_at_every_seam() {
    sink();
    let base = materialize();
    let data = base.path().join("data");
    let main = data.join("quilltap.db");
    let iota_mp = mount_of(&main, "projects", "officialMountPointId", IOTA);
    let gamma_mp = mount_of(&main, "groups", "officialMountPointId", GAMMA);
    let aria_mp = mount_of(&main, "characters", "characterDocumentMountPointId", ARIA);
    {
        let w = Writer::open_writable(&data.join("quilltap-mount-index.db"), common::TEST_PEPPER)
            .unwrap();
        let links = DocMountFileLinksRepository::new(w.connection());
        for mp in [&iota_mp, &gamma_mp, &aria_mp] {
            links
                .delete_database_document(mp, "properties.json")
                .expect("delete keystone");
        }
    }

    let (addr, state) = common::serve_instance(base.path(), |c| c).await;
    let client = reqwest::Client::new();
    let dispatch = format!("http://{addr}/api/dispatch");
    take_lines();

    // ── 1. the dispatch seam over HTTP, per entity ──
    let arms = [
        (
            json!({ "type": "projectUpdate", "projectId": IOTA, "project": { "name": "Iota" } }),
            "Project document store unavailable",
            "projectId",
            IOTA,
            "officialMountPointId",
            iota_mp.as_str(),
        ),
        (
            json!({ "type": "groupGet", "groupId": GAMMA }),
            "Group document store unavailable",
            "groupId",
            GAMMA,
            "officialMountPointId",
            gamma_mp.as_str(),
        ),
        (
            json!({ "type": "characterGet", "characterId": ARIA }),
            "Character vault unavailable",
            "characterId",
            ARIA,
            "characterDocumentMountPointId",
            aria_mp.as_str(),
        ),
    ];
    for (req, sentence, id_key, id, mount_key, mount) in &arms {
        let r = client.post(&dispatch).json(req).send().await.unwrap();
        let st = r.status().as_u16();
        let body: Value = r.json().await.unwrap();
        assert_eq!(st, 503, "{sentence}: status {body}");
        // The wire body keeps v4's two keys and never carries the mount id.
        assert_eq!(body["error"], json!(sentence), "{sentence}: body error");
        assert_eq!(body[*id_key], json!(id), "{sentence}: body id");
        assert!(
            !body.to_string().contains(mount),
            "{sentence}: the mount id must not reach the wire: {body}"
        );
        let lines = take_lines();
        assert_eq!(
            lines,
            vec![want_line(
                "POST /api/dispatch",
                sentence,
                id_key,
                id,
                mount_key,
                mount
            )],
            "{sentence}: ONE middleware ERROR, v4's sentence and keys in order"
        );
    }

    // ── 2. the dispatch seam over Tauri IPC: no HTTP request in scope ──
    let bytes = serde_json::to_vec(&json!({ "type": "groupGet", "groupId": GAMMA })).unwrap();
    let (status, body) = quilltap_web::dispatch::dispatch_body(&state, &bytes).await;
    assert_eq!(status.as_u16(), 503);
    assert_eq!(body["groupId"], json!(GAMMA));
    assert_eq!(
        take_lines(),
        vec![want_line(
            "groupGet",
            "Group document store unavailable",
            "groupId",
            GAMMA,
            "officialMountPointId",
            &gamma_mp
        )],
        "IPC: the prefix is the verb"
    );

    // ── 3. a REST seam: the route's own method + path, no query string ──
    let r = client
        .post(format!(
            "http://{addr}/api/v1/characters/{ARIA}?action=rename"
        ))
        .json(&json!({ "oldName": "Aria", "newName": "Arietta" }))
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    let body: Value = r.json().await.unwrap();
    assert_eq!(status, 503, "REST rename on a broken vault: {body}");
    assert_eq!(body["characterId"], json!(ARIA));
    assert_eq!(
        take_lines(),
        vec![want_line(
            &format!("POST /api/v1/characters/{ARIA}"),
            "Character vault unavailable",
            "characterId",
            ARIA,
            "characterDocumentMountPointId",
            &aria_mp
        )],
        "REST: v4's pathname, the query dropped"
    );

    // ── 4. the silence leg: a sound store answers 200 and logs no such line ──
    let r = client
        .post(&dispatch)
        .json(&json!({ "type": "projectGet", "projectId": KAPPA }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 200);
    let r = client
        .post(&dispatch)
        .json(&json!({ "type": "groupGet", "groupId": DELTA }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 200);
    assert_eq!(
        take_lines(),
        Vec::<String>::new(),
        "a sound store logs nothing"
    );
}
