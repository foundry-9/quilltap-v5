/**
 * Client-side attachment capabilities per provider — the port of v4
 * `lib/llm/attachment-support.ts`'s `PROVIDER_ATTACHMENT_CAPABILITIES` and the
 * two readers the connection-profile editor uses.
 *
 * v4 keeps this as a STATIC client map rather than reading the plugin registry,
 * for the reason its own comment gives: the map "is used by client components,
 * so it cannot import from provider-registry.ts". The providers listing carries
 * no attachment datum either (v4 `app/api/v1/providers/route.ts:25-55`; v5's
 * `ProviderInfo` mirrors it), so this hardcoded table is genuinely what v4's
 * `supportsImageUpload` seed derives from — transcribed rather than invented.
 *
 * ⚠ It is KNOWN STALE in v4 and faithfully stale here (recorded at P4.21):
 * each entry is a hand-kept mirror of its plugin's `supportedMimeTypes`, and
 * providers added since — Z_AI, DEEPSEEK — have no entry at all, so they fall
 * through the unknown-provider branch to "no attachments". A provider whose
 * plugin does accept images therefore starts a new profile with the vision box
 * unticked. That is v4's behaviour; the fix belongs upstream, not here.
 */

interface AttachmentCapability {
  types: readonly string[];
}

/** v4 `PROVIDER_ATTACHMENT_CAPABILITIES`, mime lists verbatim. */
const PROVIDER_ATTACHMENT_CAPABILITIES: Record<string, AttachmentCapability> = {
  // plugins/dist/qtap-plugin-openai
  OPENAI: { types: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'] },
  // plugins/dist/qtap-plugin-anthropic
  ANTHROPIC: {
    types: ['image/jpeg', 'image/png', 'image/gif', 'image/webp', 'application/pdf', 'text/plain'],
  },
  // plugins/dist/qtap-plugin-google
  GOOGLE: { types: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'] },
  // plugins/dist/qtap-plugin-grok — text/PDF go through the fallback system
  GROK: { types: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'] },
  // plugins/dist/qtap-plugin-ollama — multimodal models are a future maybe
  OLLAMA: { types: [] },
  // plugins/dist/qtap-plugin-openrouter — depends on the model being proxied
  OPENROUTER: { types: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'] },
  // plugins/dist/qtap-plugin-openai-compatible — varies by implementation
  OPENAI_COMPATIBLE: { types: [] },
};

/**
 * Supported MIME types for a provider; empty for anything not in the table
 * (v4 `getSupportedMimeTypes`). `baseUrl` is part of v4's signature and, in
 * v4, entirely unread — the hardcoded table is the only source. Omitted here
 * rather than carried as an ignored parameter.
 */
export function getSupportedMimeTypes(provider: string): readonly string[] {
  return PROVIDER_ATTACHMENT_CAPABILITIES[provider]?.types ?? [];
}

/** v4 `supportsMimeType`. */
export function supportsMimeType(provider: string, mimeType: string): boolean {
  return getSupportedMimeTypes(provider).includes(mimeType);
}
