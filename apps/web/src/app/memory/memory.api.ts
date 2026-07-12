/**
 * The Commonplace Book data layer (v4 `/api/v1/memories` routes + the memory
 * settings cards): TanStack query keys + thin `CoreClient` dispatch helpers over
 * the P4.6t memory variants. Reads go through {@link CoreClient.dispatchData}
 * (raw `data`) — the Shared contract pins the response BODIES, not narrowed
 * `Response` types, so each helper extracts its field defensively (lane A's
 * oracle reconciles the exact envelopes at unification).
 */

import type { CoreClient } from '../core/core-client';
import type {
  BackfillProgress,
  CharacterMemoryCount,
  HousekeepingConfig,
  HousekeepingPreview,
  HousekeepingResult,
  MemoryDto,
  MemoryHousekeepOptions,
  MemoryListRequest,
  MemoryWriteBag,
  RecallConfig,
  RegenerateAllStatus,
} from '../core/core-contract';

/** Query keys for the memory vertical. */
export const memoryKeys = {
  all: ['memories'] as const,
  list: (characterId: string, filters?: Record<string, unknown>) =>
    filters
      ? (['memories', 'list', characterId, filters] as const)
      : (['memories', 'list', characterId] as const),
  backfill: () => ['memories', 'backfill'] as const,
  housekeepingConfig: () => ['memories', 'housekeeping-config'] as const,
  characterCounts: () => ['memories', 'character-counts'] as const,
  recallConfig: () => ['memories', 'recall-config'] as const,
  regenerateStatus: () => ['memories', 'regenerate-status'] as const,
};

/** The paginated list page (v4 `{ memories, totalCount }`; `count` also pinned). */
export interface MemoryListPage {
  memories: MemoryDto[];
  totalCount: number;
  count: number;
}

/** GET a page of a character's memories (v4 `GET /api/v1/memories?…`). */
export async function fetchMemories(
  core: CoreClient,
  params: Omit<MemoryListRequest, 'type'>,
): Promise<MemoryListPage> {
  const data = await core.dispatchData({ type: 'memoryList', ...params });
  const memories = (data['memories'] as MemoryDto[] | undefined) ?? [];
  const totalCount = (data['totalCount'] as number | undefined) ?? memories.length;
  const count = (data['count'] as number | undefined) ?? memories.length;
  return { memories, totalCount, count };
}

/** POST a new manual memory (v4 `POST /api/v1/memories`). */
export async function createMemory(core: CoreClient, memory: MemoryWriteBag): Promise<MemoryDto> {
  const data = await core.dispatchData({ type: 'memoryCreate', memory });
  return (data['memory'] as MemoryDto) ?? (data as unknown as MemoryDto);
}

/** PUT an edited memory (v4 `PUT /api/v1/memories/:id` — no re-embed). */
export async function updateMemory(
  core: CoreClient,
  memoryId: string,
  memory: MemoryWriteBag,
): Promise<MemoryDto> {
  const data = await core.dispatchData({ type: 'memoryUpdate', memoryId, memory });
  return (data['memory'] as MemoryDto) ?? (data as unknown as MemoryDto);
}

/** DELETE a memory (v4 `DELETE /api/v1/memories/:id`). */
export async function deleteMemory(core: CoreClient, memoryId: string): Promise<void> {
  await core.dispatchData({ type: 'memoryDelete', memoryId });
}

// --- Housekeeping (per-character dialog) ------------------------------------

/** GET the housekeeping preview (v4 `action=housekeep` GET → `data.preview`). */
export async function fetchHousekeepingPreview(
  core: CoreClient,
  params: {
    characterId: string;
    maxMemories?: number;
    maxAgeMonths?: number;
    minImportance?: number;
    mergeSimilar?: boolean;
  },
): Promise<HousekeepingPreview> {
  const data = await core.dispatchData({ type: 'memoryHousekeepPreview', ...params });
  return (data['preview'] as HousekeepingPreview) ?? (data as unknown as HousekeepingPreview);
}

