//! The tool inventory (P4.9E3B) — v4 `GET /api/v1/tools`
//! (`app/api/v1/tools/route.ts`, 727 LOC): the 41-entry `BUILT_IN_TOOLS`
//! display table, the `BUILT_IN_TOOL_SCHEMAS` map (consulted only when
//! `includeSchemas=true`, resolved against the byte-pinned
//! [`crate::tools::definitions`] catalog), and the per-chat availability
//! switch. Pinned by `tools_inventory_equivalence`.
//!
//! ## v4 quirks carried faithfully — do not "fix"
//!
//! - **`doc_copy_file` is missing from the availability switch** (route.ts's
//!   `doc_*` case list skips it), so it stays `available: true` even without a
//!   project.
//! - **The four photo tools have no schema-map entry** — `includeSchemas`
//!   attaches no `parameters` to them — and no availability arm, despite their
//!   comment claiming the doc-edit gate. Measured again at `a14a1811`:
//!   `describe_image` (bug 92) joined ONLY as a `BUILT_IN_TOOLS` row, so the
//!   quirk carries to it unchanged.
//! - A failed chat/character load is caught and warns: the context stays
//!   null/two-true, it never errors the request.
//!
//! ## The plugin arm — the standing no-runtime deferral, named
//!
//! v5 has no plugin runtime (`toolRegistry.getAllPlugins()` has no analogue),
//! so the plugin iteration, the `pluginConfigs` read, the metadata
//! enhancement, and the `getToolHierarchy` pass are all vacuous here: v5
//! returns built-ins only. On a default install v4's registry is empty and the
//! output is identical (the oracle proves it); an install with live plugins
//! would diverge — that is the standing plugin-runtime deferral, not this
//! unit's.

use serde_json::{json, Map, Value};

use crate::api::types::{ErrorKind, Response};
use crate::db::project_doc_mount_links::ProjectDocMountLinksRepository;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, connection_profiles};
use crate::tools::definitions::definition_by_key;

/// One `BUILT_IN_TOOLS` row (route.ts:113–401) — display data, byte-exact.
struct BuiltInTool {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    category: &'static str,
}

