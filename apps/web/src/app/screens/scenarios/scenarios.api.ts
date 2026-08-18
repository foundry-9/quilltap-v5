/**
 * The scenarios data layer (v4 `useProjectScenarios` / `useGeneralScenarios`):
 * a scope-agnostic mutator over {@link CoreClient} that both the project
 * Scenarios card and the general `/scenarios` page consume. Mirrors v4's hook
 * return shape — the CRUD body ({@link ScenariosManager}) never fetches itself;
 * the scope lives here (the dispatch verbs the ops close over).
 *
 * Each list / create / update / rename / delete response carries the freshly
 * listed scenarios + any soft warnings (e.g. multiple `isDefault: true` files),
 * so a single round trip keeps the UI in sync. Reads go through
 * {@link CoreClient.dispatchData} — the p4.6n Shared contract pins the bodies.
 */

import { type Signal, signal } from '@angular/core';

import type { CoreClient } from '../../core/core-client';
import type {
  ScenarioCreateBag,
  ScenarioDto,
  ScenarioListDto,
  ScenarioUpdateBag,
} from '../../core/core-contract';

/** A create input as the manager builds it (optionals truly absent, no defaults). */
export interface ScenarioCreateInput extends ScenarioCreateBag {}

/** An update input as the manager builds it (no filename — the path rides the verb). */
export interface ScenarioUpdateInput extends ScenarioUpdateBag {}

/** The uniform result the manager branches on (v4's `{ ok } | { ok, error }`). */
export type ScenarioResult<T = unknown> = ({ ok: true } & T) | { ok: false; error: string };

/**
 * The scope-agnostic mutator handed to {@link ScenariosManager}. Signals mirror
 * v4's hook state; the async methods mirror its mutators. `mountPointId` is
 * `null` when the general mount is unprovisioned (the project scope always
 * resolves a mount).
 */
export interface ScenarioMutator {
  readonly scenarios: Signal<ScenarioDto[]>;
  readonly warnings: Signal<string[]>;
  readonly loading: Signal<boolean>;
  readonly error: Signal<string | null>;
  readonly mountPointId: Signal<string | null>;
  /** `silent` refreshes in place without flipping `loading` (tab re-activation). */
  refresh(opts?: { silent?: boolean }): Promise<void>;
  createScenario(input: ScenarioCreateInput): Promise<ScenarioResult<{ path: string }>>;
  updateScenario(scenarioPath: string, input: ScenarioUpdateInput): Promise<ScenarioResult>;
  renameScenario(
    scenarioPath: string,
    newFilename: string,
  ): Promise<ScenarioResult<{ path: string }>>;
  deleteScenario(scenarioPath: string): Promise<ScenarioResult>;
  setDefaultScenario(scenarioPath: string): Promise<ScenarioResult>;
}

/** A create/mutate response body (the list + warnings, plus the fresh path on create/rename). */
interface MutateBody {
  scenarios?: ScenarioDto[];
  warnings?: string[];
  path?: string;
  mountPointId?: string | null;
}

