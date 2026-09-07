/**
 * The AI-import wizard's pure progress-event reducer — v4 `useAIImport.ts`'s
 * SSE `switch (event.type)` (`:248-305`), split out the same way
 * `optimizer-fold.ts` mirrors `useCharacterOptimizer.ts`'s switch, so the
 * event-folding logic is independently testable against a recorded frame
 * sequence.
 *
 * The caller (`AiImportState`) applies every NON-terminal streamed
 * `generatorProgress` event through {@link applyAiImportEvent}, then applies
 * the dispatch's own `terminal` resolution through it ONE more time for
 * `done` — never both from the same event (the wire file's §B.1 / the
 * almanack-card.ts precedent, identical to the optimizer's own rule).
 */

import type { AIImportStepName, StepProgress } from './ai-import.types';
import { AI_IMPORT_INITIAL_STEPS } from './ai-import.types';

export interface AiImportFoldState {
  generating: boolean;
  steps: Record<AIImportStepName, StepProgress>;
  /** v4 `generation.result` — the assembled `QuilltapExport`, opaque here (§B.3: `unknown` on the wire). */
  result: unknown | null;
  stepResults: Record<string, unknown> | null;
  /** Per-step failures (v4 `generation.errors`), keyed by step name. */
  errors: Record<string, string>;
  /**
   * The wizard-level error banner (v4's separate `error` state,
   * `useAIImport.ts:62`) — set by a fetch/network failure, by a `done` event
   * that itself carries a top-level `error` (`:301-303`), or by a failed
   * import attempt. Distinct from {@link errors}, which is per-step.
   */
  error: string | null;
}

export const AI_IMPORT_FOLD_INITIAL: AiImportFoldState = {
  generating: false,
  steps: { ...AI_IMPORT_INITIAL_STEPS },
  result: null,
  stepResults: null,
  errors: {},
  error: null,
};

/** v4's fallback per-step error text (`useAIImport.ts:284`, `event.error || 'Step failed'`). */
export const DEFAULT_STEP_ERROR_MESSAGE = 'Step failed';

/**
 * Applies one v4 progress event (a parsed `data:` line, key order/bytes
 * verbatim) to the fold state.
 */
export function applyAiImportEvent(
  state: AiImportFoldState,
  event: Record<string, unknown>,
): AiImportFoldState {
  switch (event['type']) {
    case 'step_start': {
      const step = event['step'] as AIImportStepName;
      if (!step) return state;
      return {
        ...state,
        steps: { ...state.steps, [step]: { status: 'in_progress' } },
      };
    }

    case 'step_complete': {
      const step = event['step'] as AIImportStepName;
      if (!step) return state;
      return {
        ...state,
        steps: {
          ...state.steps,
          [step]: {
            status: 'complete',
            snippet: event['snippet'] as string | undefined,
          },
        },
      };
    }

    case 'step_error': {
      const step = event['step'] as AIImportStepName;
      if (!step) return state;
      const errorText = (event['error'] as string | undefined) ?? DEFAULT_STEP_ERROR_MESSAGE;
      return {
        ...state,
        steps: {
          ...state.steps,
          [step]: { status: 'error', error: event['error'] as string | undefined },
        },
        errors: { ...state.errors, [step]: errorText },
      };
    }

    case 'done': {
      const result = event['result'];
      const stepResults = event['stepResults'] as Record<string, unknown> | undefined;
      const errors = event['errors'] as Record<string, string> | undefined;
      // v4 reads a top-level `error` off the done event too (`:301-303`),
      // distinct from the per-step `errors` record.
      const doneError = event['error'] as string | undefined;
      return {
        ...state,
        generating: false,
        result: result !== undefined && result !== null ? result : state.result,
        stepResults: stepResults !== undefined ? stepResults : state.stepResults,
        errors: errors !== undefined ? errors : state.errors,
        error: doneError ? doneError : state.error,
      };
    }

    default:
      return state;
  }
}
