/**
 * Workspace link interception (port of v4 `WorkspaceLinkInterceptor.tsx`,
 * baseline `b8b12695`).
 *
 * One delegated **capture**-phase click handler (attached by the host, active
 * only on `/workspace`) keeps every in-app link keep-alive-safe: an anchor whose
 * href maps to a tab opens/focuses that tab in place instead of navigating (a
 * navigation would redirect back to `/workspace` and remount the whole
 * workspace, tearing down a streaming Salon).
 *
 * **Angular pin (documented divergence):** v4 relies on `preventDefault()`
 * because Next's `<Link>` checks `defaultPrevented`; Angular's `RouterLink` does
 * NOT — so the host's capture handler must ALSO `stopPropagation()` (and
 * `stopImmediatePropagation()`) so RouterLink's own bubble listener never fires.
 * Middle/modifier clicks pass through untouched.
 *
 * **`/salon/new` (P4.d16 tier 2):** v4 special-cases it here into its
 * NewChatModal, deliberately keeping it OUT of `parseHrefToIntent` (the corpus
 * pins that null). v5 has no modal, so the special case opens the v5-only
 * `salon-new` tab hosting the New-Chat screen — same shape, same place, one
 * translated destination. The `?characterId=`/`?projectId=`/`?autonomous=1`
 * seeds ride along exactly as v4 hands them to the modal.
 *
 * This module is the pure decision function; the host owns the DOM wiring.
 *
 * @module workspace/chrome/link-interceptor
 */

import { parseHrefToIntent } from '../core/route-to-intent';
import type { OpenIntent } from './workspace-intent';

/**
 * Decide what a click should do inside the workspace. Returns the tab intent to
 * open (the caller then `preventDefault`/`stopPropagation`s and opens it), or
 * `null` to let the click navigate normally (external links, modifier clicks,
 * `/salon/new`, hrefs with no tab equivalent).
 */
export function interpretWorkspaceLinkClick(e: MouseEvent): OpenIntent | null {
  // Left-click only; let modified clicks open a real browser tab/window.
  if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return null;
  if (e.defaultPrevented) return null;

  const target = e.target as HTMLElement | null;
  const anchor = target?.closest?.('a[href]') as HTMLAnchorElement | null;
  if (!anchor) return null;
  if (anchor.hasAttribute('download')) return null;
  const linkTarget = anchor.getAttribute('target');
  if (linkTarget && linkTarget !== '_self') return null;

  const href = anchor.getAttribute('href') || '';
  if (!href.startsWith('/')) return null; // external / hash / relative

  // /salon/new is the interceptor's own case — never parseHrefToIntent's (v4
  // keeps it null there; the corpus pins it).
  const [path, query = ''] = href.split('?');
  if (path === '/salon/new') {
    const sp = new URLSearchParams(query);
    const characterId = sp.get('characterId') || undefined;
    const projectId = sp.get('projectId') || undefined;
    const autonomous = sp.get('autonomous') === '1';
    return {
      kind: 'salon-new',
      payload:
        characterId || projectId || autonomous
          ? { characterId, projectId, autonomous }
          : undefined,
    };
  }

  return parseHrefToIntent(href);
}
