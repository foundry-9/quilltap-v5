import type {
  ApiKeyDto,
  EmbeddingModelDto,
  EmbeddingProviderInfoDto,
} from '../../../../core/core-contract';

/**
 * Client-side embedding-profiles helpers (v4
 * `components/settings/embedding-profiles/{types.ts,hooks/useEmbeddingProfiles.ts}`).
 * The provider metadata and badge-class maps are ported verbatim — both are
 * client-side hardcodes in v4, flagged there and kept here.
 */

/** v4's four known embedding providers (the open set is wider — see below). */
export type EmbeddingProvider = 'OPENAI' | 'OLLAMA' | 'OPENROUTER' | 'BUILTIN';

/**
 * Static provider metadata (v4 `useEmbeddingProfiles.ts:40-65`). A CLIENT-SIDE
 * hardcode — v4's own comment: "This is a fallback - ideally the API would
 * return this information." An unknown provider degrades to
 * `{displayName: name, requiresApiKey: true, requiresBaseUrl: false}`.
 */
export const PROVIDER_METADATA: Record<string, Omit<EmbeddingProviderInfoDto, 'name'>> = {
  BUILTIN: {
    displayName: 'Built-in (TF-IDF)',
    requiresApiKey: false,
    requiresBaseUrl: false,
    description:
      'Offline embeddings using TF-IDF with BM25 enhancement - no API keys required',
  },
  OPENAI: {
    displayName: 'OpenAI',
    requiresApiKey: true,
    requiresBaseUrl: false,
    description: 'OpenAI text embeddings (text-embedding-3-small, text-embedding-3-large)',
  },
  OPENROUTER: {
    displayName: 'OpenRouter',
    requiresApiKey: true,
    requiresBaseUrl: false,
    description: 'Access multiple embedding models through OpenRouter',
  },
  OLLAMA: {
    displayName: 'Ollama (Local)',
    requiresApiKey: false,
    requiresBaseUrl: true,
    description: 'Local embedding models via Ollama',
  },
};

/** v4 `types.ts:78` — provider name → `qt-badge-provider-*` class. */
export const PROVIDER_BADGE_CLASSES: Record<string, string> = {
  OPENAI: 'qt-badge-provider-openai',
  OLLAMA: 'qt-badge-provider-ollama',
  OPENROUTER: 'qt-badge-provider-openrouter',
  BUILTIN: 'qt-badge-provider-builtin',
};

/** Map raw provider names to enriched infos (v4 hook :106-113 / :117-124). */
export function buildProviderInfos(names: string[]): EmbeddingProviderInfoDto[] {
  return names.map((name) => ({
    name,
    ...(PROVIDER_METADATA[name] ?? {
      displayName: name,
      requiresApiKey: true,
      requiresBaseUrl: false,
    }),
  }));
}

/**
 * The everything-the-modal-needs bundle (v4's `loadData` non-profile trio).
 * Each field degrades to empty on a sub-fetch failure (v4's non-fatal loads).
 */
export interface EmbeddingProfilesMeta {
  apiKeys: ApiKeyDto[];
  models: Record<string, EmbeddingModelDto[]>;
  providers: EmbeddingProviderInfoDto[];
}