/** The scope-specific dispatch closures the generic mutator drives. */
interface ScenarioOps {
  list(): Promise<ScenarioListDto>;
  create(scenario: ScenarioCreateBag): Promise<MutateBody>;
  update(scenarioPath: string, scenario: ScenarioUpdateBag): Promise<MutateBody>;
  rename(scenarioPath: string, newFilename: string): Promise<MutateBody>;
  remove(scenarioPath: string): Promise<MutateBody>;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/**
 * Build a mutator over a set of scope-specific dispatch ops. Owns the signal
 * state, applies each mutate response, and derives `setDefaultScenario` by
 * re-sending an update with `isDefault: true` (v4's `setDefaultScenario`
 * idiom — there is no dedicated verb).
 */
export function makeScenarioMutator(ops: ScenarioOps): ScenarioMutator {
  const scenarios = signal<ScenarioDto[]>([]);
  const warnings = signal<string[]>([]);
  const loading = signal(true);
  const error = signal<string | null>(null);
  const mountPointId = signal<string | null>(null);

  function applyMutate(body: MutateBody): void {
    scenarios.set(body.scenarios ?? []);
    warnings.set(body.warnings ?? []);
  }

  async function refresh(opts?: { silent?: boolean }): Promise<void> {
    // A silent refresh (workspace tab re-activation) keeps the current list on
    // screen instead of flipping the manager back to its loading state.
    if (!opts?.silent) {
      loading.set(true);
    }
    error.set(null);
    try {
      const data = await ops.list();
      scenarios.set(data.scenarios ?? []);
      warnings.set(data.warnings ?? []);
      mountPointId.set(data.mountPointId ?? null);
    } catch (err) {
      error.set(errorMessage(err));
    } finally {
      loading.set(false);
    }
  }

  async function createScenario(
    input: ScenarioCreateInput,
  ): Promise<ScenarioResult<{ path: string }>> {
    try {
      const body = await ops.create(input);
      applyMutate(body);
      return { ok: true, path: body.path ?? '' };
    } catch (err) {
      return { ok: false, error: errorMessage(err) };
    }
  }

  async function updateScenario(
    scenarioPath: string,
    input: ScenarioUpdateInput,
  ): Promise<ScenarioResult> {
    try {
      applyMutate(await ops.update(scenarioPath, input));
      return { ok: true };
    } catch (err) {
      return { ok: false, error: errorMessage(err) };
    }
  }

  async function renameScenario(
    scenarioPath: string,
    newFilename: string,
  ): Promise<ScenarioResult<{ path: string }>> {
    try {
      const body = await ops.rename(scenarioPath, newFilename);
      applyMutate(body);
      return { ok: true, path: body.path ?? '' };
    } catch (err) {
      return { ok: false, error: errorMessage(err) };
    }
  }

  async function deleteScenario(scenarioPath: string): Promise<ScenarioResult> {
    try {
      applyMutate(await ops.remove(scenarioPath));
      return { ok: true };
    } catch (err) {
      return { ok: false, error: errorMessage(err) };
    }
  }

  async function setDefaultScenario(scenarioPath: string): Promise<ScenarioResult> {
    // "Set default" = re-send an update carrying the current fields + isDefault.
    const current = scenarios().find((s) => s.path === scenarioPath);
    if (!current) {
      return { ok: false, error: 'Scenario not found in current list' };
    }
    return updateScenario(scenarioPath, {
      name: current.name,
      ...(current.description !== undefined && { description: current.description }),
      isDefault: true,
      body: current.body,
    });
  }

  return {
    scenarios,
    warnings,
    loading,
    error,
    mountPointId,
    refresh,
    createScenario,
    updateScenario,
    renameScenario,
    deleteScenario,
    setDefaultScenario,
  };
}

// ---------------------------------------------------------------------------
// Scope-specific factories (the only place the dispatch verb differs).
// ---------------------------------------------------------------------------

/** The project-scoped mutator (v4 `useProjectScenarios`), base `projectScenario*`. */
export function projectScenarioMutator(core: CoreClient, projectId: string): ScenarioMutator {
  return makeScenarioMutator({
    list: async () =>
      (await core.dispatchData({ type: 'projectScenarioList', projectId })) as unknown as ScenarioListDto,
    create: async (scenario) =>
      (await core.dispatchData({
        type: 'projectScenarioCreate',
        projectId,
        scenario,
      })) as MutateBody,
    update: async (scenarioPath, scenario) =>
      (await core.dispatchData({
        type: 'projectScenarioUpdate',
        projectId,
        scenarioPath,
        scenario,
      })) as MutateBody,
    rename: async (scenarioPath, newFilename) =>
      (await core.dispatchData({
        type: 'projectScenarioRename',
        projectId,
        scenarioPath,
        newFilename,
      })) as MutateBody,
    remove: async (scenarioPath) =>
      (await core.dispatchData({
        type: 'projectScenarioDelete',
        projectId,
        scenarioPath,
      })) as MutateBody,
  });
}

/** The instance-wide mutator (v4 `useGeneralScenarios`), base `scenario*`. */
export function generalScenarioMutator(core: CoreClient): ScenarioMutator {
  return makeScenarioMutator({
    list: async () => (await core.dispatchData({ type: 'scenarioList' })) as unknown as ScenarioListDto,
    create: async (scenario) =>
      (await core.dispatchData({ type: 'scenarioCreate', scenario })) as MutateBody,
    update: async (scenarioPath, scenario) =>
      (await core.dispatchData({ type: 'scenarioUpdate', scenarioPath, scenario })) as MutateBody,
    rename: async (scenarioPath, newFilename) =>
      (await core.dispatchData({
        type: 'scenarioRename',
        scenarioPath,
        newFilename,
      })) as MutateBody,
    remove: async (scenarioPath) =>
      (await core.dispatchData({ type: 'scenarioDelete', scenarioPath })) as MutateBody,
  });
}
