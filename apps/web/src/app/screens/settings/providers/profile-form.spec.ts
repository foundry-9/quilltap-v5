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

/**
 * The `parameters` bag round-trip (P4.D81 unit 3). v5 renders one provider
 * option (Ollama's `enable_thinking`) and no others, so the bag's job here is
 * mostly to survive a save — every key the SPA shows no control for still has a
 * reader on the wire side.
 */
describe('profile form — the parameters bag', () => {
  it('loads the blob minus the three sampling keys the form owns', () => {
    const loaded = loadProfileIntoForm(
      profile({
        parameters: {
          temperature: 0.4,
          max_tokens: 512,
          top_p: 0.9,
          enable_thinking: true,
          num_ctx: 32768,
          providerPreferences: { order: ['groq'] },
        },
      }),
    );
    expect(loaded.temperature).toBe(0.4);
    expect(loaded.maxTokens).toBe(512);
    expect(loaded.topP).toBe(0.9);
    expect(loaded.parameters).toEqual({
      enable_thinking: true,
      num_ctx: 32768,
      providerPreferences: { order: ['groq'] },
    });
  });

  it('does not mutate the DTO it loaded from', () => {
    const dto = profile({ parameters: { temperature: 0.4, num_ctx: 8192 } });
    loadProfileIntoForm(dto);
    expect(dto.parameters).toEqual({ temperature: 0.4, num_ctx: 8192 });
  });

  it('sends the sampling controls AND the rest of the bag on save', () => {
    const body = buildProfileRequestBody(
      form({
        temperature: 0.7,
        maxTokens: 2048,
        topP: 0.5,
        parameters: { enable_thinking: true, num_ctx: 16384 },
      }),
    );
    expect(body['parameters']).toEqual({
      temperature: 0.7,
      max_tokens: 2048,
      top_p: 0.5,
      enable_thinking: true,
      num_ctx: 16384,
    });
  });

  it('round-trips an untouched profile byte for byte', () => {
    // The regression this exists for: before unit 3 the builder wrote a fresh
    // three-key bag, so opening an Ollama profile and pressing Update silently
    // dropped `enable_thinking` and `num_ctx`.
    const stored = {
      temperature: 1,
      max_tokens: 1000,
      top_p: 1,
      enable_thinking: true,
      num_ctx: 40960,
    };
    const body = buildProfileRequestBody(loadProfileIntoForm(profile({ parameters: stored })));
    expect(body['parameters']).toEqual(stored);
  });

  it('the courier body still sends an empty bag (v4 :115)', () => {
    expect(
      buildProfileRequestBody(
        form({ transport: 'courier', parameters: { enable_thinking: true } }),
      )['parameters'],
    ).toEqual({});
  });
});
