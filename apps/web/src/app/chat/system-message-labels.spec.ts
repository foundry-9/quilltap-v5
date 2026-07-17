import { describe, expect, it } from 'vitest';

import type { MessageDto, PascalMeta } from '../core/core-contract';
import {
  getAnnouncementImportance,
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
