/**
 * The system image-aesthetics client surface (the P4.6ar round's Shared contract
 * §2). v4's Images tab points its `AestheticEditorField`s straight at
 * `GET`/`PUT /api/v1/system/image-aesthetics?kind=…`
 * (`ImagesTabContent.tsx:54-65`); v5 goes through the
 * `systemImageAestheticsGet`/`Set` dispatch verbs, which lane A (P4.6ar)
 * provides.
 *
 * The request types were folded into the `CoreRequest` union at unification
 * name-for-name against `types.rs` (the P4.6ao precedent); they are re-exported
 * here so consumers are unchanged.
 *
 * @module screens/settings/images/system-aesthetics.api
 */

import type { CoreClient } from '../../../core/core-client';
import type {
  SystemImageAestheticsGetRequest,
  SystemImageAestheticsSetRequest,
} from '../../../core/core-contract';

export type {
  SystemImageAestheticsGetRequest,
  SystemImageAestheticsSetRequest,
} from '../../../core/core-contract';

/** v4's `kind` query param — the only two the server accepts (§2). */
export type AestheticKind = 'lantern' | 'aurora';

export const systemAestheticKeys = {
  aesthetic: (kind: AestheticKind) => ['system', 'image-aesthetics', kind] as const,
};

/** GET the house-style markdown for one kind (`{content: ''}` when unset). */
export async function fetchSystemAesthetic(core: CoreClient, kind: AestheticKind): Promise<string> {
  const req: SystemImageAestheticsGetRequest = { type: 'systemImageAestheticsGet', kind };
  const data = await core.dispatchData(req);
  return (data['content'] as string) ?? '';
}

/** PUT it back; empty content deletes the file (see the request doc). */
export async function setSystemAesthetic(
  core: CoreClient,
  kind: AestheticKind,
  content: string,
): Promise<void> {
  const req: SystemImageAestheticsSetRequest = { type: 'systemImageAestheticsSet', kind, content };
  await core.dispatchData(req);
}
