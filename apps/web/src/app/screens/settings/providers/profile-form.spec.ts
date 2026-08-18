import { describe, expect, it } from 'vitest';

import type { ApiKeyDto, ConnectionProfileDto, ProviderInfo } from '../../../core/core-contract';
import {
  buildProfileRequestBody,
  initialFormState,
  loadProfileIntoForm,
  outboundApiKeyId,
  outboundBaseUrl,
  type ProfileFormData,
} from './profile-form';

function providerInfo(
  name: string,
  requiresBaseUrl: boolean,
  baseUrlDefault?: string,
): ProviderInfo {
  return {
    id: name,
    name,
    displayName: name,
    description: '',
    abbreviation: name.slice(0, 2),
    type: 'llm',
    capabilities: { chat: true, imageGeneration: false, embeddings: false, webSearch: false },
    configRequirements: { requiresApiKey: !requiresBaseUrl, requiresBaseUrl, baseUrlDefault },
  };
}

/** The three providers v4's own Bug-73 cases use, same requirements. */
const PROVIDERS: ProviderInfo[] = [
  providerInfo('OPENAI', false),
  providerInfo('OLLAMA', true, 'http://localhost:11434'),
  providerInfo('OPENAI_COMPATIBLE', true, 'http://localhost:8080/v1'),
];

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
      buildProfileRequestBody(form({ multiCharacterPrefill: true }), PROVIDERS)[
        'multiCharacterPrefill'
      ],
    ).toBe(true);
    expect(
      buildProfileRequestBody(form({ multiCharacterPrefill: false }), PROVIDERS)[
        'multiCharacterPrefill'
      ],
    ).toBe(false);
  });

  it('the COURIER body carries it too — the one flag v4 does not force false', () => {
    // v4 `useProfileForm.ts:106-109`: `isDangerousCompatible` and
    // `allowToolUse` are hardcoded false in this branch and this one is read
    // off the form, because "the Courier renders the same assembled context
    // for the user to carry by hand, so the turn anchor still applies". Both
    // values asserted: a hardcoded `true` would pass the first arm alone.
    const on = buildProfileRequestBody(
      form({ transport: 'courier', multiCharacterPrefill: true }),
      PROVIDERS,
    );
    const off = buildProfileRequestBody(
      form({ transport: 'courier', multiCharacterPrefill: false }),
      PROVIDERS,
    );
    expect(on['multiCharacterPrefill']).toBe(true);
    expect(off['multiCharacterPrefill']).toBe(false);
    // ... while its neighbours stay forced, in both.
    expect(on['allowToolUse']).toBe(false);
    expect(off['isDangerousCompatible']).toBe(false);
  });
});

/**
 * The `parameters` bag round-trip (P4.D81 unit 3, extended at P4.D84 when the
 * schema renderer landed). The bag feeds `ProviderOptionsPanel` and must
 * survive a save: every key the active schema shows no control for still has a
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
        },
      }),
    );
    expect(loaded.temperature).toBe(0.4);
    expect(loaded.maxTokens).toBe(512);
    expect(loaded.topP).toBe(0.9);
    expect(loaded.parameters).toEqual({ enable_thinking: true, num_ctx: 32768 });
  });

  /**
   * v4's legacy-OpenRouter translation (`useProfileForm.ts:51-57`), ported at
   * P4.D84 with the schema renderer it exists to feed: without it an old
   * profile would show its ZDR box unticked while the wire still denied data
   * collection.
   */
  it('translates the legacy providerPreferences shape into the flat enableZDR key', () => {
    const loaded = loadProfileIntoForm(
      profile({ parameters: { providerPreferences: { dataCollection: 'deny' } } }),
    );
    expect(loaded.parameters).toEqual({ enableZDR: true });
  });

  it('drops the nested key without inventing a flag when it says nothing about ZDR', () => {
    const loaded = loadProfileIntoForm(
      profile({ parameters: { providerPreferences: { order: ['groq'] } } }),
    );
    expect(loaded.parameters).toEqual({});
  });

  it('leaves an explicit enableZDR alone rather than overwriting it (v4 `:54`)', () => {
    const loaded = loadProfileIntoForm(
      profile({
        parameters: { enableZDR: false, providerPreferences: { dataCollection: 'deny' } },
      }),
    );
    expect(loaded.parameters).toEqual({ enableZDR: false });
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
      PROVIDERS,
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
    const body = buildProfileRequestBody(
      loadProfileIntoForm(profile({ parameters: stored })),
      PROVIDERS,
    );
    expect(body['parameters']).toEqual(stored);
  });

  it('the courier body sends no baseUrl key at all (v4 `:113-138` returns early)', () => {
    // The courier branch returns before the chokepoint, in both apps: provider,
    // api key and base URL are all unused in that mode.
    const body = buildProfileRequestBody(
      form({ transport: 'courier', provider: 'OLLAMA', baseUrl: 'http://localhost:11434' }),
      PROVIDERS,
    );
    expect(body).not.toHaveProperty('baseUrl');
  });

  it('the courier body still sends an empty bag (v4 :115)', () => {
    expect(
      buildProfileRequestBody(
        form({ transport: 'courier', parameters: { enable_thinking: true } }),
        PROVIDERS,
      )['parameters'],
    ).toEqual({});
  });
});

