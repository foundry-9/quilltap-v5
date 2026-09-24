import { DestroyRef, Injectable, inject, signal } from '@angular/core';
import type { Subscription } from 'rxjs';

import { CoreClient } from '../core/core-client';
import {
  isScenarioBuilderProgressEvent,
  type ScenarioBuildRequestInput,
} from '../core/core-contract';
import {
  applyAgentStreamEvent,
  EMPTY_AGENT_TOOL_CALL_STATE,
  type AgentStreamEvent,
  type AgentStreamToolCall,
  type AgentToolCallState,
} from './agent-tool-calls';

export type ScenarioBuilderPhase = 'idle' | 'running' | 'done' | 'error';

/** v4's four fallback sentences (`useScenarioBuilderRun.ts` at `d1c06cd9d`). */
export const HOST_COULD_NOT_BEGIN_NO_RESPONSE = 'The Host could not begin: no response arrived.';
export const HOST_WENT_QUIET = 'The Host went quiet before the scene was finished. Do try again.';
export const HOST_COULD_NOT_COMPLETE = 'The Host could not complete the enquiry.';

/** A terminal outcome, whichever channel delivered it. */
type Outcome = { kind: 'done'; scenario: string } | { kind: 'error'; message: string };

/**
 * One Scenario Builder run at a time — v4's `useScenarioBuilderRun`
 * (`components/scenario-builder/hooks/useScenarioBuilderRun.ts` at
 * `d1c06cd9d`; the order's `hooks/useScenarioBuilderRun.ts` path is wrong) as
 * signals.
 *
 * `phase` / `toolCalls` / `reasoning` / `scenario` / `error` are v4's state;
 * `reasoning` is CUMULATIVE for the turn, so each frame REPLACES it; the scene
 * appears whole on `done` (no content frames stream).
 *
 * ## The transport divergence (forced by the boundary)
 *
 * v4 `fetch`es `POST ?action=build` and reads SSE lines out of the response
 * body; its SSE response IS the run. v5 mints a `runId`, subscribes to
 * `scenarioBuilderProgress` frames for it off the ONE Event channel BEFORE
 * dispatching `scenarioBuilderBuild` (so no frame can precede the
 * subscription), and folds each frame exactly as v4 folds a parsed line:
 * tool state, then reasoning, then `error` (fail), then `done` (finish). Stop
 * dispatches `scenarioBuilderAbort { runId }` where v4 aborts its fetch.
 *
 * v4's four fallback strings map onto v5's arms like this:
 *  - `The Host could not begin (HTTP ${status}).` — v4's `!res.ok` arm shows
 *    `data.error` first; v5's pre-stream refusal IS an error envelope whose
 *    `message` is that sentence, shown as-is. There is no HTTP status on a
 *    dispatch envelope, so the `(HTTP …)` form has no v5 input; an envelope
 *    with an empty message falls to the next sentence.
 *  - `The Host could not begin: no response arrived.` — v4's body-less
 *    response; on v5, an error envelope with no message.
 *  - `The Host went quiet before the scene was finished. Do try again.` — the
 *    stream ended with no terminal frame: the dispatch resolved and neither the
 *    frames NOR the dispatch's own value (below) carried `done` or `error`.
 *  - `The Host could not complete the enquiry.` — a throw with no message.
 *
 * ## ⚠ Deviation from the order's §S.1 sentence, measured
 *
 * §S.1 says the SPA "uses the dispatch value only for the pre-stream
 * refusals". Taken literally that loses runs: the frames and the dispatch
 * reply travel on DIFFERENT channels (the event stream vs the dispatch
 * response), so a `done` frame can land after the dispatch resolves — the
 * exact race the P4.D206 unification review found in the streamed swipe
 * (`chat/regeneration.state.ts`). §S.1 ALSO fixes the reply's shape as the
 * terminal frame's own object, so this reads it as the race's fallback: a
 * terminal FRAME wins when it came first; otherwise the reply's `done` /
 * `error` object is folded the same way; `{ aborted: true }` resolves quietly;
 * anything else is "went quiet". Both orderings are pinned by spec.
 *
 * **Provided per dialog** (never `providedIn: 'root'`): it holds one dialog's
 * run, and destroying the dialog aborts it (v4's unmount abort).
 *
 * @module scenario-builder/scenario-builder-run.state
 */
@Injectable()
export class ScenarioBuilderRun {
  private readonly core = inject(CoreClient);

  readonly phase = signal<ScenarioBuilderPhase>('idle');
  readonly toolCalls = signal<AgentStreamToolCall[]>([]);
  /** Cumulative reasoning for the current turn — DISPLAY ONLY. */
  readonly reasoning = signal('');
  /** The finished scene, once `done` arrives. */
  readonly scenario = signal<string | null>(null);
  readonly error = signal<string | null>(null);

  /** The live run's id, or null. A run whose id is no longer this is superseded. */
  private currentRunId: string | null = null;
  private subscription: Subscription | null = null;
  private toolState: AgentToolCallState = EMPTY_AGENT_TOOL_CALL_STATE;

