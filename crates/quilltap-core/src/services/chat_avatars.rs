//! The per-chat **avatar-override** service layer (P4.9E1A) — a differential
//! port of v4's `app/api/v1/chats/[id]/actions/avatars.ts` (`handleGetAvatars`
//! :18, `handleSetAvatar` :69, `handleRemoveAvatar` :130) and
//! `actions/toggle-avatar-generation.ts` (`handleToggleAvatarGeneration` :22).
//!
//! An override is stored on the CHARACTER, not the chat: `characters
//! .avatarOverrides` is an array of `{chatId, imageId}`, so "this chat's
//! avatars" is a scan of every character the user owns. v4 does exactly that
//! (`characters.findByUserId` → `flatMap` → filter by `chatId`), and the port
//! keeps the same traversal so the response array's ORDER matches: characters in
//! repository order, each character's overrides in stored order.
//!
//! Both mutating verbs write through the ported character UPDATE
//! ([`crate::db::vault_character_update::update_character`]); `avatarOverrides`
//! is not a vault-managed field, so it lands on the slim row.
//!
//! ## `toggle-avatar-generation` and the chat-column write
//!
//! v4 flips `chats.avatarGenerationEnabled` through `repos.chats.update`. v5's
//! [`ChatUpdate`](crate::db::chats::ChatUpdate) has no such field and
//! `db/chats.rs` belongs to a sibling lane this round, so the flip is a raw
//! single-column `UPDATE` — the `[[standalone-write-avoids-frozen-chatupdate]]`
//! pattern the order blesses for exactly this case. It is byte-identical to v4's
//! write: one column, no `updatedAt` mint (v4's `_update` preserves it).
//!
//! Toggling ON then enqueues one `CHARACTER_AVATAR_GENERATION` job per
//! LLM-controlled character participant, resolving the image profile
//! chat-level-first then the marked default. Every part of that is best-effort:
//! v4 wraps the whole block in a try/catch and each enqueue in its own, so a
//! failure never fails the toggle.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::{
    characters_read, chats_read, document_store_overlay::OverlayError, vault_character_update,
    DbError,
};
use crate::photos::photo_link_summary::get_photo_link_summary_by_sha256;
use crate::photos::resolve_character_avatar::resolve_character_avatar;
use crate::services::image_profile_resolution::resolve_avatar_image_profile;
use crate::services::queue_service::enqueue_character_avatar_generation;

/// v4's `{ error, status }` failure arm for this family.
#[derive(Debug, Clone)]
pub struct AvatarError {
    pub status: u16,
    pub message: String,
}

impl AvatarError {
    fn not_found(resource: &str) -> Self {
        Self {
            status: 404,
            message: format!("{resource} not found"),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: 500,
            message: message.into(),
        }
    }
}

