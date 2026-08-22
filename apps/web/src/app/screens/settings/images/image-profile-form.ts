/**
 * The image-profile form model + the provider-normalization / API-key-filtering
 * helpers, transcribed from v4 `components/image-profiles/ImageProfileForm.tsx`
 * (`FALLBACK_PROVIDERS`, `normalizeProviderName`, `getAvailableApiKeys`). Pure —
 * exercised by `image-profile-form.spec.ts`.
 */

import type { ApiKeyDto, ImageProfileDto, ImageProviderInfo } from '../../../core/core-contract';

/**
 * v4 `FALLBACK_PROVIDERS` — used only when the `list-providers` fetch fails.
 * (Model lists are v4-verbatim and intentionally stale vs the runtime registry.)
 */
export const FALLBACK_PROVIDERS: ImageProviderInfo[] = [
  {
    value: 'OPENAI',
    label: 'OpenAI (DALL-E / GPT Image)',
    defaultModels: ['gpt-image-2', 'gpt-image-1', 'dall-e-3', 'dall-e-2'],
    apiKeyProvider: 'OPENAI',
    legacyNames: [],
  },
  {
    value: 'GROK',
    label: 'Grok (xAI)',
    defaultModels: ['grok-2-image'],
    apiKeyProvider: 'GROK',
    legacyNames: [],
  },
  {
    value: 'GOOGLE',
    label: 'Google Gemini',
    defaultModels: [
      'imagen-4.0-generate-001',
      'imagen-3.0-generate-002',
      'imagen-3.0-fast-generate-001',
    ],
    apiKeyProvider: 'GOOGLE',
    legacyNames: ['GOOGLE_IMAGEN'],
  },
  {
    value: 'Z_AI',
    label: 'Z.AI (CogView / GLM-Image)',
    defaultModels: ['cogview-4-250304', 'glm-image'],
    apiKeyProvider: 'Z_AI',
    legacyNames: [],
  },
  {
    value: 'NANOGPT',
    label: 'NanoGPT (Flux / HiDream / Recraft)',
    defaultModels: [
      'hidream',
      'flux-2-flash',
      'flux-2-dev',
      'flux-2-pro',
      'recraft-v3',
      'gpt-image-1.5',
    ],
    apiKeyProvider: 'NANOGPT',
    legacyNames: [],
  },
];

/**
 * v4 `normalizeProviderName` — map a (possibly legacy) provider value to its
 * canonical registry value (e.g. `GOOGLE_IMAGEN` → `GOOGLE`); unknown values
 * pass through unchanged.
 */
export function normalizeProviderName(provider: string, providers: ImageProviderInfo[]): string {
  const match = providers.find(
    (p) => p.value === provider || (p.legacyNames ?? []).includes(provider),
  );
  return match?.value ?? provider;
}

/**
 * v4 `getAvailableApiKeys` — the API keys eligible for the selected provider:
 * keys whose `provider` equals the provider's `apiKeyProvider` (or the
 * normalized provider value, when the registry has no entry).
 */
export function availableApiKeys(
  apiKeys: ApiKeyDto[],
  provider: string,
  providers: ImageProviderInfo[],
): ApiKeyDto[] {
  const normalized = normalizeProviderName(provider, providers);
  const info = providers.find((p) => p.value === normalized);
  const expected = info?.apiKeyProvider || normalized;
  return apiKeys.filter((k) => k.provider === expected || k.provider === normalized);
}

/** The default models to offer for a provider (registry `defaultModels`). */
export function defaultModelsFor(provider: string, providers: ImageProviderInfo[]): string[] {
  const normalized = normalizeProviderName(provider, providers);
  return providers.find((p) => p.value === normalized)?.defaultModels ?? [];
}

/** The editable image-profile form (v4 `formData`). */
export interface ImageProfileFormData {
  name: string;
  provider: string;
  apiKeyId: string;
  baseUrl: string;
  modelName: string;
  parameters: Record<string, unknown>;
  isDefault: boolean;
  isDangerousCompatible: boolean;
}

/** v4 initial `formData` (create). */
export function emptyImageProfileForm(): ImageProfileFormData {
  return {
    name: '',
    provider: 'OPENAI',
    apiKeyId: '',
    baseUrl: '',
    modelName: 'dall-e-3',
    parameters: {},
    isDefault: false,
    isDangerousCompatible: false,
  };
}

/** Load an existing profile into the form for editing. */
export function imageProfileToForm(profile: ImageProfileDto): ImageProfileFormData {
  return {
    name: profile.name,
    provider: profile.provider,
    apiKeyId: profile.apiKeyId ?? '',
    baseUrl: profile.baseUrl ?? '',
    modelName: profile.modelName,
    parameters: { ...(profile.parameters ?? {}) },
    isDefault: profile.isDefault,
    isDangerousCompatible: profile.isDangerousCompatible,
  };
}

/** Build the create/update body (v4 submits the whole form; baseUrl coerced). */
export function formToImageProfileBody(form: ImageProfileFormData): {
  name: string;
  provider: string;
  apiKeyId: string | null;
  baseUrl: string | null;
  modelName: string;
  parameters: Record<string, unknown>;
  isDefault: boolean;
  isDangerousCompatible: boolean;
} {
  return {
    name: form.name.trim(),
    provider: form.provider,
    apiKeyId: form.apiKeyId || null,
    baseUrl: form.baseUrl || null,
    modelName: form.modelName,
    parameters: form.parameters,
    isDefault: form.isDefault,
    isDangerousCompatible: form.isDangerousCompatible,
  };
}