  constructor() {
    // A closed dialog must not leave a run going (v4's unmount abort).
    inject(DestroyRef).onDestroy(() => this.abortCurrent());
  }

  /**
   * Start a run (aborting any prior one). Resolves to the scene on `done`, or
   * `null` on failure / stop — failures land in {@link error}.
   */
  async run(input: ScenarioBuildRequestInput): Promise<string | null> {
    this.abortCurrent();
    const runId = mintRunId();
    this.currentRunId = runId;
    this.toolState = EMPTY_AGENT_TOOL_CALL_STATE;
    this.resetSignals();
    this.phase.set('running');

    const isLive = (): boolean => this.currentRunId === runId;
    // A holder, not a `let`: the frame callback assigns it, which TS's flow
    // analysis cannot see across the `await`.
    const settled: { outcome: Outcome | null } = { outcome: null };

    // Subscribe BEFORE dispatching: the id is ours, so nothing can race it.
    this.subscription = this.core.events$.subscribe((frame) => {
      if (!isScenarioBuilderProgressEvent(frame, runId)) return;
      // v4 stops reading at the first terminal frame; so does this.
      if (!isLive() || settled.outcome) return;
      settled.outcome = this.fold(frame.frame);
    });

    try {
      const resp = await this.core.dispatch({ type: 'scenarioBuilderBuild', runId, body: input });
      // Stopped or superseded while in flight: v4's AbortError arm — quiet.
      if (!isLive()) return null;
      if (resp.type === 'error') {
        // A pre-stream refusal — unless a terminal frame already landed: a
        // transport error AFTER `done` (a lost connection, the driver thread's
        // panic reply) must not overturn the outcome the Host already
        // delivered (the d1c06cd9d unification review).
        if (!settled.outcome) {
          return this.fail(resp.data.message || HOST_COULD_NOT_BEGIN_NO_RESPONSE);
        }
      } else if (!settled.outcome) {
        const reply = (resp.data ?? {}) as AgentStreamEvent;
        if (reply['aborted'] === true) {
          this.resetSignals();
          return null;
        }
        settled.outcome = this.fold(reply);
      }
      const outcome = settled.outcome;
      if (!outcome) return this.fail(HOST_WENT_QUIET);
      return outcome.kind === 'done' ? outcome.scenario : null;
    } catch (err) {
      if (!isLive()) return null;
      return this.fail((err instanceof Error && err.message) || HOST_COULD_NOT_COMPLETE);
    } finally {
      if (isLive()) {
        this.currentRunId = null;
        this.unsubscribe();
      }
    }
  }

  /** v4 `stop()`: abort the run and return to idle. */
  stop(): void {
    this.abortCurrent();
    this.toolState = EMPTY_AGENT_TOOL_CALL_STATE;
    this.resetSignals();
  }

  /** v4 `reset()`: back to idle without touching a run. */
  reset(): void {
    this.toolState = EMPTY_AGENT_TOOL_CALL_STATE;
    this.resetSignals();
  }

  /**
   * Fold one frame (or the dispatch's terminal object) exactly as v4 folds a
   * parsed SSE line: tool state, reasoning, then `error`, then `done`. Returns
   * the terminal outcome when the frame carries one.
   */
  private fold(event: AgentStreamEvent): Outcome | null {
    const nextToolState = applyAgentStreamEvent(this.toolState, event);
    if (nextToolState !== this.toolState) {
      this.toolState = nextToolState;
      this.toolCalls.set(nextToolState.toolCalls);
    }
    // Cumulative per turn — replace, not append.
    if (typeof event['reasoning'] === 'string') {
      this.reasoning.set(event['reasoning']);
    }
    if (event['error']) {
      const message = String(event['error']);
      this.fail(message);
      return { kind: 'error', message };
    }
    if (event['done'] && typeof event['scenario'] === 'string') {
      const scenario = event['scenario'];
      this.scenario.set(scenario);
      this.error.set(null);
      this.phase.set('done');
      return { kind: 'done', scenario };
    }
    return null;
  }

  private fail(message: string): null {
    this.phase.set('error');
    this.error.set(message);
    return null;
  }

  private resetSignals(): void {
    this.phase.set('idle');
    this.toolCalls.set([]);
    this.reasoning.set('');
    this.scenario.set(null);
    this.error.set(null);
  }

  private abortCurrent(): void {
    const runId = this.currentRunId;
    this.currentRunId = null;
    this.unsubscribe();
    if (runId) {
      // Fire and forget: `{ aborted: false }` (the run already finished) is not
      // an error, and nothing waits on the answer.
      void this.core.dispatch({ type: 'scenarioBuilderAbort', runId }).catch(() => undefined);
    }
  }

  private unsubscribe(): void {
    this.subscription?.unsubscribe();
    this.subscription = null;
  }
}

/** A client-minted run id (a uuid — v5's scope tag; v4 has none). */
function mintRunId(): string {
  return crypto.randomUUID();
}
