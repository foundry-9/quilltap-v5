/**
 * The generators wire contract (p4.9k, work order P4.9K4 — DETAIL + LIST +
 * CAST hosts): `characterOptimize`, `characterGenerateExternalPrompt` and
 * `aiImportStream`, plus the `generatorProgress` Event envelope every
 * streaming generator folds through.
 *
 * The request shapes now LIVE in `core-contract.ts` (folded at the
 * `2f4254b42` round's unification, once P4.9K1/P4.9K2 landed the verbs
 * server-side and the name-for-name diff ran against `api/types.rs`'s
 * `=== P4.9K1 ===` / `=== P4.9K2 ===` fences) and are re-exported here so
 * every importer keeps its path; the `as unknown as CoreRequest` cast this
 * file dispatched through while the verbs were SPA-only is gone.
 *
 * §B.1 (streaming shape): a streaming verb carries a client-minted
 * `progressId`; each v4 `onProgress(event)` becomes one
 * `{ type: 'generatorProgress', progressId, generator, event }` Event, and the
 * dispatch call itself resolves with `{ terminal: <v4's last event, verbatim> }`
 * when the run ends server-side. {@link asGeneratorProgress} narrows a raw
 * frame through the ONE narrowing helper (`isGeneratorProgressEvent`, in
 * `edit-generators.api.ts`) — the two lanes' copies were folded at the same
 * unification.
 */

import type { CoreClient } from '../../../core/core-client';
import type {
  AiImportStreamRequest,
  CharacterGenerateExternalPromptRequest,
  CharacterOptimizeRequest,
  GeneratorKind,
  GeneratorProgressEvent,
  OptimizerOutputMode,
  ScopedEvent,
  SystemImportExecuteRequest,
} from '../../../core/core-contract';
import { isGeneratorProgressEvent } from './edit-generators.api';

export type {
  AiImportStreamRequest,
  CharacterGenerateExternalPromptRequest,
  CharacterOptimizeRequest,
  GeneratorKind,
  GeneratorProgressEvent,
  OptimizerOutputMode,
};
export { isGeneratorProgressEvent, mintProgressId } from './edit-generators.api';

// ===========================================================================
// §B.5 — the Event kind
// ===========================================================================

/**
 * Narrows a raw {@link ScopedEvent} frame to a {@link GeneratorProgressEvent}
 * for `progressId`, or `null` when the frame is unrelated.
 */
export function asGeneratorProgress(
  frame: ScopedEvent,
  progressId: string,
): GeneratorProgressEvent | null {
  return isGeneratorProgressEvent(frame, progressId) ? frame : null;
}

// ===========================================================================
// §B.2 — characterOptimize (K1)
// ===========================================================================

/** v4 `components/characters/optimizer/types.ts` `OptimizerPhase`. */
export type OptimizerPhase = 'preflight' | 'progress' | 'review' | 'apply' | 'suggestions-file-written';

/** v4 `components/characters/optimizer/types.ts` `OptimizerFilterOptions`. */
export interface OptimizerFilterOptions {
  maxMemories: number;
  searchQuery: string;
  useSemanticSearch: boolean;
  sinceDate: string | null;
  beforeDate: string | null;
}

export interface BehavioralPattern {
  pattern: string;
  evidence: string;
  frequency: string;
}

export interface OptimizerAnalysis {
  behavioralPatterns: BehavioralPattern[];
  summary: string;
}

export interface OptimizerGeneratedWardrobeItem {
  title: string;
  description: string;
  imagePrompt?: string;
  types: string[];
  appropriateness?: string;
  isDefault?: boolean;
}

/** v4 `components/characters/optimizer/types.ts` `OptimizerSuggestion`. */
export interface OptimizerSuggestion {
  id: string;
  field: string;
  /**
   * For an existing scenario/system prompt/wardrobe item, the item's id. For a
   * `physicalDescription` suggestion, the sub-field key being refined.
   */
  subId?: string;
  subName?: string;
  title?: string;
  /** Suggested name for a brand-new system prompt or wardrobe item (only when no `subId`). */
  name?: string;
  currentValue: string;
  proposedValue: string;
  rationale: string;
  significance: number;
  memoryExcerpts: string[];
  /** Structured payload for a brand-new wardrobe item (field='wardrobeItems', no subId). */
  wardrobeItem?: OptimizerGeneratedWardrobeItem;
}

