/**
 * The image-profiles data layer (v4 `image-profiles-tab.tsx` + the
 * `/api/v1/image-profiles` routes): TanStack query keys + thin `CoreClient`
 * dispatch helpers over the P4.6p listing variants. Reads through
 * {@link CoreClient.dispatchData} (raw `data`) — the Shared contract pins the
 * response BODIES.

 */

import type { CoreClient } from '../../../core/core-client';
import type {
  HuggingFaceLookupResult,
  ImageLoraSupport,
  ImageProfileCreateBag,
  ImageProfileCreateRequest,
  ImageProfileDeleteRequest,
  ImageProfileDto,
  ImageProfileGetRequest,
  ImageProfileListModelsRequest,
  ImageProfileListRequest,
  ImageProfileLoraMetadataRequest,
  ImageProfileOptionsSchemaRequest,
  ImageProfileUpdateBag,
  ImageProfileUpdateRequest,
  ImageProviderInfo,
  ImageProviderListRequest,
} from '../../../core/core-contract';
import type { ProviderOptionsSchema } from '../providers/provider-options-schema';

type ImageProfileRequest =
  | ImageProfileListRequest
  | ImageProfileCreateRequest
  | ImageProfileGetRequest
  | ImageProfileUpdateRequest
  | ImageProfileDeleteRequest
  | ImageProfileListModelsRequest
  | ImageProfileOptionsSchemaRequest
  | ImageProfileLoraMetadataRequest
  | ImageProviderListRequest;

function listingDispatch(
  core: CoreClient,
  req: ImageProfileRequest,
): Promise<Record<string, unknown>> {
  return core.dispatchData(req);
}

export const imageProfileKeys = {
  all: ['image-profiles'] as const,
  list: (sortByCharacter?: string) =>
    sortByCharacter
      ? (['image-profiles', 'list', sortByCharacter] as const)
      : (['image-profiles', 'list'] as const),
  providers: () => ['image-profiles', 'providers'] as const,
  models: (provider: string, apiKeyId: string) =>
    ['image-profiles', 'models', provider, apiKeyId] as const,
};

/** GET the image profiles (default-first then createdAt DESC, server-ordered). */
export async function fetchImageProfiles(
  core: CoreClient,
  opts?: { sortByCharacter?: string },
): Promise<ImageProfileDto[]> {
  const data = await listingDispatch(core, {
    type: 'imageProfileList',
    ...(opts?.sortByCharacter ? { sortByCharacter: opts.sortByCharacter } : {}),
  });
  return (data['profiles'] as ImageProfileDto[]) ?? [];
}

/** GET the image-capable provider registry (v4 `?action=list-providers`). */
export async function fetchImageProviders(core: CoreClient): Promise<ImageProviderInfo[]> {
  const data = await listingDispatch(core, { type: 'imageProviderList' });
  return (data['providers'] as ImageProviderInfo[]) ?? [];
}

export async function createImageProfile(
  core: CoreClient,
  profile: ImageProfileCreateBag,
): Promise<ImageProfileDto> {
  const data = await listingDispatch(core, { type: 'imageProfileCreate', profile });
  return (data['profile'] as ImageProfileDto) ?? (data as unknown as ImageProfileDto);
}

export async function updateImageProfile(
  core: CoreClient,
  profileId: string,
  profile: ImageProfileUpdateBag,
): Promise<ImageProfileDto> {
  const data = await listingDispatch(core, { type: 'imageProfileUpdate', profileId, profile });
  return (data['profile'] as ImageProfileDto) ?? (data as unknown as ImageProfileDto);
}

export async function deleteImageProfile(core: CoreClient, profileId: string): Promise<void> {
  await listingDispatch(core, { type: 'imageProfileDelete', profileId });
}

/**
 * One `list-models` answer (v4 `app/api/v1/image-profiles/route.ts:188-194`;
 * the Shared contract of P4.D100 / P4.D102). `source` is `provider` only when
 * the provider's API was actually queried and answered; otherwise `builtin`,
 * with the live-fetch reason in `fetchError` — which the server OMITS, never
 * nulls, when there is none. `supportedModels` is always the plugin's curated
 * list (NOT the manifest's `imageGenerationModels`, and not this client's
 * `FALLBACK_PROVIDERS.defaultModels` — three similar-looking lists).
 */
