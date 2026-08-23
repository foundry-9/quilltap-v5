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
 * The staleness this header recorded at P4.21 has been REPAIRED UPSTREAM. v4
 * `a14a1811` (bug 91) adds the three rows the hand-kept table lacked — NANOGPT,
 * DEEPSEEK and Z_AI — and they are transcribed below, so the fall-through to
 * "no attachments" no longer swallows a provider whose plugin does send images.
 * The consequence flips with them: a new NanoGPT or Z.AI profile now seeds the
 * vision box ON, while DeepSeek — whose plugin genuinely strips attachments
 * before the wire — keeps it OFF.
 *
 * The table remains a HAND-KEPT mirror of each plugin's `supportedMimeTypes`
 * (v4's own comment: deriving it from the manifests would remove the drift
 * risk, but they aren't reachable from client code without the server-only
 * registry — YAGNI until that changes), so it can go stale again. It is not
 * stale today.
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
  // plugins/dist/qtap-plugin-openai-compatible — the shared base class marks
  // every attachment failed; no `image_url` part is ever emitted.
  OPENAI_COMPATIBLE: { types: [] },
  // plugins/dist/qtap-plugin-nanogpt — serialises image_url as of plugin 1.1.0
  // (bug 91; before that it inherited the OpenAI-compatible base's "not yet
  // implemented" handling and dropped images silently).
  NANOGPT: { types: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'] },
  // plugins/dist/qtap-plugin-deepseek — same inherited base, same drop.
  DEEPSEEK: { types: [] },
  // plugins/dist/qtap-plugin-z-ai — serialises image_url for vision models
  Z_AI: { types: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'] },
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

/**
 * A user-facing description of a provider's attachment support (v4
 * `getAttachmentSupportDescription`, `lib/llm/attachment-support.ts:189-216`),
 * rendered under the connection-profile modal's provider select.
 *
 * Sentence structure carried verbatim: the three categories in this order,
 * comma-joined, with the image mime subtypes upper-cased and `text/plain`
 * spelled `TXT`. A provider with no entry at all answers the single fixed
 * sentence.
 *
 * `baseUrl` is part of v4's signature and, in v4, entirely unread — the
 * hardcoded table above is the only source. Omitted here rather than carried as
 * an ignored parameter, exactly as {@link supportsMimeType} does.
 */
export function getAttachmentSupportDescription(provider: string): string {
  const mimeTypes = getSupportedMimeTypes(provider);
  if (mimeTypes.length === 0) return 'No file attachments supported';

  const parts: string[] = [];
  const images = mimeTypes.filter((t) => t.startsWith('image/'));
  const documents = mimeTypes.filter((t) => t === 'application/pdf');
  const text = mimeTypes.filter((t) => t.startsWith('text/'));

  if (images.length > 0) {
    parts.push(`Images (${images.map((t) => t.replace('image/', '').toUpperCase()).join(', ')})`);
  }
  if (documents.length > 0) {
    parts.push('PDF documents');
  }
  if (text.length > 0) {
    parts.push(
      `Text files (${text
        .map((t) => {
          const format = t.replace('text/', '');
          return format === 'plain' ? 'TXT' : format.toUpperCase();
        })
        .join(', ')})`,
    );
  }
  return parts.join(', ');
}
