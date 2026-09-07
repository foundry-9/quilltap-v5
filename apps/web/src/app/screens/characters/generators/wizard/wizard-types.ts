/**
 * The AI Wizard's field catalogue + pure helpers — v4 `components/characters/
 * ai-wizard/types.ts` (211 lines at the pin, minus the `ConnectionProfile`-typed
 * component-prop interfaces, which live as Angular component `input()`s
 * instead) transcribed at the `f699da6f6` pin
 * (`/tmp/qt-v4-pin-p49k3-f699da6f6/components/characters/ai-wizard/types.ts`).
 *
 * @module screens/characters/generators/wizard/wizard-types
 */

import { PROMPT_FIELD_HINTS } from '../../../../ui/prompt-field-hints';
import type { GeneratableField, GeneratedCharacterData } from '../edit-generators.api';

export type { GeneratableField, DescriptionSourceType } from '../edit-generators.api';

/** v4 `WizardStep`. */
export type WizardStep = 1 | 2 | 3 | 4;

/** v4 `GenerationProgress`. */
export interface GenerationProgress {
  currentField: GeneratableField | null;
  completedFields: GeneratableField[];
  snippets: Record<string, string>;
  errors: Record<string, string>;
}

export function initialGenerationProgress(): GenerationProgress {
  return { currentField: null, completedFields: [], snippets: {}, errors: {} };
}

/**
 * Normalize scenarios from `GeneratedCharacterData` — v4 `normalizeGeneratedScenarios`
 * (`types.ts:90-105`), handling both a parsed array and the raw string the
 * wizard service returns when its own JSON parse failed.
 */
export function normalizeGeneratedScenarios(
  scenarios: GeneratedCharacterData['scenarios'],
): Array<{ title: string; content: string }> {
  if (!scenarios) return [];
  if (Array.isArray(scenarios)) return scenarios;
  try {
    const stripped = scenarios
      .replace(/^```(?:json)?\s*/i, '')
      .replace(/\s*```$/, '')
      .trim();
    const parsed: unknown = JSON.parse(stripped);
    if (Array.isArray(parsed)) return parsed as Array<{ title: string; content: string }>;
    return [];
  } catch {
    return [{ title: 'Generated', content: scenarios }];
  }
}

/** v4 `FIELD_LABELS` (`types.ts:252-266`), verbatim. */
export const FIELD_LABELS: Record<GeneratableField, string> = {
  name: 'Name',
  title: 'Title',
  identity: 'Identity',
  description: 'Description',
  manifesto: 'Manifesto',
  personality: 'Personality',
  scenarios: 'Scenarios',
  exampleDialogues: 'Example Dialogues',
  firstMessage: 'First Message',
  systemPrompt: 'System Prompt',
  properties: 'Properties (Pronouns & Aliases)',
  physicalDescription: 'Physical Description',
  wardrobeItems: 'Wardrobe',
};

/** v4 `FIELD_DESCRIPTIONS` (`types.ts:272-286`), verbatim — sources from `PROMPT_FIELD_HINTS`. */
export const FIELD_DESCRIPTIONS: Record<GeneratableField, string> = {
  name: "The character's name",
  title: 'A short epithet or title (e.g., "The Wanderer")',
  identity: PROMPT_FIELD_HINTS.identity.helper,
  description: PROMPT_FIELD_HINTS.description.helper,
  manifesto: PROMPT_FIELD_HINTS.manifesto.helper,
  personality: PROMPT_FIELD_HINTS.personality.helper,
  scenarios: `${PROMPT_FIELD_HINTS.scenario.helper} Generates 2-3 named scenarios with distinct settings.`,
  exampleDialogues: PROMPT_FIELD_HINTS.exampleDialogues.helper,
  firstMessage: PROMPT_FIELD_HINTS.firstMessage.helper,
  systemPrompt: PROMPT_FIELD_HINTS.systemPrompt.helper,
  properties: 'Structured facts: pronouns and aliases (nicknames others use)',
  physicalDescription: `${PROMPT_FIELD_HINTS.physicalDescription.helper} The person only — clothing lives in the wardrobe.`,
  wardrobeItems:
    'Clothing, accessories, hairstyles, and composite outfits (top, bottom, footwear, accessories, hair)',
};

/**
 * The wizard's own view of the host character's current fields (v4
 * `WizardCharacterData`) — the shape sent as `existingData` and threaded
 * through `AIWizardModalProps.currentData`.
 */
export interface WizardCharacterData {
  title?: string;
  identity?: string;
  description?: string;
  manifesto?: string;
  personality?: string;
  scenarios?: Array<{ id: string; title: string; content: string }>;
  exampleDialogues?: string;
  systemPrompt?: string;
  firstMessage?: string;
  pronouns?: { subject: string; object: string; possessive: string } | null;
  aliases?: string[];
}

/**
 * The text-field subset the wizard writes straight back into a host form (v4
 * `app/aurora/shared/wizard-text-fields.ts`'s `GENERATED_CHARACTER_TEXT_FIELDS`
 * + `getGeneratedCharacterTextEntries`).
 */
export const GENERATED_CHARACTER_TEXT_FIELDS = [
  'name',
  'title',
  'identity',
  'description',
  'manifesto',
  'personality',
  'firstMessage',
  'exampleDialogues',
  'systemPrompt',
] as const;

export type GeneratedCharacterTextField = (typeof GENERATED_CHARACTER_TEXT_FIELDS)[number];

export function getGeneratedCharacterTextEntries(
  data: GeneratedCharacterData,
): Array<{ field: GeneratedCharacterTextField; value: string }> {
  const out: Array<{ field: GeneratedCharacterTextField; value: string }> = [];
  for (const field of GENERATED_CHARACTER_TEXT_FIELDS) {
    const value = data[field];
    if (value) {
      out.push({ field, value });
    }
  }
  return out;
}

/**
 * Merge wizard-generated aliases into an existing list, case-insensitively
 * deduped (v4 `CharacterEditView.tsx:126-133`). Returns the SAME array
 * reference when nothing new was added, so a caller can skip a state update.
 */
export function mergeGeneratedAliases(existing: string[], additions: string[]): string[] {
  const existingLower = new Set(existing.map((a) => a.toLowerCase()));
  const toAdd = additions.filter((a) => !existingLower.has(a.toLowerCase()));
  return toAdd.length > 0 ? [...existing, ...toAdd] : existing;
}
