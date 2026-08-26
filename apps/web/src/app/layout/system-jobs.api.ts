/**
 * The activity snapshot the toolbar chips read (v4 `GET /api/v1/system/jobs`).
 *
 * §Shared contract §A: the route ALWAYS answers `activeByKind` and
 * `startedByKind` — objects with exactly the five kind keys, integer values ≥ 0
 * — alongside the unchanged `stats` and `processor`. `activeByType` became
 * opt-in behind `?includeByType=true` in the same commit; the chips never ask
 * for it.
 *
 * Kept on the raw REST route rather than a dispatch verb because §A pins the
 * route, and because that is the touchpoint the chips have always used. The
 * P4.D123 server lane owns the shape on the other side.
 *
 * @module layout/system-jobs.api
 */

import { apiUrl } from '../core/api-url';

/** v4 `queryKeys.system.jobs` — the key the `jobs` realtime topic invalidates. */
export const systemJobsKeys = {
  all: ['systemJobs'] as const,
};

/** The half of the body the chips read (v4's local `ActivityResponse`). */
export interface ActivitySnapshotResponse {
  activeByKind?: unknown;
  startedByKind?: unknown;
}

/**
 * Read the snapshot. Throws on a non-2xx so TanStack keeps the last good
 * value — v4's chips set `retry: false` and lean on the same behavior ("keep
 * the last good snapshot on a transient error rather than blanking every
 * chip").
 */
export async function fetchActivitySnapshot(
  signal?: AbortSignal,
): Promise<ActivitySnapshotResponse> {
  const res = await fetch(apiUrl('/api/v1/system/jobs'), {
    signal,
    cache: 'no-store',
    headers: { 'Cache-Control': 'no-cache, no-store, must-revalidate' },
  });
  if (!res.ok) {
    throw new Error(`system/jobs answered ${res.status}`);
  }
  return (await res.json()) as ActivitySnapshotResponse;
}
