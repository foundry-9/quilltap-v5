/**
 * The projects (Prospero) data layer (v4 `app/prospero/hooks/*`): TanStack query
 * keys + thin `CoreClient` dispatch helpers for the `/prospero` list and the
 * routed detail. Reads through {@link CoreClient.dispatchData} (raw `data`) — the
 * p4.6k Shared contract pins the response bodies via its differentials.
 */

import type { CoreClient } from '../../core/core-client';
import type {
  DocumentStoreSummary,
  EnrichedChatSummary,
  ProjectBackgroundDto,
  ProjectDetail,
  ProjectFileDto,
  ProjectSummary,
} from '../../core/core-contract';

/** Query keys for the projects vertical (v4 `queryKeys.projects`). */
export const projectKeys = {
  all: ['projects'] as const,
  list: () => ['projects', 'list'] as const,
  detail: (id: string) => ['projects', 'detail', id] as const,
  chats: (id: string) => ['projects', 'chats', id] as const,
  files: (id: string) => ['projects', 'files', id] as const,
  stores: (id: string) => ['projects', 'stores', id] as const,
};

/** A project as the list card consumes it: the row plus the mapped counts. */
export interface ProjectCardModel extends ProjectSummary {
  chatCount: number;
  fileCount: number;
  characterCount: number;
}

/** Map `_count` → the flat card counts (v4 `useProjects` `mappedProjects`). */
export async function fetchProjects(core: CoreClient): Promise<ProjectCardModel[]> {
  const data = await core.dispatchData({ type: 'projectList' });
  const projects = (data['projects'] as ProjectSummary[]) ?? [];
  return projects.map((p) => ({
    ...p,
    chatCount: p._count?.chats ?? 0,
    fileCount: p._count?.files ?? 0,
    characterCount: p._count?.characters ?? 0,
  }));
}

export async function createProject(
  core: CoreClient,
  name: string,
  description?: string | null,
): Promise<ProjectDetail> {
  const data = await core.dispatchData({
    type: 'projectCreate',
    name,
    ...(description ? { description } : {}),
  });
  return (data['project'] as ProjectDetail) ?? (data as unknown as ProjectDetail);
}

export async function deleteProject(core: CoreClient, projectId: string): Promise<void> {
  await core.dispatchData({ type: 'projectDelete', projectId });
}

export async function fetchProject(core: CoreClient, projectId: string): Promise<ProjectDetail> {
  const data = await core.dispatchData({ type: 'projectGet', projectId });
  return (data['project'] as ProjectDetail) ?? (data as unknown as ProjectDetail);
}

/** A partial PUT (v4 `useProjectDetail` per-field save handlers all PUT one field). */
export async function updateProject(
  core: CoreClient,
  projectId: string,
  patch: Record<string, unknown>,
): Promise<ProjectDetail> {
  const data = await core.dispatchData({ type: 'projectUpdate', projectId, ...patch });
  return (data['project'] as ProjectDetail) ?? (data as unknown as ProjectDetail);
}

export async function removeProjectCharacter(
  core: CoreClient,
  projectId: string,
  characterId: string,
): Promise<void> {
  await core.dispatchData({ type: 'projectCharacterRemove', projectId, characterId });
}

// --- Chats (paginated, page size 20) ---

export interface ProjectChatsPage {
  chats: EnrichedChatSummary[];
  total: number;
}

export async function fetchProjectChats(
  core: CoreClient,
  projectId: string,
  opts?: { limit?: number; offset?: number },
): Promise<ProjectChatsPage> {
  const data = await core.dispatchData({
    type: 'projectChatList',
    projectId,
    ...(opts?.limit !== undefined ? { limit: opts.limit } : {}),
    ...(opts?.offset !== undefined ? { offset: opts.offset } : {}),
  });
  return {
    chats: (data['chats'] as EnrichedChatSummary[]) ?? [],
    total: (data['total'] as number) ?? 0,
  };
}

export async function removeProjectChat(
  core: CoreClient,
  projectId: string,
  chatId: string,
): Promise<void> {
  await core.dispatchData({ type: 'projectChatRemove', projectId, chatId });
}

// --- Files (list only this round) ---

export async function fetchProjectFiles(
  core: CoreClient,
  projectId: string,
): Promise<ProjectFileDto[]> {
  const data = await core.dispatchData({ type: 'projectFileList', projectId });
  return (data['files'] as ProjectFileDto[]) ?? [];
}

// --- Linked document stores (list + unlink; link picker deferred) ---

export async function fetchProjectStores(
  core: CoreClient,
  projectId: string,
): Promise<DocumentStoreSummary[]> {
  const data = await core.dispatchData({ type: 'projectMountPointList', projectId });
  return (data['mountPoints'] as DocumentStoreSummary[]) ?? [];
}

export async function unlinkProjectStore(
  core: CoreClient,
  projectId: string,
  mountPointId: string,
): Promise<void> {
  await core.dispatchData({ type: 'projectMountPointUnlink', projectId, mountPointId });
}

// --- State ---

export async function fetchProjectState(
  core: CoreClient,
  projectId: string,
): Promise<Record<string, unknown>> {
  const data = await core.dispatchData({ type: 'projectStateGet', projectId });
  return (data['state'] as Record<string, unknown>) ?? {};
}

export async function setProjectState(
  core: CoreClient,
  projectId: string,
  state: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const data = await core.dispatchData({ type: 'projectStateSet', projectId, state });
  return (data['state'] as Record<string, unknown>) ?? {};
}

export async function resetProjectState(
  core: CoreClient,
  projectId: string,
): Promise<Record<string, unknown>> {
  const data = await core.dispatchData({ type: 'projectStateReset', projectId });
  return (data['previousState'] as Record<string, unknown>) ?? {};
}

// --- Story background (display-mode resolution) ---

export async function fetchProjectBackground(
  core: CoreClient,
  projectId: string,
): Promise<ProjectBackgroundDto> {
  const data = await core.dispatchData({ type: 'projectBackgroundGet', projectId });
  return {
    url: (data['url'] as string | null) ?? null,
    sourceChatId: data['sourceChatId'] as string,
  };
}

// --- Aesthetic editors (lantern / aurora) ---

export async function fetchProjectAesthetic(
  core: CoreClient,
  projectId: string,
  kind: 'lantern' | 'aurora',
): Promise<string> {
  const data = await core.dispatchData({ type: 'projectAestheticGet', projectId, kind });
  return (data['content'] as string) ?? '';
}

export async function setProjectAesthetic(
  core: CoreClient,
  projectId: string,
  kind: 'lantern' | 'aurora',
  content: string,
): Promise<void> {
  await core.dispatchData({ type: 'projectAestheticSet', projectId, kind, content });
}
