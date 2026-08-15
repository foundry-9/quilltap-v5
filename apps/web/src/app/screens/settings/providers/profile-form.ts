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
  /**
   * The profile's `parameters` blob MINUS the three sampling keys the form owns
   * as top-level controls (v4 `useProfileForm.ts:46-50`'s `rawParams`). v4 hands
   * this to `ProviderOptionsPanel`, the schema-driven renderer for each plugin's
   * `getProviderOptionsSchema()`; v5 has no such machinery (the standing
   * documented absence — `optionsSchema` is hardcoded null), so the bag is here
   * for two reasons: the ONE hardcoded provider option v5 renders
   * (`enable_thinking`, P4.D81 unit 3), and — more importantly — so that saving
   * a profile does not silently DROP the keys nothing in the SPA renders
   * (`num_ctx`, OpenRouter's `providerPreferences`/`enableZDR`, …), all of which
   * the wire side still reads.
   */
  parameters: Record<string, unknown>;
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
  parameters: {},
};

/**
 * The sampling keys the form owns as dedicated controls; everything else in the
 * stored blob rides `ProfileFormData.parameters` untouched (v4
 * `useProfileForm.ts:47`).
 */
const TOP_LEVEL_PARAMETER_KEYS = ['temperature', 'max_tokens', 'top_p'] as const;

/** Load an existing profile into the form for editing (v4 `loadProfileIntoForm`). */
export function loadProfileIntoForm(profile: ConnectionProfileDto): ProfileFormData {
  const params = (profile.parameters ?? {}) as Record<string, unknown>;
  // The rest of the blob rides along verbatim. v4 also MIGRATES the legacy
  // OpenRouter `providerPreferences` shape into its flat schema keys here
  // (`:51-56`) — deliberately not ported: that translation exists to feed the
  // schema renderer v5 does not have, and dropping the legacy key would lose
  // data v5's own request builder still reads
  // (`request_builder/chat_completions.rs:697,947`).
  const rest: Record<string, unknown> = { ...params };
  for (const key of TOP_LEVEL_PARAMETER_KEYS) delete rest[key];
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
    parameters: rest,
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

  // v4 `:122-127`: the sampling controls first, then the provider-option keys
  // spread over them — same order. NOTE the later spread WINS in a JS object
  // literal, so a bag that somehow carried a sampling key would override the
  // form's control — in v4 and v5 alike. Unreachable in practice: the load
  // path strips the three sampling keys out of the bag first.
  const parameters: Record<string, unknown> = {
    temperature: parseFloat(String(form.temperature)),
    max_tokens: parseInt(String(form.maxTokens), 10),
    top_p: parseFloat(String(form.topP)),
    ...form.parameters,
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
