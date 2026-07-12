import { describe, expect, it } from 'vitest';

import type { ApiKeyDto, ImageProviderInfo } from '../../../core/core-contract';
import {
  availableApiKeys,
  defaultModelsFor,
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
