/**
 * The image-profiles data layer (v4 `image-profiles-tab.tsx` + the
 * `/api/v1/image-profiles` routes): TanStack query keys + thin `CoreClient`
 * dispatch helpers over the P4.6p listing variants. Reads through
 * {@link CoreClient.dispatchData} (raw `data`) — the Shared contract pins the
 * response BODIES.
 *
 * Lane A folds the `imageProfile*` / `imageProviderList` Request interfaces into
 * the `CoreRequest` union at unification; until then this lane dispatches them
 * through the localized {@link listingDispatch} `as unknown as CoreRequest` seam.
 */

import type { CoreClient } from '../../../core/core-client';
import type {
  CoreRequest,
  ImageProfileCreateInput,
  ImageProfileCreateRequest,
  ImageProfileDeleteRequest,
  ImageProfileDto,
  ImageProfileGetRequest,
  ImageProfileListRequest,
  ImageProfileUpdateInput,
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
  | ImageProviderListRequest;

/** Dispatch a listing-surface request lane A has not yet wired into `CoreRequest`. */
function listingDispatch(
  core: CoreClient,
  req: ImageProfileRequest,
): Promise<Record<string, unknown>> {
  return core.dispatchData(req as unknown as CoreRequest);
}

export const imageProfileKeys = {
  all: ['image-profiles'] as const,
  list: (sortByCharacter?: string) =>
    sortByCharacter
      ? (['image-profiles', 'list', sortByCharacter] as const)
      : (['image-profiles', 'list'] as const),
  providers: () => ['image-profiles', 'providers'] as const,
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
  profile: ImageProfileCreateInput,
): Promise<ImageProfileDto> {
  const data = await listingDispatch(core, { type: 'imageProfileCreate', profile });
  return (data['profile'] as ImageProfileDto) ?? (data as unknown as ImageProfileDto);
}

export async function updateImageProfile(
  core: CoreClient,
  profileId: string,
  profile: ImageProfileUpdateInput,
): Promise<ImageProfileDto> {
  const data = await listingDispatch(core, { type: 'imageProfileUpdate', profileId, profile });
  return (data['profile'] as ImageProfileDto) ?? (data as unknown as ImageProfileDto);
}

export async function deleteImageProfile(core: CoreClient, profileId: string): Promise<void> {
  await listingDispatch(core, { type: 'imageProfileDelete', profileId });
}
