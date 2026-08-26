//! Activity kinds — the single source of truth behind the toolbar chips
//! (v4 `lib/background-jobs/activity-kinds.ts`, `664cfca84`).
//!
//! The chips in the page toolbar ("Mem", "Emb", "Sum", "Dgr", "Img") report how
//! much work of each kind is in flight. Two very different things feed them:
//!
//!   1. Rows in `background_jobs` with status PENDING/PROCESSING, mapped to a
//!      kind by [`JOB_TYPE_ACTIVITY`].
//!   2. Non-job work registered with the in-process activity registry
//!      ([`crate::services::activity_registry`]) — the inline image tool, the
//!      Concierge classifier, embedding calls made straight from a request.
//!
//! `JOB_TYPE_ACTIVITY` is **total** on purpose: adding a member to v4's
//! `BackgroundJobTypeEnum` without deciding which chip it belongs to is a type
//! error in v4 rather than a silently invisible queue. Deliberate omissions are
//! spelled `None`, not left out. v5's job types are STRINGS (the enqueue gate
//! `api::system_data::JOB_TYPES`), so the totality property is mechanical
//! instead of type-level: [`tests::job_type_activity_is_total`] asserts this
//! table's key set equals that gate list exactly, in both directions.
//!
//! **`ACTIVITY_CHIPS` does NOT port here.** v4's chip metadata (label, title,
//! `badgeClass`, render order) is client-only display data; the Angular SPA
//! transcribes it (P4.D125). The five kind ids below are the whole
//! server↔client join.

use serde_json::{Map, Value};

/// The kinds of work a toolbar chip can report (v4 `ACTIVITY_KINDS`).
///
/// The variant order IS the wire order of `activeByKind` / `startedByKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActivityKind {
    Memory,
    Embedding,
    Summary,
    Danger,
    Image,
}

/// Every kind, in render/wire order (v4 `ACTIVITY_KINDS`).
pub const ACTIVITY_KINDS: [ActivityKind; 5] = [
    ActivityKind::Memory,
    ActivityKind::Embedding,
    ActivityKind::Summary,
    ActivityKind::Danger,
    ActivityKind::Image,
];

impl ActivityKind {
    /// The wire id — the server↔client join (v4's string union members).
    pub const fn as_str(self) -> &'static str {
        match self {
            ActivityKind::Memory => "memory",
            ActivityKind::Embedding => "embedding",
            ActivityKind::Summary => "summary",
            ActivityKind::Danger => "danger",
            ActivityKind::Image => "image",
        }
    }

    /// Position in [`ACTIVITY_KINDS`] — the counter-array index.
    pub const fn index(self) -> usize {
        match self {
            ActivityKind::Memory => 0,
            ActivityKind::Embedding => 1,
            ActivityKind::Summary => 2,
            ActivityKind::Danger => 3,
            ActivityKind::Image => 4,
        }
    }
}

/// Which chip each background-job type counts toward (v4 `JOB_TYPE_ACTIVITY`).
///
/// `None` means "deliberately uncounted here": either the work is pure
/// maintenance the user never waits on, or it already has a richer readout of
/// its own (autonomous rooms have their own badges). Transcribed from
/// `664cfca84:lib/background-jobs/activity-kinds.ts`; pinned byte-for-byte by
/// the `activity_tables_equivalence` differential.
pub const JOB_TYPE_ACTIVITY: &[(&str, Option<ActivityKind>)] = &[
    // ── Mem ──────────────────────────────────────────────────────────────────
    ("MEMORY_EXTRACTION", Some(ActivityKind::Memory)),
    ("INTER_CHARACTER_MEMORY", Some(ActivityKind::Memory)),
    ("MEMORY_REGENERATE_CHAT", Some(ActivityKind::Memory)),
    ("MEMORY_REGENERATE_ALL", Some(ActivityKind::Memory)),
    ("MEMORY_HOUSEKEEPING", Some(ActivityKind::Memory)),
    ("CARINA_MEMORY_EXTRACTION", Some(ActivityKind::Memory)),
    // ── Emb ──────────────────────────────────────────────────────────────────
    ("EMBEDDING_GENERATE", Some(ActivityKind::Embedding)),
    ("EMBEDDING_REFIT", Some(ActivityKind::Embedding)),
    ("EMBEDDING_REINDEX_ALL", Some(ActivityKind::Embedding)),
    ("EMBEDDING_REAPPLY_PROFILE", Some(ActivityKind::Embedding)),
    // ── Sum ──────────────────────────────────────────────────────────────────
    ("CONTEXT_SUMMARY", Some(ActivityKind::Summary)),
    ("TITLE_UPDATE", Some(ActivityKind::Summary)),
    ("SCENE_STATE_TRACKING", Some(ActivityKind::Summary)),
    ("CONVERSATION_RENDER", Some(ActivityKind::Summary)),
    (
        "REGENERATE_CONVERSATION_SUMMARIES",
        Some(ActivityKind::Summary),
    ),
    ("WARDROBE_OUTFIT_ANNOUNCEMENT", Some(ActivityKind::Summary)),
    // ── Dgr ──────────────────────────────────────────────────────────────────
    ("CHAT_DANGER_CLASSIFICATION", Some(ActivityKind::Danger)),
    // ── Img ──────────────────────────────────────────────────────────────────
    ("STORY_BACKGROUND_GENERATION", Some(ActivityKind::Image)),
    ("CHARACTER_AVATAR_GENERATION", Some(ActivityKind::Image)),
    (
        "CHARACTER_HEADSHOULDERS_BACKFILL",
        Some(ActivityKind::Image),
    ),
    // ── Deliberately uncounted ───────────────────────────────────────────────
    // Housekeeping the user never waits on.
    ("LLM_LOG_CLEANUP", None),
    // Autonomous rooms report through their own toolbar badges.
    ("AUTONOMOUS_ROOM_TURN", None),
    ("AUTONOMOUS_ROOM_SCHEDULE_TICK", None),
];

