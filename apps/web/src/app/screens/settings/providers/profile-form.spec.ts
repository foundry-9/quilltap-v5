import { describe, expect, it } from 'vitest';

import type { ConnectionProfileDto } from '../../../core/core-contract';
import {
  buildProfileRequestBody,
  initialFormState,
  loadProfileIntoForm,
  type ProfileFormData,
} from './profile-form';

function profile(over: Partial<ConnectionProfileDto>): ConnectionProfileDto {
  return {
    id: 'cp1',
    name: 'A profile',
    provider: 'OPENAI',
    modelName: 'gpt-4',
    parameters: {},
    isDefault: false,
    ...over,
  };
}

function form(over: Partial<ProfileFormData>): ProfileFormData {
  return { ...initialFormState, ...over };
}

/**
 * The multi-character turn anchor through the form (v4 `23af7146`'s
 * `useProfileForm` hunks — the oracle is v4's client, so each case cites the
 * line it mirrors).
 */
describe('profile form — multiCharacterPrefill', () => {
  it('a new form starts ticked (v4 `initialFormState`, types.ts:151)', () => {
    expect(initialFormState.multiCharacterPrefill).toBe(true);
  });

  it('a stored true/false loads verbatim (v4 useProfileForm.ts:75-78)', () => {
    expect(
      loadProfileIntoForm(profile({ multiCharacterPrefill: true })).multiCharacterPrefill,
    ).toBe(true);
    expect(
      loadProfileIntoForm(profile({ multiCharacterPrefill: false })).multiCharacterPrefill,
    ).toBe(false);
    // The tri-state's whole point: a deliberate OFF on a provider that defaults
    // ON must not be read back as ON.
    expect(
      loadProfileIntoForm(profile({ provider: 'OPENAI', multiCharacterPrefill: false }))
        .multiCharacterPrefill,
    ).toBe(false);
  });

  it('a null/absent stored value shows the PROVIDER DEFAULT, not a blanket true', () => {
    // "Show the provider default the server would resolve to, so the box
    // reflects actual behaviour" — v4's comment on the same line.
    expect(
      loadProfileIntoForm(profile({ provider: 'ANTHROPIC', multiCharacterPrefill: null }))
        .multiCharacterPrefill,
    ).toBe(false);
    expect(loadProfileIntoForm(profile({ provider: 'ANTHROPIC' })).multiCharacterPrefill).toBe(
      false,
    );
    expect(loadProfileIntoForm(profile({ provider: 'OPENAI' })).multiCharacterPrefill).toBe(true);
  });

  it('the API create/update body carries the box (v4 useProfileForm.ts:141)', () => {
    expect(
      buildProfileRequestBody(form({ multiCharacterPrefill: true }))['multiCharacterPrefill'],
    ).toBe(true);
    expect(
      buildProfileRequestBody(form({ multiCharacterPrefill: false }))['multiCharacterPrefill'],
    ).toBe(false);
  });

  it('the COURIER body carries it too — the one flag v4 does not force false', () => {
    // v4 `useProfileForm.ts:106-109`: `isDangerousCompatible` and
    // `allowToolUse` are hardcoded false in this branch and this one is read
    // off the form, because "the Courier renders the same assembled context
    // for the user to carry by hand, so the turn anchor still applies". Both
    // values asserted: a hardcoded `true` would pass the first arm alone.
    const on = buildProfileRequestBody(form({ transport: 'courier', multiCharacterPrefill: true }));
    const off = buildProfileRequestBody(
      form({ transport: 'courier', multiCharacterPrefill: false }),
    );
    expect(on['multiCharacterPrefill']).toBe(true);
    expect(off['multiCharacterPrefill']).toBe(false);
    // ... while its neighbours stay forced, in both.
    expect(on['allowToolUse']).toBe(false);
    expect(off['isDangerousCompatible']).toBe(false);
  });
});
