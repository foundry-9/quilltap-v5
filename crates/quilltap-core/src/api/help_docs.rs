//! The help-docs READ dispatch family (P4.9I2A) — v4 `app/api/v1/help-docs/
//! route.ts` (list / `?action=chat-count` / `?action=search&q=`) and
//! `app/api/v1/help-docs/[id]/route.ts` (one document by id OR slug).
//!
//! Response bodies mirror v4's route JSON name-for-name (the tier-2
//! differential `help_docs_routes_equivalence` is the arbiter): the LIST row is
//! `{id, slug, title, path, url}`, the single document `{id, title, path, url,
//! content}` (NO slug), a search row `{slug, titleHit, snippet}`, the count
//! `{count}`. Every handler's catch answers v4's fixed `serverError` sentence.
//!
//! ## The divergence recorded in `services::help_chat`
//!
//! v4 reads through `HelpSearch`'s in-process cache (`loadFromDatabase()` →
//! `ensureHelpDocsSynced()` → `findAll()`, once). v5 reads `help_docs` per call:
//! the boot ensure has already synced the table, so the rows are identical and
//! only the read timing differs. There is no `invalidate()` twin — and so v4's
//! `logger.info('Help documents loaded from database', { documentCount })` has
//! no event to log (deliberately NOT ported), nor do the search-path lines
//! `No embedded help docs available for search` /
//! `Section-level help scoring failed; falling back to whole-document scores`,
//! which belong to `db::help_search` (the ported semantic path), not here.

use serde_json::{json, Value};

use crate::db::help_docs::HelpDocsRepository;
use crate::db::runtime::Db;
use crate::db::{chats_read, DbError};
use crate::services::help_chat::guide_search::search_documents;
use crate::services::help_chat::{get_document, HelpDocument};

use super::types::{ErrorKind, Response};

/// v4 `HelpSearch.loadFromDatabase()`'s read + map (sans the sync — see the
/// module doc): every `help_docs` row, rowid order, with its path-derived slug.
pub fn load_documents(db: &Db) -> Result<Vec<HelpDocument>, DbError> {
    let rows = db.read_main(|c| HelpDocsRepository::new(c).find_all())?;
    Ok(rows.iter().map(HelpDocument::from_row).collect())
}

fn server_error(msg: &str) -> Response {
    Response::error(ErrorKind::Internal, msg)
}

/// v4 `handleList` (`route.ts:20-36`): `{ documents: [{id, slug, title, path,
/// url}] }`; catch → `serverError('Failed to load help documents')`.
pub fn help_docs_list(db: &Db) -> Response {
    match load_documents(db) {
        Ok(docs) => {
            tracing::info!(
                target: "quilltap::help",
                document_count = docs.len(),
                "[HelpDocs] Listed help documents"
            );
            let documents: Vec<Value> = docs.iter().map(HelpDocument::listing).collect();
            Response::HelpDocs(json!({ "documents": documents }))
        }
        Err(e) => {
            tracing::error!(target: "quilltap::help", error = %e, "[HelpDocs] Error listing help documents");
            server_error("Failed to load help documents")
        }
    }
}

/// v4 `handleChatCount` (`route.ts:128-144`): the user's SALON chats —
/// `!chat.chatType || chat.chatType === 'salon'` (JS truthiness: a missing,
/// null or empty type counts as a salon chat). `{ count }`; catch →
/// `serverError('Failed to get chat count')`.
pub fn help_docs_chat_count(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    match db.read_main(move |c| chats_read::find_by_user_id(c, &uid)) {
        Ok(chats) => {
            let count = chats
                .iter()
                .filter(|c| match c.get("chatType").and_then(Value::as_str) {
                    None | Some("") => true,
                    Some(t) => t == "salon",
                })
                .count();
            Response::HelpDocs(json!({ "count": count }))
        }
        Err(e) => {
            tracing::error!(target: "quilltap::help", error = %e, "[HelpDocs] Error getting chat count");
            server_error("Failed to get chat count")
        }
    }
}

/// v4 `handleSearch` (`route.ts:81-123`): `q` is `searchParams.get('q') ?? ''`
/// (absent → the empty string), trimmed; under two UTF-16 units answers
/// `{ matches: [] }` WITHOUT touching the table. `{ matches }`; catch →
/// `serverError('Failed to search help documents')`.
pub fn help_docs_search(db: &Db, q: Option<&str>) -> Response {
    let query = crate::jsstr::js_trim(q.unwrap_or(""));
    if crate::jsstr::utf16_len(query) < 2 {
        return Response::HelpDocs(json!({ "matches": [] }));
    }
    match load_documents(db) {
        Ok(docs) => {
            let matches = search_documents(&docs, query);
            tracing::info!(
                target: "quilltap::help",
                query = %query,
                match_count = matches.len(),
                "[HelpDocs] Guide text search"
            );
            let matches: Vec<Value> = matches.iter().map(|m| m.to_value()).collect();
            Response::HelpDocs(json!({ "matches": matches }))
        }
        Err(e) => {
            tracing::error!(target: "quilltap::help", error = %e, "[HelpDocs] Error searching help documents");
            server_error("Failed to search help documents")
        }
    }
}

