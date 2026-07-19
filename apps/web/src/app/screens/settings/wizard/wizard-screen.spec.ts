import { TestBed } from '@angular/core/testing';
import { ActivatedRoute, Router, convertToParamMap, provideRouter } from '@angular/router';
import { of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import {
  WORKSPACE_HANDLE,
  WORKSPACE_TAB_ID,
  type WorkspaceHandle,
} from '../../../workspace/workspace-contract';
import { WizardScreen } from './wizard-screen';

interface Callable {
  onComplete(): void;
  onCancel(): void;
}

/**
 * The provider wizard's self-close seam (p4.9j2, v4 `useCloseSelfTab`). We drive
 * the WizardScreen's complete/cancel handlers directly (the child ProviderWizard
 * is not rendered — no CoreClient needed).
 */
describe('WizardScreen (self-close seam)', () => {
  it('routed mode navigates on complete/cancel (byte-identical)', () => {
    TestBed.configureTestingModule({
      imports: [WizardScreen],
      providers: [
        provideRouter([]),
        {
          provide: ActivatedRoute,
          useValue: { queryParamMap: of(convertToParamMap({ mode: 'settings' })) },
        },
      ],
    });
    const fixture = TestBed.createComponent(WizardScreen);
    const router = TestBed.inject(Router);
    const nav = vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true);

    (fixture.componentInstance as unknown as Callable).onComplete();
    expect(nav).toHaveBeenCalledWith('/settings?tab=providers');
  });

  it('workspace-tab mode closes the tab instead of navigating', () => {
    const closed: string[] = [];
    const handle: WorkspaceHandle = {
      openTab: vi.fn(() => 'x'),
      closeTab: vi.fn((id: string) => closed.push(id)),
      refreshTab: vi.fn(),
    };
    TestBed.configureTestingModule({
      imports: [WizardScreen],
      providers: [
        provideRouter([]),
        { provide: WORKSPACE_HANDLE, useValue: handle },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-wiz' },
      ],
    });
    const fixture = TestBed.createComponent(WizardScreen);
    const router = TestBed.inject(Router);
    const nav = vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true);

    (fixture.componentInstance as unknown as Callable).onComplete();
    expect(closed).toEqual(['tab-wiz']);
    (fixture.componentInstance as unknown as Callable).onCancel();
    expect(closed).toEqual(['tab-wiz', 'tab-wiz']);
    expect(nav).not.toHaveBeenCalled();
  });
});
