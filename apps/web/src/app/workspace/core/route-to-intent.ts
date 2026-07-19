/**
 * Map an in-app href to a workspace tab intent (port of v4
 * `lib/navigation/route-to-intent.ts`, baseline `b8b12695`, plus the v5 route
 * adaptations noted below).
 *
 * The inverse of the old-route redirects: when a link is clicked *inside* the
 * workspace, we open (or focus) the matching tab client-side instead of doing a
 * route navigation that would unmount and rebuild the whole workspace. Returns
 * `null` for hrefs with no tab equivalent (new-chat, the salon list, external
 * links, bare project/store detail) so those navigate normally.
 *
 * **v5 adaptations (v5's route names differ from v4's `/aurora`):**
 *  - `/characters`        → aurora  (v5's roster route; v4 used `/aurora`)
 *  - `/characters/new`    → character-new
 *  - `/characters/<id>`   → character-view  (v5 has NO `/view` suffix — its
 *    `/characters/:id` IS the detail route; v4's bare `/aurora/<id>` had no tab
 *    equivalent and stayed `null`, which v5 keeps for the `/aurora/*` family it
 *    no longer routes to).
 * v4's `/aurora*` and legacy `/characters/<id>/{edit,view}` rows all still map
 * byte-identically (proven by the tier-1 corpus); the v5-only rows are covered
 * by v5-side unit tests.
 *
 * @module workspace/core/route-to-intent
 */

import type { TabKind } from '../workspace-contract';

export interface TabIntent {
  kind: TabKind;
  payload?: unknown;
}

export function parseHrefToIntent(href: string): TabIntent | null {
  if (!href || !href.startsWith('/')) return null; // external / non-path

  let path = href;
  let query = '';
  const qIdx = href.indexOf('?');
  if (qIdx >= 0) {
    path = href.slice(0, qIdx);
    query = href.slice(qIdx + 1);
  }
  const hashIdx = path.indexOf('#');
  if (hashIdx >= 0) path = path.slice(0, hashIdx);
  if (path.length > 1 && path.endsWith('/')) path = path.slice(0, -1);

  // /salon/<id> — a specific conversation (not the list, not new-chat).
  const salonMatch = path.match(/^\/salon\/([^/]+)$/);
  if (salonMatch) {
    const id = salonMatch[1];
    if (id === 'new') return null;
    return { kind: 'salon', payload: { chatId: id } };
  }

  // /aurora/<id>/edit (or legacy /characters/<id>/edit) — the character editor.
  const editMatch = path.match(/^\/(?:aurora|characters)\/([^/]+)\/edit$/);
  if (editMatch) {
    const sp = new URLSearchParams(query);
    return {
      kind: 'character-edit',
      payload: { characterId: editMatch[1], tab: sp.get('tab') ?? undefined },
    };
  }

  // /aurora/<id>/view (or legacy /characters/<id>/view) — the read-only detail.
  const viewMatch = path.match(/^\/(?:aurora|characters)\/([^/]+)\/view$/);
  if (viewMatch) {
    const sp = new URLSearchParams(query);
    return {
      kind: 'character-view',
      payload: { characterId: viewMatch[1], tab: sp.get('tab') ?? undefined },
    };
  }

  // v5 addition: a bare /characters/<id> IS the detail route (no `/view`
  // suffix in v5). `/characters/new` is the create form; anything else is a
  // read-only character-view. `/characters` (bare) and `/characters/groups/…`
  // are NOT matched here — the former is handled in the switch below, the
  // latter (a group editor) has no tab equivalent → null.
  const charBareMatch = path.match(/^\/characters\/([^/]+)$/);
  if (charBareMatch) {
    const id = charBareMatch[1];
    if (id === 'new') return { kind: 'character-new' };
    return { kind: 'character-view', payload: { characterId: id } };
  }

  switch (path) {
    case '/':
      return { kind: 'home' };
    case '/aurora':
    case '/characters': // v5 route name for the roster
      return { kind: 'aurora' };
    case '/aurora/new':
      return { kind: 'character-new' };
    case '/profile':
      return { kind: 'profile' };
    case '/about':
      return { kind: 'about' };
    case '/generate-image':
      return { kind: 'generate-image' };
    case '/settings/wizard':
      return { kind: 'settings-wizard' };
    case '/prospero':
      return { kind: 'prospero' };
    case '/scriptorium':
      return { kind: 'scriptorium' };
    case '/files':
      return { kind: 'files' };
    case '/photos':
      return { kind: 'photos' };
    case '/scenarios':
      return { kind: 'scenarios' };
    case '/custom-tools': {
      const sp = new URLSearchParams(query);
      const mountPointId = sp.get('mount') ?? undefined;
      const p = sp.get('path') ?? undefined;
      const create = sp.get('new') === '1' || undefined;
      return {
        kind: 'custom-tools',
        payload: mountPointId || p || create ? { mountPointId, path: p, create } : undefined,
      };
    }
    case '/settings': {
      const sp = new URLSearchParams(query);
      return {
        kind: 'settings',
        payload: { tab: sp.get('tab') ?? undefined, section: sp.get('section') ?? undefined },
      };
    }
    default:
      return null;
  }
}
