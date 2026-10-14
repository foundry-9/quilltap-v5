//! Port of v4 `lib/memory/time/calendar-refresh.ts` plus `calendar.ts`'s
//! instance-zone cache (`c2795733f`, bug 212) — the ONE stateful piece of the
//! memory calendar: read the instance timezone, cache it for five minutes.
//!
//! v4's why, carried: kept apart from the calendar so the calendar math stays
//! I/O-free. Called at the async entry points that resolve event time or day
//! references; the resolvers then take the zone it returns. Never fails — a
//! failed read leaves the previous value (or the host fallback) in force,
//! because recall must degrade, never block.
//!
//! v5 shape (work order P4.D289 LR-3 / LR-4 / LR-5):
//! - the cache is v4's process-global (`INSTANCE_TIMEZONE_TTL_MS`, stale when
//!   never read or `now − cachedAt ≥ TTL`) — one process serves one instance
//!   in every v5 host, as in v4;
//! - v4's env (`QUILLTAP_TIMEZONE`) and system rungs are the caller's
//!   `host_fallback` (the host's calendar NAME), so the effective zone is
//!   `memory_time_zone(cached, host_fallback)`;
//! - the read is `SELECT "timezone" FROM "chat_settings"` in rowid order (v4
//!   `collection.find({})`'s order) through `db::fallback::find_all_or_empty`,
//!   whose `Error finding all entities {collection: chat_settings}` line is
//!   v4's `_findAll` catch. v4 additionally drops a row its whole-row
//!   `ChatSettingsSchema` refuses; a one-column read does not re-run that
//!   schema — the ruled divergence `MEMORY_ZONE_ROW_UNVALIDATED`.
//! - v4's `Could not read the instance timezone; using fallback` DEBUG is
//!   UNREACHABLE through v4's real repository (two nested `safeQuery`
//!   fallbacks answer `[]` outside the strict-repository scope, which no
//!   refresh caller enters); it is pinned negative, its v5 home being the
//!   `find_all_or_empty` line above.

use std::sync::Mutex;

use crate::db::runtime::Db;
use crate::db::DbError;
use crate::memory_time::calendar::{is_valid_time_zone, memory_time_zone};

/// How long a read of the instance timezone is trusted before re-reading.
pub const INSTANCE_TIMEZONE_TTL_MS: f64 = 5.0 * 60.0 * 1000.0;

struct ZoneCache {
    instance_time_zone: Option<String>,
    cached_at_ms: f64,
}

static CACHE: Mutex<ZoneCache> = Mutex::new(ZoneCache {
    instance_time_zone: None,
    cached_at_ms: 0.0,
});

fn cache() -> std::sync::MutexGuard<'static, ZoneCache> {
    CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Test-only: forget the cached instance timezone (v4
/// `resetMemoryTimeZoneCacheForTests`). Every test that touches the cache
/// serialises on one test-binary mutex.
#[doc(hidden)]
pub fn reset_memory_time_zone_cache_for_tests() {
    let mut c = cache();
    c.instance_time_zone = None;
    c.cached_at_ms = 0.0;
}

/// The effective memory zone from the cache alone — v4 `getMemoryTimeZone()`
/// called without an argument (the cached instance zone, else the host
/// fallback, else `"UTC"`). No read.
pub fn cached_memory_time_zone(host_fallback: &str) -> String {
    let c = cache();
    memory_time_zone(c.instance_time_zone.as_deref(), host_fallback)
}

/// Every `chat_settings.timezone` cell in rowid order (v4's `findAll` order).
fn read_instance_time_zones(db: &Db) -> Result<Vec<Option<String>>, DbError> {
    db.read_main(|conn| {
        let mut stmt = conn.prepare(r#"SELECT "timezone" FROM "chat_settings""#)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, Option<String>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// Re-read the instance timezone when the cached copy is stale, and return
/// the effective memory timezone (v4 `refreshMemoryTimeZone(nowMs)`).
///
/// A fresh cache answers without a read. Otherwise the first VALID
/// `chat_settings.timezone` (rowid order) becomes the cached instance zone —
/// `None` when no row carries one — stamped at `now_ms`, and the DEBUG
/// `[MemoryCalendar] Instance timezone refreshed {instanceTimeZone,
/// effective}` is logged on the CALLER's thread (a pool read, never a writer
/// closure). A failed read logs `Error finding all entities` and caches
/// `None` exactly as v4's repository fallback (`[]`) does.
pub fn refresh_memory_time_zone(db: &Db, host_fallback: &str, now_ms: f64) -> String {
    {
        let c = cache();
        let stale = c.cached_at_ms == 0.0 || now_ms - c.cached_at_ms >= INSTANCE_TIMEZONE_TTL_MS;
        if !stale {
            return memory_time_zone(c.instance_time_zone.as_deref(), host_fallback);
        }
    }
    let all =
        crate::db::fallback::find_all_or_empty("chat_settings", || read_instance_time_zones(db));
    let configured = all
        .into_iter()
        .flatten()
        .find(|tz| is_valid_time_zone(Some(tz)));
    let effective = {
        let mut c = cache();
        c.instance_time_zone = configured.clone();
        c.cached_at_ms = now_ms;
        memory_time_zone(c.instance_time_zone.as_deref(), host_fallback)
    };
    tracing::debug!(
        instanceTimeZone = configured.as_deref().unwrap_or("null"),
        effective = effective.as_str(),
        "[MemoryCalendar] Instance timezone refreshed"
    );
    effective
}
