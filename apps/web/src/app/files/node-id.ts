/**
 * Mount-relative-path helpers (pure, no widget import).
 *
 * A ported-forward copy of v4 `components/files/svar/node-id.ts`. The bespoke
 * `qt-file-manager` (the D18 spike sent us to build-our-own — see the work
 * order status header) keys every tree node by its **mount-relative path**
 * directly, so it never needs SVAR's opaque numeric/string node ids. But the
 * relative-path arithmetic those helpers performed is identical for us and is
 * reused across `listing-to-tree`, `event-wire-map`, and `reindex-after-copy`,
 * so it ports verbatim.
 *
 * `relPathToNodeId` / `nodeIdToRelPath` are retained with v4's exact semantics
 * (the SVAR-absolute-path codec: root `'/'`, others `'/<rel>'`) — the widget
 * uses the plain relative path as identity, but the codec is kept faithful so
 * its spec pins v4's behavior.
 *
 * `relativePath` always uses `/` separators and never has a leading slash.
 *
 * Cases derived from v4 `components/files/svar/node-id.ts` (no v4 unit test
 * existed; the spec pins the source behavior).
 *
 * @module files/node-id
 */

/** Build a SVAR-style node id (absolute path) from a mount-relative path. */
export function relPathToNodeId(relativePath: string): string {
  const rel = stripLeadingSlash(relativePath);
  return rel ? `/${rel}` : '/';
}

/** Recover the mount-relative path from a SVAR-style node id. */
export function nodeIdToRelPath(id: string): string {
  return stripLeadingSlash(id);
}

/** Parent directory of a mount-relative path; '' for a top-level entry. */
export function relDirname(relativePath: string): string {
  const p = stripTrailingSlash(stripLeadingSlash(relativePath));
  const slash = p.lastIndexOf('/');
  return slash === -1 ? '' : p.slice(0, slash);
}

/** Final segment of a mount-relative path. */
export function relBasename(relativePath: string): string {
  const p = stripTrailingSlash(stripLeadingSlash(relativePath));
  const slash = p.lastIndexOf('/');
  return slash === -1 ? p : p.slice(slash + 1);
}

/** Join a directory + a single segment into a mount-relative path. */
export function relJoin(dir: string, name: string): string {
  const d = stripTrailingSlash(stripLeadingSlash(dir));
  return d ? `${d}/${name}` : name;
}

/** Lower-cased extension without the dot, or '' if none. */
export function extOf(pathOrName: string): string {
  const base = relBasename(pathOrName);
  const dot = base.lastIndexOf('.');
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : '';
}

/** Percent-encode each path segment for use in the `/files/[...path]` route. */
export function encodeRelPath(relativePath: string): string {
  return stripLeadingSlash(relativePath).split('/').map(encodeURIComponent).join('/');
}

function stripLeadingSlash(s: string): string {
  return s.startsWith('/') ? s.slice(1) : s;
}

function stripTrailingSlash(s: string): string {
  return s.endsWith('/') ? s.slice(0, -1) : s;
}
