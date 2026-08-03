/**
 * The roleplay-templates data layer (v4 `useRoleplayTemplates` + the
 * `/api/v1/roleplay-templates` routes): TanStack query keys + thin `CoreClient`
 * dispatch helpers over the P4.6p listing variants. Reads through
 * {@link CoreClient.dispatchData} (raw `data`) — the Shared contract pins the
 * response BODIES, not narrowed `Response` types.
 */

import type { CoreClient } from '../../../core/core-client';
import type {
  RoleplayTemplateCreateBag,
  RoleplayTemplateCreateRequest,
  RoleplayTemplateDeleteRequest,
  RoleplayTemplateDto,
  RoleplayTemplateGetRequest,
  RoleplayTemplateListRequest,
  RoleplayTemplateUpdateBag,
  RoleplayTemplateUpdateRequest,
} from '../../../core/core-contract';

/** The P4.6p roleplay-template Request interfaces (now folded into `CoreRequest`). */
type RoleplayTemplateRequest =
  | RoleplayTemplateListRequest
  | RoleplayTemplateCreateRequest
  | RoleplayTemplateGetRequest
  | RoleplayTemplateUpdateRequest
  | RoleplayTemplateDeleteRequest;

function listingDispatch(
  core: CoreClient,
  req: RoleplayTemplateRequest,
): Promise<Record<string, unknown>> {
  return core.dispatchData(req);
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

/**
 * GET one roleplay template by id (v4 `GET /api/v1/roleplay-templates/{id}`).
 * The response body is the BARE template object, so `dispatchData`'s `data` IS
 * the template. A missing id answers a NOT_FOUND error, which surfaces as a
 * rejected promise — v4's `!res.ok` arm — and the Salon's caller turns that
 * into the defaults.
 */
export async function fetchRoleplayTemplate(
  core: CoreClient,
  templateId: string,
): Promise<RoleplayTemplateDto> {
  const data = await listingDispatch(core, { type: 'roleplayTemplateGet', templateId });
  return data as unknown as RoleplayTemplateDto;
}

export async function createRoleplayTemplate(
  core: CoreClient,
  template: RoleplayTemplateCreateBag,
): Promise<RoleplayTemplateDto> {
  const data = await listingDispatch(core, { type: 'roleplayTemplateCreate', template });
  return (data['template'] as RoleplayTemplateDto) ?? (data as unknown as RoleplayTemplateDto);
}

export async function updateRoleplayTemplate(
  core: CoreClient,
  templateId: string,
  template: RoleplayTemplateUpdateBag,
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
