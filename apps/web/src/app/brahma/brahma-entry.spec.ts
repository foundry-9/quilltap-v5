/**
 * BrahmaEntry — asserted against v4 `sidebar-footer.tsx`'s Brahma arm:
 * eligibility-gated (disabled without a profile), and the branch
 * `inWorkspace ? openTab('brahma') : openConsole()`.
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Router } from '@angular/router';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { Subject } from 'rxjs';

beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype as unknown as { scrollIntoView?: () => void };
  if (proto && !proto.scrollIntoView) proto.scrollIntoView = () => undefined;
});

import { CoreClient } from '../core/core-client';
import { WorkspaceService } from '../workspace/workspace.service';
import { resetWorkspaceTabsFlagCache } from '../workspace/workspace-flag';
import { BrahmaEntry } from './brahma-entry';
import { BrahmaConsoleService } from './brahma-console.service';

function stubCore(profiles: { id: string; name: string; provider: string; modelName: string }[]) {
  return {
    events$: new Subject().asObservable(),
    dispatchExpect: (async () => ({
      type: 'connectionProfiles',
      data: { profiles, count: profiles.length },
    })) as unknown as CoreClient['dispatchExpect'],
    dispatchData: (async () => ({})) as CoreClient['dispatchData'],
  } as Partial<CoreClient>;
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  profiles: { id: string; name: string; provider: string; modelName: string }[],
  routerUrl: string,
): Promise<{
  fixture: ComponentFixture<BrahmaEntry>;
  service: BrahmaConsoleService;
  openTab: ReturnType<typeof vi.fn>;
}> {
  const openTab = vi.fn();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [BrahmaEntry],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: stubCore(profiles) },
      { provide: WorkspaceService, useValue: { openTab } },
      { provide: Router, useValue: { url: routerUrl } },
    ],
  });
  const service = TestBed.inject(BrahmaConsoleService);
  const fixture = TestBed.createComponent(BrahmaEntry);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, service, openTab };
}

const PROFILES = [{ id: 'p1', name: 'Everyday', provider: 'OPENAI', modelName: 'gpt-4o' }];

describe('BrahmaEntry (v4 sidebar-footer.tsx)', () => {
  beforeEach(() => {
    localStorage.clear();
    resetWorkspaceTabsFlagCache();
  });
  afterEach(() => {
    localStorage.clear();
    resetWorkspaceTabsFlagCache();
    TestBed.resetTestingModule();
  });

  it('disables the button without a connection profile', async () => {
    const { fixture } = await render([], '/salon/1');
    const button = fixture.nativeElement.querySelector('button') as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.getAttribute('title')).toContain('requires a connection profile');
  });

  it('enables the button once a profile exists', async () => {
    const { fixture } = await render(PROFILES, '/salon/1');
    const button = fixture.nativeElement.querySelector('button') as HTMLButtonElement;
    expect(button.disabled).toBe(false);
    expect(button.getAttribute('title')).toBe('Brahma Console');
  });

  it('opens the floating console when not in the workspace', async () => {
    const { fixture, service, openTab } = await render(PROFILES, '/salon/1');
    (fixture.nativeElement.querySelector('button') as HTMLButtonElement).click();
    expect(openTab).not.toHaveBeenCalled();
    expect(service.isOpen()).toBe(true);
  });

  it('opens a brahma tab when in the workspace', async () => {
    const { fixture, service, openTab } = await render(PROFILES, '/workspace');
    (fixture.nativeElement.querySelector('button') as HTMLButtonElement).click();
    expect(openTab).toHaveBeenCalledWith('brahma');
    expect(service.isOpen()).toBe(false);
  });

  it('hosts the floating console dialog', async () => {
    const { fixture } = await render(PROFILES, '/salon/1');
    expect(fixture.nativeElement.querySelector('qt-brahma-console-dialog')).not.toBeNull();
  });
});
