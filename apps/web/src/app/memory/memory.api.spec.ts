import { describe, expect, it } from 'vitest';

import type { CoreClient } from '../core/core-client';
import {
  createMemory,
  fetchExtractionLimits,
  fetchMaintenanceConfig,
  fetchMaintenanceStatus,
  runMaintenance,
  setExtractionLimits,
  setMaintenanceConfig,
  fetchConsolidationConfig,
  fetchExtractionModeConfig,
  fetchMemory,
  memoryKeys,
  setConsolidationConfig,
  setExtractionModeConfig,
} from './memory.api';

/**
 * P4.D276 — the client half of contract C2: each helper sends EXACTLY v4's
 * body (v4 posts one changed key per settings save; the housekeeping and
 * consolidate helpers retired with v4 `b045dd986` — P4.D300), and
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
});

/**
 * P4.D300 — the `83334b9a5` maintenance surface (C3 §1 / C2): v4's key bytes
 * (`lib/query/keys.ts`, `b045dd986`) and each helper's exact request.
 */
describe('memory.api — the 83334b9a5 maintenance surface (P4.D300)', () => {
  it('names the three new keys with v4 bytes', () => {
    expect(memoryKeys.maintenanceConfig()).toEqual(['memories', 'maintenance-config']);
    expect(memoryKeys.extractionLimitsConfig()).toEqual(['memories', 'extraction-limits-config']);
    expect(memoryKeys.maintenanceStatus()).toEqual(['memories', 'maintenance-status']);
  });

  it('the maintenance + limits settings helpers send the patch untouched and read `settings`', async () => {
    const a = stub({ settings: { enabled: true } });
    expect(await fetchMaintenanceConfig(a.core)).toEqual({ enabled: true });
    expect(await setMaintenanceConfig(a.core, { phases: { foldNearDuplicates: true, demote: false, retire: true } })).toEqual({ enabled: true });
    expect(await fetchExtractionLimits(a.core)).toEqual({ enabled: true });
    expect(await setExtractionLimits(a.core, { maxPerHour: 30 })).toEqual({ enabled: true });
    expect(a.seen).toEqual([
      { type: 'memoryMaintenanceConfigGet' },
      { type: 'memoryMaintenanceConfigSet', config: { phases: { foldNearDuplicates: true, demote: false, retire: true } } },
      { type: 'memoryExtractionLimitsGet' },
      { type: 'memoryExtractionLimitsSet', config: { maxPerHour: 30 } },
    ]);
    const none = stub({});
    expect(await fetchMaintenanceConfig(none.core)).toBeNull();
  });

  it('fetchMaintenanceStatus reads `ledger` (no characterId), [] when absent', async () => {
    const hit = stub({ success: true, ledger: [{ characterId: 'c1' }] });
    expect(await fetchMaintenanceStatus(hit.core)).toEqual([{ characterId: 'c1' }]);
    expect(hit.seen).toEqual([{ type: 'memoryMaintenanceStatus' }]);
    expect(await fetchMaintenanceStatus(stub({}).core)).toEqual([]);
  });

  it('runMaintenance sends the options in the caller\'s key order and hands back the reply', async () => {
    const { core, seen } = stub({ success: true, dryRun: false, jobId: 'j1', jobIds: ['j1'] });
    const reply = await runMaintenance(core, { characterId: 'c1', phases: ['retire'], dryRun: false });
    expect(reply).toEqual({ success: true, dryRun: false, jobId: 'j1', jobIds: ['j1'] });
    expect(JSON.stringify(seen)).toBe(
      '[{"type":"memoryMaintain","options":{"characterId":"c1","phases":["retire"],"dryRun":false}}]',
    );
  });

  it('createMemory reads `{ memory, action }`; `action` may be absent (a pre-round server)', async () => {
    const bag = { characterId: 'c1', content: 'C', summary: 'S', keywords: [], importance: 0.5 };
    expect(await createMemory(stub({ memory: { id: 'm' }, action: 'absorb' }).core, bag)).toEqual({
      memory: { id: 'm' },
      action: 'absorb',
    });
    expect((await createMemory(stub({ memory: { id: 'm' } }).core, bag)).action).toBeUndefined();
  });
});