impl From<DbError> for AvatarError {
    fn from(e: DbError) -> Self {
        AvatarError::internal(e.to_string())
    }
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn read_main_mount<T, F>(db: &Db, f: F) -> Result<T, DbError>
where
    F: FnOnce(&Connection, &Connection) -> Result<T, DbError>,
{
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// The `{chatId, characterId, imageId, character, image}` element v4 builds for
/// both `get-avatars` and `set-avatar`. `image` is `null` when the id resolves to
/// nothing (only reachable on the GET — `set-avatar` 404s first).
fn build_override_entry(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    character_name: &str,
    image_id: &str,
) -> Result<Value, DbError> {
    let resolved = resolve_character_avatar(main, mount, Some(image_id))?;
    let image = match resolved {
        Some(r) => {
            let link_summary = match r.sha256.as_deref().filter(|s| !s.is_empty()) {
                Some(sha) => get_photo_link_summary_by_sha256(mount, sha)?,
                None => Value::Null,
            };
            json!({
                "id": r.id,
                "filepath": r.url,
                "url": Value::Null,
                "sha256": r.sha256,
                "linkSummary": link_summary,
            })
        }
        None => Value::Null,
    };
    Ok(json!({
        "chatId": chat_id,
        "characterId": character_id,
        "imageId": image_id,
        "character": { "id": character_id, "name": character_name },
        "image": image,
    }))
}

/// v4 `handleGetAvatars` — every avatar override any of the user's characters
/// carries for this chat, enriched with the resolved image + its link summary.
pub fn get_avatars(db: &Db, user_id: &str, chat_id: &str) -> Result<Value, AvatarError> {
    let cid = chat_id.to_string();
    if db
        .read_main(move |c| chats_read::find_by_id(c, &cid))?
        .is_none()
    {
        return Err(AvatarError::not_found("Chat"));
    }
    let uid = user_id.to_string();
    let entries = read_main_mount(db, |main, mount| {
        let characters = characters_read::find_by_user_id(main, mount, &uid)?;
        let mut out = Vec::new();
        for character in &characters {
            let character_id = s(character, "id").unwrap_or_default();
            let character_name = s(character, "name").unwrap_or_default();
            let Some(overrides) = character.get("avatarOverrides").and_then(Value::as_array) else {
                continue;
            };
            for o in overrides {
                if s(o, "chatId").as_deref() != Some(chat_id) {
                    continue;
                }
                let image_id = s(o, "imageId").unwrap_or_default();
                out.push(build_override_entry(
                    main,
                    mount,
                    chat_id,
                    &character_id,
                    &character_name,
                    &image_id,
                )?);
            }
        }
        Ok(out)
    })
    // v4 wraps the whole handler in a try/catch → serverError.
    .map_err(|_| AvatarError::internal("Failed to fetch avatar overrides"))?;

    Ok(json!({ "data": entries }))
}

/// Write the character's `avatarOverrides` array through the ported character
/// UPDATE (an unmanaged field → the slim row).
async fn write_overrides(
    db: &Db,
    character_id: &str,
    overrides: Vec<Value>,
) -> Result<(), DbError> {
    let cid = character_id.to_string();
    let mut patch = Map::new();
    patch.insert("avatarOverrides".into(), Value::Array(overrides));
    db.write(move |w| {
        let mount =
            w.mount_index()
                .map(|m| m.connection())
                .ok_or(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ))?;
        let main = w.main().connection();
        // avatarOverrides is a slim key — the P4.22 `Unavailable` refusal is
        // unreachable here; collapse to the closure's DbError.
        vault_character_update::update_character(main, mount, &cid, &patch)
            .map_err(OverlayError::into_db)?;
        Ok(())
    })
    .await
}

/// v4 `handleSetAvatar` — pin an image as this character's avatar in this chat.
/// Replaces any existing override for the chat, else appends.
pub async fn set_avatar(
    db: &Db,
    chat_id: &str,
    character_id: &str,
    image_id: &str,
) -> Result<Value, AvatarError> {
    let cid = character_id.to_string();
    let Some(character) = read_main_mount(db, |main, mount| {
        characters_read::find_by_id(main, mount, &cid)
    })?
    else {
        return Err(AvatarError::not_found("Character"));
    };
    let iid = image_id.to_string();
    let resolved = read_main_mount(db, |main, mount| {
        resolve_character_avatar(main, mount, Some(&iid))
    })?;
    let Some(resolved) = resolved else {
        return Err(AvatarError::not_found("Image"));
    };

    let existing: Vec<Value> = character
        .get("avatarOverrides")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let entry = json!({ "chatId": chat_id, "imageId": image_id });
    let mut updated = existing.clone();
    match existing
        .iter()
        .position(|o| s(o, "chatId").as_deref() == Some(chat_id))
    {
        Some(idx) => updated[idx] = entry,
        None => updated.push(entry),
    }
    write_overrides(db, character_id, updated).await?;

    let link_summary = match resolved.sha256.as_deref().filter(|s| !s.is_empty()) {
        Some(sha) => {
            let sha = sha.to_string();
            db.read_mount_index(move |mount| get_photo_link_summary_by_sha256(mount, &sha))?
        }
        None => Value::Null,
    };
    let data = json!({
        "chatId": chat_id,
        "characterId": character_id,
        "imageId": image_id,
        "character": {
            "id": s(&character, "id").unwrap_or_default(),
            "name": s(&character, "name").unwrap_or_default(),
        },
        "image": {
            "id": resolved.id,
            "filepath": resolved.url,
            "url": Value::Null,
            "sha256": resolved.sha256,
            "linkSummary": link_summary,
        },
    });
    Ok(json!({ "data": data }))
}

/// v4 `handleRemoveAvatar` — drop this chat's override from the character.
pub async fn remove_avatar(
    db: &Db,
    chat_id: &str,
    character_id: &str,
) -> Result<Value, AvatarError> {
    let cid = character_id.to_string();
    let Some(character) = read_main_mount(db, |main, mount| {
        characters_read::find_by_id(main, mount, &cid)
    })?
    else {
        return Err(AvatarError::not_found("Character"));
    };
    let updated: Vec<Value> = character
        .get("avatarOverrides")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|o| s(o, "chatId").as_deref() != Some(chat_id))
        .collect();
    write_overrides(db, character_id, updated).await?;
    Ok(json!({ "data": { "success": true } }))
}

