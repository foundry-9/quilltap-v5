import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { App } from './app';
import { CoreClient } from './core/core-client';
import type { HealthStatus } from './core/core-client';

/**
 * A CoreClient stub that lets a test pin the initial health result.
 *
 * It also carries the live-stream surface (`events$` / `connection` /
 * `resyncCounter`), because the App now injects the realtime hub at bootstrap
 * (P4.D125) and the hub subscribes to that stream on construction.
 */
function stubClient(health: HealthStatus | Promise<HealthStatus>): Partial<CoreClient> {
  return {
    fetchHealth: () => Promise.resolve(health),
    connect: vi.fn(),
    disconnect: vi.fn(),
    events$: new Subject(),
    connection: signal('idle'),
    resyncCounter: signal(0),
  } as unknown as Partial<CoreClient>;
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<App>> {
  TestBed.configureTestingModule({
    imports: [App],
    // The realtime hub the App bootstraps needs a QueryClient, which the real
    // `appConfig` always provides (v4 mounts its RealtimeProvider inside
    // QueryProvider for the same reason).
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(App);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

describe('App startup gate', () => {
  it('shows the starting-up screen before the first health result', async () => {
    // A health check that never resolves keeps the gate in `loading`.
    const fixture = await render(stubClient(new Promise<HealthStatus>(() => {})));
    expect(fixture.nativeElement.textContent).toContain('Quilltap is starting up');
  });

  it('routes a locked/needs-passphrase vault to the unlock screen', async () => {
    const fixture = await render(stubClient({ kind: 'locked', pepperState: 'needs-passphrase' }));
    expect(fixture.nativeElement.textContent).toContain('Quilltap Awaits Your Credentials');
  });

  it('stamps the theme on <html> before the gate screens paint (finding #15)', async () => {
    // v4's ThemeProvider wraps EVERY page, unlock included. Without the App
    // root constructing the theme service, the pre-unlock screens render with
    // NO .light/.dark class — light-mode text on the dark auth backdrop.
    await render(stubClient({ kind: 'locked', pepperState: 'needs-passphrase' }));
    const root = document.documentElement;
    expect(root.classList.contains('light') || root.classList.contains('dark')).toBe(true);
    expect(root.dataset['theme']).toBeDefined();
  });

  it('routes a needs-setup vault to the setup wizard', async () => {
    const fixture = await render(stubClient({ kind: 'locked', pepperState: 'needs-setup' }));
    expect(fixture.nativeElement.textContent).toContain('Welcome to Quilltap');
  });

  it('shows the Database In Use screen on a lock conflict', async () => {
    const fixture = await render(stubClient({ kind: 'lock-conflict', lockConflict: { reason: 'held' } }));
    expect(fixture.nativeElement.textContent).toContain('Database In Use');
  });
});
