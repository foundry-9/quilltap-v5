import type { ConnectionProfileDto } from '../../../core/core-contract';
import { defaultMultiCharacterPrefill } from './multi-character-prefill';

/** The connection-profile modal's editable form (v4 `types.ts` `ProfileFormData`). */
export interface ProfileFormData {
  name: string;
  transport: 'api' | 'courier';
  courierDeltaMode: boolean;
  provider: string;
  apiKeyId: string;
  baseUrl: string;
  modelName: string;
  temperature: number;
  maxTokens: number;
  topP: number;
  isDefault: boolean;
  isCheap: boolean;
  isDangerousCompatible: boolean;
  allowToolUse: boolean;
  pseudoToolMode: 'auto' | 'native' | 'simple-json' | 'text-block';
  /**
   * Multi-character turn anchor (v4 `types.ts:112-119`, `23af7146`). True sends
   * the assistant `[Name]` prefill; false anchors the turn with a prose
   * instruction in the system prompt instead. Seeded from the provider default
   * when a profile has never recorded a choice — the form itself is never
   * tri-state, only the stored column is.
   */
  multiCharacterPrefill: boolean;
  supportsImageUpload: boolean;
  allowWebSearch: boolean;
  useNativeWebSearch: boolean;
  modelClass: string;
  maxContext: string;
}

export const initialFormState: ProfileFormData = {
  name: '',
  transport: 'api',
  courierDeltaMode: true,
  provider: 'OPENAI',
  apiKeyId: '',
  baseUrl: '',
  modelName: 'gpt-3.5-turbo',
  temperature: 1,
  maxTokens: 4096,
  topP: 1,
  isDefault: false,
  isCheap: false,
  isDangerousCompatible: false,
  allowToolUse: true,
  pseudoToolMode: 'auto',
  multiCharacterPrefill: true,
  supportsImageUpload: false,
  allowWebSearch: false,
  useNativeWebSearch: false,
  modelClass: '',
  maxContext: '',
};

/** Load an existing profile into the form for editing (v4 `loadProfileIntoForm`). */
export function loadProfileIntoForm(profile: ConnectionProfileDto): ProfileFormData {
  const params = (profile.parameters ?? {}) as Record<string, unknown>;
  return {
    name: profile.name,
    transport: profile.transport ?? 'api',
    courierDeltaMode: profile.courierDeltaMode ?? true,
    provider: profile.provider,
    apiKeyId: profile.apiKeyId ?? '',
    baseUrl: profile.baseUrl ?? '',
    modelName: profile.modelName,
    temperature: (params['temperature'] as number) ?? 1,
    maxTokens: (params['max_tokens'] as number) ?? 1000,
    topP: (params['top_p'] as number) ?? 1,
    isDefault: profile.isDefault,
    isCheap: profile.isCheap ?? false,
    isDangerousCompatible: profile.isDangerousCompatible ?? false,
    allowToolUse: profile.allowToolUse ?? true,
    pseudoToolMode: profile.pseudoToolMode ?? 'auto',
    // Null means the profile predates the field; show the provider default the
    // server would resolve to, so the box reflects actual behaviour (v4
    // `useProfileForm.ts:75-78`).
    multiCharacterPrefill:
      profile.multiCharacterPrefill ?? defaultMultiCharacterPrefill(profile.provider),
    supportsImageUpload: profile.supportsImageUpload ?? false,
    allowWebSearch: profile.allowWebSearch ?? false,
    useNativeWebSearch: profile.useNativeWebSearch ?? false,
    modelClass: profile.modelClass ?? '',
    maxContext: profile.maxContext ? String(profile.maxContext) : '',
  };
}

/** Build the create/update request body (v4 `useProfileForm.buildRequestBody`). */
export function buildProfileRequestBody(form: ProfileFormData): Record<string, unknown> {
  if (form.transport === 'courier') {
    return {
      name: form.name,
      transport: 'courier',
      courierDeltaMode: form.courierDeltaMode !== false,
      provider: form.provider || 'COURIER',
      modelName: form.modelName || 'Manual (clipboard)',
      apiKeyId: null,
      isDefault: form.isDefault,
      isCheap: form.isCheap,
      isDangerousCompatible: false,
      allowToolUse: false,
      // Not a tool flag — the Courier renders the same assembled context for
      // the user to carry by hand, so the turn anchor still applies (v4
      // `useProfileForm.ts:107-109`; its sibling booleans are forced false
      // here and this one pointedly is not).
      multiCharacterPrefill: form.multiCharacterPrefill,
      supportsImageUpload: false,
      allowWebSearch: false,
      useNativeWebSearch: false,
      modelClass: null,
      maxContext: null,
      parameters: {},
    };
  }

  const parameters: Record<string, unknown> = {
    temperature: parseFloat(String(form.temperature)),
    max_tokens: parseInt(String(form.maxTokens), 10),
    top_p: parseFloat(String(form.topP)),
  };

  return {
    name: form.name,
    transport: 'api',
    provider: form.provider,
    modelName: form.modelName,
    isDefault: form.isDefault,
    isCheap: form.isCheap,
    isDangerousCompatible: form.isDangerousCompatible,
    allowToolUse: form.allowToolUse,
    pseudoToolMode: form.pseudoToolMode,
    multiCharacterPrefill: form.multiCharacterPrefill,
    supportsImageUpload: form.supportsImageUpload,
    allowWebSearch: form.allowWebSearch,
    useNativeWebSearch: form.useNativeWebSearch,
    modelClass: form.modelClass || null,
    maxContext: form.maxContext ? parseInt(form.maxContext, 10) : null,
    apiKeyId: form.apiKeyId || null,
    ...(form.baseUrl ? { baseUrl: form.baseUrl } : {}),
    parameters,
  };
}

/** The provider's config requirements the form gates on (v4 `getProviderRequirements`). */
export interface ProviderRequirements {
  requiresApiKey: boolean;
  requiresBaseUrl: boolean;
  supportsWebSearch: boolean;
  supportsToolUse: boolean;
}
