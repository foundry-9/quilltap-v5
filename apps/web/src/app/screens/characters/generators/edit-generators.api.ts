/**
 * The generators wire contract for the EDIT-side lane (P4.9K3): the §B DTOs
 * this lane's components dispatch. The request shapes now LIVE in
 * `core-contract.ts` (folded at the `2f4254b42` round's unification, after
 * P4.9K1/P4.9K2 landed the verbs server-side and the name-for-name diff ran
 * against `api/types.rs`'s `=== P4.9K1 ===` / `=== P4.9K2 ===` fences) and are
 * re-exported here so every importer keeps its path; the
 * `as unknown as CoreRequest` casts this file carried while the verbs were
 * SPA-only are gone — `characterRename`, `characterWizard` and
 * `characterWizardStream` are real `CoreRequest` members.
 *
 * The response shapes (v4's `RenamePreviewResponse`, the wizard's generated
 * data) stay here: they are what THIS lane's components read, not wire
 * requests.
 *
 * @module screens/characters/generators/edit-generators.api
 */

import type { CoreClient } from '../../../core/core-client';
import type {
  CharacterRenameRequest,
  CharacterWizardStreamRequest,
  CoreRequest,
  DescriptionSourceType,
  GeneratableField,
  GeneratorKind,
  GeneratorProgressEvent,
  RenamePair,
  ScopedEvent,
  WizardRequest,
} from '../../../core/core-contract';

export type {
  CharacterRenameRequest,
  CharacterWizardStreamRequest,
  DescriptionSourceType,
  GeneratableField,
  GeneratorKind,
  GeneratorProgressEvent,
  RenamePair,
  WizardRequest,
};

// ---------------------------------------------------------------------------
// §B.1 — the one streaming shape every generator uses
// ---------------------------------------------------------------------------

/** Narrow a raw stream frame to a generator-progress frame for `mine`. */
export function isGeneratorProgressEvent(
  frame: ScopedEvent | Record<string, unknown>,
  mine: string,
): frame is GeneratorProgressEvent {
  const f = frame as Record<string, unknown>;
  return f['type'] === 'generatorProgress' && f['progressId'] === mine;
}

/** Mint a client-side progress id (v4 `crypto.randomUUID()` at the call site). */
export function mintProgressId(): string {
  return crypto.randomUUID();
}

/**
 * Subscribe to `core.events$`, filtering to `generatorProgress` frames for
 * `progressId`, calling `onEvent` with each v4-shaped progress event until
 * the dispatch's own promise resolves or rejects (mirroring v4's fold loop,
 * which runs entirely inside the one `fetch` awaiting the stream). Returns
 * the dispatch's resolved `data` (the `{ terminal: ... }` envelope per §B.1).
 */
export function streamGenerator(
  core: CoreClient,
  request: CoreRequest,
  progressId: string,
  onEvent: (event: Record<string, unknown>) => void,
): Promise<Record<string, unknown>> {
  const sub = core.events$.subscribe((frame) => {
    if (isGeneratorProgressEvent(frame, progressId)) {
      onEvent(frame.event);
    }
  });
  return core.dispatchData(request).finally(() => sub.unsubscribe());
}

// ---------------------------------------------------------------------------
// §B.2 — K1's per-character trio (rename is the only one this lane calls)
// ---------------------------------------------------------------------------

/** v4's `RenamePreviewResponse` (§B.2), verbatim key order. */
export interface RenameReplacementResult {
  field: string;
  location: string;
  oldText: string;
  newText: string;
  context?: string;
}

export interface RenameSummary {
  characterFields: number;
  physicalDescriptions: number;
  memories: number;
  chatTitles: number;
  chatMessages: number;
  total: number;
}

export interface RenamePreviewResponse {
  characterId: string;
  characterName: string;
  dryRun: boolean;
  replacements: RenameReplacementResult[];
  summary: RenameSummary;
}

/** Dispatch `characterRename` (K1's verb). */
export async function dispatchCharacterRename(
  core: CoreClient,
  request: CharacterRenameRequest,
): Promise<RenamePreviewResponse> {
  const data = await core.dispatchData(request);
  return data as unknown as RenamePreviewResponse;
}

// ---------------------------------------------------------------------------
// §B.3 — K2's wizard pair (the generated-data shapes the wizard reads)
// ---------------------------------------------------------------------------

/** v4's `GeneratedPhysicalDescription` (ai-wizard `types.ts`). */
export interface GeneratedPhysicalDescription {
  name: string;
  headAndShouldersPrompt: string;
  shortPrompt: string;
  mediumPrompt: string;
  longPrompt: string;
  completePrompt: string;
  fullDescription: string;
}

/** v4's `GeneratedWardrobeItem`. */
export interface GeneratedWardrobeItem {
  title: string;
  description: string;
  imagePrompt?: string;
  types: string[];
  appropriateness?: string;
  isDefault?: boolean;
  components?: string[];
  replace?: boolean;
}

/** v4's `GeneratedProperties`. */
export interface GeneratedProperties {
  pronouns: { subject: string; object: string; possessive: string } | null;
  aliases: string[];
}

/** v4's `GeneratedCharacterData` — the wizard's whole result shape. */
export interface GeneratedCharacterData {
  name?: string;
  title?: string;
  identity?: string;
  description?: string;
  manifesto?: string;
  personality?: string;
  scenarios?: Array<{ title: string; content: string }> | string;
  exampleDialogues?: string;
  firstMessage?: string;
  systemPrompt?: string;
  properties?: GeneratedProperties;
  physicalDescription?: GeneratedPhysicalDescription;
  wardrobeItems?: GeneratedWardrobeItem[];
}

/** v4's wizard `done` terminal event shape (`useAIWizard.ts:358-368`). */
export interface WizardDoneEvent {
  type: 'done';
  fullContent: GeneratedCharacterData;
  errors?: Record<string, string>;
  error?: string;
}