/// v4 `BUILT_IN_TOOLS`, in declaration order.
const BUILT_IN_TOOLS: &[BuiltInTool] = &[
    BuiltInTool { id: "ask_carina", name: "Ask Carina", description: "Ask a designated answerer character a quick standalone question without joining the conversation", category: "utility" },
    BuiltInTool { id: "send_mail", name: "Send Mail", description: "Post a Markdown letter to another character, delivered by Suparṇā into their vault mailbox", category: "utility" },
    BuiltInTool { id: "list_email", name: "List Email", description: "List the letters in your own mailbox, with the exact way to read, answer, or discard each", category: "utility" },
    BuiltInTool { id: "generate_image", name: "Generate Image", description: "Generate images using AI image generation providers", category: "media" },
    BuiltInTool { id: "search", name: "Search", description: "Search through the Scriptorium (character memories, past conversations, and story backgrounds)", category: "search" },
    BuiltInTool { id: "search_web", name: "Search Web", description: "Search the web for current information", category: "search" },
    BuiltInTool { id: "project_info", name: "Project Info", description: "Access project information and files", category: "project" },
    BuiltInTool { id: "help_search", name: "Help Search", description: "Search Quilltap help documentation for features, settings, and usage guidance", category: "help" },
    BuiltInTool { id: "help_settings", name: "Help Settings", description: "Read instance settings to understand current configuration (API keys are never shown)", category: "help" },
    BuiltInTool { id: "help_navigate", name: "Help Navigate", description: "Navigate the user's browser to a specific Quilltap page or settings section", category: "help" },
    BuiltInTool { id: "rng", name: "Random Number Generator", description: "Roll dice, flip coins, or randomly select a chat participant (spin the bottle)", category: "utility" },
    BuiltInTool { id: "run_custom", name: "Custom Tools", description: "Run one of this scene's user-authored custom tools (Tools/*.tool.json) — Pascal rolls it server-side and announces the outcome. Only offered when the chat can actually see at least one valid definition.", category: "utility" },
    BuiltInTool { id: "state", name: "State Manager", description: "Get, set, or delete persistent key-value state for the chat", category: "utility" },
    BuiltInTool { id: "self_inventory", name: "Self-Inventory", description: "Return an introspection report for the calling character: vault files, memory and chat stats, assembled system prompt, and last-turn LLM token usage", category: "utility" },
    BuiltInTool { id: "whisper", name: "Whisper", description: "Send a private message to a specific character in a multi-character chat", category: "utility" },
    BuiltInTool { id: "wardrobe_list", name: "List Wardrobe", description: "Browse wardrobe items (own plus shared project / Quilltap General items) for the current character", category: "wardrobe" },
    BuiltInTool { id: "wardrobe_read", name: "Read Wardrobe Item", description: "Read the full detail of one wardrobe item, including its Portrait Cue", category: "wardrobe" },
    BuiltInTool { id: "wardrobe_create", name: "Create Wardrobe Item", description: "Create a new wardrobe item, optionally equip it, or gift it to another character", category: "wardrobe" },
    BuiltInTool { id: "wardrobe_update", name: "Update Wardrobe Item", description: "Edit the stored fields of an existing wardrobe item (own items only)", category: "wardrobe" },
    BuiltInTool { id: "wardrobe_archive", name: "Archive Wardrobe Item", description: "Retire a wardrobe item (own items only; restorable by a human)", category: "wardrobe" },
    BuiltInTool { id: "wardrobe_wear", name: "Wear Wardrobe Items", description: "Put on one or more wardrobe items (single garments or composite outfits)", category: "wardrobe" },
    BuiltInTool { id: "wardrobe_take_off", name: "Take Off Wardrobe Items", description: "Take off worn wardrobe items or empty slots", category: "wardrobe" },
    BuiltInTool { id: "doc_read_file", name: "Read Document", description: "Read file contents from document stores or project files", category: "documents" },
    BuiltInTool { id: "doc_write_file", name: "Write Document", description: "Write or create a file in document stores or project files", category: "documents" },
    BuiltInTool { id: "doc_str_replace", name: "Find & Replace in Document", description: "Find and replace exact text in a file (unique match required)", category: "documents" },
    BuiltInTool { id: "doc_insert_text", name: "Insert Text in Document", description: "Insert text at a specific position in a file", category: "documents" },
    BuiltInTool { id: "doc_grep", name: "Search Documents", description: "Search for text across files in document stores and project files", category: "documents" },
    BuiltInTool { id: "doc_list_files", name: "List Documents", description: "List files available in document stores and project files", category: "documents" },
    BuiltInTool { id: "doc_read_frontmatter", name: "Read Frontmatter", description: "Read YAML frontmatter from a markdown file", category: "documents" },
    BuiltInTool { id: "doc_update_frontmatter", name: "Update Frontmatter", description: "Update YAML frontmatter properties in a markdown file", category: "documents" },
    BuiltInTool { id: "doc_read_heading", name: "Read Heading Section", description: "Read all content under a specific heading in a markdown file", category: "documents" },
    BuiltInTool { id: "doc_update_heading", name: "Update Heading Section", description: "Replace content under a specific heading in a markdown file", category: "documents" },
    BuiltInTool { id: "doc_move_file", name: "Move/Rename Document", description: "Move or rename a file in document stores or project files", category: "documents" },
    BuiltInTool { id: "doc_copy_file", name: "Copy Document", description: "Copy a file from one document store to a different document store", category: "documents" },
    BuiltInTool { id: "doc_delete_file", name: "Delete Document", description: "Permanently delete a file from document stores or project files", category: "documents" },
    BuiltInTool { id: "doc_create_folder", name: "Create Folder", description: "Create a new folder in document stores or project files", category: "documents" },
    BuiltInTool { id: "doc_delete_folder", name: "Delete Folder", description: "Delete an empty folder from document stores or project files", category: "documents" },
    BuiltInTool { id: "keep_image", name: "Keep Image", description: "Save a generated image to the character's photo album with optional caption and tags", category: "photos" },
    BuiltInTool { id: "list_images", name: "List Kept Images", description: "Search or list images previously saved to the photo album", category: "photos" },
    BuiltInTool { id: "attach_image", name: "Attach Kept Image", description: "Re-attach a previously kept image to the current chat message", category: "photos" },
    BuiltInTool { id: "describe_image", name: "Describe Image", description: "Look at an image and report what it depicts", category: "photos" },
];

