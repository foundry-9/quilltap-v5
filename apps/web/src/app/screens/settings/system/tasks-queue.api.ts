import type { CoreClient } from '../../../core/core-client';

/**
 * The Tasks Queue client surface (v4 `components/tools/tasks-queue/`): the DTOs
 * the card renders plus thin wrappers over the six §1 verbs P4.9G1 delivers.
 * Response bodies are read defensively (`dispatchData`) — their `type` strings
 * are not load-bearing (the settings/home precedent).
 *
 * @module screens/settings/system/tasks-queue.api
 */

/** v4 `types.ts:1-9` — `activeTotal` is computed server-side (`activeJobs.length`). */
export interface QueueStats {
  pending: number;
  processing: number;
  failed: number;
  completed: number;
  dead: number;
  paused: number;
  activeTotal: number;
}

/**
 * v4 `types.ts:11-16`. The card reads only `.running`; the real server emits
 * `{running, processing, inFlight, childCrashed}` (v4's `types.ts` names are
 * stale, `processor-host.ts:332`). Typed loosely — only `running` is used.
 */
export interface ProcessorStatus {
  running: boolean;
  [key: string]: unknown;
}

/** One active-queue row (v4 `types.ts:18-32`). */
export interface JobDetail {
  id: string;
  type: string;
  typeName: string;
  status: 'PENDING' | 'PROCESSING' | 'FAILED' | 'PAUSED' | string;
  priority: number;
  attempts: number;
  maxAttempts: number;
  scheduledAt: string;
  startedAt: string | null;
  lastError: string | null;
  estimatedTokens: number;
  chatId?: string;
  characterName?: string;
}

/** The detail-modal read (v4 `types.ts:34-39`). */
export interface FullJobDetail extends JobDetail {
  payload: Record<string, unknown>;
  createdAt: string;
  updatedAt: string;
  userId: string;
}

/** The `systemTasksQueue` body (v4 `types.ts:41-48`). */
export interface QueueData {
  stats: QueueStats;
  jobs: JobDetail[];
  totalEstimatedTokens: number;
  processorStatus: ProcessorStatus;
  maxConcurrentJobs: number;
}

/** v4 `index.tsx:50-58` / `TaskItem.tsx:24-32` — 1.2M / 3.4K / 500. */
export function formatTokens(t: number): string {
  if (t >= 1_000_000) return `${(t / 1_000_000).toFixed(1)}M`;
  if (t >= 1000) return `${(t / 1000).toFixed(1)}K`;
  return t.toString();
}

/** v4 `formatRelativeDate` (`lib/format-time.ts:93-114`) — for `scheduledAt`. */
export function formatRelativeDate(dateString: string): string {
  if (!dateString) return '';
  try {
    const diffMins = Math.floor((Date.now() - new Date(dateString).getTime()) / 60000);
    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffMins < 1440) return `${Math.floor(diffMins / 60)}h ago`;
    return new Date(dateString).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return String(dateString);
  }
}

export const tasksQueueKeys = {
  all: ['tasksQueue'] as const,
};

export async function fetchTasksQueue(core: CoreClient): Promise<QueueData> {
  const data = await core.dispatchData({ type: 'systemTasksQueue' });
  return data as unknown as QueueData;
}

export async function controlTasksQueue(core: CoreClient, action: 'start' | 'stop'): Promise<void> {
  await core.dispatchData({ type: 'systemTasksQueueControl', action });
}

export async function setJobConcurrency(core: CoreClient, maxConcurrentJobs: number): Promise<void> {
  await core.dispatchData({ type: 'systemJobConcurrencySet', maxConcurrentJobs });
}

export async function fetchJob(core: CoreClient, jobId: string): Promise<FullJobDetail> {
  const data = await core.dispatchData({ type: 'systemJobGet', jobId });
  return (data as { job: FullJobDetail }).job;
}

export async function controlJob(
  core: CoreClient,
  jobId: string,
  action: 'pause' | 'resume',
): Promise<void> {
  await core.dispatchData({ type: 'systemJobControl', jobId, action });
}

export async function deleteJob(core: CoreClient, jobId: string): Promise<void> {
  await core.dispatchData({ type: 'systemJobDelete', jobId });
}