export type OptimizerSubStepKind =
  | 'general'
  | 'scenario'
  | 'systemPrompt'
  | 'physicalDescription'
  | 'wardrobe'
  | 'properties'
  | 'newSystemPrompts';

export interface OptimizerSubStep {
  kind: OptimizerSubStepKind;
  label: string;
  index: number;
  total: number;
}

export type SuggestionDecision = 'accepted' | 'rejected' | 'edited';

/** v4's terminal `done` progress event — the optimizer's dispatch resolution. */
export interface OptimizerDoneEvent {
  type: 'done';
  analysis?: OptimizerAnalysis;
  suggestions?: OptimizerSuggestion[];
  suggestionsFilePath?: string;
}

/** v4's terminal `error` progress event. */
export interface OptimizerErrorEvent {
  type: 'error';
  error?: string;
}

export type OptimizerTerminalEvent = OptimizerDoneEvent | OptimizerErrorEvent;

/**
 * Dispatches `characterOptimize`; resolves with v4's terminal `done`/`error`
 * event (§B.1 — the dispatch resolution, not the last-seen streamed frame,
 * is authoritative). A refusal before the run starts throws
 * {@link CoreDispatchError} and emits no progress frame at all.
 */
export async function dispatchCharacterOptimize(
  core: CoreClient,
  request: CharacterOptimizeRequest,
): Promise<OptimizerTerminalEvent> {
  const data = await core.dispatchData(request);
  // §B.1: the dispatch resolves with `{ terminal }` — nothing else is defined.
  return data['terminal'] as OptimizerTerminalEvent;
}

// ===========================================================================
// §B.2 — characterGenerateExternalPrompt (K1)
// ===========================================================================

/** v4's response body `{ prompt, tokensUsed }` (`post.ts:328-330`). */
export interface ExternalPromptResult {
  prompt: string;
  tokensUsed: number;
}

export async function dispatchGenerateExternalPrompt(
  core: CoreClient,
  request: CharacterGenerateExternalPromptRequest,
): Promise<ExternalPromptResult> {
  const data = await core.dispatchData(request);
  return data as unknown as ExternalPromptResult;
}

// ===========================================================================
// §B.3 — aiImportStream (K2)
// ===========================================================================

/** v4's terminal `done` event `{type, result, stepResults, errors?}`. */
export interface AiImportDoneEvent {
  type: 'done';
  result?: unknown;
  stepResults?: Record<string, unknown>;
  errors?: Record<string, string>;
}

export async function dispatchAiImportStream(
  core: CoreClient,
  request: AiImportStreamRequest,
): Promise<AiImportDoneEvent> {
  const data = await core.dispatchData(request);
  return (data['terminal'] ?? data) as AiImportDoneEvent;
}

// ===========================================================================
// systemImportExecute — the AI import wizard's final step (EXISTING verb,
// `core-contract.ts:6837`). A real `CoreRequest` member — no cast needed.
// ===========================================================================

/**
 * v4 `AIImportWizard.tsx:332` — `{exportData: generation.result, options:
 * {conflictStrategy: 'duplicate', importMemories: true}}`. The response body
 * the wizard reads (`{success, imported, warnings, importedCharacterIds}`);
 * the generic settings Import dialog (`import-dialog.ts`'s local
 * `ImportResult`) reads the same verb's response through a narrower view that
 * omits `success`/`importedCharacterIds` — this is the AI-import-specific one.
 */
export interface SystemImportExecuteResult {
  success: boolean;
  imported?: Record<string, number>;
  warnings?: string[];
  importedCharacterIds?: string[];
}

export async function dispatchSystemImportExecute(
  core: CoreClient,
  request: SystemImportExecuteRequest,
): Promise<SystemImportExecuteResult> {
  const data = await core.dispatchData(request);
  return data as unknown as SystemImportExecuteResult;
}
