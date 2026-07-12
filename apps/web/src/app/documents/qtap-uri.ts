/**
 * `qtap://` document-URI producer (the client subset).
 *
 * A `qtap://` URI is the human- and model-readable serialization of the
 * document-addressing triple `{ scope, mountPoint, path }`. The Document Pane
 * only ever EMITS a URI for its own open document (the "copy URL" affordance),
 * so this is the producing half of v4's `lib/doc-edit/qtap-uri.ts` — pure
 * string work, byte-for-byte with v4's `formatQtapUri` / `encodeAuthority` /
 * `encodePath`. Parsing (the qtap:// link-opening path) is a lane-B server
 * concern (`chatDocumentResolve`); we never decode here.
 *
 * @module documents/qtap-uri
 */

import type { DocumentScope } from '../core/core-contract';

const QTAP_URI_SCHEME = 'qtap://';

/**
 * The reserved authority for the acting character's own vault (v4
 * `SELF_VAULT_TOKEN` in `path-resolver.ts`). Matched case-insensitively.
 */
const SELF_VAULT_TOKEN = 'self';

/** The fully-decoded address a `qtap://` URI denotes (the fields we emit). */
export interface QtapUriParts {
  scope: DocumentScope;
  /** Present only when `scope === 'document_store'` (the SELF token or a name/UUID). */
  mountPoint?: string;
  /** Relative path within the store/scope. '' means the store root. */
  path: string;
}

/** Authority slot: `project` / `general` verbatim, else `self` or the encoded mount name. */
function encodeAuthority(parts: QtapUriParts): string {
  if (parts.scope === 'project') return 'project';
  if (parts.scope === 'general') return 'general';
  // document_store
  const mp = parts.mountPoint ?? '';
  if (mp.toLowerCase() === SELF_VAULT_TOKEN) return 'self';
  return encodeURIComponent(mp);
}

/** Encode a relative path: split on `/`, percent-encode each segment, rejoin. */
function encodePath(path: string): string {
  if (path === '') return '';
  return path
    .split('/')
    .map((seg) => encodeURIComponent(seg))
    .join('/');
}

/**
 * Build the canonical `qtap://` URI for a document address (v4 `formatQtapUri`,
 * the subset the pane needs: no heading/level/query). Authority and each path
 * segment are percent-encoded.
 */
export function formatQtapUri(parts: QtapUriParts): string {
  const authority = encodeAuthority(parts);
  return `${QTAP_URI_SCHEME}${authority}/${encodePath(parts.path ?? '')}`;
}

/**
 * The Document Pane's URI (v4 `DocumentPane` `documentUri`): project/general
 * scopes emit their reserved authority; a document_store scope emits the mount
 * name (falling back to the `self` token when blank).
 */
export function documentPaneUri(doc: {
  scope: DocumentScope;
  mountPoint?: string | null;
  filePath: string;
}): string {
  if (doc.scope === 'project' || doc.scope === 'general') {
    return formatQtapUri({ scope: doc.scope, path: doc.filePath });
  }
  return formatQtapUri({
    scope: 'document_store',
    mountPoint: doc.mountPoint?.trim() || 'self',
    path: doc.filePath,
  });
}
