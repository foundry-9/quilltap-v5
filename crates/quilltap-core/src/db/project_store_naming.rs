//! Project document-store naming — port of v4
//! `lib/mount-index/project-store-naming.ts`.
//!
//! A pure leaf over already-fetched store rows: identify a project's *own*
//! auto-created Scriptorium store (named `"Project Files: <name>"`) among a list of
//! linked stores, and pick the primary one. Consumed by the `project_info` tool's
//! `resolveProjectDocumentStore` to summarize the store the Files card shows.
//!
//! Faithful to v4: a database-backed **documents** store whose name matches the
//! migration prefix is preferred; else the first eligible store; else `None`.
//! Filesystem/obsidian mounts and character stores never participate.

/// The name prefix the Stage-1 migration gives every project's auto-created store.
pub const PROJECT_OWN_STORE_NAME_PREFIX: &str = "Project Files: ";

/// v4 `isProjectOwnStoreName` — a name is the project's own store iff it starts with
/// [`PROJECT_OWN_STORE_NAME_PREFIX`]. (v4 also guards `typeof name === 'string'`;
/// here `name` is always a `&str`.)
pub fn is_project_own_store_name(name: &str) -> bool {
    name.starts_with(PROJECT_OWN_STORE_NAME_PREFIX)
}

/// A linked store row, the subset `pickPrimaryProjectStore` reads (v4 `StoreLike`).
///
/// v4's leaf is generic (`<T extends StoreLike>`) and reads only
/// `name`/`mountType`/`storeType`; the returned store's other fields (`id`) are the
/// caller's. We carry `id` alongside so the caller can map the chosen store back —
/// the leaf itself ignores it, staying pure over the three decision fields.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreLike {
    /// Ignored by the pick logic; carried so the caller can resolve the choice.
    pub id: String,
    pub name: String,
    /// `'filesystem' | 'obsidian' | 'database'`.
    pub mount_type: String,
    /// `'documents' | 'character'`; `None` defaults to `'documents'` (v4 `?? 'documents'`).
    pub store_type: Option<String>,
}

/// v4 `pickPrimaryProjectStore` — select the project's "own" document store.
///
/// Eligible = `mountType === 'database'` AND `(storeType ?? 'documents') ===
/// 'documents'`. Among the eligible, prefer the first whose name matches the
/// migration convention; else the first eligible. `None` when nothing is eligible.
pub fn pick_primary_project_store(stores: &[StoreLike]) -> Option<&StoreLike> {
    let eligible: Vec<&StoreLike> = stores
        .iter()
        .filter(|s| {
            s.mount_type == "database"
                && s.store_type.as_deref().unwrap_or("documents") == "documents"
        })
        .collect();
    if eligible.is_empty() {
        return None;
    }
    // `.find(isProjectOwnStoreName) ?? eligible[0]`.
    Some(
        eligible
            .iter()
            .find(|s| is_project_own_store_name(&s.name))
            .copied()
            .unwrap_or(eligible[0]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(id: &str, name: &str, mount_type: &str, store_type: Option<&str>) -> StoreLike {
        StoreLike {
            id: id.to_string(),
            name: name.to_string(),
            mount_type: mount_type.to_string(),
            store_type: store_type.map(str::to_string),
        }
    }

    #[test]
    fn prefers_the_migration_named_store() {
        let stores = [
            store("a", "Hand Linked", "database", Some("documents")),
            store("b", "Project Files: Demo", "database", Some("documents")),
        ];
        assert_eq!(pick_primary_project_store(&stores).unwrap().id, "b");
    }

    #[test]
    fn falls_back_to_first_eligible() {
        let stores = [
            store("a", "Hand Linked", "database", Some("documents")),
            store("b", "Another", "database", None), // storeType absent → documents
        ];
        assert_eq!(pick_primary_project_store(&stores).unwrap().id, "a");
    }

    #[test]
    fn ignores_filesystem_and_character_stores() {
        let stores = [
            store("fs", "Project Files: Demo", "filesystem", Some("documents")),
            store("ch", "Project Files: Demo", "database", Some("character")),
        ];
        assert!(pick_primary_project_store(&stores).is_none());
    }

    #[test]
    fn empty_is_none() {
        assert!(pick_primary_project_store(&[]).is_none());
    }
}
