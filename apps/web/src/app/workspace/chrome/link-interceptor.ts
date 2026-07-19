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
 * **v5 divergence:** v4 intercepts `/salon/new` into its NewChatModal; v5 never
 * ported that modal (P4.6q), so `/salon/new` passes through — the New-Chat PAGE
 * navigates and its redirect guard funnels creation (`/salon/:id`) straight back
 * into the workspace.
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

  // /salon/new passes through (v5 has no NewChatModal — the page navigates and
  // the redirect guard funnels the created chat back into the workspace).
  const path = href.split('?')[0];
  if (path === '/salon/new') return null;

  return parseHrefToIntent(href);
}
