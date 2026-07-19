import { Component } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { describe, expect, it, vi } from 'vitest';

import { WORKSPACE_TAB_ID } from '../workspace/workspace-contract';
import { EntityTabs, type Tab } from './entity-tabs';

@Component({
  imports: [EntityTabs],
  template: `
    <qt-entity-tabs [tabs]="tabs" [defaultTab]="'providers'">
      <ng-template let-active>
        <div class="active-content">{{ active }}</div>
      </ng-template>
    </qt-entity-tabs>
  `,
})
class Host {
  readonly tabs: Tab[] = [
    { id: 'providers', label: 'AI Providers' },
    { id: 'appearance', label: 'Appearance' },
    { id: 'chat', label: 'Chat' },
  ];
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.configureTestingModule({ imports: [Host], providers: [provideRouter([])] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

describe('EntityTabs (deep links)', () => {
  it('renders every tab and shows the default tab content', async () => {
    const fixture = await render();
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('AI Providers');
    expect(text).toContain('Appearance');
    expect(text).toContain('Chat');
    // The projected content receives the active (default) tab id.
    expect(fixture.nativeElement.querySelector('.active-content').textContent.trim()).toBe(
      'providers',
    );
  });

  it('marks the URL-active tab and clears the section on a tab switch', async () => {
    const fixture = await render();
    const router = TestBed.inject(Router);
    const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);

    const appearanceButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.includes('Appearance'),
    ) as HTMLButtonElement;
    appearanceButton.click();

    expect(navigate).toHaveBeenCalledWith(
      [],
      expect.objectContaining({
        queryParams: { tab: 'appearance', section: null },
        replaceUrl: true,
      }),
    );
  });

  it('switching back to the default tab removes the ?tab= param', async () => {
    const fixture = await render();
    const router = TestBed.inject(Router);
    const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);

    const providersButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) =>
      (b as HTMLButtonElement).textContent?.includes('AI Providers'),
    ) as HTMLButtonElement;
    providersButton.click();

    expect(navigate).toHaveBeenCalledWith(
      [],
      expect.objectContaining({ queryParams: { tab: null, section: null } }),
    );
  });
});

/**
 * Workspace-tab mode (p4.9j2) — v4 `EntityTabs` `persistToUrl={false}` local
 * state. With `WORKSPACE_TAB_ID` provided, tab switching is local state and the
 * router is never touched (a hosted tab has no URL to persist to).
 */
describe('EntityTabs (workspace-tab mode)', () => {
  async function renderTab(): Promise<ComponentFixture<Host>> {
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [provideRouter([]), { provide: WORKSPACE_TAB_ID, useValue: 'tab-1' }],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    return fixture;
  }

  it('shows the default tab and switches tabs WITHOUT navigating', async () => {
    const fixture = await renderTab();
    const router = TestBed.inject(Router);
    const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);

    expect(fixture.nativeElement.querySelector('.active-content').textContent.trim()).toBe(
      'providers',
    );

    const appearanceButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.includes('Appearance'),
    ) as HTMLButtonElement;
    appearanceButton.click();
    fixture.detectChanges();

    // Local state moved; the router was never touched.
    expect(fixture.nativeElement.querySelector('.active-content').textContent.trim()).toBe(
      'appearance',
    );
    expect(navigate).not.toHaveBeenCalled();
  });
});
