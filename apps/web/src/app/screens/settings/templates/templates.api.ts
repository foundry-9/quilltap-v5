/**
 * The roleplay-templates data layer (v4 `useRoleplayTemplates` + the
 * `/api/v1/roleplay-templates` routes): TanStack query keys + thin `CoreClient`
 * dispatch helpers over the P4.6p listing variants. Reads through
 * {@link CoreClient.dispatchData} (raw `data`) — the Shared contract pins the
 * response BODIES, not narrowed `Response` types.
 *
 * Lane A folds the `roleplayTemplate*` Request interfaces into the `CoreRequest`
 * union at unification; until then this lane dispatches them through the
 * localized {@link listingDispatch} `as unknown as CoreRequest` seam (the
 * appendix pins the wire shapes; only the union membership is pending).
 */

import type { CoreClient } from '../../../core/core-client';
import type {
  CoreRequest,
  RoleplayTemplateCreateInput,
  RoleplayTemplateCreateRequest,
  RoleplayTemplateDeleteRequest,
  RoleplayTemplateDto,
  RoleplayTemplateGetRequest,
  RoleplayTemplateListRequest,
  RoleplayTemplateUpdateInput,
  RoleplayTemplateUpdateRequest,
} from '../../../core/core-contract';

/** The P4.6p roleplay-template Request interfaces (appendix), pre-union-merge. */
type RoleplayTemplateRequest =
  | RoleplayTemplateListRequest
  | RoleplayTemplateCreateRequest
  | RoleplayTemplateGetRequest
  | RoleplayTemplateUpdateRequest
  | RoleplayTemplateDeleteRequest;

/**
 * Dispatch a listing-surface request lane A has not yet wired into
 * `CoreRequest`. The cast is confined to this seam; the request is typed against
 * the pinned appendix interfaces so a typo still fails the build.
 */
function listingDispatch(
  core: CoreClient,
  req: RoleplayTemplateRequest,
): Promise<Record<string, unknown>> {
  return core.dispatchData(req as unknown as CoreRequest);
}

export const templateKeys = {
  all: ['roleplay-templates'] as const,
  list: () => ['roleplay-templates', 'list'] as const,
};

/** GET the roleplay templates (bare JSON array: built-in-first, then name). */
export async function fetchRoleplayTemplates(core: CoreClient): Promise<RoleplayTemplateDto[]> {
  const data = await listingDispatch(core, { type: 'roleplayTemplateList' });
  // The pinned envelope is a BARE array; `dispatchData` wraps a non-object body
  // under a synthetic key, so accept either the array or a `{templates}`/`{data}`
  // wrapper defensively (reconciled at unification against lane A's real body).
  if (Array.isArray(data)) {
    return data as RoleplayTemplateDto[];
  }
  return (data['templates'] ??
    data['roleplayTemplates'] ??
    data['data'] ??
    []) as RoleplayTemplateDto[];
}

export async function createRoleplayTemplate(
  core: CoreClient,
  template: RoleplayTemplateCreateInput,
): Promise<RoleplayTemplateDto> {
  const data = await listingDispatch(core, { type: 'roleplayTemplateCreate', template });
  return (data['template'] as RoleplayTemplateDto) ?? (data as unknown as RoleplayTemplateDto);
}

export async function updateRoleplayTemplate(
  core: CoreClient,
  templateId: string,
  template: RoleplayTemplateUpdateInput,
): Promise<RoleplayTemplateDto> {
  const data = await listingDispatch(core, {
    type: 'roleplayTemplateUpdate',
    templateId,
    template,
  });
  return (data['template'] as RoleplayTemplateDto) ?? (data as unknown as RoleplayTemplateDto);
}

export async function deleteRoleplayTemplate(core: CoreClient, templateId: string): Promise<void> {
  await listingDispatch(core, { type: 'roleplayTemplateDelete', templateId });
}
