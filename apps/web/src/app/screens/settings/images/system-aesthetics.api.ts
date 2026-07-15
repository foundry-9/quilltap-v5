/**
 * The system image-aesthetics client surface (the P4.6ar round's Shared contract
 * §2). v4's Images tab points its `AestheticEditorField`s straight at
 * `GET`/`PUT /api/v1/system/image-aesthetics?kind=…`
 * (`ImagesTabContent.tsx:54-65`); v5 goes through the
 * `systemImageAestheticsGet`/`Set` dispatch verbs, which lane A (P4.6ar)
 * provides.
 *
 * The request types live here — in a lane-owned api file — rather than in
 * `core-contract.ts`, per the round's Ownership section: the request `type` is
 * bridged with a cast at the dispatch calls below until the unifier folds them
 * into the `CoreRequest` union name-for-name against `types.rs`. This is the
 * established P4.6ao / P4.6am precedent for a cross-lane verb.
 *
 * @module screens/settings/images/system-aesthetics.api
 */

import type { CoreClient } from '../../../core/core-client';
import type { CoreRequest } from '../../../core/core-contract';

/** v4's `kind` query param — the only two the server accepts (§2). */
export type AestheticKind = 'lantern' | 'aurora';

/** v4 `GET /api/v1/system/image-aesthetics?kind=lantern|aurora` (§2). */
export interface SystemImageAestheticsGetRequest {
  type: 'systemImageAestheticsGet';
  kind: AestheticKind;
}

/** v4 `PUT /api/v1/system/image-aesthetics?kind=…` (§2). */
export interface SystemImageAestheticsSetRequest {
  type: 'systemImageAestheticsSet';
  kind: AestheticKind;
  /**
   * Optional on the wire (`#[serde(default)]`), but the client always sends it:
   * empty content is meaningful — it DELETES the stored file and restores the
   * fallback. The field passes the editor's string verbatim and the server owns
   * that semantics (pinned by lane A's `image_aesthetics_routes_equivalence`).
   */
  content?: string;
}

export const systemAestheticKeys = {
  aesthetic: (kind: AestheticKind) => ['system', 'image-aesthetics', kind] as const,
};

/** GET the house-style markdown for one kind (`{content: ''}` when unset). */
export async function fetchSystemAesthetic(core: CoreClient, kind: AestheticKind): Promise<string> {
  const req: SystemImageAestheticsGetRequest = { type: 'systemImageAestheticsGet', kind };
  const data = await core.dispatchData(req as unknown as CoreRequest);
  return (data['content'] as string) ?? '';
}

/** PUT it back; empty content deletes the file (see the request doc). */
export async function setSystemAesthetic(
  core: CoreClient,
  kind: AestheticKind,
  content: string,
): Promise<void> {
  const req: SystemImageAestheticsSetRequest = { type: 'systemImageAestheticsSet', kind, content };
  await core.dispatchData(req as unknown as CoreRequest);
}
