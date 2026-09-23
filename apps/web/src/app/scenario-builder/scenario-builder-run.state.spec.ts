import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub, type CoreStreamStub } from '../core/core-client.testing';
import type { CoreResponse, ScenarioBuildRequestInput, ScopedEvent } from '../core/core-contract';
import {
  HOST_COULD_NOT_BEGIN_NO_RESPONSE,
  HOST_COULD_NOT_COMPLETE,
  HOST_WENT_QUIET,
  ScenarioBuilderRun,
} from './scenario-builder-run.state';

/**
 * The run state — v4 `useScenarioBuilderRun` (`d1c06cd9d`) over v5's transport:
 * the frames arrive on the stub's `events$` scoped by the client-minted
 * `runId`, and the dispatch is held open until the spec settles it, so every
 * ordering of "frame" vs "dispatch resolves" is driven explicitly.
 */

const INPUT: ScenarioBuildRequestInput = {
  mode: 'in-world',
  location: 'the Lantern Inn',
  time: 'a rainy evening',
  details: '',
  connectionProfileId: 'profile-1',
  projectId: null,
  characterIds: ['char-1'],
  chatId: null,
};

interface Harness {
  run: ScenarioBuilderRun;
  stream: CoreStreamStub;
  requests: Record<string, unknown>[];
  /** Resolve the in-flight build dispatch with a response. */
  answer: (resp: CoreResponse) => void;
  /** Reject the in-flight build dispatch. */
  throwing: (err: unknown) => void;
  /** Emit a progress frame for the build's `runId` (or another id). */
  emit: (body: Record<string, unknown>, runId?: string) => void;
  buildRunId: () => string;
}

function harness(): Harness {
  const stream = coreStreamStub();
  const requests: Record<string, unknown>[] = [];
  let resolveBuild!: (r: CoreResponse) => void;
  let rejectBuild!: (e: unknown) => void;
  const dispatch = vi.fn((req: Record<string, unknown>) => {
    requests.push(req);
    if (req['type'] === 'scenarioBuilderBuild') {
      return new Promise<CoreResponse>((res, rej) => {
        resolveBuild = res;
        rejectBuild = rej;
      });
    }
    return Promise.resolve({
      type: 'scenarioBuilder',
      data: { aborted: true },
    } as unknown as CoreResponse);
  });

  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [
      ScenarioBuilderRun,
      { provide: CoreClient, useValue: { ...stream, dispatch } as unknown as CoreClient },
    ],
  });

  const builds = () => requests.filter((r) => r['type'] === 'scenarioBuilderBuild');
  const h: Harness = {
    run: TestBed.inject(ScenarioBuilderRun),
    stream,
    requests,
    answer: (resp) => resolveBuild(resp),
    throwing: (err) => rejectBuild(err),
    buildRunId: () => builds()[builds().length - 1]['runId'] as string,
    emit: (body, runId) =>
      stream.frames.next({
        type: 'scenarioBuilderProgress',
        progressId: runId ?? h.buildRunId(),
        frame: body,
      } as unknown as ScopedEvent),
  };
  return h;
}

const ok = (data: Record<string, unknown>): CoreResponse =>
  ({ type: 'scenarioBuilder', data }) as unknown as CoreResponse;
const refused = (message: string): CoreResponse =>
  ({ type: 'error', data: { kind: 'validation', message } }) as unknown as CoreResponse;

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => TestBed.resetTestingModule());

describe('ScenarioBuilderRun — dispatch shape', () => {
  it('dispatches scenarioBuilderBuild with a minted uuid runId and the body RAW', async () => {
    const h = harness();
    void h.run.run(INPUT);
    expect(h.requests).toHaveLength(1);
    expect(h.requests[0]['type']).toBe('scenarioBuilderBuild');
    expect(h.requests[0]['runId']).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
    );
    expect(h.requests[0]['body']).toBe(INPUT);
    expect(h.run.phase()).toBe('running');
  });

  it('mints a fresh runId per run', async () => {
    const h = harness();
    const first = h.run.run(INPUT);
    const firstId = h.buildRunId();
    h.answer(ok({ done: true, scenario: 'A.' }));
    await first;
    void h.run.run(INPUT);
    expect(h.buildRunId()).not.toBe(firstId);
  });
});

