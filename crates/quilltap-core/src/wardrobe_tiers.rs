//! The shared wardrobe tiers (v4 `lib/wardrobe/shared-tiers.ts`, `8600c83f`).
//!
//! The mounts that make up everything a character can wear *without owning it*:
//! their groups' stores and the chat's project stores. (Quilltap General is the
//! instance singleton — always in scope, never passed around.)
//!
//! One type and one resolver, so the tiers travel together. A call site that
//! threads only `project_mount_point_ids` is the bug this module exists to
//! prevent: items moved into a group's `Wardrobe/` folder were invisible to
//! every read path, so nobody could wear the household livery hanging by the
//! door.
//!
//! Precedence, mirroring [`dedupe_tier_triple`](crate::db::tiered_mount_pool::dedupe_tier_triple):
//! **character > group > project > general**.
//!
//! The project tier is roster-gated (v4 `9753d0eb2`): a character off the
//! chat's project roster gets no project stores, unless the caller is the
//! human operator dressing the character ([`SharedWardrobeTierOptions::operator`]
//! — the Salon's outfit dialog), in which case the roster does not apply. The
//! gate is [`crate::project_roster_access`]'s; the equipped-outfit, avatar and
//! announcement reads that go through `resolve_project_mount_point_ids_for_chat`
//! directly are deliberately NOT gated — v4 gates exactly this resolver.
//!
//! v4 states both fields optional so a caller with no chat/project context can
//! pass a partial object; v5 states them as plain (possibly empty) lists on one
//! struct, which is the same guarantee with the "forgot a tier" hole welded
//! shut — there is no way to construct the carrier that names only one tier
//! without saying so.
//!
//! **Removed at v4 `561466cfe` (the 4.9.0 knip sweep):**
//! `resolveSharedWardrobeTiersForProject` and `noSharedWardrobeTiers`. v4
//! deleted both as unused exports; v5's twin of the first
//! (`resolve_shared_wardrobe_tiers_for_project`) had likewise never acquired a
//! call site here — the project-keyed entrance goes through
//! [`SharedWardrobeTiers::project_only`] or straight to
//! `resolve_project_mount_point_ids` — so it went with it, dead in both trees.
//! The second is NOT vestigial here: v4's free function returned a fresh
//! `EMPTY` spread, while v5 spells it [`SharedWardrobeTiers::none`], which is
//! load-bearing at sixteen call sites (the no-context archetype reads). A
//! `pub` item in a library crate is invisible to `dead_code`, which is why this
//! class needs a v4 deletion to surface it at all.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::chats_read;
use crate::db::tiered_mount_pool::resolve_group_mount_point_ids_for_character;
use crate::project_roster_access::roster_gated_project_id;
use crate::tools::wardrobe_shared::resolve_project_mount_point_ids;

/// The shared mounts in scope for one character's wardrobe (v4
/// `SharedWardrobeTiers` / `ResolvedSharedWardrobeTiers` — v5 has only the
/// resolved shape).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedWardrobeTiers {
    /// Group stores (official + linked) of every group the character belongs to.
    /// Resolved per character, never per chat: a character never gains a
    /// co-participant's group stores.
    pub group_mount_point_ids: Vec<String>,
    /// Document stores linked to the chat's project.
    pub project_mount_point_ids: Vec<String>,
}

impl SharedWardrobeTiers {
    /// v4 `noSharedWardrobeTiers()` — the no-context tiers: Quilltap General
    /// alone.
    pub fn none() -> Self {
        SharedWardrobeTiers::default()
    }

    /// The project tier alone, for the handful of call sites that genuinely have
    /// no character to key the group tier on (an instance-wide archetype read).
    /// **Not** a shortcut for "I didn't thread the group tier" — v4 keys the
    /// group tier on a character, so a site with a character in hand must go
    /// through [`shared_wardrobe_tiers_for_character`].
    pub fn project_only(project_mount_point_ids: &[String]) -> Self {
        SharedWardrobeTiers {
            group_mount_point_ids: Vec::new(),
            project_mount_point_ids: project_mount_point_ids.to_vec(),
        }
    }

