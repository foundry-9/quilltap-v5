/**
 * The project-detail card expansion memory (v4
 * `app/prospero/[id]/hooks/useProjectCardState.ts`): on the FIRST visit to a
 * project all cards are expanded; on every subsequent visit they start
 * collapsed. First-ness is recorded in localStorage under
 * `quilltap_project_visited_{id}`.
 */

const STORAGE_KEY_PREFIX = 'quilltap_project_visited_';

/**
 * Resolve whether this is the project's first visit and stamp it visited. SSR /
 * non-DOM contexts report a first visit (v4 parity) without touching storage.
 * The stamp is written on first read so the NEXT visit collapses.
 */
export function resolveFirstVisit(projectId: string): boolean {
  if (typeof window === 'undefined' || !window.localStorage) {
    return true;
  }
  const key = `${STORAGE_KEY_PREFIX}${projectId}`;
  const hasVisited = window.localStorage.getItem(key);
  if (hasVisited) {
    return false;
  }
  window.localStorage.setItem(key, 'true');
  return true;
}
