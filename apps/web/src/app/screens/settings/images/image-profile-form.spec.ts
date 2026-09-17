import { describe, expect, it } from 'vitest';

import type { ApiKeyDto, ImageProviderInfo } from '../../../core/core-contract';
import {
  availableApiKeys,
  defaultModelsFor,
  FALLBACK_PROVIDERS,
  formToImageProfileBody,
  imageProfileToForm,
  normalizeProviderName,
} from './image-profile-form';

const PROVIDERS: ImageProviderInfo[] = [
  {
    value: 'OPENAI',
    label: 'OpenAI',
    defaultModels: ['dall-e-3', 'dall-e-2'],
    apiKeyProvider: 'OPENAI',
    legacyNames: [],
  },
  {
    value: 'GOOGLE',
    label: 'Google Gemini',
    defaultModels: ['imagen-4.0-generate-001'],
    apiKeyProvider: 'GOOGLE',
    legacyNames: ['GOOGLE_IMAGEN'],
  },
];

function key(over: Partial<ApiKeyDto>): ApiKeyDto {
  return {
    id: 'k',
    provider: 'OPENAI',
    label: 'Key',
    isActive: true,
    lastUsed: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    keyPreview: 'sk-…',
    ...over,
  };
}

describe('normalizeProviderName', () => {
  it('passes a canonical value through', () => {
    expect(normalizeProviderName('OPENAI', PROVIDERS)).toBe('OPENAI');
  });

  it('maps a legacy value to its canonical provider', () => {
    expect(normalizeProviderName('GOOGLE_IMAGEN', PROVIDERS)).toBe('GOOGLE');
  });

  it('leaves an unknown value unchanged', () => {
    expect(normalizeProviderName('MYSTERY', PROVIDERS)).toBe('MYSTERY');
  });
});

describe('availableApiKeys', () => {
  it('keeps only the keys for the provider', () => {
    const keys = [
      key({ id: 'a', provider: 'OPENAI' }),
      key({ id: 'b', provider: 'GOOGLE' }),
      key({ id: 'c', provider: 'ANTHROPIC' }),
    ];
    expect(availableApiKeys(keys, 'OPENAI', PROVIDERS).map((k) => k.id)).toEqual(['a']);
  });

  it('matches keys stored under a legacy provider name for the canonical provider', () => {
    const keys = [key({ id: 'g', provider: 'GOOGLE' })];
    // Selecting the legacy GOOGLE_IMAGEN provider still surfaces the GOOGLE key.
    expect(availableApiKeys(keys, 'GOOGLE_IMAGEN', PROVIDERS).map((k) => k.id)).toEqual(['g']);
  });
});

describe('defaultModelsFor', () => {
  it('returns the provider default models', () => {
    expect(defaultModelsFor('OPENAI', PROVIDERS)).toEqual(['dall-e-3', 'dall-e-2']);
  });

  it('resolves through a legacy name', () => {
    expect(defaultModelsFor('GOOGLE_IMAGEN', PROVIDERS)).toEqual(['imagen-4.0-generate-001']);
  });
});

describe('formToImageProfileBody', () => {
  it('coerces empty apiKeyId / baseUrl to null and preserves parameter key order', () => {
    const body = formToImageProfileBody({
      name: '  My Profile  ',
      provider: 'OPENAI',
      apiKeyId: '',
      baseUrl: '',
      modelName: 'dall-e-3',
      parameters: { quality: 'hd', style: 'vivid', size: '1024x1024' },
      isDefault: true,
      isDangerousCompatible: false,
    });
    expect(body.name).toBe('My Profile');
    expect(body.apiKeyId).toBeNull();
    expect(body.baseUrl).toBeNull();
    expect(Object.keys(body.parameters)).toEqual(['quality', 'style', 'size']);
    expect(body.isDefault).toBe(true);
  });
});

