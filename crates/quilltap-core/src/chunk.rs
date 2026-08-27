//! v4 `lib/utils/chunk.ts` (`805ef12bf`) — the bind-variable budget for chunked
//! SQLite `IN (…)` queries, plus the order-preserving splitter both memory
//! deletion sites feed it to.
//!
//! One home, two consumers, exactly as v4 has it: the doomed-id → character
//! resolve in [`crate::db::memories::MemoriesRepository::delete_many_with_unlink`]
//! and the `characterId`-scoped
//! [`crate::db::memories::MemoriesRepository::bulk_delete`]. Both build a single
//! `IN (…)` list with one bind variable per id, and a full-wipe restore or a
//! large character cascade on an instance with tens of thousands of memories
//! passes more ids than SQLite will accept in one statement.

/// Bind-variable budget for chunked SQLite `IN (…)` queries. The engine's
/// compile-time cap (`SQLITE_MAX_VARIABLE_NUMBER`) is 999 in older builds and
/// 32766 in current ones; 900 stays safely under both, so a chunked query never
/// throws "too many SQL variables" regardless of the linked binary.
///
/// v4's constant, verbatim (`lib/utils/chunk.ts:7`). SQLite3MC is the same
/// engine underneath, so the ceiling — and the budget — are the same number.
pub const SQLITE_VARIABLE_CHUNK_SIZE: usize = 900;

/// Split `items` into consecutive chunks of at most `size` elements, preserving
/// order. An empty input yields no chunks.
///
/// v4's `chunkArray` throws `chunkArray size must be a positive integer, got
/// ${size}` on a non-positive **or non-integer** size. Only the first half of
/// that guard is expressible here — `size` is a `usize`, so "non-integer" is
/// unreachable by construction — and the message is kept byte-for-byte for the
/// half that is, since it is the same developer-facing contract.
///
/// v4 returns freshly-sliced arrays; this returns borrowed sub-slices, which is
/// the same sequence with no copy. Both consumers only ever read.
pub fn chunk_array<T>(items: &[T], size: usize) -> std::slice::Chunks<'_, T> {
    assert!(
        size >= 1,
        "chunkArray size must be a positive integer, got {size}"
    );
    items.chunks(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4 `__tests__/unit/lib/utils/chunk.test.ts`: order is preserved and the
    /// tail chunk is short.
    #[test]
    fn splits_in_order_with_a_short_tail() {
        let items: Vec<usize> = (0..7).collect();
        let chunks: Vec<&[usize]> = chunk_array(&items, 3).collect();
        assert_eq!(chunks, vec![&[0, 1, 2][..], &[3, 4, 5][..], &[6][..]]);
    }

    #[test]
    fn an_exact_multiple_has_no_short_tail() {
        let items: Vec<usize> = (0..6).collect();
        let chunks: Vec<&[usize]> = chunk_array(&items, 3).collect();
        assert_eq!(chunks, vec![&[0, 1, 2][..], &[3, 4, 5][..]]);
    }

    /// "An empty input yields no chunks" — NOT one empty chunk.
    #[test]
    fn empty_input_yields_no_chunks() {
        let items: Vec<usize> = Vec::new();
        assert_eq!(chunk_array(&items, 3).count(), 0);
    }

    #[test]
    fn a_size_larger_than_the_input_yields_one_chunk() {
        let items: Vec<usize> = (0..3).collect();
        let chunks: Vec<&[usize]> = chunk_array(&items, 900).collect();
        assert_eq!(chunks, vec![&[0, 1, 2][..]]);
    }

    /// The 2,000-id shape v4's tests pin at both call sites: 900 / 900 / 200.
    #[test]
    fn two_thousand_ids_split_at_the_sqlite_budget() {
        let items: Vec<usize> = (0..2_000).collect();
        let sizes: Vec<usize> = chunk_array(&items, SQLITE_VARIABLE_CHUNK_SIZE)
            .map(<[usize]>::len)
            .collect();
        assert_eq!(sizes, vec![900, 900, 200]);
        // …and nothing is lost or reordered across the boundaries.
        let flat: Vec<usize> = chunk_array(&items, SQLITE_VARIABLE_CHUNK_SIZE)
            .flatten()
            .copied()
            .collect();
        assert_eq!(flat, items);
    }

    #[test]
    fn the_budget_is_v4s_constant() {
        assert_eq!(SQLITE_VARIABLE_CHUNK_SIZE, 900);
    }

    #[test]
    #[should_panic(expected = "chunkArray size must be a positive integer, got 0")]
    fn a_zero_size_is_refused_with_v4s_sentence() {
        let items: Vec<usize> = (0..3).collect();
        let _ = chunk_array(&items, 0);
    }
}
