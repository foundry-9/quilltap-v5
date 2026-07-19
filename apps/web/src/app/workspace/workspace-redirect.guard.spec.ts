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
});
