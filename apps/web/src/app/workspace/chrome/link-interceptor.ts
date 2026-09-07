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
  // **A v5-only rule, and v4 needs none** (P4.80). v4's chat card is a `<div>`
  // whose own `handleCardClick` early-returns for `closest('button')`
  // (`ChatCard.tsx:188-194`), so a click on one of its action buttons never
  // reaches v4's interceptor as a link click at all. v5's card wraps its whole
  // body in an `<a routerLink>`, which makes every action button — delete,
  // copy-link, the Scriptorium badge, the project-remove X — an anchor
  // DESCENDANT. Without this guard the capture handler `stopImmediatePropagation`s
  // first and the button's own handler never runs: the button silently opens the
  // chat instead of doing its job. Measured on the copy-link button, which
  // shipped long before P4.80 tripped over it with the delete button.
  //
  // Widening only: this can make the interceptor pass a click THROUGH, never
  // open a tab it would not have opened.
  const button = target?.closest?.('button');
  if (button && anchor.contains(button)) return null;
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
        characterId || projectId || autonomous ? { characterId, projectId, autonomous } : undefined,
    };
  }

  return parseHrefToIntent(href);
}
