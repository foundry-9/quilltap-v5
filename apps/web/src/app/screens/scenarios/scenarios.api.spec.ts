import { describe, expect, it } from 'vitest';

import type { CoreClient } from '../../core/core-client';
import type { ScenarioDto } from '../../core/core-contract';
import {
  generalScenarioMutator,
  projectScenarioMutator,
  type ScenarioMutator,
} from './scenarios.api';

/**
 * P4.D121 unit 4 — the scenarios mutator's archive half (v4 `d25dacc1`'s
 * `useGeneralScenarios` / `useProjectScenarios`, which that commit converged
 * onto one shape; v5 already had the one shape).
 *
 * Three claims:
 *  1. `includeArchived` rides every LIST read, and flipping the toggle is a NEW
 *     read rather than a filter over what is loaded.
 *  2. `setScenarioArchived` re-sends the row's fields with the flag, dropping
 *     an `isDefault` claim on the way in — an archived scenario can never be
 *     the default, and a dead `isDefault: true` would sit in the file.
 *  3. The mutate verbs CARRY `includeArchived` (v4 threads it onto the mutate
 *     URLs; the server's Update/Rename/Delete honour it on their fresh-list
 *     returns), so the mutate body is applied directly in BOTH toggle states —
 *     the lane's interim relist divergence was retired at unification. CREATE
 *     is the exception: v4's create route reads the BODY's `archived`, not the
 *     param (the survey-§E.4 quirk), so its response is applied as-is.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

function dto(over: Partial<ScenarioDto> = {}): ScenarioDto {
  return {
    path: 'Scenarios/tavern.md',
    filename: 'tavern',
    name: 'Tavern',
    isDefault: false,
    rawIsDefault: false,
    archived: false,
    body: 'A cozy inn.',
    lastModified: '2026-01-01T00:00:00.000Z',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  };
}

interface Harness {
  mutator: ScenarioMutator;
  sent: Req[];
  /** What the next LIST read answers; the mutate reads answer `mutateList`. */
  listRows: ScenarioDto[];
  mutateList: ScenarioDto[];
}

function harness(
  make: (core: CoreClient) => ScenarioMutator,
  init: { listRows?: ScenarioDto[]; mutateList?: ScenarioDto[] } = {},
): Harness {
  const state: Harness = {
    sent: [],
    listRows: init.listRows ?? [dto()],
    mutateList: init.mutateList ?? [],
    mutator: undefined as unknown as ScenarioMutator,
  };
  const core = {
    dispatchData: async (req: Req) => {
      state.sent.push(req);
      if (req.type.endsWith('ScenarioList') || req.type === 'scenarioList') {
        return { mountPointId: 'mp', scenarios: state.listRows, warnings: ['from list'] };
      }
      return { scenarios: state.mutateList, warnings: ['from mutate'], path: 'Scenarios/x.md' };
    },
  } as unknown as CoreClient;
  state.mutator = make(core);
  return state;
}

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) await Promise.resolve();
}

