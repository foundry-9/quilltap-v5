import { describe, expect, it } from 'vitest';

import { getSystemSenderDisplayName } from './system-message-labels';
import { STAFF_DISPLAY_NAMES, staffDisplayName } from './staff-display-names';

/**
 * v4 `lib/chat/staff-display-names.ts`, extracted by `0246c6c8` so the Salon's
 * announcement chip and every later transcript surface read one table.
 */

describe('staffDisplayName (v4 lib/chat/staff-display-names @ 0246c6c8)', () => {
  it('names all eleven Staff members exactly as v4 does', () => {
    expect(STAFF_DISPLAY_NAMES).toEqual({
      lantern: 'The Lantern',
      aurora: 'Aurora',
      librarian: 'The Librarian',
      concierge: 'The Concierge',
      prospero: 'Prospero',
      host: 'The Host',
      commonplaceBook: 'The Commonplace Book',
      ariel: 'Ariel',
      carina: 'Carina',
      // The diacritic is part of the name, not decoration.
      suparna: 'Suparṇā',
      pascal: 'Pascal',
    });
  });

  it('returns the empty string for an ordinary participant message', () => {
    expect(staffDisplayName(null)).toBe('');
    expect(staffDisplayName(undefined)).toBe('');
    expect(staffDisplayName('')).toBe('');
  });

  it('falls back to the raw tag for a sender this build does not know', () => {
    // A row written by a newer build must still show something rather than
    // vanish — part of the contract, not an accident of the lookup.
    expect(staffDisplayName('quartermaster')).toBe('quartermaster');
  });

  it('is the one table the Salon labels read', () => {
    for (const [sender, name] of Object.entries(STAFF_DISPLAY_NAMES)) {
      expect(getSystemSenderDisplayName(sender as never)).toBe(name);
    }
    expect(getSystemSenderDisplayName(null)).toBe('');
  });
});