/// Which chip a job type counts toward, tolerant of unknown strings — a row
/// written by a newer build than the one reading it (v4
/// `activityKindForJobType`, whose `?? null` is this `None`).
pub fn activity_kind_for_job_type(job_type: &str) -> Option<ActivityKind> {
    JOB_TYPE_ACTIVITY
        .iter()
        .find(|(t, _)| *t == job_type)
        .and_then(|(_, kind)| *kind)
}

/// Per-kind counters — v4's `Record<ActivityKind, number>` (`emptyActivityCounts`
/// is [`ActivityCounts::default`]).
///
/// The field order IS the wire key order (§Shared contract §A.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActivityCounts {
    pub memory: i64,
    pub embedding: i64,
    pub summary: i64,
    pub danger: i64,
    pub image: i64,
}

impl ActivityCounts {
    /// The count for one kind.
    pub fn get(&self, kind: ActivityKind) -> i64 {
        match kind {
            ActivityKind::Memory => self.memory,
            ActivityKind::Embedding => self.embedding,
            ActivityKind::Summary => self.summary,
            ActivityKind::Danger => self.danger,
            ActivityKind::Image => self.image,
        }
    }

    /// Set the count for one kind.
    pub fn set(&mut self, kind: ActivityKind, value: i64) {
        match kind {
            ActivityKind::Memory => self.memory = value,
            ActivityKind::Embedding => self.embedding = value,
            ActivityKind::Summary => self.summary = value,
            ActivityKind::Danger => self.danger = value,
            ActivityKind::Image => self.image = value,
        }
    }

    /// The JSON object v4's route puts on the wire: exactly the five keys, in
    /// kind order, integer values.
    pub fn to_json(self) -> Value {
        let mut m = Map::new();
        for kind in ACTIVITY_KINDS {
            m.insert(kind.as_str().to_string(), Value::from(self.get(kind)));
        }
        Value::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// v4's totality property, mechanically: the table assigns EVERY job type
    /// the enqueue gate accepts, and maps no type beyond it. v4 gets this from
    /// `Record<BackgroundJobType, …>`; v5's types are strings, so this test is
    /// the twin of v4's own `activity-registry.test.ts` pair
    /// ("assigns every background job type…" / "maps no job types beyond the
    /// enum").
    #[test]
    fn job_type_activity_is_total() {
        let gate: BTreeSet<&str> = crate::api::system_data::JOB_TYPES.iter().copied().collect();
        let table: BTreeSet<&str> = JOB_TYPE_ACTIVITY.iter().map(|(t, _)| *t).collect();
        assert_eq!(
            table, gate,
            "JOB_TYPE_ACTIVITY must assign exactly the enqueue gate's job types"
        );
        assert_eq!(
            JOB_TYPE_ACTIVITY.len(),
            gate.len(),
            "JOB_TYPE_ACTIVITY has a duplicate key"
        );
    }

    #[test]
    fn unknown_job_type_is_uncounted_rather_than_a_panic() {
        assert_eq!(activity_kind_for_job_type("SOME_FUTURE_JOB"), None);
    }

    #[test]
    fn every_image_job_type_counts_under_the_image_chip() {
        for t in [
            "STORY_BACKGROUND_GENERATION",
            "CHARACTER_AVATAR_GENERATION",
            "CHARACTER_HEADSHOULDERS_BACKFILL",
        ] {
            assert_eq!(activity_kind_for_job_type(t), Some(ActivityKind::Image));
        }
    }

    #[test]
    fn deliberate_omissions_are_spelled_none() {
        for t in [
            "LLM_LOG_CLEANUP",
            "AUTONOMOUS_ROOM_TURN",
            "AUTONOMOUS_ROOM_SCHEDULE_TICK",
        ] {
            assert!(JOB_TYPE_ACTIVITY
                .iter()
                .any(|(k, v)| *k == t && v.is_none()));
        }
    }

    #[test]
    fn counts_serialize_with_exactly_the_five_keys_in_kind_order() {
        let counts = ActivityCounts {
            memory: 1,
            embedding: 2,
            summary: 3,
            danger: 4,
            image: 5,
        };
        assert_eq!(
            counts.to_json().to_string(),
            r#"{"memory":1,"embedding":2,"summary":3,"danger":4,"image":5}"#
        );
    }
}