/**
 * Bug 73 — the base URL as it is allowed to leave the form.
 *
 * The unit half of v4's `profile-modal-base-url.test.tsx` (`d123658d`): the
 * chokepoint itself and the always-send save body. The gesture half — driving
 * the provider dropdown and reading the bytes that leave — is in
 * `profile-modal.spec.ts`.
 */
describe('profile form — outboundBaseUrl (Bug 73)', () => {
  it('drops a base URL the resolved provider does not take', () => {
    expect(outboundBaseUrl(PROVIDERS, 'OPENAI', 'http://localhost:11434')).toBe('');
  });

  it('still sends it on a provider that does take one', () => {
    expect(outboundBaseUrl(PROVIDERS, 'OLLAMA', 'http://localhost:11434')).toBe(
      'http://localhost:11434',
    );
  });

  it('leaves a stored base URL alone when the provider list has not loaded', () => {
    // The tab renders the modal before the providers listing answers, and the
    // fetch can fail outright. An unknown provider is not evidence that it
    // takes no base URL, so an existing Ollama profile must not be cleared by a
    // save (v4 `useProfileForm.ts:56`).
    expect(outboundBaseUrl([], 'OLLAMA', 'http://localhost:11434')).toBe('http://localhost:11434');
    expect(outboundBaseUrl(PROVIDERS, 'SOME_PLUGIN', 'http://box.local:9090/v1')).toBe(
      'http://box.local:9090/v1',
    );
  });

  it('answers the empty string, never undefined, when there is nothing to send', () => {
    expect(outboundBaseUrl(PROVIDERS, 'OPENAI', '')).toBe('');
    expect(outboundBaseUrl([], 'OLLAMA', '')).toBe('');
  });

  it('ALWAYS sends the baseUrl key on the save body, empty when the provider takes none', () => {
    // ⚠ The mutation proof for the always-send spelling: omitting the key (what
    // v5 did before this lane, and v4 before `d123658d`) leaves the update
    // handler's `baseUrl !== undefined` gate untripped, so every already
    // poisoned row stays broken forever with no gesture that clears it.
    const cleared = buildProfileRequestBody(
      form({ provider: 'OPENAI', baseUrl: 'http://localhost:11434' }),
      PROVIDERS,
    );
    expect(Object.prototype.hasOwnProperty.call(cleared, 'baseUrl')).toBe(true);
    expect(cleared['baseUrl']).toBe('');

    // ...and a fresh profile that never had one still sends the key.
    const fresh = buildProfileRequestBody(form({ provider: 'OPENAI', baseUrl: '' }), PROVIDERS);
    expect(Object.prototype.hasOwnProperty.call(fresh, 'baseUrl')).toBe(true);
    expect(fresh['baseUrl']).toBe('');
  });

  it('carries the real value through on a provider that takes one', () => {
    const body = buildProfileRequestBody(
      form({ provider: 'OLLAMA', baseUrl: 'http://localhost:11434' }),
      PROVIDERS,
    );
    expect(body['baseUrl']).toBe('http://localhost:11434');
  });

  it('carries an unknown provider’s stored URL verbatim onto the save body', () => {
    const body = buildProfileRequestBody(
      form({ provider: 'OLLAMA', baseUrl: 'http://localhost:11434' }),
      [],
    );
    expect(body['baseUrl']).toBe('http://localhost:11434');
  });
});