/// v4 `handleToggleAvatarGeneration` — flip `chats.avatarGenerationEnabled` and,
/// when turning ON, enqueue an avatar-generation job for every LLM-controlled
/// character participant.
pub async fn toggle_avatar_generation(
    db: &Db,
    user_id: &str,
    chat_id: &str,
) -> Result<Value, AvatarError> {
    let cid = chat_id.to_string();
    let Some(chat) = db.read_main(move |c| chats_read::find_by_id(c, &cid))? else {
        // v4 answers serverError here — the POST dispatcher already 404'd a
        // missing chat, so this arm is only reachable on a race.
        return Err(AvatarError::internal("Chat not found"));
    };
    // `!chat.avatarGenerationEnabled` — null/false/absent → true.
    let new_value = !chat
        .get("avatarGenerationEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let cid = chat_id.to_string();
    let updated = db
        .write(move |w| {
            let n = w.main().connection().execute(
                "UPDATE chats SET avatarGenerationEnabled = ?1 WHERE id = ?2",
                rusqlite::params![new_value as i64, cid],
            )?;
            Ok(n > 0)
        })
        .await?;
    if !updated {
        return Err(AvatarError::internal("Failed to update chat"));
    }

    tracing::info!(
        chatId = chat_id,
        avatarGenerationEnabled = new_value,
        context = "avatar-generation",
        "[Chats v1] Avatar generation toggled"
    );

    if new_value {
        // Best-effort from here down (v4's outer try/catch): chat-level first,
        // then the user's default, each checked for an API key (v4
        // `resolveAvatarImageProfile(user.id, repos, { chat })`, `9ecebf986` —
        // `chat` is the PRE-toggle row, as v4 reads it). With no usable
        // profile the sweep logs v4's DEBUG and enqueues nothing.
        let cid = chat_id.to_string();
        let (user, chat_row) = (user_id.to_string(), chat.clone());
        let resolved_profile = db.read_main(move |c| {
            Ok(
                resolve_avatar_image_profile(c, &user, None, Some(&chat_row))
                    .and_then(|p| s(&p, "id")),
            )
        });
        // v4's outer catch: non-fatal — the toggle succeeded, generation is
        // best-effort. v4 reads the participants off `repos.chats.update`'s
        // returned row; v5's write answers no row, so the re-read is the one
        // step here that can fail after the profile resolve, and it lands in
        // the same catch.
        let gen_error = |error: &DbError| {
            tracing::warn!(
                chatId = chat_id,
                error = %crate::db::fallback::error_text(error),
                context = "avatar-generation",
                "[Chats v1] Failed to enqueue initial avatar generation after toggle-on"
            );
        };
        let image_profile_id = match resolved_profile {
            Ok(Some(id)) => Some(id),
            Ok(None) => {
                tracing::debug!(
                    chatId = chat_id,
                    context = "avatar-generation",
                    "[Chats v1] No usable image profile; skipping initial avatar generation"
                );
                None
            }
            Err(e) => {
                gen_error(&e);
                None
            }
        };
        let chat_after = match image_profile_id {
            Some(_) => match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
                Ok(row) => row,
                Err(e) => {
                    gen_error(&e);
                    None
                }
            },
            None => None,
        };
        if let (Some(image_profile_id), Some(chat_after)) = (image_profile_id, chat_after) {
            let participants = chat_after
                .get("participants")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for p in &participants {
                if s(p, "type").as_deref() != Some("CHARACTER") {
                    continue;
                }
                let Some(character_id) = s(p, "characterId").filter(|v| !v.is_empty()) else {
                    continue;
                };
                if s(p, "controlledBy").as_deref() == Some("user") {
                    continue;
                }
                // v4 wraps each enqueue in its own try/catch and only warns —
                // one character failing never strands the rest.
                if let Err(char_error) = enqueue_character_avatar_generation(
                    db,
                    user_id,
                    chat_id,
                    &character_id,
                    &image_profile_id,
                    None,
                    // v4's toggle-avatar-generation action builds the payload
                    // literal with no `force` key (`toggle-avatar-generation
                    // .ts:80`), so the cache serves this like every other
                    // automatic trigger.
                    false,
                )
                .await
                {
                    tracing::warn!(
                        chatId = chat_id,
                        characterId = %character_id,
                        error = %crate::db::fallback::error_text(&char_error),
                        context = "avatar-generation",
                        "[Chats v1] Failed to enqueue avatar generation for character"
                    );
                }
            }
        }
    }

    Ok(json!({ "avatarGenerationEnabled": new_value }))
}

