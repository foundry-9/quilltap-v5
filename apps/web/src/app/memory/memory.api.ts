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
  AdmissionActionKind,
  BackfillProgress,
  CharacterMemoryCount,
  ConsolidationReport,
  HousekeepingConfig,
  HousekeepingPreview,
  HousekeepingResult,
  MemoryConsolidationConfig,
  MemoryCreateReply,
  MemoryDto,
  MemoryExtractionModeConfig,
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
  /** Every per-chat count (the unscoped `memories` hint's prefix). */
  chatCounts: () => ['memories', 'chat-count'] as const,
  /** One chat's memory count (v4 `queryKeys.memories.chatCount`) — the sidebar's `Delete Memories (n)`. */
  chatCount: (chatId: string | null | undefined) => ['memories', 'chat-count', chatId] as const,
  recallConfig: () => ['memories', 'recall-config'] as const,
  regenerateStatus: () => ['memories', 'regenerate-status'] as const,
  /** v4 `queryKeys.memories.consolidationConfig` (`lib/query/keys.ts:198`). */
  consolidationConfig: () => ['memories', 'consolidation-config'] as const,
  /** v4 `queryKeys.memories.extractionModeConfig` (`lib/query/keys.ts:199`). */
  extractionModeConfig: () => ['memories', 'extraction-mode-config'] as const,
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

/**
 * GET one memory by id (v4 `GET /api/v1/memories/:id` → `{ memory }`) — the
 * digest viewer's read. `null` when the reply carries no `memory`.
 */
export async function fetchMemory(core: CoreClient, memoryId: string): Promise<MemoryDto | null> {
  const data = await core.dispatchData({ type: 'memoryGet', memoryId });
  return (data['memory'] as MemoryDto | undefined) ?? null;
}

/**
 * POST a new manual memory (v4 `POST /api/v1/memories` → `{ memory, action }`
 * at `83334b9a5`; C2's `MemoryCreate` reply). The 201-vs-200 split v4's editor
 * reads rides `action` here — the dispatch edge carries no status. A pre-round
 * server answers `{ memory }` alone, so at run time `action` may be absent;
 * callers test it (L-5).
 */
export async function createMemory(
  core: CoreClient,
  memory: MemoryWriteBag,
): Promise<MemoryCreateReply> {
  const data = await core.dispatchData({ type: 'memoryCreate', memory });
  return {
    memory: (data['memory'] as MemoryDto) ?? (data as unknown as MemoryDto),
    action: data['action'] as AdmissionActionKind,
  };
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

// --- Consolidation + extraction grain (P4.D276, v4 `d58548051`) -------------

/** GET the consolidation settings (v4 `?action=consolidation-config` → `settings`). */
export async function fetchConsolidationConfig(
  core: CoreClient,
): Promise<Partial<MemoryConsolidationConfig> | null> {
  const data = await core.dispatchData({ type: 'memoryConsolidationConfigGet' });
  return (data['settings'] as Partial<MemoryConsolidationConfig> | undefined) ?? null;
}

/**
 * POST a consolidation-settings patch. The patch is sent UNTOUCHED — v4 posts
 * exactly the one changed field, and the server's `.partial()` default-fill is
 * the server's to answer for (P4.D271 R-A).
 */
export async function setConsolidationConfig(
  core: CoreClient,
  patch: Partial<MemoryConsolidationConfig>,
): Promise<Partial<MemoryConsolidationConfig> | null> {
  const data = await core.dispatchData({ type: 'memoryConsolidationConfigSet', config: patch });
  return (data['settings'] as Partial<MemoryConsolidationConfig> | undefined) ?? null;
}

/** GET the extraction-grain settings (v4 `?action=extraction-mode-config` → `settings`). */
export async function fetchExtractionModeConfig(
  core: CoreClient,
): Promise<Partial<MemoryExtractionModeConfig> | null> {
  const data = await core.dispatchData({ type: 'memoryExtractionModeConfigGet' });
  return (data['settings'] as Partial<MemoryExtractionModeConfig> | undefined) ?? null;
}

/** POST an extraction-grain patch (sent untouched, as {@link setConsolidationConfig}). */
export async function setExtractionModeConfig(
  core: CoreClient,
  patch: Partial<MemoryExtractionModeConfig>,
): Promise<Partial<MemoryExtractionModeConfig> | null> {
  const data = await core.dispatchData({ type: 'memoryExtractionModeConfigSet', config: patch });
  return (data['settings'] as Partial<MemoryExtractionModeConfig> | undefined) ?? null;
}

/**
 * POST a consolidation for one character (v4 `?action=consolidate`). v4 posts
 * ONLY `{ characterId, dryRun }` (`memory-consolidation-card.tsx:165`); a dry
 * run answers `{ report }`, a real one `{ jobId }`.
 */
export async function consolidateMemories(
  core: CoreClient,
  options: { characterId: string; dryRun: boolean },
): Promise<{ report?: ConsolidationReport; jobId?: string }> {
  const data = await core.dispatchData({
    type: 'memoryConsolidate',
    options: { characterId: options.characterId, dryRun: options.dryRun },
  });
  return {
    report: data['report'] as ConsolidationReport | undefined,
    jobId: data['jobId'] as string | undefined,
  };
}
