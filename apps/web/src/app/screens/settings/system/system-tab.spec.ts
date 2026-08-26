import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { coreStreamStub } from '../../../core/core-client.testing';
import { SystemTab } from './system-tab';

/** A permissive CoreClient stub — the tab mounts every card, several of which fetch. */
function coreStub(): Partial<CoreClient> {
  const dispatchExpect = (async (req: { type: string }) => {
    if (req.type === 'unlockState') {
      return { type: 'unlockState', data: { state: 'unlocked', hasUserPassphrase: false } };
    }
    return { type: 'chatSettings', data: { avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR' } };
  }) as unknown as CoreClient['dispatchExpect'];
  const dispatchData = (async () => ({ logs: [], jobs: [], stats: {}, processorStatus: {}, maxConcurrentJobs: 4 })) as unknown as CoreClient['dispatchData'];
  return { ...coreStreamStub(), dispatchExpect, dispatchData } as unknown as Partial<CoreClient>;
}

async function mount(): Promise<ComponentFixture<SystemTab>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [SystemTab],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: coreStub() }],
  });
  const fixture = TestBed.createComponent(SystemTab);
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('SystemTab', () => {
  it('renders the eight cards in v4 order and NOT the WON`T-PORT Plugins card', async () => {
    const fixture = await mount();
    const titles = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-card-title'),
    ).map((el) => el.textContent?.trim());
    expect(titles).toEqual([
      'Encryption Passphrase',
      'Auto-Lock',
      'Backup & Restore',
      'Import / Export',
      'LLM Logging',
      'Tasks Queue',
      'LLM Logs',
      'Delete All Data',
    ]);
    expect(fixture.nativeElement.textContent).not.toContain('Plugins');
  });
});
