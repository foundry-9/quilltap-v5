//! The home-dashboard payload (P4.6au) — a differential port of v4
//! `lib/services/home-data.service.ts` (`getHomeData`, 224 lines). A pure
//! composition over already-ported repos/services: the parallel fetches, the
//! help-chat filter, the enriched recent-chats slice, the three-source
//! project-activity sort, the character sort chain (favorites → chat count →
//! base-sensitivity name collation), and the DTO shapes whose omit-vs-null
//! splits are wire-visible.
//!
//! Consumed by the `systemHome` dispatch verb (`api::system::system_home`) and
//! its REST edge `GET /api/v1/system/home` (v4 `app/api/v1/system/home/route.ts`
//! — `successResponse(data)` is the RAW payload). v4's second consumer, the
//! server-rendered `/` (`app/page.tsx` + `redirectToWorkspaceTab`), is a
//! documented v5 deferral: the tabbed workspace is unported and v5's `/` is the
//! SPA Home screen.
//!
//! Pinned by `home_routes_equivalence` over the committed `home-{main,mount}.db`
//! fixture family — whose 14 salon chats exceed the 12-row slice, so the family
//! discriminates the one place this file departs from v4's order of operations:
//! the recent-chats slice happens BEFORE the enrichment, not after (see the
//! comment at the call site — payload-identical, and the whole of P4.64's
//! 22× measured saving).

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::clock::{iso_from_unix_ms, iso_to_ms};
use crate::collation::locale_compare_base;
use crate::db::files::FilesRepository;
use crate::db::projects::ProjectsRepository;
use crate::db::{characters_read, chats_read, users, DbError};
use crate::services::character_enrichment::{enrich_with_default_image, EnrichedDefaultImage};
use crate::services::chat_enrichment::{self, EnrichedChatSummary, EnrichedParticipantSummary};

// ===========================================================================
// The DTO shapes (v4 `components/homepage/types.ts` — field names are the wire
// contract; declaration order is the wire order under `preserve_order`).
// ===========================================================================

/// v4 `HomeData` — the whole payload, in v4's return-literal order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeData {
    pub display_name: String,
    pub last_chat_id: Option<String>,
    pub recent_chats: Vec<RecentChat>,
    pub projects: Vec<HomepageProject>,
    pub characters: Vec<HomepageCharacter>,
}

/// v4 `RecentChat` — the homepage chat card, mapped from the enriched summary.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentChat {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub last_message_at: Option<String>,
    pub is_dangerous_chat: bool,
    /// v4 `chat.storyBackground?.filepath || null` — JS `||`: an empty filepath
    /// coerces to null too, not just an absent background.
    pub story_background_url: Option<String>,
    pub participants: Vec<RecentChatParticipant>,
    #[serde(rename = "_count")]
    pub count: RecentChatCount,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentChatParticipant {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub is_active: bool,
    pub display_order: i64,
    /// v4 ternary `p.character ? {…} : null` — an unresolved character is an
    /// EXPLICIT null, never omitted.
    pub character: Option<RecentChatCharacter>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentChatCharacter {
    pub id: String,
    pub name: String,
    /// v4 `defaultImageId: … || undefined` — a falsy id (absent OR empty) is
    /// OMITTED, unlike `defaultImage` below which is an explicit null. The
    /// omit-vs-null split is wire-visible (the P4.6p per-field rule).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_image_id: Option<String>,
    pub default_image: Option<RecentChatImage>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecentChatImage {
    pub id: String,
    pub filepath: String,
    /// v4 `url: … || undefined` — omitted when falsy (and the LIST enrichment
    /// path never populates it, so it is omitted in practice).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecentChatCount {
    pub messages: i64,
}

/// v4 `HomepageProject`. `description`/`color`/`icon` are pass-throughs of the
/// hydrated project row (`k: project.k`): present-even-null is emitted, an
/// absent key (JS `undefined`) is omitted — mirrored as `Option<Value>` where
/// `None` = absent.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomepageProject {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Value>,
    pub chat_count: i64,
    /// The max of (project.updatedAt, latest chat lastMessageAt, latest file
    /// updatedAt), re-serialized through `new Date(t).toISOString()`.
    pub last_activity: String,
}

/// v4 `HomepageCharacter` — every field always present (the coercions are all
/// `|| null` / `?? false` / `|| []`, never omit).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomepageCharacter {
    pub id: String,
    pub name: String,
    /// v4 `char.title || null` — an empty-string title coerces to null.
    pub title: Option<String>,
    /// v4 `defaultImage?.id ?? null` — from the ENRICHED image, not the raw
    /// `char.defaultImageId` (a dangling id resolves to null here).
    pub default_image_id: Option<String>,
    pub default_image: Option<EnrichedDefaultImage>,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub npc: bool,
    pub chat_count: i64,
    pub default_connection_profile_id: Option<String>,
}

