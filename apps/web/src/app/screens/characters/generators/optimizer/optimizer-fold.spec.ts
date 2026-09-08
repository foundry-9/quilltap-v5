import { describe, expect, it } from 'vitest';

import {
  applyOptimizerEvent,
  DEFAULT_ERROR_MESSAGE,
  NO_SUGGESTIONS_MESSAGE,
  OPTIMIZER_FOLD_INITIAL,
  settleStreamEnd,
  type OptimizerFoldState,
} from './optimizer-fold';

/**
 * Parity spec — v4's `useCharacterOptimizer.ts:145-227` `switch (event.type)`
 * over a RECORDED sequence of frames, hand-transcribed here (not read from a
 * server) so this spec and {@link applyOptimizerEvent} cannot drift into
 * agreement by accident. The sequence models one full run: loading →
 * analyzing → generating (two incremental sub-steps) → done, plus a separate
 * error-terminal case and the no-suggestions branch.
 */
const RECORDED_RUN: Record<string, unknown>[] = [
  { type: 'start' },
  { type: 'step_start', step: 'loading' },
  { type: 'step_complete', step: 'loading', memoryCount: 42, filteredCount: 90 },
  { type: 'step_start', step: 'analyzing' },
  {
    type: 'step_complete',
    step: 'analyzing',
    analysis: { behavioralPatterns: [{ pattern: 'p', evidence: 'e', frequency: 'often' }], summary: 's' },
  },
  { type: 'step_start', step: 'generating' },
  {
    type: 'substep_start',
    subStep: { kind: 'general', label: 'General fields', index: 1, total: 3 },
  },
  {
    type: 'substep_complete',
    partialSuggestions: [
      {
        id: 'sug-1',
        field: 'identity',
        currentValue: 'old',
        proposedValue: 'new',
        rationale: 'r',
        significance: 0.5,
        memoryExcerpts: [],
      },
    ],
  },
  {
    type: 'substep_start',
    subStep: { kind: 'scenario', label: 'Scenario', index: 2, total: 3 },
  },
  {
    type: 'substep_complete',
    partialSuggestions: [
      {
        id: 'sug-2',
        field: 'scenarios',
        subId: 'scn-1',
        currentValue: 'old scene',
        proposedValue: 'new scene',
        rationale: 'r2',
        significance: 0.8,
        memoryExcerpts: ['excerpt'],
      },
    ],
  },
  {
    type: 'step_complete',
    step: 'generating',
    suggestions: [
      {
        id: 'sug-1',
        field: 'identity',
        currentValue: 'old',
        proposedValue: 'new',
        rationale: 'r',
        significance: 0.5,
        memoryExcerpts: [],
      },
      {
        id: 'sug-2',
        field: 'scenarios',
        subId: 'scn-1',
        currentValue: 'old scene',
        proposedValue: 'new scene',
        rationale: 'r2',
        significance: 0.8,
        memoryExcerpts: ['excerpt'],
      },
    ],
  },
  { type: 'done', suggestions: undefined },
];

function runFrames(frames: readonly Record<string, unknown>[]): OptimizerFoldState {
  return frames.reduce<OptimizerFoldState>(
    (state, event) => applyOptimizerEvent(state, event, 'apply'),
    OPTIMIZER_FOLD_INITIAL,
  );
}

describe('applyOptimizerEvent — the recorded run', () => {
  it('accumulates memory/filtered counts on the loading step_complete', () => {
    const state = runFrames(RECORDED_RUN.slice(0, 3));
    expect(state.memoryCount).toBe(42);
    expect(state.filteredCount).toBe(90);
  });

  it('sets the analysis on the analyzing step_complete', () => {
    const state = runFrames(RECORDED_RUN.slice(0, 5));
    expect(state.analysis?.summary).toBe('s');
    expect(state.analysis?.behavioralPatterns).toHaveLength(1);
  });

  it('accumulates substep_complete partials incrementally, in order', () => {
    const state = runFrames(RECORDED_RUN.slice(0, 8));
    expect(state.suggestions.map((s) => s.id)).toEqual(['sug-1']);
    const state2 = runFrames(RECORDED_RUN.slice(0, 10));
    expect(state2.suggestions.map((s) => s.id)).toEqual(['sug-1', 'sug-2']);
  });

  it('the generating step_complete REPLACES suggestions with its own full list', () => {
    const state = runFrames(RECORDED_RUN.slice(0, 11));
    expect(state.suggestions.map((s) => s.id)).toEqual(['sug-1', 'sug-2']);
    expect(state.progressSubStep).toBeNull();
  });

  it('a terminal done with no suggestions keeps the accumulated ones and moves to review', () => {
    const state = runFrames(RECORDED_RUN);
    expect(state.phase).toBe('review');
    expect(state.loading).toBe(false);
    expect(state.progressStep).toBeNull();
    expect(state.suggestions).toHaveLength(2);
  });
});