// v4 `toggle-avatar-generation.ts`'s lines on the toggle-on sweep: the INFO
// `Avatar generation toggled`, the per-character WARN (`:77`, inside the loop's
// own try — one character failing never strands the rest) and the outer
// WARN (`:88`, the best-effort catch). Each is pinned with its whole field bag
// in v4's key order, and each WARN with a SILENCE leg. The instance is a REAL
// fresh-provisioned one with one keyed default image profile and one chat.
#[cfg(test)]
mod toggle_log_tests {
    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::test_support::captured_with;

    /// The pepper `provision_fresh_instance`'s own self-test uses (never a real one).
    const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";
    const USER: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
    const CHAT: &str = "11111111-1111-4111-8111-111111111111";
    const ALICE: &str = "22222222-2222-4222-8222-222222222222";
    const BRAM: &str = "44444444-4444-4444-8444-444444444444";
    const PROFILE: &str = "33333333-3333-4333-8333-333333333333";
    const PER_CHARACTER: &str = "[Chats v1] Failed to enqueue avatar generation for character";
    const OUTER: &str = "[Chats v1] Failed to enqueue initial avatar generation after toggle-on";

    /// A fresh instance, one keyed default image profile, one chat with two
    /// LLM-controlled characters (and a user seat the sweep skips); `plant`
    /// runs on the main connection last.
    fn instance(plant: &'static str) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().expect("tempdir");
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).expect("data dir");
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).expect("provision");
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: Some(data.join("quilltap-llm-logs.db")),
            },
            PEPPER,
        )
        .expect("open instance");
        let now = "2020-01-01T00:00:00.000Z".to_string();
        db.write_blocking(move |ws| {
            let main = ws.main().connection();
            crate::db::image_profiles::ImageProfilesRepository::new(main).create(
                &crate::db::image_profiles::IpCreate {
                    user_id: USER.to_string(),
                    name: "Original Desk".to_string(),
                    provider: "OPENAI".to_string(),
                    api_key_id: Some("key-1".to_string()),
                    base_url: None,
                    model_name: "dall-e-3".to_string(),
                    parameters: json!({}),
                    is_default: true,
                    is_dangerous_compatible: false,
                    tags: Vec::new(),
                },
                &crate::db::image_profiles::CreateOptions {
                    id: PROFILE.to_string(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
            )?;
            let participant = |id: &str, character: &str, controlled_by: &str| {
                json!({
                    "id": id, "type": "CHARACTER", "characterId": character,
                    "controlledBy": controlled_by, "createdAt": now, "updatedAt": now,
                })
            };
            let chat_create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
                "userId": USER,
                "title": "A Sitting",
                "participants": [
                    participant("p1", ALICE, "llm"),
                    participant("p2", BRAM, "llm"),
                    participant("p3", "55555555-5555-4555-8555-555555555555", "user"),
                ],
            }))
            .expect("chat create shape");
            ws.main().chats().create(
                &chat_create,
                &crate::db::chats::CreateOptions {
                    id: CHAT.to_string(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            )?;
            ws.main().connection().execute_batch(plant)?;
            Ok(())
        })
        .expect("seed the instance");
        (dir, db)
    }

    fn toggle(db: &Db) -> (Result<Value, AvatarError>, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        captured_with(|| rt.block_on(toggle_avatar_generation(db, USER, CHAT)))
    }

    /// The `message k=v…` tail of every line carrying `needle`.
    fn tagged(lines: &[String], needle: &str) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.contains(needle))
            .map(|l| l.splitn(3, ' ').nth(2).unwrap_or_default().to_string())
            .collect()
    }

    fn jobs_for(db: &Db) -> Vec<String> {
        db.read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT json_extract(payload, '$.characterId') FROM background_jobs \
                 WHERE type = 'CHARACTER_AVATAR_GENERATION' ORDER BY json_extract(payload, '$.characterId')",
            )?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read jobs")
    }

    /// The clean sweep: the INFO, both characters enqueued, and neither WARN.
    #[test]
    fn a_clean_toggle_on_logs_the_info_and_neither_warn() {
        let (_dir, db) = instance("");
        let (r, lines) = toggle(&db);
        assert_eq!(
            r.expect("toggled"),
            json!({ "avatarGenerationEnabled": true })
        );
        assert_eq!(
            tagged(&lines, "Avatar generation toggled"),
            vec![format!(
                "[Chats v1] Avatar generation toggled chatId={CHAT} avatarGenerationEnabled=true context=avatar-generation"
            )],
            "{lines:#?}"
        );
        assert!(tagged(&lines, PER_CHARACTER).is_empty(), "{lines:#?}");
        assert!(tagged(&lines, OUTER).is_empty(), "{lines:#?}");
        assert_eq!(jobs_for(&db), vec![ALICE.to_string(), BRAM.to_string()]);
    }

    /// One character's enqueue refused (a planted trigger on the job INSERT):
    /// its WARN, and the other character still enqueued.
    #[test]
    fn a_refused_enqueue_warns_per_character_and_the_sweep_continues() {
        let (_dir, db) = instance(
            "CREATE TRIGGER refuse_alice BEFORE INSERT ON background_jobs \
             WHEN json_extract(NEW.payload, '$.characterId') = '22222222-2222-4222-8222-222222222222' \
             BEGIN SELECT RAISE(ABORT, 'planted'); END;",
        );
        let (r, lines) = toggle(&db);
        assert!(r.is_ok(), "the toggle still lands: {r:?}");
        assert_eq!(
            tagged(&lines, PER_CHARACTER),
            vec![format!(
                "{PER_CHARACTER} chatId={CHAT} characterId={ALICE} error=planted context=avatar-generation"
            )],
            "{lines:#?}"
        );
        assert!(tagged(&lines, OUTER).is_empty(), "{lines:#?}");
        assert_eq!(jobs_for(&db), vec![BRAM.to_string()]);
    }

    /// The participants re-read fails after the write (a planted trigger
    /// leaves a BLOB title the strict read refuses): the outer WARN, nothing
    /// enqueued, the toggle still answered.
    #[test]
    fn a_failed_sweep_warns_once_and_the_toggle_lands() {
        let (_dir, db) = instance(
            "CREATE TRIGGER corrupt_after_toggle AFTER UPDATE OF avatarGenerationEnabled ON chats \
             BEGIN UPDATE chats SET title = X'00' WHERE id = NEW.id; END;",
        );
        let (r, lines) = toggle(&db);
        assert_eq!(
            r.expect("toggled"),
            json!({ "avatarGenerationEnabled": true })
        );
        let outer = tagged(&lines, OUTER);
        assert_eq!(outer.len(), 1, "{lines:#?}");
        assert!(
            outer[0].starts_with(&format!("{OUTER} chatId={CHAT} error="))
                && outer[0].ends_with(" context=avatar-generation"),
            "{outer:?}"
        );
        assert!(tagged(&lines, PER_CHARACTER).is_empty(), "{lines:#?}");
        assert!(jobs_for(&db).is_empty());
    }
}
