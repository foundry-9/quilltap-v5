import { describe, expect, it } from 'vitest';

import type { MessageDto, PascalMeta } from '../core/core-contract';
import {
  getAnnouncementAccentClasses,
  getAnnouncementImportance,
  getAnnouncementOutcomeState,
  getSystemKindDisplayLabel,
  getSystemSenderDisplayName,
} from './system-message-labels';

type StaffFields = Pick<MessageDto, 'systemSender' | 'systemKind' | 'content' | 'pascalMeta'>;

const staff = (overrides: Partial<StaffFields> = {}): StaffFields => ({
  systemSender: 'pascal',
  systemKind: 'custom-tool-result',
  content: '',
  ...overrides,
});

const pascalMeta = (overrides: Partial<PascalMeta> = {}): PascalMeta => ({
  tool: 'scan_hawking_radiation',
  definitionTier: 'global',
  definitionMountId: 'm1',
  params: {},
  rollForm: 'range',
  raw: 5,
  value: 5,
  state: 'success',
  outcomeIndex: 0,
  invokedBy: 'user',
  ...overrides,
});

describe('system-message-labels — Pascal (P4.6ba)', () => {
  it('names Pascal in the sender map', () => {
    expect(getSystemSenderDisplayName('pascal')).toBe('Pascal');
  });

  it("labels a roll outcome by the tool's title, not the generic kind", () => {
    const label = getSystemKindDisplayLabel(
      staff({ pascalMeta: pascalMeta({ toolTitle: 'Scan Hawking Radiation' }) }),
    );
    expect(label).toBe('Scan Hawking Radiation');
  });

  it('falls back to the declaration name when a legacy row has no toolTitle', () => {
    const label = getSystemKindDisplayLabel(staff({ pascalMeta: pascalMeta({ toolTitle: undefined }) }));
    expect(label).toBe('scan_hawking_radiation');
  });

  it('falls back to the static "roll outcome" label when no pascalMeta is present', () => {
    expect(getSystemKindDisplayLabel(staff({ pascalMeta: null }))).toBe('roll outcome');
  });

  it('labels a custom-tool-error with the table copy', () => {
    expect(getSystemKindDisplayLabel(staff({ systemKind: 'custom-tool-error', pascalMeta: null }))).toBe(
      "the table couldn't deal",
    );
  });

  it('rates roll outcomes and errors high', () => {
    expect(getAnnouncementImportance(staff())).toBe('high');
    expect(getAnnouncementImportance(staff({ systemKind: 'custom-tool-error' }))).toBe('high');
    expect(getAnnouncementImportance(staff({ systemKind: 'anything-else' }))).toBe('high');
  });
});

/** v4 `system-message-labels.test.ts` (231be14c), case for case. */
describe('getAnnouncementOutcomeState / getAnnouncementAccentClasses (P4.d21)', () => {
  const roll = (meta: Partial<PascalMeta> | null) =>
    ({
      systemSender: 'pascal',
      pascalMeta: meta as PascalMeta | null,
    }) as Pick<StaffFields, 'systemSender' | 'pascalMeta'>;

  it('reports the state the roll landed on', () => {
    for (const state of ['success', 'partial', 'failure', 'info'] as const) {
      expect(getAnnouncementOutcomeState(roll({ state }))).toBe(state);
      expect(getAnnouncementAccentClasses(roll({ state }))).toBe(
        `qt-pascal-result qt-pascal-result--${state}`,
      );
    }
  });

  it('leaves every other Staff sender unaccented', () => {
    expect(getAnnouncementOutcomeState({ systemSender: 'librarian', pascalMeta: null })).toBeNull();
    expect(getAnnouncementAccentClasses({ systemSender: 'host', pascalMeta: null })).toBe('');
    // Prospero authors the custom-tool ERROR chip, and it carries no roll record.
    expect(getAnnouncementAccentClasses({ systemSender: 'prospero', pascalMeta: null })).toBe('');
  });

  it('falls back to the importance dot on a roll record with no usable state', () => {
    expect(getAnnouncementOutcomeState(roll(null))).toBeNull();
    expect(getAnnouncementOutcomeState(roll({}))).toBeNull();
    // A state from a future build this one doesn't know how to colour.
    expect(getAnnouncementOutcomeState(roll({ state: 'triumph' as 'success' }))).toBeNull();
    expect(getAnnouncementAccentClasses(roll({ state: 'triumph' as 'success' }))).toBe('');
  });
});
