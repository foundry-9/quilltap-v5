/**
 * Activity kinds — the client half of v4 `lib/background-jobs/activity-kinds.ts`
 * (`664cfca84`).
 *
 * The chips in the page toolbar ("Mem", "Emb", "Sum", "Dgr", "Img") report how
 * much work of each kind is in flight. Two very different things feed them:
 * rows in `background_jobs` with status PENDING/PROCESSING, and non-job work
 * registered with the in-process activity registry (the inline image tool, the
 * Concierge classifier, embedding calls made straight from a request).
 *
 * **What v5 transcribes, and what it does not.** v4's module is shared
 * client/server, so it also carries `JOB_TYPE_ACTIVITY` — the total
 * `Record<BackgroundJobType, ActivityKind | null>` that maps a job row onto a
 * chip. In v5 that mapping lives in Rust (the P4.D123 server lane); the five
 * kind ids below are the whole server↔client join (§Shared contract §A.4), and
 * the labels, titles and CSS classes are client-side. Transcribing the job-type
 * table here would be a second copy that could only ever drift.
 *
 * @module layout/activity-kinds
 */

/** The kinds of work a toolbar chip can report — the §A.2 key order, exactly. */
export const ACTIVITY_KINDS = ['memory', 'embedding', 'summary', 'danger', 'image'] as const;

export type ActivityKind = (typeof ACTIVITY_KINDS)[number];

export interface ActivityChip {
  readonly kind: ActivityKind;
  readonly label: string;
  readonly title: string;
  readonly badgeClass: string;
}

/**
 * Display metadata for the toolbar chips, in render order — transcribed
 * verbatim from v4's `ACTIVITY_CHIPS`, quirk included: the `image` chip's class
 * is `qt-queue-badge-story`, not `-image`, because the CSS predates the rename
 * from "story backgrounds" to image work generally.
 */
export const ACTIVITY_CHIPS: readonly ActivityChip[] = [
  {
    kind: 'memory',
    label: 'Mem',
    title: 'Memory work (extraction, regeneration, housekeeping)',
    badgeClass: 'qt-queue-badge-memory',
  },
  {
    kind: 'embedding',
    label: 'Emb',
    title: 'Embedding work (indexing, refits, live query embeddings)',
    badgeClass: 'qt-queue-badge-embedding',
  },
  {
    kind: 'summary',
    label: 'Sum',
    title:
      'Summarization and post-turn processing (summaries, titles, scene state, rendering)',
    badgeClass: 'qt-queue-badge-summary',
  },
  {
    kind: 'danger',
    label: 'Dgr',
    title: 'the Concierge classification (per-message and chat-level)',
    badgeClass: 'qt-queue-badge-danger',
  },
  {
    kind: 'image',
    label: 'Img',
    title: 'Image work, end to end (prompt crafting, generation, landing)',
    badgeClass: 'qt-queue-badge-story',
  },
] as const;

/** One counter per kind (v4 `Record<ActivityKind, number>`). */
export type ActivityCounts = Record<ActivityKind, number>;

/** Empty counter map, one zeroed entry per kind (v4 `emptyActivityCounts`). */
export function emptyActivityCounts(): ActivityCounts {
  return { memory: 0, embedding: 0, summary: 0, danger: 0, image: 0 };
}

/**
 * v4 `coerceCounts` — read a count map defensively.
 *
 * Tolerant of an absent or unknown-shaped map on purpose: the chips must render
 * zeros against a server older than the round that started sending
 * `activeByKind`, rather than blanking or throwing.
 */
export function coerceCounts(raw: unknown): ActivityCounts {
  const out = emptyActivityCounts();
  if (!raw || typeof raw !== 'object') return out;
  for (const kind of ACTIVITY_KINDS) {
    const value = (raw as Record<string, unknown>)[kind];
    if (typeof value === 'number' && Number.isFinite(value)) {
      out[kind] = value;
    }
  }
  return out;
}

/** v4 `hasActivity` — is anything in flight at all? Drives the poll cadence. */
export function hasActivity(counts: ActivityCounts): boolean {
  return ACTIVITY_KINDS.some((kind) => counts[kind] > 0);
}

/**
 * v4's pulse rule, extracted so it can be unit-tested on its own.
 *
 * `startedByKind` is a monotonic per-kind total of ended spans since the server
 * booted. A kind whose counter ADVANCED did work between two reads — pulse it
 * even though the work has already finished. The counter resets when the server
 * restarts, so a DECREASE is a fresh baseline rather than a blip, and the very
 * first read is a delta base with nothing to compare against.
 *
 * @param previous The previous read, or `null` on the first one.
 * @returns The kinds that advanced; empty on a first read or a reset.
 */
export function blippedKinds(
  previous: ActivityCounts | null,
  started: ActivityCounts,
): ActivityKind[] {
  if (!previous) return [];
  return ACTIVITY_KINDS.filter((kind) => started[kind] > previous[kind]);
}
