/**
 * workspaceRedirectGuard — the old-route → workspace redirect (v4
 * workspace-redirect.ts). Needs the Angular DI context (Router); runs under
 * `ng test`.
 */

import { TestBed } from '@angular/core/testing';
import {
  ActivatedRouteSnapshot,
  convertToParamMap,
  provideRouter,
  RouterStateSnapshot,
  UrlTree,
} from '@angular/router';
import { beforeEach, describe, expect, it } from 'vitest';

import { workspaceRedirectGuard } from './workspace-redirect.guard';
import { resetWorkspaceTabsFlagCache, WORKSPACE_TABS_KEY } from './workspace-flag';

function snap(
  params: Record<string, string> = {},
  query: Record<string, string> = {},
): ActivatedRouteSnapshot {
  return {
    paramMap: convertToParamMap(params),
    queryParamMap: convertToParamMap(query),
  } as unknown as ActivatedRouteSnapshot;
}

const STATE = {} as RouterStateSnapshot;

function run(guard: ReturnType<typeof workspaceRedirectGuard>, route: ActivatedRouteSnapshot) {
  return TestBed.runInInjectionContext(() => guard(route, STATE));
}

describe('workspaceRedirectGuard', () => {
  beforeEach(() => {
    localStorage.clear();
    resetWorkspaceTabsFlagCache();
    TestBed.configureTestingModule({ providers: [provideRouter([])] });
  });

  it('is a no-op (renders the legacy route) when the flag is off', () => {
    localStorage.setItem(WORKSPACE_TABS_KEY, '0');
    resetWorkspaceTabsFlagCache();
    expect(run(workspaceRedirectGuard('home'), snap())).toBe(true);
  });

  it('redirects a singleton to /workspace?open=…', () => {
    const tree = run(workspaceRedirectGuard('aurora'), snap()) as UrlTree;
    expect(tree).toBeInstanceOf(UrlTree);
    expect(tree.toString().split('?')[0]).toBe('/workspace');
    expect(tree.queryParams).toEqual({ open: 'aurora' });
  });

  it('carries the salon chatId from the route param', () => {
    const guard = workspaceRedirectGuard('salon', (r) => ({ chatId: r.paramMap.get('id') }));
    const tree = run(guard, snap({ id: 'c1' })) as UrlTree;
    expect(tree.queryParams).toEqual({ open: 'salon', chatId: 'c1' });
  });

  it('carries settings tab/section and drops empty params', () => {
    const guard = workspaceRedirectGuard('settings', (r) => ({
      tab: r.queryParamMap.get('tab'),
      section: r.queryParamMap.get('section'),
    }));
    const tree = run(guard, snap({}, { tab: 'system' })) as UrlTree;
    expect(tree.queryParams).toEqual({ open: 'settings', tab: 'system' }); // no section key
  });

  it('carries custom-tools mount/path/new', () => {
    const guard = workspaceRedirectGuard('custom-tools', (r) => ({
      mount: r.queryParamMap.get('mount'),
      path: r.queryParamMap.get('path'),
      new: r.queryParamMap.get('new'),
    }));
    const tree = run(guard, snap({}, { mount: 'm1', new: '1' })) as UrlTree;
    expect(tree.queryParams).toEqual({ open: 'custom-tools', mount: 'm1', new: '1' });
  });

  // --- P4.d16 (v4 `8d86847a`): the deep links that used to escape ----------
  it('redirects the salon list to the salon-list tab', () => {
    const tree = run(workspaceRedirectGuard('salon-list'), snap()) as UrlTree;
    expect(tree.queryParams).toEqual({ open: 'salon-list' });
  });

  it('carries both ids for the terminal pop-out', () => {
    const guard = workspaceRedirectGuard('terminal', (r) => ({
      chatId: r.paramMap.get('id'),
      sessionId: r.paramMap.get('sessionId'),
    }));
    const tree = run(guard, snap({ id: 'c1', sessionId: 's9' })) as UrlTree;
    expect(tree.queryParams).toEqual({ open: 'terminal', chatId: 'c1', sessionId: 's9' });
  });

  it('carries the drill ids for the project / store / group detail routes', () => {
    const project = workspaceRedirectGuard('prospero', (r) => ({
      projectId: r.paramMap.get('id'),
    }));
    expect((run(project, snap({ id: 'p1' })) as UrlTree).queryParams).toEqual({
      open: 'prospero',
      projectId: 'p1',
    });

    const store = workspaceRedirectGuard('scriptorium', (r) => ({ storeId: r.paramMap.get('id') }));
    expect((run(store, snap({ id: 's1' })) as UrlTree).queryParams).toEqual({
      open: 'scriptorium',
      storeId: 's1',
    });

    const group = workspaceRedirectGuard('aurora', (r) => ({ groupId: r.paramMap.get('id') }));
    expect((run(group, snap({ id: 'g1' })) as UrlTree).queryParams).toEqual({
      open: 'aurora',
      groupId: 'g1',
    });
  });

  it('preserves ?tab= and forwards ?action= on the character detail', () => {
    const guard = workspaceRedirectGuard('character-view', (r) => ({
      characterId: r.paramMap.get('id'),
      tab: r.queryParamMap.get('tab'),
      action: r.queryParamMap.get('action'),
    }));
    const tree = run(guard, snap({ id: 'ch1' }, { tab: 'conversations' })) as UrlTree;
    expect(tree.queryParams).toEqual({
      open: 'character-view',
      characterId: 'ch1',
      tab: 'conversations',
    }); // no action key when absent
    const withAction = run(guard, snap({ id: 'ch1' }, { action: 'chat' })) as UrlTree;
    expect(withAction.queryParams).toEqual({
      open: 'character-view',
      characterId: 'ch1',
      action: 'chat',
    });
  });

  // p4.9j3 item 4: the settings-wizard bypass for the fresh-instance handoff.
  const wizardBypass = (r: ActivatedRouteSnapshot) => r.queryParamMap.get('mode') === 'setup';

  it('flag ON + mode=setup ⇒ passes through (no redirect; the fresh wizard renders standalone)', () => {
    const guard = workspaceRedirectGuard('settings-wizard', undefined, wizardBypass);
    expect(run(guard, snap({}, { mode: 'setup' }))).toBe(true);
  });

  it('flag ON without mode=setup ⇒ redirects into the workspace as today', () => {
    const guard = workspaceRedirectGuard('settings-wizard', undefined, wizardBypass);
    const tree = run(guard, snap()) as UrlTree;
    expect(tree).toBeInstanceOf(UrlTree);
    expect(tree.queryParams).toEqual({ open: 'settings-wizard' });
  });
});
