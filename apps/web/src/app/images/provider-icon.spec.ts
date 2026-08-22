import { describe, expect, it } from 'vitest';

import { providerIconDefaults } from './provider-icon';

/**
 * `PROVIDER_DEFAULTS`, asserted against v4
 * `components/image-profiles/ProviderIcon.tsx:52-63` (at `d5830439`). The two
 * rows added by v4 `ca22ec45` / `781fc420` are pinned by name because without
 * them both providers fall through to the generic three-character abbreviation
 * — `Z_A` and `NAN` — which is what v5 rendered before this lane.
 */
describe('providerIconDefaults', () => {
  it('carries the Z.AI row (v4 `ca22ec45`)', () => {
    expect(providerIconDefaults('Z_AI')).toEqual({ color: 'qt-text-success', abbrev: 'ZAI' });
  });

  it('carries the NanoGPT row (v4 `781fc420`)', () => {
    expect(providerIconDefaults('NANOGPT')).toEqual({ color: 'qt-text-primary', abbrev: 'NGPT' });
  });

  it('still falls through for a genuinely unknown provider (v4 :170-173)', () => {
    expect(providerIconDefaults('MYSTERY')).toEqual({
      color: 'qt-text-secondary',
      abbrev: 'MYS',
    });
  });

  it('keeps the pre-existing rows untouched', () => {
    expect(providerIconDefaults('OPENAI')).toEqual({ color: 'qt-text-success', abbrev: 'OAI' });
    expect(providerIconDefaults('OLLAMA')).toEqual({ color: 'qt-text-secondary', abbrev: 'OLL' });
  });
});