    /// The mounts to fold over Quilltap General, **weakest tier first**, so a
    /// later mount's item shadows an earlier one: project, then group (v4's
    /// `[...projectMountPointIds, ...groupMountPointIds]`).
    pub fn scoped_mounts(&self) -> Vec<String> {
        let mut scoped = self.project_mount_point_ids.clone();
        scoped.extend(self.group_mount_point_ids.iter().cloned());
        scoped
    }
}

/// v4 `SharedWardrobeTierOptions` (`9753d0eb2`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SharedWardrobeTierOptions {
    /// The human operator is choosing on the character's behalf (the Salon's
    /// outfit dialog), so the project roster does not apply. Character tool calls
    /// never set this.
    pub operator: bool,
}

/// v4 `resolveSharedWardrobeTiersForChat(chatId, characterId, options)` — both
/// shared tiers for a character in a chat. Each tier fails soft to `[]` on its
/// own (the underlying resolvers swallow and log), so a missing chat or a
/// character with no group memberships simply narrows the pool.
///
/// The project tier is roster-gated: a character off the chat's project roster
/// gets no project stores, unless `options.operator` is set (see
/// [`crate::project_roster_access`]).
pub fn resolve_shared_wardrobe_tiers_for_chat(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    options: SharedWardrobeTierOptions,
) -> SharedWardrobeTiers {
    SharedWardrobeTiers {
        group_mount_point_ids: resolve_group_mount_point_ids_for_character(
            main,
            mount,
            character_id,
        ),
        project_mount_point_ids: resolve_project_tier_for_chat(
            main,
            mount,
            chat_id,
            character_id,
            options,
        ),
    }
}

/// v4 `resolveProjectTierForChat` (`shared-tiers.ts:82-110`): the chat's
/// project, roster-gated, then its linked stores.
///
/// `chat_id` and `character_id` are JS-truthiness arguments (`&str` here): an
/// empty chat id answers `[]` with no line; an empty character admits.
///
/// The chat is read through `chats_read::find_by_id_or_none` — v4's
/// `chats.findById` is the fallback `_findById`, so a FAILED read logs `Error
/// finding entity by ID {collection: chats, id}` and answers `null` → `[]`. v4's
/// own `catch` around that read, WARN `[Wardrobe] Project lookup for chat
/// failed`, is therefore UNREACHABLE on a repository failure (the P4.90 /
/// P4.D225 "catch behind a fallback `safeQuery`" class) and is recorded, not
/// ported (P4.D245 §D5).
fn resolve_project_tier_for_chat(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    options: SharedWardrobeTierOptions,
) -> Vec<String> {
    if chat_id.is_empty() {
        return Vec::new();
    }
    let Some(project_id) = chats_read::find_by_id_or_none(main, chat_id)
        .as_ref()
        .and_then(|chat| chat.get("projectId"))
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
    else {
        return Vec::new();
    };
    let gated = if options.operator {
        Some(project_id.clone())
    } else {
        roster_gated_project_id(
            main,
            mount,
            Some(&project_id),
            (!character_id.is_empty()).then_some(character_id),
        )
    };
    let Some(gated) = gated else {
        tracing::debug!(
            chatId = %chat_id,
            projectId = %project_id,
            characterId = %character_id,
            "[Wardrobe] Character off project roster — project wardrobe withheld"
        );
        return Vec::new();
    };
    resolve_project_mount_point_ids(mount, Some(&gated))
}

