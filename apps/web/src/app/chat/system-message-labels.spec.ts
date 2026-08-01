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

  it('prefers the rendered chip label over the title — the per-run name of the deal', () => {
    // v4 `system-message-labels.test.ts` (c4d4b0de), case for case.
    expect(
      getSystemKindDisplayLabel(
        staff({
          pascalMeta: pascalMeta({
            tool: 'agent_lambda',
            toolTitle: 'Agent lambda',
            chipLabel: 'Agent lambda — Jackie',
          }),
        }),
      ),
    ).toBe('Agent lambda — Jackie');
  });

  it('ignores a blank chip label rather than showing an empty chip', () => {
    expect(
      getSystemKindDisplayLabel(
        staff({
          pascalMeta: pascalMeta({ tool: 'unlock', toolTitle: 'Force the Lock', chipLabel: '   ' }),
        }),
      ),
    ).toBe('Force the Lock');
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

/**
 * The legacy-kind inference (P4.26). v4's `inferKindFromContent` sniffs the
 * persona-voiced body of a row written before the `systemKind` column landed.
 * v5 had deferred it, so every such row lost BOTH halves of what a reader skims:
 * the kind label went blank and the importance dot fell back to the sender's
 * `'*'` tier. Ported verbatim; every branch of v4's switch is exercised here.
 */
describe('resolveRawKind — the legacy content inference (P4.26)', () => {
  const ann = (
    systemSender: NonNullable<MessageDto['systemSender']>,
    content: string,
  ): StaffFields => ({ systemSender, systemKind: null, content });

  const label = (sender: NonNullable<MessageDto['systemSender']>, content: string) =>
    getSystemKindDisplayLabel(ann(sender, content));
  const tier = (sender: NonNullable<MessageDto['systemSender']>, content: string) =>
    getAnnouncementImportance(ann(sender, content));

  /** v4 `system-message-labels.test.ts` — "infers the kind from content". */
  it('reproduces v4’s own inference cases', () => {
    expect(tier('host', 'The Host marks the time at half past three.')).toBe('low');
    expect(tier('host', 'The Host welcomes Beatrice to the table.')).toBe('high');
    expect(tier('librarian', 'The Librarian has set down a new volume, "Notes".')).toBe('high');
    expect(tier('librarian', 'The Librarian has filed fresh alterations to "Notes".')).toBe('high');
    expect(tier('librarian', 'The Librarian has relocated the volume "a.md".')).toBe('high');
  });

  it('infers every Host branch', () => {
    expect(label('host', 'The Host welcomes Beatrice.')).toBe('add');
    expect(label('host', 'The Host bids Beatrice farewell.')).toBe('remove');
    expect(label('host', 'The Host notes that Beatrice is now silent.')).toBe('status change');
    expect(label('host', 'The Host sets the scene: a foggy quay.')).toBe('scenario');
    expect(label('host', 'The Host outlines the company present.')).toBe('roster');
    expect(label('host', 'The Host marks the time at three.')).toBe('time');
    expect(label('host', 'The Host introduces your character.')).toBe('user character');
    expect(label('host', 'The Host inclines his head.')).toBe('nothing to add');
    expect(label('host', 'Beatrice, declining the floor, says nothing.')).toBe('nothing to add');
    expect(
      label('host', 'The Host turns to Beatrice and invites them to take the floor.'),
    ).toBe('invited to speak');
    expect(label('host', 'The Host whispers a private note: SILENT mode.')).toBe(
      'silent mode (entering)',
    );
    expect(label('host', 'The Host whispers a private note: the silence is lifted.')).toBe(
      'silent mode (leaving)',
    );
    expect(
      label('host', 'The Host whispers a private note, recounting how they came to be here.'),
    ).toBe('join scenario');
    // A private note matching none of the three → the generic Host label.
    expect(label('host', 'The Host whispers a private note about the weather.')).toBe(
      'announcement',
    );
    expect(label('host', 'Something else entirely.')).toBe('announcement');
  });

  it('infers every Librarian branch', () => {
    expect(label('librarian', 'The Librarian relocated the volume.')).toBe('moved');
    expect(label('librarian', 'The Librarian transcribed a copy.')).toBe('copied');
    expect(label('librarian', 'The Librarian set down a new volume.')).toBe('created');
    expect(label('librarian', 'The Librarian set down a fresh, empty page.')).toBe('created');
    expect(label('librarian', 'The Librarian affixed the illustration.')).toBe('blob written');
    expect(label('librarian', 'The Librarian affixed the asset.')).toBe('blob written');
    expect(label('librarian', 'The Librarian filed fresh alterations.')).toBe('edited');
    expect(label('librarian', 'The Librarian rechristened it.')).toBe('renamed');
    expect(label('librarian', 'The Librarian filed the following alterations.')).toBe('saved');
    expect(label('librarian', 'The Librarian removed "a.md".')).toBe('deleted');
    expect(label('librarian', 'The Librarian struck from the catalogue.')).toBe('deleted');
    expect(label('librarian', 'The Librarian set aside a fresh shelf.')).toBe('folder created');
    expect(label('librarian', 'The Librarian dismantled the empty shelf.')).toBe('folder deleted');
    expect(label('librarian', 'Laid upon the table for your perusal.')).toBe('attached');
    expect(label('librarian', 'The Librarian deposits a précis.')).toBe('summary');
    expect(label('librarian', 'The Librarian laid out a fresh, blank page.')).toBe('opened');
    expect(label('librarian', 'The Librarian has set out the volume.')).toBe('opened');
    expect(label('librarian', 'Something else entirely.')).toBe('announcement');
  });

  it('infers the remaining senders’ branches and per-sender defaults', () => {
    expect(label('lantern', 'The Lantern projected a new backdrop.')).toBe('background');
    expect(label('lantern', 'The Lantern, acting upon the instructions of Beatrice…')).toBe(
      'character image',
    );
    expect(label('lantern', 'The Lantern did something.')).toBe('image');

    expect(label('aurora', 'Aurora refreshed the portrait.')).toBe('avatar');
    expect(label('aurora', 'Aurora marks an alteration.')).toBe('outfit change');
    expect(label('aurora', 'Aurora pronounces upon their attire.')).toBe('opening outfit');
    expect(label('aurora', 'Aurora did something.')).toBe('wardrobe');

    expect(label('concierge', 'anything')).toBe('danger');

    expect(label('prospero', 'Prospero notes that the connection changed.')).toBe(
      'connection change',
    );
    expect(label('prospero', 'Prospero opens his ledger.')).toBe('project information');
    expect(label('prospero', 'Prospero did something.')).toBe('announcement');

    expect(label('commonplaceBook', 'The book lays open at your bookmark.')).toBe('memory recap');
    expect(label('commonplaceBook', 'The book turns to the entries.')).toBe('relevant memories');
    expect(
      label('commonplaceBook', 'The book opens to the pages where you have noted those present.'),
    ).toBe('inter-character memories');
    expect(label('commonplaceBook', 'The book did something.')).toBe('consolidated');

    expect(label('ariel', 'Ariel opened a terminal.')).toBe('terminal opened');
    expect(label('ariel', 'Ariel closed it.')).toBe('terminal closed');
    expect(label('ariel', 'Ariel did something.')).toBe('terminal');

    expect(label('suparna', 'anything')).toBe('mail delivery');
    expect(label('pascal', 'anything')).toBe('roll outcome');
    // Carina has no arm in v4's switch and falls out to the generic label.
    expect(label('carina', 'anything')).toBe('announcement');
  });

  it('keeps an explicit systemKind ahead of the inference', () => {
    // The body says "welcomes" (→ add / high) but the column says timestamp.
    const row: StaffFields = {
      systemSender: 'host',
      systemKind: 'timestamp',
      content: 'The Host welcomes Beatrice.',
    };
    expect(getSystemKindDisplayLabel(row)).toBe('time');
    expect(getAnnouncementImportance(row)).toBe('low');
  });

  it('still yields no label at all when there is no sender', () => {
    expect(getSystemKindDisplayLabel({ systemSender: null, systemKind: null, content: 'x' })).toBe(
      '',
    );
    expect(getAnnouncementImportance({ systemSender: null, systemKind: null, content: 'x' })).toBe(
      'medium',
    );
  });
});
