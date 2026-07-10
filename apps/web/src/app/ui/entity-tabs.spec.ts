import { Component } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { describe, expect, it, vi } from 'vitest';

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