/// v4 `sharedWardrobeTiersForCharacter(characterId, projectMountPointIds)` —
/// pair an already-resolved project tier with one character's group tier.
///
/// For the loops that resolve a whole cast against a single chat: the project
/// stores are the same for everyone and are fetched once by the caller, while
/// the group stores differ per character and must be fetched inside the loop.
pub fn shared_wardrobe_tiers_for_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    project_mount_point_ids: &[String],
) -> SharedWardrobeTiers {
    SharedWardrobeTiers {
        group_mount_point_ids: resolve_group_mount_point_ids_for_character(
            main,
            mount,
            character_id,
        ),
        project_mount_point_ids: project_mount_point_ids.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_mounts_puts_the_project_tier_first_so_group_shadows_it() {
        let tiers = SharedWardrobeTiers {
            group_mount_point_ids: vec!["g1".into(), "g2".into()],
            project_mount_point_ids: vec!["p1".into()],
        };
        assert_eq!(tiers.scoped_mounts(), vec!["p1", "g1", "g2"]);
    }

    #[test]
    fn no_tiers_is_general_alone() {
        assert!(SharedWardrobeTiers::none().scoped_mounts().is_empty());
    }

    // ---- P4.D245 (v4 `9753d0eb2`): the roster-gated project tier ----

    /// MAIN: the fresh-schema tables (so `chats_read` finds every column it
    /// selects), a `chats` row (`chat-1` → `p-1`), a project-less `chat-np`, and
    /// the slim `projects` row for `p-1` (store `r-1`); MOUNT: the project link
    /// and the overlay join with `r-1`'s `properties.json` (roster `[cccccccc-cccc-4ccc-8ccc-cccccccccccc]`,
    /// Allow Any OFF). The `group_character_members` table is empty so the group
    /// tier is `[]`.
    fn fixture() -> (Connection, Connection) {
        let main = Connection::open_in_memory().unwrap();
        let schema: Value =
            serde_json::from_str(include_str!("services/provisioning/fresh_schema.json")).unwrap();
        for ddl in schema["main"].as_array().unwrap() {
            main.execute_batch(ddl.as_str().unwrap()).unwrap();
        }
        main.execute_batch(
            r#"INSERT INTO "chats" ("id", "userId", "participants", "title", "projectId", "createdAt", "updatedAt")
                 VALUES ('chat-1','u','[]','t','p-1','2020-01-01T00:00:00.000Z','2020-01-01T00:00:00.000Z');
               INSERT INTO "chats" ("id", "userId", "participants", "title", "createdAt", "updatedAt")
                 VALUES ('chat-np','u','[]','t','2020-01-01T00:00:00.000Z','2020-01-01T00:00:00.000Z');
               INSERT INTO "projects" ("id", "name", "officialMountPointId", "createdAt", "updatedAt")
                 VALUES ('p-1','Papers','r-1','2020-01-01T00:00:00.000Z','2020-01-01T00:00:00.000Z');"#,
        )
        .unwrap();
        let mount = Connection::open_in_memory().unwrap();
        mount
            .execute_batch(
                r#"CREATE TABLE "project_doc_mount_links" ("id" TEXT PRIMARY KEY, "projectId" TEXT NOT NULL,
                     "mountPointId" TEXT NOT NULL, "createdAt" TEXT, "updatedAt" TEXT);
                   INSERT INTO "project_doc_mount_links" VALUES ('l-1','p-1','r-1','','');
                   CREATE TABLE "group_character_members" ("id" TEXT PRIMARY KEY, "groupId" TEXT NOT NULL,
                     "characterId" TEXT NOT NULL, "createdAt" TEXT, "updatedAt" TEXT);
                   CREATE TABLE "group_doc_mount_links" ("id" TEXT PRIMARY KEY, "groupId" TEXT NOT NULL,
                     "mountPointId" TEXT NOT NULL, "createdAt" TEXT, "updatedAt" TEXT);
                   CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY NOT NULL);
                   CREATE TABLE doc_mount_documents (id TEXT PRIMARY KEY NOT NULL, fileId TEXT NOT NULL, content TEXT);
                   CREATE TABLE doc_mount_file_links (id TEXT PRIMARY KEY NOT NULL, fileId TEXT NOT NULL, mountPointId TEXT NOT NULL, relativePath TEXT NOT NULL);
                   INSERT INTO doc_mount_files VALUES ('f-1');
                   INSERT INTO doc_mount_documents VALUES ('d-1','f-1','{"allowAnyCharacter":false,"characterRoster":["cccccccc-cccc-4ccc-8ccc-cccccccccccc"]}');
                   INSERT INTO doc_mount_file_links VALUES ('fl-1','f-1','r-1','properties.json');"#,
            )
            .unwrap();
        (main, mount)
    }

    fn tiers(
        main: &Connection,
        mount: &Connection,
        chat: &str,
        character: &str,
        operator: bool,
    ) -> (SharedWardrobeTiers, Vec<String>) {
        crate::test_support::captured_with(|| {
            resolve_shared_wardrobe_tiers_for_chat(
                main,
                mount,
                chat,
                character,
                SharedWardrobeTierOptions { operator },
            )
        })
    }

    #[test]
    fn the_project_tier_is_withheld_off_roster_with_v4s_debug() {
        let (main, mount) = fixture();
        let (t, lines) = tiers(&main, &mount, "chat-1", "c-off", false);
        assert!(t.project_mount_point_ids.is_empty(), "{t:?}");
        let dbg = lines
            .iter()
            .find(|l| {
                l.contains("[Wardrobe] Character off project roster — project wardrobe withheld")
            })
            .unwrap_or_else(|| panic!("no withheld DEBUG: {lines:?}"));
        assert!(
            dbg.starts_with("DEBUG quilltap_core::wardrobe_tiers"),
            "{dbg}"
        );
        assert!(
            dbg.contains("chatId=chat-1 projectId=p-1 characterId=c-off"),
            "{dbg}"
        );
        // The chokepoint's own line precedes it.
        assert!(
            lines
                .iter()
                .any(|l| l.contains("[ProjectRoster] Tool access check")
                    && l.contains("allowed=false")),
            "{lines:?}"
        );
    }

    #[test]
    fn the_project_tier_is_served_on_roster_and_to_the_operator_off_roster() {
        let (main, mount) = fixture();
        let (on, lines) = tiers(
            &main,
            &mount,
            "chat-1",
            "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            false,
        );
        assert_eq!(on.project_mount_point_ids, vec!["r-1".to_string()]);
        assert!(
            !lines.iter().any(|l| l.contains("wardrobe withheld")),
            "{lines:?}"
        );

        // The operator dressing an OFF-roster character: the project as-is, and
        // NO chokepoint line at all — the policy is never consulted.
        let (op, lines) = tiers(&main, &mount, "chat-1", "c-off", true);
        assert_eq!(op.project_mount_point_ids, vec!["r-1".to_string()]);
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("[ProjectRoster]") || l.contains("[Wardrobe]")),
            "{lines:?}"
        );
    }

    #[test]
    fn an_empty_chat_id_a_project_less_chat_and_a_missing_chat_answer_empty_silently() {
        let (main, mount) = fixture();
        for chat in ["", "chat-np", "chat-ghost"] {
            let (t, lines) = tiers(&main, &mount, chat, "c-off", false);
            assert!(t.project_mount_point_ids.is_empty(), "{chat}: {t:?}");
            assert!(
                !lines.iter().any(|l| l.contains("[ProjectRoster]")
                    || l.contains("[Wardrobe]")
                    || l.starts_with("ERROR ")),
                "{chat}: {lines:?}"
            );
        }
    }

    #[test]
    fn an_empty_character_admits_as_v4s_truthiness_does() {
        // `rosterGatedProjectId(projectId, '')` → `projectRosterAdmits` returns
        // true on the falsy character → the project id passes through.
        let (main, mount) = fixture();
        let (t, lines) = tiers(&main, &mount, "chat-1", "", false);
        assert_eq!(t.project_mount_point_ids, vec!["r-1".to_string()]);
        assert!(
            !lines.iter().any(|l| l.contains("[ProjectRoster]")),
            "{lines:?}"
        );
    }

    #[test]
    fn a_failing_chat_read_logs_the_fallback_home_line_and_answers_empty() {
        // §D5: v4's `[Wardrobe] Project lookup for chat failed` WARN sits behind
        // `chats._findById`'s fallback and can never fire; the reachable line is
        // the repository's own `Error finding entity by ID {collection: chats}`.
        let main = Connection::open_in_memory().unwrap(); // no `chats` table
        let (_, mount) = fixture();
        let (t, lines) = tiers(&main, &mount, "chat-1", "c-off", false);
        assert!(t.project_mount_point_ids.is_empty(), "{t:?}");
        assert!(
            lines.iter().any(|l| l.starts_with(
                "ERROR quilltap::db Error finding entity by ID collection=chats id=chat-1"
            )),
            "{lines:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Project lookup for chat failed")),
            "the v4-unreachable WARN is not ported: {lines:?}"
        );
    }
}