describe('ScenarioBuilderRun — frames first, then the dispatch resolves', () => {
  it('folds tool calls, replaces reasoning, and finishes on done', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.emit({ toolsDetected: 1, toolNames: ['search'], toolArguments: [{ query: 'inn' }] });
    expect(h.run.toolCalls()).toEqual([
      { name: 'search', arguments: { query: 'inn' }, pending: true },
    ]);
    h.emit({ reasoning: 'First I' });
    h.emit({ reasoning: 'First I shall look.' });
    // Cumulative per turn — REPLACE, never append.
    expect(h.run.reasoning()).toBe('First I shall look.');
    h.emit({ toolResult: { index: 0, success: true, result: 'hits' } });
    expect(h.run.toolCalls()[0]).toMatchObject({ pending: false, success: true });
    h.emit({ done: true, scenario: 'Rain on the cobbles.', provider: 'x', modelName: 'y' });
    expect(h.run.phase()).toBe('done');
    expect(h.run.scenario()).toBe('Rain on the cobbles.');
    h.answer(ok({ done: true, scenario: 'Rain on the cobbles.' }));
    await expect(result).resolves.toBe('Rain on the cobbles.');
  });

  it('an error frame fails with its error text (not the details)', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.emit({ reasoning: 'hmm' });
    h.emit({ error: 'The Host returned from his enquiries empty-handed.', details: 'no-answer' });
    expect(h.run.phase()).toBe('error');
    expect(h.run.error()).toBe('The Host returned from his enquiries empty-handed.');
    // The reasoning so far is kept (v4's fail spreads prev).
    expect(h.run.reasoning()).toBe('hmm');
    h.answer(ok({ error: 'The Host returned from his enquiries empty-handed.' }));
    await expect(result).resolves.toBeNull();
  });

  it('stops reading at the first terminal frame (a later done is ignored)', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.emit({ error: 'boom' });
    h.emit({ done: true, scenario: 'too late' });
    h.answer(ok({ error: 'boom' }));
    await expect(result).resolves.toBeNull();
    expect(h.run.phase()).toBe('error');
    expect(h.run.scenario()).toBeNull();
  });

  it('done without a string scenario is not terminal', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.emit({ done: true });
    expect(h.run.phase()).toBe('running');
    h.answer(ok({}));
    await expect(result).resolves.toBeNull();
    expect(h.run.error()).toBe(HOST_WENT_QUIET);
  });

  it('ignores frames for another run id', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.emit({ reasoning: 'not mine' }, 'some-other-run');
    h.emit({ done: true, scenario: 'not mine' }, 'some-other-run');
    expect(h.run.reasoning()).toBe('');
    expect(h.run.phase()).toBe('running');
    h.emit({ done: true, scenario: 'mine' });
    h.answer(ok({ done: true, scenario: 'mine' }));
    await expect(result).resolves.toBe('mine');
  });

  it('ignores a swipeProgress frame carrying the same id', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.stream.frames.next({
      type: 'swipeProgress',
      progressId: h.buildRunId(),
      frame: { done: true, scenario: 'wrong channel' },
    } as unknown as ScopedEvent);
    expect(h.run.phase()).toBe('running');
    h.emit({ done: true, scenario: 'right' });
    h.answer(ok({ done: true, scenario: 'right' }));
    await expect(result).resolves.toBe('right');
  });
});

describe('ScenarioBuilderRun — the dispatch resolves before the terminal frame', () => {
  it('folds the reply’s done object when no terminal frame came first', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.emit({ toolsDetected: 1, toolNames: ['search'], toolArguments: [{}] });
    h.answer(ok({ done: true, scenario: 'From the reply.', provider: 'p', modelName: 'm' }));
    await expect(result).resolves.toBe('From the reply.');
    expect(h.run.phase()).toBe('done');
    expect(h.run.scenario()).toBe('From the reply.');
    // The activity the frames built survives.
    expect(h.run.toolCalls()).toHaveLength(1);
    // A late done frame after the reply changes nothing.
    h.emit({ done: true, scenario: 'late frame' });
    expect(h.run.scenario()).toBe('From the reply.');
  });

  it('folds the reply’s error object when no terminal frame came first', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.answer(ok({ error: 'The Host has been detained.', errorType: 'scenario_builder_failed' }));
    await expect(result).resolves.toBeNull();
    expect(h.run.phase()).toBe('error');
    expect(h.run.error()).toBe('The Host has been detained.');
  });

  it('a reply with neither done nor error is the Host going quiet', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.answer(ok({}));
    await expect(result).resolves.toBeNull();
    expect(h.run.phase()).toBe('error');
    expect(h.run.error()).toBe(HOST_WENT_QUIET);
  });

  it('a reply of { aborted: true } returns quietly to idle', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.emit({ reasoning: 'x' });
    h.answer(ok({ aborted: true }));
    await expect(result).resolves.toBeNull();
    expect(h.run.phase()).toBe('idle');
    expect(h.run.reasoning()).toBe('');
    expect(h.run.error()).toBeNull();
  });
});

