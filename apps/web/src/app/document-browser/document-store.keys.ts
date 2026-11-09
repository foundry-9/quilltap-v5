/**
 * The document-store query keys — v4 `lib/query/keys.ts:273-290`
 * (`queryKeys.mountPoints`) and `queryKeys.projects.stores`, spelled exactly
 * as v4 spells them (P4.D387 item 6). Everything one store shows lives under
 * `store(id)`, so the `mountPoints` realtime topic (scoped by store id)
 * reaches its detail, listing, link lookups, contents and renderings at once.
 *
 * @module document-browser/document-store.keys
 */

export const mountPointKeys = {
  all: ['mount-points'] as const,
  lists: ['mount-points', 'list'] as const,
  list: (filters?: unknown) => ['mount-points', 'list', filters ?? {}] as const,
  store: (id: string) => ['mount-points', 'store', id] as const,
  detail: (id: string) => ['mount-points', 'store', id, 'detail'] as const,
  files: (id: string) => ['mount-points', 'store', id, 'files'] as const,
  links: (id: string, path: string) => ['mount-points', 'store', id, 'links', path] as const,
  content: (id: string, path: string) => ['mount-points', 'store', id, 'content', path] as const,
  rendered: (id: string, path: string) => ['mount-points', 'store', id, 'rendered', path] as const,
};

/** v4 `queryKeys.projects.stores(projectId)` — a project's linked stores. */
export const projectStoreKeys = {
  stores: (projectId: string) => ['projects', projectId, 'stores'] as const,
};
