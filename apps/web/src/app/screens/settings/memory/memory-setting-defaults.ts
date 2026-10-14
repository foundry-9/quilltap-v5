import type {
  MemoryConsolidationConfig,
  MemoryExtractionLimitsConfig,
  MemoryMaintenanceConfig,
} from '../../../core/core-contract';

/**
 * The default value of every memory settings document the Settings → Memory
 * cards edit — v4 `MEMORY_SETTING_DEFAULTS` (`lib/memory/settings-defaults.ts:
 * 92-100` at `83334b9a5`, each one its schema's `.parse({})`), the three the
 * SPA reads. Recorded against v4's real `Schema.parse({})` by
 * `oracle/memory-maintenance-text.recorder.ts`.
 */
export const MEMORY_SETTING_DEFAULTS: {
  readonly maintenance: MemoryMaintenanceConfig;
  readonly consolidation: MemoryConsolidationConfig;
  readonly extractionLimits: MemoryExtractionLimitsConfig;
} = Object.freeze({
  maintenance: {
    enabled: false,
    perCharacterCap: 2000,
    perCharacterCapOverrides: {},
    phases: { foldNearDuplicates: true, demote: true, retire: true },
  },
  consolidation: {
    enabled: false,
    connectionProfileId: null,
    clusterThreshold: 0.72,
    tfidfClusterThreshold: 0.3,
    minClusterSize: 3,
    maxClusterSize: 30,
    matureAfterDays: 7,
    maxClustersPerRun: 40,
    watermark: 150,
    coldRetentionDays: null,
  },
  extractionLimits: {
    enabled: false,
    maxPerHour: 20,
    softStartFraction: 0.7,
    softFloor: 0.7,
  },
});
