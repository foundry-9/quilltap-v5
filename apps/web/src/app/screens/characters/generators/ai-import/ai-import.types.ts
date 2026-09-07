/**
 * AI Character Import ("Summon From Lore") — the step catalogue and small
 * value types, transcribed from v4 `components/settings/ai-import/types.ts`
 * (104 lines). This is the K2/K4 twin of the optimizer's own
 * `detail-generators.api.ts` types: kept as a standalone module (rather than
 * folded into the wire file) because `aiImportStream`'s request/response DTOs
 * already live in `detail-generators.api.ts` §B.3 and this file owns only the
 * wizard's OWN vocabulary — the step names, their display strings, and the
 * per-step progress shape — none of which cross the wire as a typed field
 * (the wire's `event` payload is `Record<string, unknown>`; the fold in
 * `ai-import-fold.ts` is what gives those bytes a shape).
 */

/** UI wizard steps, 1-indexed (v4 `WizardUIStep`, `types.ts:14`). */
export type WizardUIStep = 1 | 2 | 3 | 4;

/** v4 `AIImportStepName` (`types.ts:20-32`). */
export type AIImportStepName =
  | 'analyzing'
  | 'character_basics'
  | 'first_message'
  | 'system_prompts'
  | 'physical_descriptions'
  | 'wardrobe_items'
  | 'pronouns'
  | 'memories'
  | 'chats'
  | 'assembly'
  | 'validation'
  | 'repair';

/** v4 `StepStatus` (`types.ts:34`). */
export type StepStatus = 'pending' | 'in_progress' | 'complete' | 'error' | 'skipped';

/** v4 `StepProgress` (`types.ts:36-40`). */
export interface StepProgress {
  status: StepStatus;
  snippet?: string;
  error?: string;
}

/** v4 `AIImportOptions` (`types.ts:46-50`). */
export interface AIImportOptions {
  profileId: string;
  includeMemories: boolean;
  includeChats: boolean;
}

/** v4 `UploadedSourceFile` (`types.ts:56-60`). */
export interface UploadedSourceFile {
  id: string;
  name: string;
  size: number;
}

/** v4 `STEP_DISPLAY_NAMES` (`types.ts:78-91`), transcribed byte-for-byte. */
export const STEP_DISPLAY_NAMES: Record<AIImportStepName, string> = {
  analyzing: 'Analyzing Source Material',
  character_basics: 'Extracting Character Basics',
  first_message: 'Generating Dialogue',
  system_prompts: 'Creating System Prompts',
  physical_descriptions: 'Describing Appearance',
  wardrobe_items: 'Generating Wardrobe',
  pronouns: 'Determining Pronouns & Aliases',
  memories: 'Generating Memories',
  chats: 'Creating Example Chat',
  assembly: 'Assembling Export',
  validation: 'Validating Data',
  repair: 'Repairing Issues',
};

/** Steps that are always shown in progress (v4 `CORE_STEPS`, `types.ts:94-103`). */
export const CORE_STEPS: AIImportStepName[] = [
  'character_basics',
  'first_message',
  'system_prompts',
  'physical_descriptions',
  'wardrobe_items',
  'pronouns',
  'assembly',
  'validation',
];

/**
 * v4 `useAIImport.ts`'s `INITIAL_STEPS` (`:22-35`) — every step name seeded
 * `{status: 'pending'}`. The fold's initial state is built from this, and
 * `startGeneration` resets to a fresh copy of it on every run (`:196`).
 */
export const AI_IMPORT_INITIAL_STEPS: Record<AIImportStepName, StepProgress> = {
  analyzing: { status: 'pending' },
  character_basics: { status: 'pending' },
  first_message: { status: 'pending' },
  system_prompts: { status: 'pending' },
  physical_descriptions: { status: 'pending' },
  wardrobe_items: { status: 'pending' },
  pronouns: { status: 'pending' },
  memories: { status: 'pending' },
  chats: { status: 'pending' },
  assembly: { status: 'pending' },
  validation: { status: 'pending' },
  repair: { status: 'pending' },
};

/** v4 `AIImportWizard`'s `stepLabels` (`AIImportWizard.tsx:699`). */
export const WIZARD_STEP_LABELS: readonly [string, string, string, string] = [
  'Source Material',
  'Configuration',
  'Generation',
  'Review',
];

/**
 * v4 `ReviewStep`'s `fieldNames` (`AIImportWizard.tsx:438-447`) — the
 * completion-matrix labels, keyed by the `stepResults` field they report on.
 */
export const REVIEW_FIELD_NAMES: Record<string, string> = {
  character_basics: 'Character Basics',
  first_message: 'Dialogue',
  system_prompts: 'System Prompts',
  physical_descriptions: 'Appearance',
  wardrobe_items: 'Wardrobe',
  pronouns: 'Pronouns & Aliases',
  memories: 'Memories',
  chats: 'Example Chat',
};
