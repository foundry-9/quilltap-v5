import { describe, expect, it } from 'vitest';

import { FIELD_BADGE_CLASS, FIELD_HINT_KEYS, FIELD_LABELS } from './field-meta';

/**
 * Parity spec — a SECOND, independently-typed transcription of v4
 * `components/characters/optimizer/field-meta.ts` (whole file) and
 * `components/prompt-fields/field-hints.ts:128-139`, so `field-meta.ts` and
 * this spec cannot drift into agreement by accident.
 */
const V4_FIELD_LABELS: Record<string, string> = {
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

const V4_FIELD_BADGE_CLASS: Record<string, string> = {
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

const V4_FIELD_HINT_KEYS: Record<string, string> = {
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

describe('optimizer field-meta — v4 transcription parity', () => {
  it('FIELD_LABELS matches the independent v4 transcription byte-for-byte', () => {
    expect(FIELD_LABELS).toEqual(V4_FIELD_LABELS);
  });
  it('FIELD_BADGE_CLASS matches the independent v4 transcription byte-for-byte', () => {
    expect(FIELD_BADGE_CLASS).toEqual(V4_FIELD_BADGE_CLASS);
  });
  it('FIELD_HINT_KEYS matches the independent v4 transcription byte-for-byte', () => {
    expect(FIELD_HINT_KEYS).toEqual(V4_FIELD_HINT_KEYS);
  });
  it('every FIELD_LABELS key has a badge class (no field renders unbadged)', () => {
    for (const key of Object.keys(FIELD_LABELS)) {
      expect(FIELD_BADGE_CLASS[key]).toBeDefined();
    }
  });
});