/**
 * Bug 76 — the api key as it is allowed to leave the form.
 *
 * The unit half of v4's `__tests__/unit/components/settings/
 * profile-modal-api-key.test.tsx` (`8bd802a3`): the chokepoint itself and the
 * always-send save body. The gesture half — driving the provider dropdown and
 * reading the bytes that leave — is in `profile-modal.spec.ts`.
 *
 * This closes this port's own dogfood finding #90, filed 2026-08-17 against a
 * faithfully ported v4 defect and fixed v4-side at `8bd802a3`.
 */
describe('profile form — outboundApiKeyId (Bug 76)', () => {
  /** v4's own three providers, same requirements (`profile-modal-api-key.test.tsx:26-49`). */
  function keyProvider(
    name: string,
    requiresApiKey: boolean,
    requiresBaseUrl: boolean,
  ): ProviderInfo {
    return {
      id: name,
      name,
      displayName: name,
      description: '',
      abbreviation: name.slice(0, 2),
      type: 'llm',
      capabilities: { chat: true, imageGeneration: false, embeddings: false, webSearch: false },
      configRequirements: { requiresApiKey, requiresBaseUrl },
    };
  }

  const KEY_PROVIDERS: ProviderInfo[] = [
    keyProvider('ANTHROPIC', true, false),
    keyProvider('OPENAI', true, false),
    keyProvider('OLLAMA', false, true),
  ];

  function apiKey(id: string, provider: string): ApiKeyDto {
    return {
      id,
      provider,
      label: `${provider} key`,
      isActive: true,
      lastUsed: null,
      createdAt: '2026-01-01T00:00:00.000Z',
      updatedAt: '2026-01-01T00:00:00.000Z',
      keyPreview: 'sk-…abcd',
    };
  }

  const API_KEYS: ApiKeyDto[] = [
    apiKey('key-anthropic', 'ANTHROPIC'),
    apiKey('key-openai', 'OPENAI'),
  ];

  it('drops the key on a provider that renders no key control at all', () => {
    // Refusal count 1: OLLAMA declares `requiresApiKey: false`, so the select
    // is not rendered — nothing on screen could show the stored id.
    expect(outboundApiKeyId(KEY_PROVIDERS, API_KEYS, 'OLLAMA', 'key-anthropic')).toBe('');
  });

  it('drops a key belonging to a DIFFERENT hosted provider', () => {
    // Refusal count 2: the select's options are filtered to the current
    // provider, so an Anthropic key on OPENAI matches nothing and reads blank.
    expect(outboundApiKeyId(KEY_PROVIDERS, API_KEYS, 'OPENAI', 'key-anthropic')).toBe('');
  });

  it('still sends a key the select would list for this provider', () => {
    expect(outboundApiKeyId(KEY_PROVIDERS, API_KEYS, 'OPENAI', 'key-openai')).toBe('key-openai');
    expect(outboundApiKeyId(KEY_PROVIDERS, API_KEYS, 'ANTHROPIC', 'key-anthropic')).toBe(
      'key-anthropic',
    );
  });

  it('leaves a stored key alone when the PROVIDER list has not loaded', () => {
    // Absence is not evidence: an unloaded (or failed) provider fetch is no
    // reason to judge the provider keyless (v4's `known &&` guard). The key
    // itself still has to belong to the stored provider, so the case that
    // survives is the one where it does.
    expect(outboundApiKeyId([], API_KEYS, 'ANTHROPIC', 'key-anthropic')).toBe('key-anthropic');
  });

  it('an unknown provider still faces the displayability filter once keys HAVE loaded', () => {
    // The `known &&` guard only spares the keyless refusal; the select's option
    // filter is unconditional above an empty list, and a plugin provider with no
    // key of its own reads blank exactly as any other mismatch does. Measured
    // against v4's chokepoint rather than assumed — the two absence arms guard
    // different things.
    expect(outboundApiKeyId(KEY_PROVIDERS, API_KEYS, 'SOME_PLUGIN', 'key-anthropic')).toBe('');
    // ...and with a key that DOES name the plugin provider, it goes out.
    expect(
      outboundApiKeyId(
        KEY_PROVIDERS,
        [...API_KEYS, apiKey('key-plugin', 'SOME_PLUGIN')],
        'SOME_PLUGIN',
        'key-plugin',
      ),
    ).toBe('key-plugin');
  });

  it('leaves a stored key alone when the KEY list has not loaded', () => {
    // The other absence arm: an empty key list means "not loaded", never "no
    // keys exist" — stripping a key off a working profile would be the worse
    // bug (v4's `apiKeys.length > 0` guard).
    expect(outboundApiKeyId(KEY_PROVIDERS, [], 'OPENAI', 'key-anthropic')).toBe('key-anthropic');
  });

  it('the keyless refusal still wins when the key list has not loaded', () => {
    // Ordering matters: v4 asks the provider first, so a keyless provider drops
    // the id whether or not the key list ever answered.
    expect(outboundApiKeyId(KEY_PROVIDERS, [], 'OLLAMA', 'key-anthropic')).toBe('');
  });

  it('answers the empty string, never undefined, when there is nothing to send', () => {
    expect(outboundApiKeyId(KEY_PROVIDERS, API_KEYS, 'OPENAI', '')).toBe('');
    expect(outboundApiKeyId([], [], 'OPENAI', '')).toBe('');
  });

  it('ALWAYS sends the apiKeyId key on the save body, null when the provider cannot use it', () => {
    // ⚠ The mutation proof for the always-send spelling: omitting the key (what
    // v5 did before this lane, and v4 before `8bd802a3`) leaves the update
    // handler's `apiKeyId !== undefined` gate untripped, so every already
    // poisoned row stays broken and refused forever with no gesture that
    // clears it.
    const cleared = buildProfileRequestBody(
      form({ provider: 'OLLAMA', apiKeyId: 'key-anthropic' }),
      KEY_PROVIDERS,
      API_KEYS,
    );
    expect(Object.prototype.hasOwnProperty.call(cleared, 'apiKeyId')).toBe(true);
    expect(cleared['apiKeyId']).toBeNull();

    // ...and a fresh profile that never had one still sends the key.
    const fresh = buildProfileRequestBody(
      form({ provider: 'OPENAI', apiKeyId: '' }),
      KEY_PROVIDERS,
      API_KEYS,
    );
    expect(Object.prototype.hasOwnProperty.call(fresh, 'apiKeyId')).toBe(true);
    expect(fresh['apiKeyId']).toBeNull();
  });

  it('carries the real value through on a provider that takes it', () => {
    const body = buildProfileRequestBody(
      form({ provider: 'OPENAI', apiKeyId: 'key-openai' }),
      KEY_PROVIDERS,
      API_KEYS,
    );
    expect(body['apiKeyId']).toBe('key-openai');
  });

  it('carries a stored key verbatim onto the save body when neither list has loaded', () => {
    const body = buildProfileRequestBody(
      form({ provider: 'ANTHROPIC', apiKeyId: 'key-anthropic' }),
      [],
      [],
    );
    expect(body['apiKeyId']).toBe('key-anthropic');
  });

  it('leaves the courier branch’s pinned null alone', () => {
    // The courier transport forces `apiKeyId: null` regardless — the chokepoint
    // never runs there (v4 `useProfileForm.ts:104`).
    const body = buildProfileRequestBody(
      form({ transport: 'courier', provider: 'ANTHROPIC', apiKeyId: 'key-anthropic' }),
      KEY_PROVIDERS,
      API_KEYS,
    );
    expect(body['apiKeyId']).toBeNull();
  });
});
