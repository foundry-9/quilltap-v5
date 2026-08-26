import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { coreStreamStub } from '../../core/core-client.testing';
import { WORKSPACE_TAB_ID } from '../../workspace/workspace-contract';
import { Settings } from './settings';

/**
 * Workspace-tab mode (p4.9j2): the Settings shell hosts with a `tab` input and
 * NO `ActivatedRoute` (the tab-mode invariant). The `system` tab now renders the
 * Data & System vertical (P4.9G2), so a permissive CoreClient stub answers any
 * card fetch; the beat exercises the input-precedence + optional-route
 * adaptation. `section` is accepted but unused (v4 `_section`).
 */
describe('Settings (workspace-tab mode)', () => {
  const coreStub = {
    ...coreStreamStub(),
    dispatchExpect: (async () => ({
      type: 'unlockState',
      data: { state: 'unlocked', hasUserPassphrase: false },
    })) as unknown as CoreClient['dispatchExpect'],
    dispatchData: (async () => ({})) as unknown as CoreClient['dispatchData'],
  } as unknown as Partial<CoreClient>;

  async function render(tab: string): Promise<ComponentFixture<Settings>> {
    TestBed.configureTestingModule({
      imports: [Settings],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-1' },
        { provide: CoreClient, useValue: coreStub },
      ],
    });
    const fixture = TestBed.createComponent(Settings);
    fixture.componentRef.setInput('tab', tab);
    fixture.componentRef.setInput('section', 'ignored-when-hosted');
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    return fixture;
  }

  it('opens the tab from the input and drives the subsystem background, no ActivatedRoute', async () => {
    const fixture = await render('system');
    const container = fixture.nativeElement.querySelector('.qt-page-container') as HTMLElement;
    // v4 `useSettingsBackgroundStyle(tab)` — the `tab` override maps to `prospero`.
    expect(container.getAttribute('data-subsystem')).toBe('prospero');
    // The `system` tab now renders the Data & System vertical (P4.9G2).
    expect(fixture.nativeElement.querySelector('qt-settings-system')).not.toBeNull();
    // The Data & System tab is the active one.
    const activeTab = fixture.nativeElement.querySelector('.qt-tab-active') as HTMLElement;
    expect(activeTab.textContent).toContain('Data & System');
  });
});