describe('ScenarioBuilderRun — pre-stream refusals and throws', () => {
  it('shows a refusal envelope’s message as-is (v4’s data.error rule)', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.answer(
      refused(
        'This connection profile has tool use switched off, and the Host cannot make enquiries without tools. Choose another profile.',
      ),
    );
    await expect(result).resolves.toBeNull();
    expect(h.run.phase()).toBe('error');
    expect(h.run.error()).toBe(
      'This connection profile has tool use switched off, and the Host cannot make enquiries without tools. Choose another profile.',
    );
  });

  it('a refusal with no message falls to “could not begin: no response arrived”', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.answer(refused(''));
    await expect(result).resolves.toBeNull();
    expect(h.run.error()).toBe(HOST_COULD_NOT_BEGIN_NO_RESPONSE);
  });

  it('a thrown error shows its message', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.throwing(new Error('socket hang up'));
    await expect(result).resolves.toBeNull();
    expect(h.run.error()).toBe('socket hang up');
  });

  it('a throw with no message falls to “could not complete the enquiry”', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.throwing('not an Error');
    await expect(result).resolves.toBeNull();
    expect(h.run.error()).toBe(HOST_COULD_NOT_COMPLETE);
  });
});

describe('ScenarioBuilderRun — stop, supersede, destroy', () => {
  it('stop() dispatches scenarioBuilderAbort for the live run and resets to idle', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    const runId = h.buildRunId();
    h.emit({ toolsDetected: 1, toolNames: ['search'], toolArguments: [{}] });
    h.run.stop();
    expect(h.requests.at(-1)).toEqual({ type: 'scenarioBuilderAbort', runId });
    expect(h.run.phase()).toBe('idle');
    expect(h.run.toolCalls()).toEqual([]);
    // Frames still in flight for the stopped run change nothing.
    h.emit({ done: true, scenario: 'after stop' }, runId);
    expect(h.run.scenario()).toBeNull();
    // The build dispatch then answers `{ aborted: true }`: quiet.
    h.answer(ok({ aborted: true }));
    await expect(result).resolves.toBeNull();
    expect(h.run.phase()).toBe('idle');
  });

  it('stop() with nothing running dispatches nothing', () => {
    const h = harness();
    h.run.stop();
    expect(h.requests).toEqual([]);
  });

  it('a stopped run whose dispatch still answers done stays stopped', async () => {
    const h = harness();
    const result = h.run.run(INPUT);
    h.run.stop();
    h.answer(ok({ done: true, scenario: 'raced the stop' }));
    await expect(result).resolves.toBeNull();
    expect(h.run.phase()).toBe('idle');
    expect(h.run.scenario()).toBeNull();
  });

  it('a new run() aborts the prior run first', async () => {
    const h = harness();
    void h.run.run(INPUT);
    const firstId = h.buildRunId();
    void h.run.run(INPUT);
    const types = h.requests.map((r) => r['type']);
    expect(types).toEqual(['scenarioBuilderBuild', 'scenarioBuilderAbort', 'scenarioBuilderBuild']);
    expect(h.requests[1]).toEqual({ type: 'scenarioBuilderAbort', runId: firstId });
    // The first run's frames no longer reach the state.
    h.emit({ reasoning: 'stale' }, firstId);
    expect(h.run.reasoning()).toBe('');
  });

  it('a finished run is not aborted by the next run()', async () => {
    const h = harness();
    const first = h.run.run(INPUT);
    h.emit({ done: true, scenario: 'A.' });
    h.answer(ok({ done: true, scenario: 'A.' }));
    await first;
    void h.run.run(INPUT);
    expect(h.requests.map((r) => r['type'])).toEqual([
      'scenarioBuilderBuild',
      'scenarioBuilderBuild',
    ]);
  });

  it('destroying the injector aborts a live run (v4’s unmount abort)', async () => {
    const h = harness();
    void h.run.run(INPUT);
    const runId = h.buildRunId();
    TestBed.resetTestingModule();
    await flush();
    expect(h.requests.at(-1)).toEqual({ type: 'scenarioBuilderAbort', runId });
  });

  it('reset() returns to idle without dispatching', () => {
    const h = harness();
    h.run.reset();
    expect(h.run.phase()).toBe('idle');
    expect(h.requests).toEqual([]);
  });
});
