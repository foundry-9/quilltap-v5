/**
 * A fake `CoreClient` for the Scenario Builder dialogs — spec support only.
 *
 * Plays the part of v4's `installFetchRouter` (`ScenarioBuilderDialog.test.tsx`
 * at `d1c06cd9d`) over v5's transport: a `scenarioBuilderBuild` dispatch
 * records its body, emits the queued frames on `events$` scoped by the
 * request's `runId` (exactly as the server would — the run state subscribed
 * BEFORE dispatching), then resolves with the terminal frame's object (§S.1's
 * reply shape). The profile list, the capabilities probe, the group list and
 * the four save verbs answer from configurable fields.
 *
 * @module scenario-builder/scenario-builder-dialog.testing
 */

import { vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import { coreStreamStub, type CoreStreamStub } from '../core/core-client.testing';
import type { CoreResponse, ScopedEvent } from '../core/core-contract';

export function profile(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: 'profile-1',
    name: 'Sonnet',
    provider: 'anthropic',
    modelName: 'claude-sonnet-5',
    isDefault: false,
    allowToolUse: true,
    allowWebSearch: true,
    ...overrides,
  };
}

export interface FakeCore {
  core: CoreClient;
  stream: CoreStreamStub;
  profiles: Record<string, unknown>[];
  capabilities: { webSearchConfigured: boolean; curlConfigured: boolean };
  groups: Array<{ id: string; name: string }>;
  /** One frame list per build, consumed in order. */
  buildFrames: Array<Array<Record<string, unknown>>>;
  /** When set, the next build is held open until `releaseBuild()` is called. */
  holdBuild: boolean;
  releaseBuild: () => void;
  buildBodies: Record<string, unknown>[];
  /** Every dispatched request, in order. */
  requests: Record<string, unknown>[];
  /** What the save verbs answer (a response, or a function of the request). */
  saveResponse: CoreResponse | ((req: Record<string, unknown>) => CoreResponse);
}

const SAVE_TYPES = new Set([
  'scenarioCreate',
  'projectScenarioCreate',
  'groupScenarioCreate',
  'characterScenarioCreate',
]);

export function fakeCore(): FakeCore {
  const stream = coreStreamStub();
  const fake: FakeCore = {
    core: null as unknown as CoreClient,
    stream,
    profiles: [],
    capabilities: { webSearchConfigured: true, curlConfigured: false },
    groups: [],
    buildFrames: [],
    holdBuild: false,
    releaseBuild: () => undefined,
    buildBodies: [],
    requests: [],
    saveResponse: {
      type: 'scenarioBuilder',
      data: { path: 'Scenarios/a-scene.md' },
    } as unknown as CoreResponse,
  };

  const dispatch = vi.fn(async (req: Record<string, unknown>): Promise<CoreResponse> => {
    fake.requests.push(req);
    const type = req['type'] as string;
    if (type === 'scenarioBuilderBuild') {
      fake.buildBodies.push(req['body'] as Record<string, unknown>);
      const frames = fake.buildFrames.shift() ?? [];
      const runId = req['runId'] as string;
      if (fake.holdBuild) {
        await new Promise<void>((resolve) => {
          fake.releaseBuild = resolve;
        });
      }
      for (const frame of frames) {
        stream.frames.next({
          type: 'scenarioBuilderProgress',
          progressId: runId,
          frame,
        } as unknown as ScopedEvent);
      }
      const terminal = frames.find((f) => f['done'] || f['error']) ?? {};
      return { type: 'scenarioBuilder', data: terminal } as unknown as CoreResponse;
    }
    if (type === 'scenarioBuilderAbort') {
      return { type: 'scenarioBuilder', data: { aborted: true } } as unknown as CoreResponse;
    }
    if (SAVE_TYPES.has(type)) {
      const r = fake.saveResponse;
      return typeof r === 'function' ? r(req) : r;
    }
    return { type: 'error', data: { kind: 'internal', message: `unrouted ${type}` } } as never;
  });

  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    fake.requests.push(req);
    if (req['type'] === 'scenarioBuilderCapabilities') return { ...fake.capabilities };
    if (req['type'] === 'groupList') return { groups: fake.groups };
    throw new Error(`unrouted ${String(req['type'])}`);
  });

  const dispatchExpect = vi.fn(async (req: Record<string, unknown>) => {
    fake.requests.push(req);
    if (req['type'] === 'connectionProfileList') {
      return {
        type: 'connectionProfiles',
        data: { profiles: fake.profiles, count: fake.profiles.length },
      };
    }
    throw new Error(`unrouted ${String(req['type'])}`);
  });

  fake.core = { ...stream, dispatch, dispatchData, dispatchExpect } as unknown as CoreClient;
  return fake;
}