/// v4 `GET /api/v1/help-docs/[id]` (`[id]/route.ts:26-59`): `getDocument(idOrSlug)`
/// — a DB id OR a slug; miss → `notFound('Help document')` (+ the warn line);
/// hit → `{ document: {id, title, path, url, content} }` (NO slug); catch →
/// `serverError('Failed to get help document')`.
pub fn help_doc_get(db: &Db, id_or_slug: &str) -> Response {
    match load_documents(db) {
        Ok(docs) => match get_document(&docs, id_or_slug) {
            None => {
                tracing::warn!(
                    target: "quilltap::help",
                    document_id = %id_or_slug,
                    "[HelpDoc] Document not found"
                );
                Response::error(ErrorKind::NotFound, "Help document not found")
            }
            Some(doc) => {
                tracing::info!(
                    target: "quilltap::help",
                    document_id = %id_or_slug,
                    title = %doc.title,
                    "[HelpDoc] Document retrieved"
                );
                Response::HelpDocs(json!({
                    "document": {
                        "id": doc.id,
                        "title": doc.title,
                        "path": doc.path,
                        "url": doc.url,
                        "content": doc.content,
                    }
                }))
            }
        },
        Err(e) => {
            tracing::error!(target: "quilltap::help", error = %e, "[HelpDoc] Error getting document");
            server_error("Failed to get help document")
        }
    }
}

/// Capture-layer pins for the four v4 route log lines this module carries
/// (`[HelpDocs] Listed help documents` / `[HelpDocs] Guide text search` /
/// `[HelpDoc] Document not found` / `[HelpDoc] Document retrieved`), over a fresh
/// copy of the committed `help-chat-*` fixture. The differential
/// (`help_docs_routes_equivalence`) cannot see a log line; these can.
#[cfg(test)]
mod log_context_tests {
    use super::*;
    use crate::db::runtime::DbPaths;
    use std::sync::{Arc, Mutex};

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    struct FieldVisitor(String);
    impl tracing::field::Visit for FieldVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0.push_str(&format!(" {}={:?}", field.name(), value));
        }
    }
    struct CaptureLayer(Arc<Mutex<Vec<String>>>);
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _c: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let meta = event.metadata();
            let mut v = FieldVisitor(format!("{} {}", meta.level(), meta.target()));
            event.record(&mut v);
            self.0.lock().unwrap().push(v.0);
        }
    }
    fn captured<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let sub = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        let out = {
            let _g = tracing::subscriber::set_default(sub);
            f()
        };
        let lines = logs.lock().unwrap().clone();
        (out, lines)
    }
    pub(crate) fn fixture_db() -> (tempfile::TempDir, Db) {
        let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../quilltap-web/tests/fixtures");
        let dir = tempfile::tempdir().unwrap();
        std::fs::copy(src.join("help-chat-main.db"), dir.path().join("main.db")).unwrap();
        std::fs::copy(src.join("help-chat-mount.db"), dir.path().join("mount.db")).unwrap();
        let db = Db::open(
            DbPaths {
                main: dir.path().join("main.db"),
                mount_index: Some(dir.path().join("mount.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    #[test]
    fn the_four_route_lines_fire() {
        let (_dir, db) = fixture_db();
        let (_, lines) = captured(|| help_docs_list(&db));
        assert!(
            lines.iter().any(|l| l.contains("INFO")
                && l.contains("[HelpDocs] Listed help documents")
                && l.contains("document_count=17")),
            "{lines:?}"
        );
        let (_, lines) = captured(|| help_docs_search(&db, Some("Brahma")));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("[HelpDocs] Guide text search") && l.contains("match_count=")),
            "{lines:?}"
        );
        let (_, lines) = captured(|| help_docs_search(&db, Some("a")));
        assert!(
            lines.is_empty(),
            "the short-circuit logs nothing: {lines:?}"
        );
        let (_, lines) = captured(|| help_doc_get(&db, "no-such-doc"));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("WARN") && l.contains("[HelpDoc] Document not found")),
            "{lines:?}"
        );
        let (_, lines) = captured(|| help_doc_get(&db, "brahma-console"));
        assert!(
            lines.iter().any(|l| l.contains("INFO")
                && l.contains("[HelpDoc] Document retrieved")
                && l.contains("The Brahma Console")),
            "{lines:?}"
        );
    }
}
