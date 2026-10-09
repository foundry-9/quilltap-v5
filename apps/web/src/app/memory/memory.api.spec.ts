import { describe, expect, it } from 'vitest';

import type { CoreClient } from '../core/core-client';
import {
  consolidateMemories,
  fetchConsolidationConfig,
  fetchExtractionModeConfig,
  fetchHousekeepingPreview,
  fetchMemory,
  memoryKeys,
  setConsolidationConfig,
  setExtractionModeConfig,
} from './memory.api';

/**
 * P4.D276 — the client half of contract C2: each helper sends EXACTLY v4's
 * body (v4 posts one changed key per settings save, `{ characterId, dryRun }`
 * per consolidation, and no `mergeSimilar` anywhere since `d58548051`), and
 * the query keys are v4's bytes (`lib/query/keys.ts:198-199`).
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

function stub(reply: Record<string, unknown>): { core: CoreClient; seen: Req[] } {
  const seen: Req[] = [];
  const core = {
    dispatchData: async (req: Req) => {
      seen.push(req);
      return reply;
    },
  } as unknown as CoreClient;
  return { core, seen };
}

describe('memory.api — the d58548051 surface (P4.D276)', () => {
  it('names the two new config keys with v4 bytes (keys.ts:198-199)', () => {
    expect(memoryKeys.consolidationConfig()).toEqual(['memories', 'consolidation-config']);
    expect(memoryKeys.extractionModeConfig()).toEqual(['memories', 'extraction-mode-config']);
  });

  it('fetchMemory dispatches memoryGet and reads `memory`, null when absent', async () => {
    const hit = stub({ memory: { id: 'd1' } });
    expect(await fetchMemory(hit.core, 'd1')).toEqual({ id: 'd1' });
    expect(hit.seen).toEqual([{ type: 'memoryGet', memoryId: 'd1' }]);
    const miss = stub({});
    expect(await fetchMemory(miss.core, 'd2')).toBeNull();
  });

  it('fetchHousekeepingPreview sends no mergeSimilar (v4 hunk -51)', async () => {
    const { core, seen } = stub({ preview: { wouldDemote: 0 } });
    await fetchHousekeepingPreview(core, {
      characterId: 'c1',
      maxMemories: 1000,
      maxAgeMonths: 6,
      minImportance: 0.3,
    });
    expect(seen).toEqual([
      {
        type: 'memoryHousekeepPreview',
        characterId: 'c1',
        maxMemories: 1000,
        maxAgeMonths: 6,
        minImportance: 0.3,
      },
    ]);
  });

  it('consolidation config: Get reads `settings`; Set sends the ONE key untouched', async () => {
    const get = stub({ settings: { enabled: true } });
    expect(await fetchConsolidationConfig(get.core)).toEqual({ enabled: true });
    expect(get.seen).toEqual([{ type: 'memoryConsolidationConfigGet' }]);

    const set = stub({ settings: { watermark: 200 } });
    expect(await setConsolidationConfig(set.core, { watermark: 200 })).toEqual({ watermark: 200 });
    expect(set.seen).toEqual([{ type: 'memoryConsolidationConfigSet', config: { watermark: 200 } }]);

    const cleared = stub({ settings: {} });
    await setConsolidationConfig(cleared.core, { coldRetentionDays: null });
    expect(cleared.seen).toEqual([
      { type: 'memoryConsolidationConfigSet', config: { coldRetentionDays: null } },
    ]);

    expect(await fetchConsolidationConfig(stub({}).core)).toBeNull();
  });

  it('extraction-mode config: Get reads `settings`; Set sends the ONE key untouched', async () => {
    const get = stub({ settings: { otherPass: 'fold' } });
    expect(await fetchExtractionModeConfig(get.core)).toEqual({ otherPass: 'fold' });
    expect(get.seen).toEqual([{ type: 'memoryExtractionModeConfigGet' }]);

    const set = stub({ settings: { otherPass: 'turn' } });
    await setExtractionModeConfig(set.core, { otherPass: 'turn' });
    expect(set.seen).toEqual([
      { type: 'memoryExtractionModeConfigSet', config: { otherPass: 'turn' } },
    ]);
  });

  it('consolidateMemories wraps exactly { characterId, dryRun } in `options` (v4 :165)', async () => {
    const dry = stub({ success: true, dryRun: true, report: { characterId: 'c1' } });
    const out = await consolidateMemories(dry.core, { characterId: 'c1', dryRun: true });
    expect(dry.seen).toEqual([
      { type: 'memoryConsolidate', options: { characterId: 'c1', dryRun: true } },
    ]);
    expect(out).toEqual({ report: { characterId: 'c1' }, jobId: undefined });

    const real = stub({ success: true, dryRun: false, jobId: 'job-9' });
    const queued = await consolidateMemories(real.core, { characterId: 'c1', dryRun: false });
    expect(real.seen).toEqual([
      { type: 'memoryConsolidate', options: { characterId: 'c1', dryRun: false } },
    ]);
    expect(queued).toEqual({ report: undefined, jobId: 'job-9' });
  });
});
