/**
 * The Profile screen's data layer (v4 `components/profile/types.ts` +
 * `ProfileView.tsx`'s fetch + `ProfileEditSection`'s two mutations +
 * `DataDirectorySection`'s fetch): the §3 payload types, the TanStack query
 * keys (v4 `queryKeys.userProfile.detail` / `queryKeys.system.dataDir`), and
 * the dispatch helpers.
 *
 * §3 (the P4.9a ∥ P4.9c ∥ P4.9b ∥ P4.9d Shared contract): `userProfileGet`,
 * `userProfileUpdate`, `userProfileSetAvatar`, `systemDataDir`. The field names
 * and the omit-vs-null splits are v4's verbatim — `profile_routes_equivalence`
 * pins the bytes. The §3 verbs were FOLDED into `CoreRequest` at unification
 * (the request types live in `core-contract.ts`'s P4.9c block).
 */

import type { CoreClient } from '../../core/core-client';
import type { UserProfileUpdateRequest } from '../../core/core-contract';

/**
 * v4 `components/profile/types.ts:7-15`. NOTE the nullable fields are declared
 * `| null` there but the wire OMITS them when the column is NULL on a READ (an
 * update that wrote NULL echoes `null`) — so every one is optional here too,
 * and consumers must treat "absent" and `null` identically.
 */
export interface UserProfile {
  id: string;
  username: string;
  email?: string | null;
  name?: string | null;
  image?: string | null;
  createdAt: string;
  updatedAt: string;
}

/** v4 `DataDirectorySection.tsx:12-19` + the fields the route also returns. */
export interface DataDirInfo {
  path: string;
  source: 'environment' | 'platform-default';
  sourceDescription: string;
  platform: 'docker' | 'linux' | 'darwin' | 'win32';
  isDocker: boolean;
  isVM: boolean;
  isElectronShell: boolean;
  shellVersion: string | null;
  shellCapabilities: string[];
  canOpen: boolean;
  hostPath: string;
}

export const profileKeys = {
  detail: ['userProfile', 'detail'] as const,
  dataDir: ['system', 'dataDir'] as const,
};

/**
 * v4 `ProfileView.tsx:24-26` tolerates BOTH `{profile}` and a bare profile
 * object, so this unwrapper does too. v5's server only ever sends `{profile}`
 * (pinned by the differential) — the tolerance is carried because it is v4's
 * behavior, not because the shape is in doubt.
 */
function unwrapProfile(data: Record<string, unknown>): UserProfile {
  const nested = (data as { profile?: unknown }).profile;
  return (nested ?? data) as unknown as UserProfile;
}

export async function fetchProfile(core: CoreClient): Promise<UserProfile> {
  return unwrapProfile(await core.dispatchData({ type: 'userProfileGet' }));
}

/**
 * v4 `ProfileEditSection.tsx:37-44` — PUT `{name, email}`.
 *
 * ⚠ v4 sends `name || null` / `email || null`, and the route's Zod schema
 * REJECTS an explicit null (`.optional()` admits absent, not null) — so
 * clearing either box in v4 answers a 400 and changes nothing. v5 OMITS an
 * empty field instead of sending null, which is the same no-op without the
 * spurious error. (The server keeps v4's rejection verbatim; see
 * `api::user_profile`'s header and `put_explicit_nulls` in the differential.)
 */
export async function updateProfile(
  core: CoreClient,
  patch: { name?: string; email?: string },
): Promise<UserProfile> {
  const request: UserProfileUpdateRequest = { type: 'userProfileUpdate' };
  if (patch.name) {
    request.name = patch.name;
  }
  if (patch.email) {
    request.email = patch.email;
  }
  return unwrapProfile(await core.dispatchData(request));
}

/** v4 `ProfileEditSection.tsx:66-69` — PATCH `?action=set-avatar` `{imageId}`. */
export async function setProfileAvatar(
  core: CoreClient,
  imageId: string | null,
): Promise<UserProfile> {
  return unwrapProfile(
    await core.dispatchData({ type: 'userProfileSetAvatar', imageId }),
  );
}

export async function fetchDataDir(core: CoreClient): Promise<DataDirInfo> {
  const data = await core.dispatchData({ type: 'systemDataDir' });
  return data as unknown as DataDirInfo;
}