describe('makeScenarioMutator — the archive half (v4 d25dacc1)', () => {
  it('carries includeArchived on the list read, both scopes', async () => {
    const general = harness((core) => generalScenarioMutator(core));
    await general.mutator.refresh();
    expect(general.sent.at(-1)).toEqual({ type: 'scenarioList', includeArchived: false });

    const project = harness((core) => projectScenarioMutator(core, 'p1'));
    await project.mutator.refresh();
    expect(project.sent.at(-1)).toEqual({
      type: 'projectScenarioList',
      projectId: 'p1',
      includeArchived: false,
    });
  });

  it('flipping the toggle is a NEW read, not a client-side filter', async () => {
    const h = harness((core) => generalScenarioMutator(core));
    await h.mutator.refresh();
    const before = h.sent.length;
    h.mutator.setShowArchived(true);
    await settle();
    expect(h.mutator.showArchived()).toBe(true);
    expect(h.sent.length).toBe(before + 1);
    expect(h.sent.at(-1)!['includeArchived']).toBe(true);
  });

  it('setScenarioArchived re-sends the row and drops an isDefault claim', async () => {
    const h = harness((core) => generalScenarioMutator(core), {
      listRows: [dto({ isDefault: true, description: 'at dusk' })],
    });
    await h.mutator.refresh();
    await h.mutator.setScenarioArchived('Scenarios/tavern.md', true);

    const update = h.sent.find((r) => r.type === 'scenarioUpdate')!;
    expect(update['scenarioPath']).toBe('Scenarios/tavern.md');
    expect(update['scenario']).toEqual({
      name: 'Tavern',
      description: 'at dusk',
      isDefault: false,
      archived: true,
      body: 'A cozy inn.',
    });
  });

  it('restoring preserves the row’s own isDefault', async () => {
    const h = harness((core) => generalScenarioMutator(core), {
      listRows: [dto({ isDefault: true, archived: true })],
    });
    await h.mutator.refresh();
    await h.mutator.setScenarioArchived('Scenarios/tavern.md', false);
    const update = h.sent.find((r) => r.type === 'scenarioUpdate')!;
    expect(update['scenario']).toMatchObject({ isDefault: true, archived: false });
  });

  it('omits `description` entirely when the row carries none', async () => {
    const h = harness((core) => generalScenarioMutator(core));
    await h.mutator.refresh();
    await h.mutator.setScenarioArchived('Scenarios/tavern.md', true);
    const bag = h.sent.find((r) => r.type === 'scenarioUpdate')!['scenario'] as Record<
      string,
      unknown
    >;
    expect('description' in bag).toBe(false);
  });

  it('answers v4’s sentence when the path is not in the current list', async () => {
    const h = harness((core) => generalScenarioMutator(core));
    await h.mutator.refresh();
    const result = await h.mutator.setScenarioArchived('Scenarios/nowhere.md', true);
    expect(result).toEqual({ ok: false, error: 'Scenario not found in current list' });
    expect(h.sent.some((r) => r.type === 'scenarioUpdate')).toBe(false);
  });

  it('a mutation applies its own body and fires NO extra list read (both toggle states)', async () => {
    const h = harness((core) => generalScenarioMutator(core), {
      mutateList: [dto({ path: 'Scenarios/after.md', name: 'After' })],
    });
    await h.mutator.refresh();
    const before = h.sent.filter((r) => r.type === 'scenarioList').length;
    await h.mutator.deleteScenario('Scenarios/tavern.md');
    expect(h.sent.filter((r) => r.type === 'scenarioList').length).toBe(before);
    expect(h.mutator.scenarios().map((s) => s.name)).toEqual(['After']);
    expect(h.mutator.warnings()).toEqual(['from mutate']);
  });

  it('with the toggle ON, the mutate verbs CARRY includeArchived — v4 threads it onto the mutate URLs and the server honours it (the interim relist divergence, retired at unification)', async () => {
    const h = harness((core) => generalScenarioMutator(core), {
      mutateList: [dto({ name: 'Tavern', archived: true })],
    });
    h.mutator.setShowArchived(true);
    await settle();
    const before = h.sent.filter((r) => r.type === 'scenarioList').length;

    await h.mutator.updateScenario('Scenarios/tavern.md', { body: 'x' });

    // No relist — the mutate response IS the list, and it asked for archived.
    expect(h.sent.filter((r) => r.type === 'scenarioList').length).toBe(before);
    const update = h.sent.find((r) => r.type === 'scenarioUpdate');
    expect(update).toMatchObject({ includeArchived: true });
    expect(h.mutator.scenarios()[0].archived).toBe(true);
    expect(h.mutator.warnings()).toEqual(['from mutate']);
  });

  it('with the toggle OFF, the mutate verbs carry includeArchived: false; CREATE never carries it (v4 reads the BODY there)', async () => {
    const h = harness((core) => generalScenarioMutator(core));
    await h.mutator.deleteScenario('Scenarios/tavern.md');
    expect(h.sent.find((r) => r.type === 'scenarioDelete')).toMatchObject({
      includeArchived: false,
    });
    await h.mutator.createScenario({ name: 'N', body: 'b', filename: 'n' });
    const create = h.sent.find((r) => r.type === 'scenarioCreate');
    expect(create).toBeDefined();
    expect(create).not.toHaveProperty('includeArchived');
  });
});
