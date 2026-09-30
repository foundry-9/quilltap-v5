//! v4's FALLBACK repository reads, as their callers see them — the ONE home
//! for the shape (unified at the `97b25fc53` follow-ups round; before it the
//! same two lines lived in four hand-copies across `chats_read`,
//! `characters_read`, `image_profiles` and `dangerous_content::understudy`).
//!
//! v4 `BaseRepository._findById` is `safeQuery(…, 'Error finding entity by ID',
//! { id }, null)` and `_findAll` is `safeQuery(…, 'Error finding all
//! entities', {}, [])` (`base.repository.ts:247-277`): a FAILED read logs
//! that ERROR with the repository's `collection` injected first
//! (`safeQuery`'s `enrichedContext`, `:101`) and the error's message, and
//! answers the fallback — it never throws. `getCollection()` runs INSIDE the
//! same `safeQuery`, so a caller that hands `read` the whole pool checkout
//! (`db.read_main(…)`) gets v4's line for a pool failure too, once.
//!
//! `target: "quilltap::db"` is v4's `Repository` logger module as the
//! differentials map it (`danger_routing_equivalence`'s `Repository →
//! quilltap::db`); every emitter of these two lines uses it.

use super::DbError;

/// v4 `_findById` as its callers see it: `read()`'s `Err` logs `Error finding
/// entity by ID {collection, id, error}` and answers `None`.
pub fn find_by_id_or_none<T>(
    collection: &'static str,
    id: &str,
    read: impl FnOnce() -> Result<Option<T>, DbError>,
) -> Option<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = collection,
            id = %id,
            error = %error,
            "Error finding entity by ID"
        );
        None
    })
}

/// v4 `_findAll` as its callers see it: `read()`'s `Err` logs `Error finding
/// all entities {collection, error}` and answers `[]`.
pub fn find_all_or_empty<T>(
    collection: &'static str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Vec<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = collection,
            error = %error,
            "Error finding all entities"
        );
        Vec::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_read_logs_v4s_line_and_answers_the_fallback() {
        let (got, lines) = crate::test_support::captured_with(|| {
            find_by_id_or_none::<i32>("widgets", "w-1", || Err(DbError::Internal("posed".into())))
        });
        assert_eq!(got, None);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error finding entity by ID collection=widgets id=w-1 error="
            ),
            "{}",
            lines[0]
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            find_all_or_empty::<i32>("widgets", || Err(DbError::Internal("posed".into())))
        });
        assert!(got.is_empty());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error finding all entities collection=widgets error="
            ),
            "{}",
            lines[0]
        );
    }

    #[test]
    fn a_successful_read_logs_nothing() {
        let (got, lines) = crate::test_support::captured_with(|| {
            find_by_id_or_none("widgets", "w-1", || Ok(Some(7)))
        });
        assert_eq!(got, Some(7));
        assert!(lines.is_empty(), "{lines:?}");
        let (got, lines) =
            crate::test_support::captured_with(|| find_all_or_empty("widgets", || Ok(vec![1, 2])));
        assert_eq!(got, vec![1, 2]);
        assert!(lines.is_empty(), "{lines:?}");
    }
}