describe('applyOptimizerEvent — terminal branches', () => {
  it('done with zero suggestions and outputMode "apply" sets the no-suggestions sentence and stays on the current phase', () => {
    const state = applyOptimizerEvent(
      { ...OPTIMIZER_FOLD_INITIAL, phase: 'progress' },
      { type: 'done', suggestions: [] },
      'apply',
    );
    expect(state.phase).toBe('progress');
    expect(state.noSuggestionsMessage).toBe(NO_SUGGESTIONS_MESSAGE);
  });

  it('done with outputMode "suggestions-file" and a file path moves to suggestions-file-written', () => {
    const state = applyOptimizerEvent(
      { ...OPTIMIZER_FOLD_INITIAL, phase: 'progress' },
      { type: 'done', suggestions: [], suggestionsFilePath: 'Suggestions/refinement-x.md' },
      'suggestions-file',
    );
    expect(state.phase).toBe('suggestions-file-written');
    expect(state.suggestionsFilePath).toBe('Suggestions/refinement-x.md');
    expect(state.noSuggestionsMessage).toBeNull();
  });

  it('error sets the message and clears loading, leaving other state untouched', () => {
    const seeded: OptimizerFoldState = {
      ...OPTIMIZER_FOLD_INITIAL,
      loading: true,
      memoryCount: 7,
      phase: 'progress',
    };
    const state = applyOptimizerEvent(seeded, { type: 'error', error: 'boom' }, 'apply');
    expect(state.error).toBe('boom');
    expect(state.loading).toBe(false);
    expect(state.memoryCount).toBe(7);
    // P4.84: v4's `error` case (`useCharacterOptimizer.ts:220-226`) calls
    // `setError` and `setLoading(false)` and NOTHING else — in particular no
    // `setPhase`, so the modal stays on whatever pane it was showing and the
    // message lands there. A terminal that also moved the phase would redden.
    expect(state.phase).toBe('progress');
  });

  it('error with no `error` field falls back to v4s fixed sentence', () => {
    const state = applyOptimizerEvent(OPTIMIZER_FOLD_INITIAL, { type: 'error' }, 'apply');
    expect(state.error).toBe(DEFAULT_ERROR_MESSAGE);
  });

  it('an unknown event type is a no-op (mutation guard: a naive default-case handler that resets state would fail this)', () => {
    const seeded: OptimizerFoldState = { ...OPTIMIZER_FOLD_INITIAL, memoryCount: 3, phase: 'progress' };
    const state = applyOptimizerEvent(seeded, { type: 'unknown_frame_kind' }, 'apply');
    expect(state).toEqual(seeded);
  });

  it('step_start for a non-generating step clears any stale progressSubStep', () => {
    const seeded: OptimizerFoldState = {
      ...OPTIMIZER_FOLD_INITIAL,
      progressSubStep: { kind: 'general', label: 'x', index: 1, total: 1 },
    };
    const state = applyOptimizerEvent(seeded, { type: 'step_start', step: 'loading' }, 'apply');
    expect(state.progressSubStep).toBeNull();
  });

  it('step_start for "generating" preserves any existing progressSubStep (mutation guard: an always-clear implementation would fail this)', () => {
    const subStep = { kind: 'general' as const, label: 'x', index: 1, total: 1 };
    const seeded: OptimizerFoldState = { ...OPTIMIZER_FOLD_INITIAL, progressSubStep: subStep };
    const state = applyOptimizerEvent(seeded, { type: 'step_start', step: 'generating' }, 'apply');
    expect(state.progressSubStep).toBe(subStep);
  });
});

describe('settleStreamEnd — v4 useCharacterOptimizer.ts:233-238, after EVERY stream end', () => {
  const landed = {
    id: 'sug-9',
    field: 'identity',
    currentValue: 'old',
    suggestedValue: 'new',
    reasoning: 'r',
  } as unknown as OptimizerFoldState['suggestions'][number];

  it('an `error` terminal AFTER suggestions landed still ends at review (the error pane sits beneath)', () => {
    const afterError: OptimizerFoldState = {
      ...OPTIMIZER_FOLD_INITIAL,
      phase: 'progress',
      loading: false,
      error: 'the vault write failed',
      suggestions: [landed],
    };
    const settled = settleStreamEnd(afterError);
    expect(settled.phase).toBe('review');
    expect(settled.error).toBe('the vault write failed');
    expect(settled.loading).toBe(false);
  });

  it('a suggestions-file `done` that carried suggestions ends at review (v4’s shipped quirk, reproduced — a v4 filing candidate)', () => {
    const afterDone: OptimizerFoldState = {
      ...OPTIMIZER_FOLD_INITIAL,
      phase: 'suggestions-file-written',
      loading: false,
      suggestions: [landed],
    };
    expect(settleStreamEnd(afterDone).phase).toBe('review');
  });

  it('with no suggestions the phase is left where the run put it, only the spinner stops', () => {
    const stillLoading: OptimizerFoldState = { ...OPTIMIZER_FOLD_INITIAL, phase: 'progress', loading: true };
    const settled = settleStreamEnd(stillLoading);
    expect(settled.loading).toBe(false);
    expect(settled.phase).toBe('progress');
  });
});
