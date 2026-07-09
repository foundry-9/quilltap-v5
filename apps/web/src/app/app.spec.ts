import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { App } from './app';
import { CoreClient } from './core/core-client';
import type { HealthStatus } from './core/core-client';

/** A CoreClient stub that lets a test pin the initial health result. */
function stubClient(health: HealthStatus | Promise<HealthStatus>): Partial<CoreClient> {
  return {
    fetchHealth: () => Promise.resolve(health),
    connect: vi.fn(),
    disconnect: vi.fn(),
  } as Partial<CoreClient>;
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<App>> {
  TestBed.configureTestingModule({
    imports: [App],
    providers: [{ provide: CoreClient, useValue: client }],
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

  it('routes a needs-setup vault to the setup wizard', async () => {
    const fixture = await render(stubClient({ kind: 'locked', pepperState: 'needs-setup' }));
    expect(fixture.nativeElement.textContent).toContain('Welcome to Quilltap');
  });

  it('shows the Database In Use screen on a lock conflict', async () => {
    const fixture = await render(stubClient({ kind: 'lock-conflict', lockConflict: { reason: 'held' } }));
    expect(fixture.nativeElement.textContent).toContain('Database In Use');
  });
});
