import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { WORKSPACE_TAB_ID } from '../../workspace/workspace-contract';
import { Settings } from './settings';

/**
 * Workspace-tab mode (p4.9j2): the Settings shell hosts with a `tab` input and
 * NO `ActivatedRoute` (the tab-mode invariant). The `system` tab renders the
 * placeholder — no CoreClient-backed sub-tab body — so this exercises the
 * input-precedence + optional-route adaptation in isolation. `section` is
 * accepted but unused (v4 `_section`).
 */
describe('Settings (workspace-tab mode)', () => {
  async function render(tab: string): Promise<ComponentFixture<Settings>> {
    TestBed.configureTestingModule({
      imports: [Settings],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-1' },
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
    // The `system` tab renders the "not yet fitted out" placeholder.
    expect(fixture.nativeElement.querySelector('qt-settings-placeholder')).not.toBeNull();
    // The Data & System tab is the active one.
    const activeTab = fixture.nativeElement.querySelector('.qt-tab-active') as HTMLElement;
    expect(activeTab.textContent).toContain('Data & System');
  });
});