describe('imageProfileToForm', () => {
  it('hydrates a profile into the form (null apiKeyId → empty string)', () => {
    const form = imageProfileToForm({
      id: 'p1',
      userId: 'u',
      name: 'DALL-E',
      provider: 'OPENAI',
      apiKeyId: null,
      baseUrl: null,
      modelName: 'dall-e-3',
      parameters: { quality: 'hd' },
      isDefault: false,
      isDangerousCompatible: true,
      tags: [],
      createdAt: '2024-01-01T00:00:00.000Z',
      updatedAt: '2024-01-01T00:00:00.000Z',
      apiKey: null,
    });
    expect(form.apiKeyId).toBe('');
    expect(form.isDangerousCompatible).toBe(true);
    expect(form.parameters).toEqual({ quality: 'hd' });
  });
});

/**
 * The `FALLBACK_PROVIDERS` rows, asserted verbatim against v4
 * `components/image-profiles/ImageProfileForm.tsx:48-53` (at `d8d2890ee`). Only
 * used when the `list-providers` fetch fails, but the strings are user-visible
 * the moment it does, so they are pinned character-for-character.
 */
describe('FALLBACK_PROVIDERS', () => {
  it('carries v4’s five providers in v4’s order', () => {
    expect(FALLBACK_PROVIDERS.map((p) => p.value)).toEqual([
      'OPENAI',
      'GROK',
      'GOOGLE',
      'Z_AI',
      'NANOGPT',
    ]);
  });

  it('carries the OpenAI row verbatim (v4 `d8d2890ee` — PR #62, GPT Image 2.5)', () => {
    const openai = FALLBACK_PROVIDERS.find((p) => p.value === 'OPENAI');
    expect(openai?.label).toBe('OpenAI (DALL-E / GPT Image)');
    expect(openai?.defaultModels).toEqual([
      'gpt-image-2.5-sunburst',
      'gpt-image-2.5-flare',
      'gpt-image-2',
      'gpt-image-1.5',
      'gpt-image-1',
      'gpt-image-1-mini',
      'dall-e-3',
      'dall-e-2',
    ]);
    expect(openai?.apiKeyProvider).toBe('OPENAI');
  });

  /**
   * v4's CLIENT file and its plugin capability table disagree about which of
   * `gpt-image-1` / `gpt-image-1-mini` comes first. This list is the CLIENT
   * one, so the spec pins the disagreement rather than quietly resolving it —
   * a future copy from the server's order would redden here and have to say so.
   */
  it('keeps v4’s client-side ordering of gpt-image-1 before gpt-image-1-mini', () => {
    const models = FALLBACK_PROVIDERS.find((p) => p.value === 'OPENAI')?.defaultModels ?? [];
    expect(models.indexOf('gpt-image-1')).toBeLessThan(models.indexOf('gpt-image-1-mini'));
  });

  it('carries the Z.AI row verbatim (v4 `ca22ec45`)', () => {
    const zai = FALLBACK_PROVIDERS.find((p) => p.value === 'Z_AI');
    expect(zai?.label).toBe('Z.AI (CogView / GLM-Image)');
    expect(zai?.defaultModels).toEqual(['cogview-4-250304', 'glm-image']);
    expect(zai?.apiKeyProvider).toBe('Z_AI');
  });

  it('carries the NanoGPT row verbatim (v4 `781fc420`)', () => {
    const nano = FALLBACK_PROVIDERS.find((p) => p.value === 'NANOGPT');
    expect(nano?.label).toBe('NanoGPT (Flux / HiDream / Recraft)');
    expect(nano?.defaultModels).toEqual([
      'hidream',
      'flux-2-flash',
      'flux-2-dev',
      'flux-2-pro',
      'recraft-v3',
      'gpt-image-1.5',
    ]);
    expect(nano?.apiKeyProvider).toBe('NANOGPT');
  });

  it('offers the new providers’ default models through defaultModelsFor', () => {
    expect(defaultModelsFor('Z_AI', FALLBACK_PROVIDERS)[0]).toBe('cogview-4-250304');
    expect(defaultModelsFor('NANOGPT', FALLBACK_PROVIDERS)[0]).toBe('hidream');
  });
});
