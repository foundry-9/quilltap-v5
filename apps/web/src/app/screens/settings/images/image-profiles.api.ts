/**
 * The image-profiles data layer (v4 `image-profiles-tab.tsx` + the
 * `/api/v1/image-profiles` routes): TanStack query keys + thin `CoreClient`
 * dispatch helpers over the P4.6p listing variants. Reads through
 * {@link CoreClient.dispatchData} (raw `data`) — the Shared contract pins the
 * response BODIES.

 */

import type { CoreClient } from '../../../core/core-client';
import type {
  ImageProfileCreateBag,
  ImageProfileCreateRequest,
  ImageProfileDeleteRequest,
  ImageProfileDto,
  ImageProfileGetRequest,
  ImageProfileListModelsRequest,
  ImageProfileListRequest,
  ImageProfileUpdateBag,
  ImageProfileUpdateRequest,
  ImageProviderInfo,
  ImageProviderListRequest,
} from '../../../core/core-contract';

type ImageProfileRequest =
  | ImageProfileListRequest
  | ImageProfileCreateRequest
  | ImageProfileGetRequest
  | ImageProfileUpdateRequest
  | ImageProfileDeleteRequest
  | ImageProfileListModelsRequest
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
    ...(typeof data['fetchError'] === 'string' ? { fetchError: data['fetchError'] } : {}),
  };
}
