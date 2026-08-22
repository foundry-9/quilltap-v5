import type { ApiKeyDto, ConnectionProfileDto, ProviderInfo } from '../../../core/core-contract';
import { providerAcceptsApiKey } from './api-key-support';
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
   * as top-level controls (v4 `useProfileForm.ts:46-50`'s `rawParams`), and with
   * the legacy OpenRouter `providerPreferences` shape translated to its flat
   * schema key. Both apps hand this to the schema-driven renderer for each
   * plugin's `getProviderOptionsSchema()` (v5: `ProviderOptionsPanel`, P4.D84).
   * Keys the active schema declares no control for — `num_ctx`, another
   * provider's options — still ride the bag untouched, so saving a profile
   * never silently drops what the wire side still reads.
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
  // The rest of the blob rides along verbatim, minus v4's legacy-OpenRouter
  // translation (`useProfileForm.ts:51-57`): the nested `providerPreferences`
  // shape becomes the flat `enableZDR` the options schema exposes, and the
  // nested key is dropped so a re-save does not carry both. Ported at P4.D84
  // with the schema renderer it feeds — until then v5 kept the nested key
  // because nothing rendered the flat one. Both spellings reach the same wire
  // bytes (`model/request_builder/chat_completions.rs:697,947` merges
  // `enableZDR` into `dataCollection: 'deny'`), so the translation is
  // lossless for the one field it covers; v4 loses any sibling
  // `providerPreferences.order` on the same save, and v5 now loses it too.
  const rest: Record<string, unknown> = { ...params };
  for (const key of TOP_LEVEL_PARAMETER_KEYS) delete rest[key];
  const legacyPrefs = rest['providerPreferences'] as { dataCollection?: string } | undefined;
  if (legacyPrefs?.dataCollection === 'deny' && rest['enableZDR'] === undefined) {
    rest['enableZDR'] = true;
  }
  delete rest['providerPreferences'];
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
    // Null means the profile predates the field (or arrived by import); show
    // the provider default the server would resolve to, so the box reflects
    // actual behaviour. The provider rule is all that is knowable here — the
    // thinking half needs the plugin's rule and the model's facts, neither of
    // which this loader is given. ProfileModal corrects a null row once the
    // model list lands (bug 85; v4 `useProfileForm.ts:141-150`).
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

/**
 * The base URL as it is allowed to leave the form (v4
 * `useProfileForm.ts` `outboundBaseUrl`, `:38-57`).
 *
 * A provider that does not require one hides the field (the modal's
 * `reqs().requiresBaseUrl` gate), so whatever is still sitting in form state
 * belongs to a provider the user has since moved off — most often the
 * `localhost:11434` that selecting Ollama auto-filled. Sending it points every
 * probe, and the saved row, at the wrong endpoint with nothing on screen to
 * explain it and no gesture that clears it (v4 Bug 73). The value stays in form
 * state so switching back restores it; it simply never reaches the wire.
 *
 * A provider missing from `providers` is not evidence of anything — the list
 * has not loaded, or its fetch failed — so the stored value is left alone there
 * rather than clearing a working profile on a failed fetch.
 *
 * ⚠ This is the ONE gate; every outbound site reads it rather than the form's
 * raw `baseUrl`. v5 has five (v4's four plus the modal's edit-time model
 * fetch, which reads the SAVED profile rather than form state and so resolves
 * its own requirement — see `ProfileModal.savedProviderTakesBaseUrl`).
 */
export function outboundBaseUrl(
  providers: readonly ProviderInfo[],
  provider: string,
  baseUrl: string,
): string {
  const known = providers.find((p) => p.name === provider);
  if (known && !known.configRequirements?.requiresBaseUrl) return '';
  return baseUrl || '';
}

/**
 * The api key as it is allowed to leave the form (v4 `useProfileForm.ts`
 * `outboundApiKeyId`, `:63-98`) — the exact twin of {@link outboundBaseUrl},
 * one field over (v4 Bug 76; this port's own dogfood finding #90).
 *
 * `onProviderChange` deliberately never clears `apiKeyId` — the value stays in
 * form state so switching back restores it — but the select cannot express what
 * is stored once the provider moves. On a keyless provider it is not rendered
 * at all; on a different hosted provider its options are filtered to that
 * provider (`ProfileModal.keysForProvider`), so the stored id matches nothing
 * and the control reads blank. Sending it anyway had the dialog saying no key
 * was selected while the wire carried one, and the save refused with
 * `API key provider does not match profile provider` — naming a field the
 * dialog does not show, with no gesture on a keyless provider that clears it.
 *
 * So: send only what the select could currently display, refusing on BOTH
 * counts the select can — the provider renders no key control, or the id is
 * outside the options the select would list.
 *
 * Absence is not evidence in either list. A provider list that has not loaded
 * is no reason to judge the provider keyless (the `known &&` guard), and an api
 * key list that has not loaded is no reason to call a stored id undisplayable
 * (the `length > 0` guard) — stripping a key off a working profile would be the
 * worse bug.
 *
 * "Keyless" here is {@link providerAcceptsApiKey}, not `requiresApiKey`: an
 * OpenAI-Compatible profile pointed at a hosted endpoint holds an optional key,
 * and the stricter reading silently dropped it on the way to the wire (v4 bug
 * 81) — which is the same failure this gate exists to prevent, one flag over.
 *
 * ⚠ This is the ONE gate; every outbound site reads it rather than the form's
 * raw `apiKeyId`. v5 has five (v4's four plus the modal's edit-time model
 * fetch, which reads the SAVED profile rather than form state and so resolves
 * its own requirement — see `ProfileModal.autoFetchModelsForEdit`).
 */
export function outboundApiKeyId(
  providers: readonly ProviderInfo[],
  apiKeys: readonly ApiKeyDto[],
  provider: string,
  apiKeyId: string,
): string {
  const stored = apiKeyId || '';
  if (!stored) return '';

  const known = providers.find((p) => p.name === provider);
  if (known && !providerAcceptsApiKey(known.configRequirements)) return '';

  // The select's own option filter, asked as a question.
  if (apiKeys.length > 0) {
    const displayable = apiKeys.some((key) => key.id === stored && key.provider === provider);
    if (!displayable) return '';
  }

  return stored;
}

/** Build the create/update request body (v4 `useProfileForm.buildRequestBody`). */
export function buildProfileRequestBody(
  form: ProfileFormData,
  providers: readonly ProviderInfo[],
  apiKeys: readonly ApiKeyDto[] = [],
): Record<string, unknown> {
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
    // Always sent, never conditionally: `null` is how the row is *cleared* of a
    // key the current provider cannot use, so a profile that carried one across
    // a provider change — or arrived that way by import — heals on its next
    // save rather than being refused forever (v4 Bug 76). Omitting the key
    // leaves the update handler's `apiKeyId !== undefined` gate untripped and
    // every already-poisoned row broken forever. Both handlers map a
    // null/absent value to a cleared column.
    apiKeyId: outboundApiKeyId(providers, apiKeys, form.provider, form.apiKeyId) || null,
    // Always sent, never conditionally: an empty string is how the row is
    // *cleared* of a base URL the current provider does not take, so a profile
    // that picked one up from an earlier provider heals on its next save rather
    // than staying broken and invisible (v4 Bug 73). Omitting the key leaves
    // the update handler's `baseUrl !== undefined` gate untripped and every
    // already-poisoned row broken forever. Both handlers map a falsy value to
    // NULL.
    baseUrl: outboundBaseUrl(providers, form.provider, form.baseUrl),
    parameters,
  };
}

/** The provider's config requirements the form gates on (v4 `getProviderRequirements`). */
export interface ProviderRequirements {
  requiresApiKey: boolean;
  /**
   * Whether a key *may* be attached, as against `requiresApiKey`'s "must it
   * be?" — the question the key field, its star, and its placeholder are all
   * decided by (v4 bug 81). Resolved through `providerAcceptsApiKey`, so an
   * absent flag means the same answer as `requiresApiKey`.
   */
  acceptsApiKey: boolean;
  requiresBaseUrl: boolean;
  supportsWebSearch: boolean;
  supportsToolUse: boolean;
}
