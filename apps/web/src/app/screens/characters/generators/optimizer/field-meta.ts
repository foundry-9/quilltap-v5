/**
 * Optimizer field metadata (v4 `components/characters/optimizer/field-meta.ts`,
 * transcribed byte-for-byte): the display label and badge class for each
 * character field a suggestion may touch, shared by the review surfaces so the
 * two never drift apart. Also carries {@link FIELD_HINT_KEYS}, v4's
 * `components/prompt-fields/field-hints.ts:128-139` map from a suggestion's
 * `field` to the {@link PROMPT_FIELD_HINTS} key that supplies its worked
 * example on the review card.
 */

import type { PromptFieldHintKey } from '../../../../ui/prompt-field-hints';

export const FIELD_LABELS: Record<string, string> = {
  identity: 'Identity',
  description: 'Description',
  manifesto: 'Manifesto',
  personality: 'Personality',
  scenarios: 'Scenario',
  exampleDialogues: 'Example Dialogues',
  firstMessage: 'First Message',
  systemPrompt: 'System Prompt',
  systemPrompts: 'System Prompt',
  physicalDescription: 'Physical Description',
  talkativeness: 'Talkativeness',
  wardrobeItems: 'Wardrobe',
  aliases: 'Alias',
  title: 'Title',
};

export const FIELD_BADGE_CLASS: Record<string, string> = {
  identity: 'qt-badge-primary',
  description: 'qt-badge-secondary',
  manifesto: 'qt-badge-primary',
  personality: 'qt-badge-character',
  scenarios: 'qt-badge-project',
  exampleDialogues: 'qt-badge-chat',
  firstMessage: 'qt-badge-message',
  systemPrompt: 'qt-badge-memory',
  systemPrompts: 'qt-badge-memory',
  physicalDescription: 'qt-badge-user-character',
  talkativeness: 'qt-badge-chat',
  wardrobeItems: 'qt-badge-user-character',
  aliases: 'qt-badge-primary',
  title: 'qt-badge-primary',
};

export const FIELD_HINT_KEYS: Partial<Record<string, PromptFieldHintKey>> = {
  identity: 'identity',
  description: 'description',
  manifesto: 'manifesto',
  personality: 'personality',
  scenarios: 'scenario',
  exampleDialogues: 'exampleDialogues',
  firstMessage: 'firstMessage',
  systemPrompt: 'systemPrompt',
  systemPrompts: 'systemPrompt',
  physicalDescription: 'physicalDescription',
};
