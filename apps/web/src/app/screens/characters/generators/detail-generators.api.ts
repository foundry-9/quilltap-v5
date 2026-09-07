/**
 * The generators wire contract (p4.9k, work order P4.9K4 — DETAIL + LIST +
 * CAST hosts): the request/response DTOs for `characterOptimize`,
 * `characterGenerateExternalPrompt`, and `aiImportStream`, plus the
 * `generatorProgress` Event envelope every streaming generator folds through.
 *
 * §B (the round's Shared contract, binding — identical text is declared in
 * every SPA lane's own wire file per §S.3: `core-contract.ts` is frozen this
 * round, so each lane owns its copy and the unifier folds duplicates). K1
 * ships `characterOptimize` + `characterGenerateExternalPrompt` server-side;
 * K2 ships `aiImportStream`. Neither has landed yet, so every request type
 * here is NOT a member of `CoreRequest` — dispatch casts at the one seam
 * (the `file-manager-transport.ts` / P4.6x precedent), retired when the
 * unifier folds these into `core-contract.ts`.
 *
 * §B.1 (streaming shape): a streaming verb carries a client-minted
 * `progressId`; each v4 `onProgress(event)` becomes one
 * `{ type: 'generatorProgress', progressId, generator, event }` Event, and the
 * dispatch call itself resolves with `{ terminal: <v4's last event, verbatim> }`
 * when the run ends server-side. `Event::GeneratorProgress` is not yet a
 * `ScopedEvent` member (P4.9K0's fence in `core-contract.ts`), so
 * {@link asGeneratorProgress} narrows a raw frame defensively rather than
 * relying on the union.
 */

import type { CoreClient } from '../../../core/core-client';
import type { CoreRequest, ScopedEvent, SystemImportExecuteRequest } from '../../../core/core-contract';

// ===========================================================================
// §B.5 — the Event kind
// ===========================================================================

export type GeneratorKind = 'optimizer' | 'wizard' | 'aiImport';

/** Wire: `{ type: 'generatorProgress', progressId, generator, event }`. */
export interface GeneratorProgressEvent {
  type: 'generatorProgress';
  progressId: string;
  generator: GeneratorKind;
  /** v4's own progress-event object for this generator, key order + bytes verbatim. */
  event: Record<string, unknown>;
}

/**
 * Narrows a raw {@link ScopedEvent} frame to a {@link GeneratorProgressEvent}
 * for `progressId`, or `null` when the frame is unrelated. `progressId` is
 * already a real `ScopedEvent` field; `type`/`generator`/`event` are not (the
 * variant hasn't landed in `core-contract.ts` yet), so those three are read
 * through a defensive cast.
 */
export function asGeneratorProgress(
  frame: ScopedEvent,
  progressId: string,
): GeneratorProgressEvent | null {
  if (frame.progressId !== progressId) return null;
  const raw = frame as unknown as Partial<GeneratorProgressEvent>;
  if (raw.type !== 'generatorProgress' || raw.event === undefined) return null;
  return raw as GeneratorProgressEvent;
}

function dispatchGenerator(core: CoreClient, request: object): Promise<Record<string, unknown>> {
  return core.dispatchData(request as unknown as CoreRequest);
}

// ===========================================================================
// §B.2 — characterOptimize (K1)
// ===========================================================================

export type OptimizerOutputMode = 'apply' | 'suggestions-file';

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

/** v4 `optimizeStreamSchema` (`app/api/v1/characters/[id]/handlers/post.ts:60-68`). */
export interface CharacterOptimizeRequest {
  type: 'characterOptimize';
  characterId: string;
  progressId: string;
  connectionProfileId: string;
  maxMemories?: number;
  searchQuery?: string;
  useSemanticSearch?: boolean;
  sinceDate?: string | null;
  beforeDate?: string | null;
  outputMode?: OptimizerOutputMode;
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
  const data = await dispatchGenerator(core, request);
  // §B.1: the dispatch resolves with `{ terminal }` — nothing else is defined.
  return data['terminal'] as OptimizerTerminalEvent;
}

// ===========================================================================
// §B.2 — characterGenerateExternalPrompt (K1)
// ===========================================================================

/** v4 `generateExternalPromptSchema` (`post.ts:70-75`). */
export interface CharacterGenerateExternalPromptRequest {
  type: 'characterGenerateExternalPrompt';
  characterId: string;
  connectionProfileId: string;
  systemPromptId: string;
  scenarioId?: string;
  maxTokens: number;
}

/** v4's response body `{ prompt, tokensUsed }` (`post.ts:328-330`). */
export interface ExternalPromptResult {
  prompt: string;
  tokensUsed: number;
}

export async function dispatchGenerateExternalPrompt(
  core: CoreClient,
  request: CharacterGenerateExternalPromptRequest,
): Promise<ExternalPromptResult> {
  const data = await dispatchGenerator(core, request);
  return data as unknown as ExternalPromptResult;
}

// ===========================================================================
// §B.3 — aiImportStream (K2)
// ===========================================================================

/** v4's hand-rolled body read (`app/api/v1/system/tools/route.ts:1196-1212`). */
export interface AiImportStreamRequest {
  type: 'aiImportStream';
  progressId: string;
  profileId: string;
  sourceFileIds?: string[];
  sourceText?: string;
  includeMemories?: boolean;
  includeChats?: boolean;
  existingResult?: unknown;
  regenerateSteps?: string[];
}

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
  const data = await dispatchGenerator(core, request);
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