/** POST the housekeeping run (v4 `action=housekeep` POST → `data.result`). */
export async function runHousekeeping(
  core: CoreClient,
  options: MemoryHousekeepOptions,
): Promise<HousekeepingResult> {
  const data = await core.dispatchData({ type: 'memoryHousekeep', options });
  return (data['result'] as HousekeepingResult) ?? (data as unknown as HousekeepingResult);
}

// --- Settings cards ---------------------------------------------------------

/** GET the instance-wide backfill progress. */
export async function fetchBackfillProgress(core: CoreClient): Promise<BackfillProgress> {
  const data = await core.dispatchData({ type: 'memoryBackfillProgress' });
  const progress =
    (data['progress'] as BackfillProgress | undefined) ?? (data as unknown as BackfillProgress);
  return { remaining: progress?.remaining ?? 0, inFlight: progress?.inFlight ?? 0 };
}

/** POST a backfill start (enqueues embedding jobs for embedding-less memories). */
export async function startBackfill(
  core: CoreClient,
  batchSize = 500,
): Promise<{ enqueued: number; remaining: number; message?: string }> {
  const data = await core.dispatchData({ type: 'memoryBackfillStart', batchSize });
  return {
    enqueued: (data['enqueued'] as number | undefined) ?? 0,
    remaining: (data['remaining'] as number | undefined) ?? 0,
    message: data['message'] as string | undefined,
  };
}

/** GET the housekeeping config (v4 defaults injected server-side). */
export async function fetchHousekeepingConfig(core: CoreClient): Promise<HousekeepingConfig | null> {
  const data = await core.dispatchData({ type: 'memoryHousekeepingConfigGet' });
  return (data['settings'] as HousekeepingConfig | undefined) ?? null;
}

/** POST a housekeeping-config merge-patch (send only changed fields). */
export async function setHousekeepingConfig(
  core: CoreClient,
  config: Partial<HousekeepingConfig>,
): Promise<HousekeepingConfig | null> {
  const data = await core.dispatchData({ type: 'memoryHousekeepingConfigSet', config });
  return (data['settings'] as HousekeepingConfig | undefined) ?? null;
}

/** GET the per-character memory-count roster (memoryCount desc). */
export async function fetchCharacterMemoryCounts(
  core: CoreClient,
): Promise<CharacterMemoryCount[]> {
  const data = await core.dispatchData({ type: 'memoryCharacterCounts' });
  const raw = data['characters'];
  return Array.isArray(raw) ? (raw as CharacterMemoryCount[]) : [];
}

/** POST a global housekeeping sweep (enqueues a background job). */
export async function runHousekeepingSweep(core: CoreClient): Promise<void> {
  await core.dispatchData({ type: 'memoryHousekeepSweep' });
}

/** GET the recall config (v4 defaults merged client-side). */
export async function fetchRecallConfig(core: CoreClient): Promise<Partial<RecallConfig> | null> {
  const data = await core.dispatchData({ type: 'memoryRecallConfigGet' });
  return (data['settings'] as Partial<RecallConfig> | undefined) ?? null;
}

/** POST a recall-config merge-patch. */
export async function setRecallConfig(
  core: CoreClient,
  config: Partial<RecallConfig>,
): Promise<Partial<RecallConfig> | null> {
  const data = await core.dispatchData({ type: 'memoryRecallConfigSet', config });
  return (data['settings'] as Partial<RecallConfig> | undefined) ?? null;
}

/** GET the regenerate-all in-flight status. */
export async function fetchRegenerateStatus(core: CoreClient): Promise<RegenerateAllStatus> {
  const data = await core.dispatchData({ type: 'memoryRegenerateAllStatus' });
  return {
    inFlightFanOut: (data['inFlightFanOut'] as number | undefined) ?? 0,
    inFlightWipes: (data['inFlightWipes'] as number | undefined) ?? 0,
    inFlightExtractions: (data['inFlightExtractions'] as number | undefined) ?? 0,
    inFlight: (data['inFlight'] as number | undefined) ?? 0,
  };
}

/** POST a regenerate-all (destructive: wipes + rebuilds every chat-linked memory). */
export async function regenerateAllMemories(
  core: CoreClient,
): Promise<{ message?: string }> {
  const data = await core.dispatchData({ type: 'memoryRegenerateAll' });
  return { message: data['message'] as string | undefined };
}
