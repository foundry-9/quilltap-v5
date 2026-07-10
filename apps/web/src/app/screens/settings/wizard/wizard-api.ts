import type { CoreClient } from '../../../core/core-client';
import type { ProviderInfo } from '../../../core/core-contract';

/**
 * The wizard's API helpers (v4 `components/setup-wizard/wizard-api.ts`) mapped
 * onto the pinned dispatch variants. The embedding/image profile creators are a
 * named deferral (their route families are not in this slice) — the wizard's
 * embedding/image steps skip, so they are never called.
 */

/** Fetch the LLM providers (v4 `fetchProviders` — filter `type: 'llm'`). */
export async function fetchProviders(core: CoreClient): Promise<ProviderInfo[]> {
  const resp = await core.dispatchExpect({ type: 'providerList' }, 'providers');
  return resp.data.providers.filter((p) => p.type === 'llm');
}

/** Store a new API key, returning its id (v4 `createApiKey`). */
export async function createApiKey(
  core: CoreClient,
  provider: string,
  label: string,
  apiKey: string,
): Promise<{ id: string }> {
  const resp = await core.dispatchExpect(
    { type: 'apiKeyCreate', provider, label, apiKey },
    'apiKey',
  );
  return { id: resp.data.apiKey.id };
}

/** Validate a key/provider connection (v4 `testConnection`). */
export async function testConnection(
  core: CoreClient,
  provider: string,
  apiKeyId: string,
  baseUrl?: string,
): Promise<{ valid: boolean; error?: string }> {
  const data = await core.dispatchData({
    type: 'connectionProfileTest',
    profile: { provider, apiKeyId, baseUrl },
  });
  return { valid: data['valid'] === true, error: data['error'] as string | undefined };
}

/** Fetch chat models for a provider (v4 `fetchModels`). */
export async function fetchModels(
  core: CoreClient,
  provider: string,
  apiKeyId?: string,
  baseUrl?: string,
): Promise<string[]> {
  const resp = await core.dispatchExpect(
    { type: 'modelFetch', provider, apiKeyId, baseUrl },
    'models',
  );
  return resp.data.models ?? [];
}

/** Create the chat connection profile, returning its id (v4 `createConnectionProfile`). */
export async function createConnectionProfile(
  core: CoreClient,
  params: {
    name: string;
    provider: string;
    apiKeyId?: string;
    baseUrl?: string;
    modelName: string;
    isDefault?: boolean;
    isCheap?: boolean;
  },
): Promise<{ id: string }> {
  const resp = await core.dispatchExpect(
    {
      type: 'connectionProfileCreate',
      profile: {
        name: params.name,
        provider: params.provider,
        apiKeyId: params.apiKeyId || null,
        baseUrl: params.baseUrl || null,
        modelName: params.modelName,
        isDefault: params.isDefault ?? false,
        isCheap: params.isCheap ?? false,
        parameters: {},
      },
    },
    'connectionProfile',
  );
  return { id: resp.data.profile.id };
}

/** Update the cheap-LLM strategy in chat settings (v4 `updateChatSettings`). */
export async function updateChatSettings(
  core: CoreClient,
  cheapLLMSettings: { strategy: string; profileId?: string },
): Promise<void> {
  await core.dispatchExpect(
    { type: 'chatSettingsUpdate', settings: { cheapLLMSettings } },
    'chatSettings',
  );
}

/** Send a test message end-to-end (v4 `testMessage`). */
export async function testMessage(
  core: CoreClient,
  params: { provider: string; apiKeyId?: string; baseUrl?: string; modelName: string },
): Promise<{ success: boolean; message?: string; error?: string }> {
  const data = await core.dispatchData({
    type: 'connectionProfileTestMessage',
    profile: {
      provider: params.provider,
      apiKeyId: params.apiKeyId,
      baseUrl: params.baseUrl,
      modelName: params.modelName,
      parameters: { max_tokens: 50 },
    },
  });
  return {
    success: data['success'] === true,
    message: data['message'] as string | undefined,
    error: data['error'] as string | undefined,
  };
}

/** Load existing config for settings-mode re-entry (v4 `loadExistingConfig`). */
export interface ExistingConfig {
  apiKeys: Array<{ id: string; provider: string; label: string; isActive: boolean }>;
  connectionProfiles: Array<{
    id: string;
    provider: string;
    modelName: string;
    isDefault: boolean;
    isCheap?: boolean;
  }>;
  cheapLLMStrategy?: string;
}

export async function loadExistingConfig(core: CoreClient): Promise<ExistingConfig> {
  const [keys, profiles, settings] = await Promise.all([
    core.dispatch({ type: 'apiKeyList' }),
    core.dispatch({ type: 'connectionProfileList' }),
    core.dispatch({ type: 'chatSettings' }),
  ]);
  const apiKeys =
    keys.type === 'apiKeys'
      ? keys.data.apiKeys.map((k) => ({
          id: k.id,
          provider: k.provider,
          label: k.label,
          isActive: k.isActive,
        }))
      : [];
  const connectionProfiles =
    profiles.type === 'connectionProfiles'
      ? profiles.data.profiles.map((p) => ({
          id: p.id,
          provider: p.provider,
          modelName: p.modelName,
          isDefault: p.isDefault,
          isCheap: p.isCheap,
        }))
      : [];
  const cheapLLMStrategy =
    settings.type === 'chatSettings'
      ? ((settings.data['cheapLLMSettings'] as { strategy?: string } | undefined)?.strategy ??
        undefined)
      : undefined;
  return { apiKeys, connectionProfiles, cheapLLMStrategy };
}
