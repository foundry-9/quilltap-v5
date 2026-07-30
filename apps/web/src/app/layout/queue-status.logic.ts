/**
 * Pure logic for the queue-status badges (v4
 * `components/layout/queue-status-badges.tsx:22-84,175-179`) — the bucket
 * table, the zero check, and the per-queue sum, extracted for unit tests (the
 * `autonomous.logic.ts` precedent).
 */

/** v4 `QUEUE_CHANGE_EVENT` — the same DOM CustomEvent name, kept deliberately
 *  (v4's mechanism is a window event so ANY code can fire it without a
 *  dependency on the badges; the same holds in v5, so the mechanism carries —
 *  recorded as the order's "match v4's mechanism unless Angular idiom forbids"
 *  decision). */
export const QUEUE_CHANGE_EVENT = 'quilltap:queue-change';

/**
 * v4 `notifyQueueChange()` — call after any action that enqueues background
 * jobs; the badges wake up and poll until every count is zero again.
 */
export function notifyQueueChange(): void {
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent(QUEUE_CHANGE_EVENT));
  }
}

export interface QueueType {
  key: string;
  label: string;
  title: string;
  jobTypes: readonly string[];
  badgeClass: string;
}

/** v4 `QUEUE_TYPES` (:37-73) — five buckets, these exact job-type keys. */
export const QUEUE_TYPES: readonly QueueType[] = [
  {
    key: 'memory',
    label: 'Mem',
    title: 'Memory extraction queue',
    jobTypes: [
      'MEMORY_EXTRACTION',
      'INTER_CHARACTER_MEMORY',
      'MEMORY_REGENERATE_CHAT',
      'MEMORY_REGENERATE_ALL',
    ],
    badgeClass: 'qt-queue-badge-memory',
  },
  {
    key: 'embedding',
    label: 'Emb',
    title: 'Embedding queue',
    jobTypes: ['EMBEDDING_GENERATE', 'EMBEDDING_REFIT', 'EMBEDDING_REINDEX_ALL'],
    badgeClass: 'qt-queue-badge-embedding',
  },
  {
    key: 'summary',
    label: 'Sum',
    title: 'Post-turn processing queue (summaries, titles, scene state, rendering)',
    jobTypes: [
      'CONTEXT_SUMMARY',
      'TITLE_UPDATE',
      'SCENE_STATE_TRACKING',
      'CONVERSATION_RENDER',
      'REGENERATE_CONVERSATION_SUMMARIES',
    ],
    badgeClass: 'qt-queue-badge-summary',
  },
  {
    key: 'danger',
    label: 'Dgr',
    title: 'Danger classification queue',
    jobTypes: ['CHAT_DANGER_CLASSIFICATION'],
    badgeClass: 'qt-queue-badge-danger',
  },
  {
    key: 'story',
    label: 'Img',
    title: 'Image generation queue (story backgrounds, character avatars)',
    jobTypes: ['STORY_BACKGROUND_GENERATION', 'CHARACTER_AVATAR_GENERATION'],
    badgeClass: 'qt-queue-badge-story',
  },
];

/** v4 `hasActiveJobs` (:82-84). */
export function hasActiveJobs(activeByType: Record<string, number>): boolean {
  return Object.values(activeByType).some((count) => count > 0);
}

/** v4 `getQueueCount` (:175-179). */
export function getQueueCount(
  activeByType: Record<string, number>,
  jobTypes: readonly string[],
): number {
  return jobTypes.reduce((sum, type) => sum + (activeByType[type] || 0), 0);
}
