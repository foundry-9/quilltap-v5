/**
 * The generators wire contract for the EDIT-side lane (P4.9K3): the §B DTOs
 * this lane's components dispatch, declared VERBATIM from the work order's
 * binding §B block (`docs/developer/porting/work-orders/
 * p4.9k3-generators-spa-edit-new.md`). `core-contract.ts` is frozen for this
 * round — every SPA lane declares its own copy of the §B shapes it uses, and
 * the unifier folds identical duplicate declarations into `core-contract.ts`
 * at `/unify` (§S.3).
 *
 * `characterRename` (§B.2) is K1's verb; `characterWizard`/
 * `characterWizardStream` (§B.3) are K2's. Neither exists on the real server
 * in this lane's tree, so every dispatch call here casts through
 * `as unknown as CoreRequest` (the `file-manager-transport.ts` precedent) —
 * retired at unification once the real union members land, exactly as
 * `images.api.ts`'s header describes for its own now-landed verb.
 *
 * @module screens/characters/generators/edit-generators.api
 */

import type { CoreClient } from '../../../core/core-client';
import type { CoreRequest, ScopedEvent } from '../../../core/core-contract';

// ---------------------------------------------------------------------------
// §B.1 — the one streaming shape every generator uses
// ---------------------------------------------------------------------------

/** The generator kinds (§B.5) — the wire spelling of `GeneratorKind`. */
export type GeneratorKind = 'optimizer' | 'wizard' | 'aiImport';

/**
 * §B.5's `Event::GeneratorProgress`, folded flat into the wire the same way
 * `ChatStreamFrame`/`CreationProgressFrame` do (`ScopedEvent`'s doc comment)
 * — except THIS event carries its own `type` tag rather than folding
 * untagged, per §B.1's frame shape. `event` is v4's own progress-event
 * object, byte-for-byte (key order and all).
 */
export interface GeneratorProgressEvent {
  type: 'generatorProgress';
  progressId: string;
  generator: GeneratorKind;
  event: Record<string, unknown>;
}

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
  request: Record<string, unknown>,
  progressId: string,
  onEvent: (event: Record<string, unknown>) => void,
): Promise<Record<string, unknown>> {
  const sub = core.events$.subscribe((frame) => {
    if (isGeneratorProgressEvent(frame, progressId)) {
      onEvent(frame.event);
    }
  });
  return core
    .dispatchData(request as unknown as CoreRequest)
    .finally(() => sub.unsubscribe());
}

// ---------------------------------------------------------------------------
// §B.2 — K1's per-character trio (rename is the only one this lane calls)
// ---------------------------------------------------------------------------

/** v4's `additionalReplacements`/`primaryRename` pair shape (`renameSchema`). */
export interface RenamePair {
  oldValue: string;
  newValue: string;
  caseSensitive: boolean;
}

/** `characterRename` request (§B.2, K1's verb). */
export interface CharacterRenameRequest {
  type: 'characterRename';
  characterId: string;
  primaryRename?: RenamePair;
  additionalReplacements?: RenamePair[];
  dryRun?: boolean;
}

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

/** Dispatch `characterRename` (K1's verb — casts until unification folds it). */
export async function dispatchCharacterRename(
  core: CoreClient,
  request: CharacterRenameRequest,
): Promise<RenamePreviewResponse> {
  const data = await core.dispatchData(request as unknown as CoreRequest);
  return data as unknown as RenamePreviewResponse;
}

// ---------------------------------------------------------------------------
// §B.3 — K2's wizard pair
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

/** The wizard's field-vantage id set (v4 `GeneratableField`). */
export type GeneratableField =
  | 'name'
  | 'title'
  | 'identity'
  | 'description'
  | 'manifesto'
  | 'personality'
  | 'scenarios'
  | 'exampleDialogues'
  | 'firstMessage'
  | 'systemPrompt'
  | 'properties'
  | 'physicalDescription'
  | 'wardrobeItems';

/** v4 `DescriptionSourceType`. */
export type DescriptionSourceType = 'existing' | 'upload' | 'gallery' | 'document' | 'skip';

/** The `WizardRequest` body §B.3 pins verbatim (v4 `wizardRequestSchema`). */
export interface WizardRequest {
  primaryProfileId: string;
  visionProfileId?: string;
  sourceType: DescriptionSourceType;
  imageId?: string;
  documentId?: string;
  characterName: string;
  existingData?: Record<string, unknown>;
  background: string;
  fieldsToGenerate: GeneratableField[];
  characterId?: string;
}

/** `characterWizard` (non-streaming; §B.3). Unused by this lane's UI, kept for completeness. */
export interface CharacterWizardRequest extends WizardRequest {
  type: 'characterWizard';
}

/** `characterWizardStream` (§B.3, K2's verb — the wizard's ONLY live call). */
export interface CharacterWizardStreamRequest extends WizardRequest {
  type: 'characterWizardStream';
  progressId: string;
}

/** v4's wizard `done` terminal event shape (`useAIWizard.ts:358-368`). */
export interface WizardDoneEvent {
  type: 'done';
  fullContent: GeneratedCharacterData;
  errors?: Record<string, string>;
  error?: string;
}