/// v4 `BUILT_IN_TOOL_SCHEMAS` (route.ts:70–108): the 37 ids carrying an
/// OpenAI-format definition, mapped to their [`crate::tools::definitions`]
/// catalog keys (the byte-pinned parameters source). The four photo tools are
/// deliberately absent (module header).
const SCHEMA_KEYS: &[(&str, &str)] = &[
    ("ask_carina", "askCarina"),
    ("send_mail", "sendMail"),
    ("list_email", "listEmail"),
    ("generate_image", "imageGeneration"),
    ("search", "searchScriptorium"),
    ("search_web", "webSearch"),
    ("project_info", "projectInfo"),
    ("help_search", "helpSearch"),
    ("help_settings", "helpSettings"),
    ("help_navigate", "helpNavigate"),
    ("rng", "rng"),
    ("run_custom", "runCustom"),
    ("state", "state"),
    ("self_inventory", "selfInventory"),
    ("whisper", "whisper"),
    ("wardrobe_list", "wardrobeList"),
    ("wardrobe_read", "wardrobeRead"),
    ("wardrobe_create", "wardrobeCreate"),
    ("wardrobe_update", "wardrobeUpdate"),
    ("wardrobe_archive", "wardrobeArchive"),
    ("wardrobe_wear", "wardrobeWear"),
    ("wardrobe_take_off", "wardrobeTakeOff"),
    ("doc_read_file", "docReadFile"),
    ("doc_write_file", "docWriteFile"),
    ("doc_str_replace", "docStrReplace"),
    ("doc_insert_text", "docInsertText"),
    ("doc_grep", "docGrep"),
    ("doc_list_files", "docListFiles"),
    ("doc_read_frontmatter", "docReadFrontmatter"),
    ("doc_update_frontmatter", "docUpdateFrontmatter"),
    ("doc_read_heading", "docReadHeading"),
    ("doc_update_heading", "docUpdateHeading"),
    ("doc_move_file", "docMoveFile"),
    ("doc_copy_file", "docCopyFile"),
    ("doc_delete_file", "docDeleteFile"),
    ("doc_create_folder", "docCreateFolder"),
    ("doc_delete_folder", "docDeleteFolder"),
];

/// The catalog parameters for one schema-map id (`None` for the photo tools).
fn schema_parameters(id: &str) -> Option<Value> {
    let key = SCHEMA_KEYS.iter().find(|(i, _)| *i == id)?.1;
    // Fall back to name lookup if a catalog key ever drifts (the unit test
    // pins that every entry resolves).
    definition_by_key(key)
        .or_else(|| {
            crate::tools::definitions::definition_json_by_name(id)
                .map(|j| serde_json::from_str(j).expect("catalog JSON is valid"))
        })
        .and_then(|def| def.get("parameters").cloned())
}

/// The per-chat availability context (v4 `chatContext`).
struct ChatContext {
    has_image_profile: bool,
    has_project: bool,
    has_document_stores: bool,
    allows_web_search: bool,
    is_multi_character: bool,
    can_dress_themselves: bool,
    can_create_outfits: bool,
}

fn truthy_str(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| !s.is_empty())
}

/// Build the chat context (v4 route.ts:452–517). Any failure is caught → the
/// context stays `None` (chat arm) or the two wardrobe flags stay true.
fn build_chat_context(db: &Db, user_id: &str, chat_id: &str) -> Option<ChatContext> {
    let cid = chat_id.to_string();
    let chat = db
        .read_main(move |c| chats_read::find_by_id(c, &cid))
        .ok()??;
    if chat.get("userId").and_then(Value::as_str) != Some(user_id) {
        return None;
    }
    let participants: Vec<&Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let character_participant = participants.iter().copied().find(|p| {
        p.get("type").and_then(Value::as_str) == Some("CHARACTER")
            && p.get("isActive").and_then(Value::as_bool) == Some(true)
    });

    let has_image_profile = truthy_str(chat.get("imageProfileId"))
        || character_participant.is_some_and(|p| truthy_str(p.get("imageProfileId")));
    let has_project = truthy_str(chat.get("projectId"));

    let mut allows_web_search = false;
    if let Some(conn_id) = character_participant
        .and_then(|p| p.get("connectionProfileId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let conn_id = conn_id.to_string();
        if let Ok(Some(profile)) =
            db.read_main(move |c| connection_profiles::find_by_id(c, &conn_id))
        {
            allows_web_search =
                profile.get("allowWebSearch").and_then(Value::as_bool) == Some(true);
        }
    }

    let active_character_count = participants
        .iter()
        .filter(|p| {
            p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                && p.get("isActive").and_then(Value::as_bool) == Some(true)
        })
        .count();
    let is_multi_character = active_character_count > 1;

    // Wardrobe capability flags (default: enabled when null; a failed load
    // warns and leaves them true).
    let mut can_dress_themselves = true;
    let mut can_create_outfits = true;
    if let Some(character_id) = character_participant
        .and_then(|p| p.get("characterId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let chid = character_id.to_string();
        let character = db.read_main(|main| {
            db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &chid))
        });
        if let Ok(Some(character)) = character {
            can_dress_themselves =
                character.get("canDressThemselves").and_then(Value::as_bool) != Some(false);
            can_create_outfits =
                character.get("canCreateOutfits").and_then(Value::as_bool) != Some(false);
        }
    }

    let mut has_document_stores = false;
    if has_project {
        if let Some(project_id) = chat.get("projectId").and_then(Value::as_str) {
            let pid = project_id.to_string();
            let links = db.read_mount_index(|mount| {
                ProjectDocMountLinksRepository::new(mount).find_by_project_id(&pid)
            });
            // Non-critical, default to false (v4's empty catch).
            has_document_stores = links.map(|l| !l.is_empty()).unwrap_or(false);
        }
    }

    Some(ChatContext {
        has_image_profile,
        has_project,
        has_document_stores,
        allows_web_search,
        is_multi_character,
        can_dress_themselves,
        can_create_outfits,
    })
}

