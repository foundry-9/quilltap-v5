import { describe, expect, it } from 'vitest';

import {
  buildProviderInfos,
  PROVIDER_BADGE_CLASSES,
  PROVIDER_METADATA,
} from './embedding-profiles.types';

/**
 * The NanoGPT embedding-provider surface, pinned against v4 at `d5830439`:
 * `hooks/useEmbeddingProfiles.ts:59-64` (metadata) and `types.ts:5,82` (the
 * union and the badge class). Both maps are client-side hardcodes in v4 —
 * flagged there, transcribed here — so nothing but a literal comparison proves
 * them.
 */
describe('NanoGPT embedding provider', () => {
  it('carries v4’s metadata verbatim (useEmbeddingProfiles.ts:59-64)', () => {
    expect(PROVIDER_METADATA['NANOGPT']).toEqual({
      displayName: 'NanoGPT',
      requiresApiKey: true,
      requiresBaseUrl: false,
      description: 'OpenAI, BGE, Jina, Qwen, and Gemini embedding models through NanoGPT',
    });
  });

  it('carries v4’s badge class (types.ts:82)', () => {
    expect(PROVIDER_BADGE_CLASSES['NANOGPT']).toBe('qt-badge-provider-nanogpt');
  });

  it('sits between OPENROUTER and BUILTIN in the badge map (v4 key order)', () => {
    expect(Object.keys(PROVIDER_BADGE_CLASSES)).toEqual([
      'OPENAI',
      'OLLAMA',
      'OPENROUTER',
      'NANOGPT',
      'BUILTIN',
    ]);
  });

  it('enriches through buildProviderInfos rather than the unknown-provider default', () => {
    const [info] = buildProviderInfos(['NANOGPT']);
    expect(info.displayName).toBe('NanoGPT');
    // The unknown-provider fallback would echo the raw name and carry no
    // description; this asserts the metadata row is the one being read.
    expect(info.description).toContain('through NanoGPT');
  });

  it('leaves a genuinely unknown provider on the degraded default', () => {
    const [info] = buildProviderInfos(['MYSTERY']);
    expect(info).toEqual({
      name: 'MYSTERY',
      displayName: 'MYSTERY',
      requiresApiKey: true,
      requiresBaseUrl: false,
    });
  });
});