export interface ImageModelListing {
  provider: string;
  models: string[];
  supportedModels: string[];
  source: 'provider' | 'builtin';
  /**
   * Per-model LoRA support, keyed by model id (v4 `84f33ce94`; the Shared
   * contract §A places it between `source` and the conditional `fetchError`).
   * A model that resolves no support is **ABSENT from the map**, never present
   * with a zero cap — absence is the editor's "offer no LoRA rows" signal.
   */
  loraSupport: Record<string, ImageLoraSupport>;
  fetchError?: string;
}

/**
 * v4 `fetchModels` (`ImageProfileForm.tsx:135-171`) — query the provider for its
 * image models, or its plugin's built-in list when there is no key to query
 * with.
 *
 * The caller normalizes the provider first (v4 `:141`: `GOOGLE_IMAGEN` →
 * `GOOGLE`). That matters: the server resolves the legacy alias internally for
 * the lookup but ECHOES the raw string it was sent, so normalizing on this side
 * is what keeps the echoed `provider` canonical. `apiKeyId` is sent only when
 * set, matching v4's conditional `searchParams.set` (`:145-147`).
 */
export async function fetchImageModels(
  core: CoreClient,
  provider: string,
  apiKeyId?: string,
): Promise<ImageModelListing> {
  const data = await listingDispatch(core, {
    type: 'imageProfileListModels',
    provider,
    ...(apiKeyId ? { apiKeyId } : {}),
  });
  return {
    provider: (data['provider'] as string) ?? provider,
    models: (data['models'] as string[]) ?? [],
    supportedModels: (data['supportedModels'] as string[]) ?? [],
    // v4 `:154` — anything that is not exactly 'provider' reads as 'builtin'.
    source: data['source'] === 'provider' ? 'provider' : 'builtin',
    loraSupport: asLoraSupportMap(data['loraSupport']),
    ...(typeof data['fetchError'] === 'string' ? { fetchError: data['fetchError'] } : {}),
  };
}

/**
 * The `loraSupport` map, read defensively. A server that has not landed the
 * map yet (or one answering an older shape) reads as "no model declares
 * support", which is the same thing the map's own absence rule means — so the
 * editor degrades to offering no LoRA rows rather than to a crash.
 */
function asLoraSupportMap(raw: unknown): Record<string, ImageLoraSupport> {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return {};
  return raw as Record<string, ImageLoraSupport>;
}

/** One `options-schema` answer (Shared contract §A). */
export interface ImageOptionsSchemaAnswer {
  provider: string;
  model: string | null;
  /** `null` exactly when the provider's plugin declares no schema. */
  optionsSchema: ProviderOptionsSchema | null;
  /** `null`, never a zero-cap object, when the model resolves no support. */
  loraSupport: ImageLoraSupport | null;
}

/**
 * v4 `ImageProfileForm`'s options-schema fetch (`:194-231`) — ask the
 * provider's plugin what THIS model's options look like, and whether it takes
 * LoRA adapters.
 *
 * Image gateways route to hundreds of models with different legal sizes, so
 * the answer is per model and not per provider. A provider without the hook
 * answers with a null schema and the legacy hand-written panel takes over.
 */
export async function fetchImageOptionsSchema(
  core: CoreClient,
  provider: string,
  model?: string,
): Promise<ImageOptionsSchemaAnswer> {
  const data = await listingDispatch(core, {
    type: 'imageProfileOptionsSchema',
    provider,
    ...(model ? { model } : {}),
  });
  return {
    provider: (data['provider'] as string) ?? provider,
    model: (data['model'] as string | null) ?? null,
    optionsSchema: (data['optionsSchema'] as ProviderOptionsSchema | null) ?? null,
    loraSupport: (data['loraSupport'] as ImageLoraSupport | null) ?? null,
  };
}

/**
 * v4 `LoraListEditor`'s query (`:117-141`) — ask HuggingFace about one LoRA
 * source, host-side.
 *
 * The token rides the request BODY, never a query string: it is a credential,
 * and the lookup runs on the host so the browser never contacts HuggingFace
 * and the token never reaches the page's address bar or any proxy log.
 *
 * Both outcomes are ordinary answers — a failed lookup is `{ok: false}` at
 * HTTP 200. A failed REQUEST is the caller's business: v4 collapses it into
 * the same `network` shape, because from the reader's chair they are the same
 * disappointment.
 */
export async function queryLoraMetadata(
  core: CoreClient,
  source: string,
  hfToken?: string,
): Promise<HuggingFaceLookupResult> {
  const data = await listingDispatch(core, {
    type: 'imageProfileLoraMetadata',
    source,
    ...(hfToken ? { hfToken } : {}),
  });
  return data as unknown as HuggingFaceLookupResult;
}