/// The `doc_*` availability arm's case list — route.ts:693–706. NOTE
/// `doc_copy_file` is absent (the v4 quirk in the module header).
const DOC_AVAILABILITY_IDS: &[&str] = &[
    "doc_read_file",
    "doc_write_file",
    "doc_str_replace",
    "doc_insert_text",
    "doc_grep",
    "doc_list_files",
    "doc_read_frontmatter",
    "doc_update_frontmatter",
    "doc_read_heading",
    "doc_update_heading",
    "doc_move_file",
    "doc_delete_file",
    "doc_create_folder",
    "doc_delete_folder",
];

/// v4 `GET /api/v1/tools`. `web_search_configured` is the host's
/// `isWebSearchConfigured()` (Serper registered OR the env key — v5 has no
/// plugin registry, so the host passes the env-key half).
pub fn tools_list(
    db: &Db,
    user_id: &str,
    chat_id: Option<&str>,
    include_schemas: bool,
    web_search_configured: bool,
) -> Response {
    let mut tools: Vec<Value> = BUILT_IN_TOOLS
        .iter()
        .map(|t| {
            let mut m = Map::new();
            m.insert("id".into(), json!(t.id));
            m.insert("name".into(), json!(t.name));
            m.insert("description".into(), json!(t.description));
            m.insert("source".into(), json!("built-in"));
            m.insert("category".into(), json!(t.category));
            m.insert("userInvocable".into(), json!(true));
            if include_schemas {
                if let Some(parameters) = schema_parameters(t.id) {
                    m.insert("parameters".into(), parameters);
                }
            }
            Value::Object(m)
        })
        .collect();

    // (The plugin iteration / metadata / hierarchy passes sit here in v4 —
    // vacuous without a plugin runtime; see the module header.)

    let chat_context = chat_id.and_then(|cid| build_chat_context(db, user_id, cid));

    if let Some(ctx) = chat_context {
        for tool in &mut tools {
            let id = tool
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let obj = tool.as_object_mut().expect("tool objects");
            obj.insert("available".into(), json!(true));
            let mut unavailable = |reason: &str| {
                obj.insert("available".into(), json!(false));
                obj.insert("unavailableReason".into(), json!(reason));
            };
            match id.as_str() {
                "generate_image" => {
                    if !ctx.has_image_profile {
                        unavailable("Requires an image generation profile to be configured for the character");
                    }
                }
                "project_info" => {
                    if !ctx.has_project {
                        unavailable("Chat must be associated with a project");
                    }
                }
                "search_web" => {
                    if !ctx.allows_web_search {
                        unavailable("Web search must be enabled in the connection profile");
                    } else if !web_search_configured {
                        unavailable("No search provider configured. Please add a search provider API key in Settings > API Keys.");
                    }
                }
                "whisper" => {
                    if !ctx.is_multi_character {
                        unavailable("Whisper requires a multi-character chat with more than one active character");
                    }
                }
                "wardrobe_list" | "wardrobe_read" | "wardrobe_wear" | "wardrobe_take_off" => {
                    if !ctx.can_dress_themselves {
                        unavailable("Character does not have wardrobe self-dressing enabled");
                    }
                }
                "wardrobe_create" | "wardrobe_update" | "wardrobe_archive" => {
                    if !ctx.can_create_outfits {
                        unavailable("Character does not have wardrobe item creation enabled");
                    }
                }
                _ if DOC_AVAILABILITY_IDS.contains(&id.as_str()) => {
                    if !ctx.has_project {
                        unavailable("Chat must be associated with a project");
                    } else if !ctx.has_document_stores {
                        unavailable("Project must have linked document stores (configure in Project > The Scriptorium)");
                    }
                }
                _ => {}
            }
        }
    }

    let count = tools.len();
    Response::ToolsInventory(json!({ "tools": tools, "count": count }))
}

/// The catch-all (v4 route.ts:723–726) — unreached in practice; the reads
/// above degrade instead of throwing.
#[allow(dead_code)]
fn list_failed() -> Response {
    Response::error(ErrorKind::Internal, "Failed to list available tools")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every schema-map entry must resolve to catalog parameters — a missing
    /// key would silently drop `parameters` from `includeSchemas` output.
    #[test]
    fn every_schema_key_resolves() {
        for (id, _) in SCHEMA_KEYS {
            assert!(
                schema_parameters(id).is_some(),
                "schema-map id {id} did not resolve in the definitions catalog"
            );
        }
        assert_eq!(SCHEMA_KEYS.len(), 37);
        assert_eq!(BUILT_IN_TOOLS.len(), 40);
    }
}