// ===========================================================================
// The composition (v4 `getHomeData`)
// ===========================================================================

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// JS truthiness of a JSON value (the home service only tests strings, bools,
/// and null/absent).
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Some(_) => true,
    }
}

/// `chat.lastMessageAt ? new Date(chat.lastMessageAt) : null` → Unix ms. A
/// non-ISO string would be JS's Invalid Date (NaN, poisoning `Math.max` and
/// throwing in `toISOString`) — unreachable for repo-written rows, so the
/// parse failure maps to "no usable time" like the harness's other consumers.
fn ms_of(v: &Value, key: &str) -> Option<i64> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .and_then(iso_to_ms)
}

/// v4 `getHomeData(repos, { userId, fallbackName })`. `user_id` is a parameter
/// (not hard-coded `SINGLE_USER_ID`) so the differential drives with the
/// fixture's own user ids; the engine passes `SINGLE_USER_ID` and a `None`
/// fallback (v5 single-user has no session name apart from the users row).
pub fn get_home_data(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    fallback_name: Option<&str>,
) -> Result<HomeData, DbError> {
    // v4: `userId ? await repos.users.findById(userId) : null` — v5 single-user
    // always has a userId, so the no-userId arms collapse (the phase-4 pattern);
    // a missing users ROW still exercises the `user == null` branch.
    let user_name: Option<String> = users::find_name_by_id(main, user_id)?.flatten();

    // The parallel fetch block (sequential here — same reads, one connection).
    let all_chats_raw = chats_read::find_by_user_id(main, user_id)?;
    let all_projects = ProjectsRepository::new(main, mount)
        .find_all()
        .map_err(|e| DbError::Internal(format!("project overlay: {e}")))?;
    let all_characters = characters_read::find_by_user_id(main, mount, user_id)?;
    let all_files = FilesRepository::new(main).find_all()?;

    // Help-chat filter: `!c.chatType || c.chatType === 'salon'` — note this is
    // NOT the Salon list's filter (autonomous chats never show on the homepage,
    // visibility notwithstanding).
    let mut salon_chats: Vec<Value> = all_chats_raw
        .iter()
        .filter(|c| {
            let ct = c.get("chatType").and_then(Value::as_str);
            matches!(ct, None | Some("") | Some("salon"))
        })
        .cloned()
        .collect();

    // enrichChatsForList (sorts by lastMessageAt ?? updatedAt desc internally)
    // → cleanEnrichedChats (the `#[serde(skip)]` field) → slice(0, 12).
    //
    // ⚠ ORDER OF OPERATIONS, deliberately not v4's: v4 enriches EVERY salon
    // chat and only then slices twelve. v5 sorts and slices FIRST, then
    // enriches those twelve. The rows are identical — the slice order is
    // `sort_chats_for_list`'s key, a RAW chat field that no part of the
    // enrichment feeds; the sort is stable on both sides, so re-sorting the
    // already-sorted twelve inside `enrich_chats_for_list` is a no-op; and
    // nothing below reads an enriched chat past the twelfth (`last_chat_id` is
    // the first, `recent_chats` is the map over the twelve). Only the cost
    // moves. P4.64 measured it on the real instance (773 salon chats): the
    // enrichment ran 8.6–12.2 s — ~97% of `systemHome`'s ~9 s — against 145 ms
    // for the same twelve, because every CHARACTER participant re-hydrates its
    // character through the vault overlay (nine single-file reads plus two
    // folder scans against the mount index, per lookup, ~2,000 lookups per
    // dashboard) and 761 of the 773 results were then thrown away.
    //
    // The one thing that can differ is an error only a discarded chat would
    // have raised: v4 (and v5 until now) fails the whole dashboard on it. Every
    // read the enrichment makes for a discarded chat it also makes for the top
    // twelve, against the same tables, so a failure that spares all twelve and
    // strikes only a row nobody renders is not a shape these reads produce.
    chat_enrichment::sort_chats_for_list(&mut salon_chats);
    salon_chats.truncate(12);
    let recent_enriched = chat_enrichment::enrich_chats_for_list(main, mount, salon_chats)?;

    let last_chat_id = recent_enriched.first().map(|c| c.id.clone());

    let recent_chats: Vec<RecentChat> = recent_enriched.iter().map(map_recent_chat).collect();

    // ── Project stats: chat counts + latest chat lastMessageAt per project ──
    use std::collections::HashMap;
    struct ChatStats {
        count: i64,
        last_message_ms: Option<i64>,
    }
    let mut project_chat_stats: HashMap<String, ChatStats> = HashMap::new();
    for chat in &all_chats_raw {
        // JS `if (chat.projectId)` — truthy: an empty-string id is skipped too.
        if !truthy(chat.get("projectId")) {
            continue;
        }
        let pid = s(chat, "projectId").unwrap_or_default();
        let chat_last = ms_of(chat, "lastMessageAt");
        match project_chat_stats.get_mut(&pid) {
            Some(existing) => {
                existing.count += 1;
                if let Some(t) = chat_last {
                    if existing.last_message_ms.map(|e| t > e).unwrap_or(true) {
                        existing.last_message_ms = Some(t);
                    }
                }
            }
            None => {
                project_chat_stats.insert(
                    pid,
                    ChatStats {
                        count: 1,
                        last_message_ms: chat_last,
                    },
                );
            }
        }
    }

    // ── Latest file updatedAt per project ──
    let mut project_file_stats: HashMap<String, i64> = HashMap::new();
    for file in &all_files {
        let Some(pid) = file.project_id.as_deref().filter(|p| !p.is_empty()) else {
            continue;
        };
        let Some(t) = iso_to_ms(&file.updated_at) else {
            continue;
        };
        project_file_stats
            .entry(pid.to_string())
            .and_modify(|e| {
                if t > *e {
                    *e = t;
                }
            })
            .or_insert(t);
    }

    // ── Projects, sorted by most recent activity, capped at 12 ──
    let activity_ms = |project: &Value| -> i64 {
        let pid = s(project, "id").unwrap_or_default();
        let project_time = ms_of(project, "updatedAt").unwrap_or(0);
        let chat_time = project_chat_stats
            .get(&pid)
            .and_then(|st| st.last_message_ms)
            .unwrap_or(0);
        let file_time = project_file_stats.get(&pid).copied().unwrap_or(0);
        project_time.max(chat_time).max(file_time)
    };
    let mut sorted_projects: Vec<&Value> = all_projects.iter().collect();
    // Descending; stable, so ties keep findAll order (V8's sort is stable too).
    sorted_projects.sort_by_key(|p| std::cmp::Reverse(activity_ms(p)));
    sorted_projects.truncate(12);

    let projects: Vec<HomepageProject> = sorted_projects
        .iter()
        .map(|project| {
            let pid = s(project, "id").unwrap_or_default();
            HomepageProject {
                id: pid.clone(),
                name: s(project, "name").unwrap_or_default(),
                description: project.get("description").cloned(),
                color: project.get("color").cloned(),
                icon: project.get("icon").cloned(),
                chat_count: project_chat_stats.get(&pid).map(|st| st.count).unwrap_or(0),
                last_activity: iso_from_unix_ms(activity_ms(project)),
            }
        })
        .collect();

    // ── Character chat counts: EVERY participant with a characterId, over
    // allChatsRaw — i.e. BEFORE the help-chat filter (help/autonomous chats
    // count too), and regardless of the character's own npc/controlledBy. ──
    let mut character_chat_counts: HashMap<String, i64> = HashMap::new();
    for chat in &all_chats_raw {
        let Some(participants) = chat.get("participants").and_then(Value::as_array) else {
            continue;
        };
        for participant in participants {
            if truthy(participant.get("characterId")) {
                let cid = s(participant, "characterId").unwrap_or_default();
                *character_chat_counts.entry(cid).or_insert(0) += 1;
            }
        }
    }

    // ── The AI character grid: filter `!c.npc && c.controlledBy !== 'user'`,
    // sort favorites → chatCount desc → base-sensitivity name, cap at 24. ──
    let count_of = |c: &Value| -> i64 {
        s(c, "id")
            .and_then(|id| character_chat_counts.get(&id).copied())
            .unwrap_or(0)
    };
    let mut sorted_characters: Vec<&Value> = all_characters
        .iter()
        .filter(|c| {
            !truthy(c.get("npc")) && c.get("controlledBy").and_then(Value::as_str) != Some("user")
        })
        .collect();
    sorted_characters.sort_by(|a, b| {
        // v4 `a.isFavorite !== b.isFavorite` is a RAW value compare (absent vs
        // null vs false are three distinct JS values); the branch then tests
        // truthiness. Repo-written rows carry real booleans, where this is
        // exactly bool inequality.
        if a.get("isFavorite") != b.get("isFavorite") {
            return if truthy(a.get("isFavorite")) {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        let (a_chats, b_chats) = (count_of(a), count_of(b));
        if a_chats != b_chats {
            return b_chats.cmp(&a_chats);
        }
        locale_compare_base(
            s(a, "name").unwrap_or_default().as_str(),
            s(b, "name").unwrap_or_default().as_str(),
        )
    });
    sorted_characters.truncate(24);

    let mut characters: Vec<HomepageCharacter> = Vec::with_capacity(sorted_characters.len());
    for c in sorted_characters {
        let default_image = enrich_with_default_image(
            main,
            mount,
            c.get("defaultImageId").and_then(Value::as_str),
        )?;
        characters.push(HomepageCharacter {
            id: s(c, "id").unwrap_or_default(),
            name: s(c, "name").unwrap_or_default(),
            title: s(c, "title").filter(|t| !t.is_empty()),
            default_image_id: default_image.as_ref().map(|i| i.id.clone()),
            default_image,
            tags: c
                .get("tags")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            is_favorite: truthy(c.get("isFavorite")),
            npc: truthy(c.get("npc")),
            chat_count: count_of(c),
            default_connection_profile_id: s(c, "defaultConnectionProfileId")
                .filter(|p| !p.is_empty()),
        });
    }

    // `user?.name || fallbackName || 'there'` — JS `||`: empty strings fall
    // through both stages.
    let display_name = user_name
        .filter(|n| !n.is_empty())
        .or_else(|| fallback_name.map(str::to_string).filter(|n| !n.is_empty()))
        .unwrap_or_else(|| "there".to_string());

    Ok(HomeData {
        display_name,
        last_chat_id,
        recent_chats,
        projects,
        characters,
    })
}

/// The enriched-chat → `RecentChat` projection (v4's `.map` literal).
fn map_recent_chat(chat: &EnrichedChatSummary) -> RecentChat {
    RecentChat {
        id: chat.id.clone(),
        title: chat.title.clone(),
        updated_at: chat.updated_at.clone(),
        last_message_at: chat.last_message_at.clone(),
        is_dangerous_chat: chat.is_dangerous_chat,
        story_background_url: chat
            .story_background
            .as_ref()
            .map(|bg| bg.filepath.clone())
            .filter(|f| !f.is_empty()),
        participants: chat.participants.iter().map(map_participant).collect(),
        count: RecentChatCount {
            messages: chat.count.messages,
        },
    }
}

fn map_participant(p: &EnrichedParticipantSummary) -> RecentChatParticipant {
    RecentChatParticipant {
        id: p.id.clone(),
        kind: p.kind.clone(),
        is_active: p.is_active,
        display_order: p.display_order,
        character: p.character.as_ref().map(|c| RecentChatCharacter {
            id: c.id.clone(),
            name: c.name.clone(),
            default_image_id: c.default_image_id.clone().filter(|i| !i.is_empty()),
            default_image: c.default_image.as_ref().map(|i| RecentChatImage {
                id: i.id.clone(),
                filepath: i.filepath.clone(),
                url: i.url.clone().filter(|u| !u.is_empty()),
            }),
            tags: c.tags.clone(),
        }),
    }
}
