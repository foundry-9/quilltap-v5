/**
 * The optimizer's pure progress-event reducer — v4 `useCharacterOptimizer.ts`'s
 * `switch (event.type)` over each parsed SSE line (`:145-227`), split out as a
 * standalone function so it is independently testable against a recorded frame
 * sequence (the green-room `applyGreenRoomFrame` reducer precedent). The
 * caller applies every NON-terminal streamed `generatorProgress` event through
 * this reducer, then applies the dispatch's own `terminal` resolution through
 * it ONE more time for `done`/`error` — never both from the same event (§B.1:
 * the dispatch resolution, not a race against the stream, is authoritative for
 * completion — the almanack-card.ts precedent).
 */

import type {
  OptimizerAnalysis,
  OptimizerOutputMode,
  OptimizerPhase,
  OptimizerSubStep,
  OptimizerSuggestion,
} from '../detail-generators.api';

export interface OptimizerFoldState {
  phase: OptimizerPhase;
  analysis: OptimizerAnalysis | null;
  suggestions: OptimizerSuggestion[];
  memoryCount: number;
  filteredCount: number;
  loading: boolean;
  progressStep: string | null;
  progressSubStep: OptimizerSubStep | null;
  noSuggestionsMessage: string | null;
  suggestionsFilePath: string | null;
  error: string | null;
}

export const OPTIMIZER_FOLD_INITIAL: OptimizerFoldState = {
  phase: 'preflight',
  analysis: null,
  suggestions: [],
  memoryCount: 0,
  filteredCount: 0,
  loading: false,
  progressStep: null,
  progressSubStep: null,
  noSuggestionsMessage: null,
  suggestionsFilePath: null,
  error: null,
};

/** v4's fixed no-suggestions sentence (`:213-215`). */
export const NO_SUGGESTIONS_MESSAGE =
  'The memoirs have been consulted most thoroughly, yet your character appears already quite splendidly rendered. No refinements were deemed necessary by our panel of discerning automata.';

/** v4's fallback `error` sentence when the event carries none (`:220-224`). */
export const DEFAULT_ERROR_MESSAGE = 'An unforeseen calamity has befallen the refinement process.';

/**
 * Applies one v4 progress event (a parsed `data:` line, key order/bytes
 * verbatim) to the fold state. `outputMode` is threaded in rather than held on
 * the state because v4 keeps it in a `useRef` set once at `startOptimization`
 * and read only by the terminal `done` branch.
 */
export function applyOptimizerEvent(
  state: OptimizerFoldState,
  event: Record<string, unknown>,
  outputMode: OptimizerOutputMode,
): OptimizerFoldState {
  switch (event['type']) {
    case 'start':
      return { ...state, loading: true, phase: 'progress' };

    case 'step_start': {
      const step = event['step'] as string | undefined;
      return {
        ...state,
        progressStep: step ?? null,
        progressSubStep: step !== 'generating' ? null : state.progressSubStep,
      };
    }

    case 'substep_start': {
      const subStep = event['subStep'];
      return subStep ? { ...state, progressSubStep: subStep as OptimizerSubStep } : state;
    }

    case 'substep_complete': {
      const partial = event['partialSuggestions'];
      if (Array.isArray(partial) && partial.length > 0) {
        return { ...state, suggestions: [...state.suggestions, ...(partial as OptimizerSuggestion[])] };
      }
      return state;
    }

    case 'step_complete': {
      const step = event['step'];
      if (step === 'loading') {
        return {
          ...state,
          memoryCount: (event['memoryCount'] as number) ?? 0,
          filteredCount:
            event['filteredCount'] !== undefined
              ? (event['filteredCount'] as number)
              : state.filteredCount,
        };
      }
      if (step === 'analyzing') {
        return { ...state, analysis: event['analysis'] as OptimizerAnalysis };
      }
      if (step === 'generating') {
        return {
          ...state,
          suggestions: (event['suggestions'] as OptimizerSuggestion[]) ?? state.suggestions,
          progressSubStep: null,
        };
      }
      return state;
    }

    case 'suggestions_file_written':
      return { ...state, suggestionsFilePath: (event['suggestionsFilePath'] as string) ?? null };

    case 'done': {
      const doneSuggestions = event['suggestions'] as OptimizerSuggestion[] | undefined;
      const suggestions = doneSuggestions?.length ? doneSuggestions : state.suggestions;
      const filePath = (event['suggestionsFilePath'] as string | undefined) ?? null;
      const suggestionsFilePath = filePath ?? state.suggestionsFilePath;

      let phase = state.phase;
      let noSuggestionsMessage: string | null = null;
      if (outputMode === 'suggestions-file' && filePath) {
        phase = 'suggestions-file-written';
      } else if (suggestions.length > 0) {
        phase = 'review';
      } else {
        // v4 makes no `setPhase` call on this branch — phase stays 'progress'.
        noSuggestionsMessage = NO_SUGGESTIONS_MESSAGE;
      }

      return {
        ...state,
        suggestions,
        suggestionsFilePath,
        loading: false,
        progressStep: null,
        progressSubStep: null,
        phase,
        noSuggestionsMessage,
      };
    }

    case 'error':
      return {
        ...state,
        error: (event['error'] as string) ?? DEFAULT_ERROR_MESSAGE,
        loading: false,
      };

    default:
      return state;
  }
}
