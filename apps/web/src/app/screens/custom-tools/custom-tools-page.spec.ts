import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CoreRequest, CoreResponse, ScopedEvent } from '../../core/core-contract';
import { WORKSPACE_TAB_ID } from '../../workspace/workspace-contract';
import { CustomToolsPage } from './custom-tools-page';

/**
 * Workspace-tab mode (p4.9j2): the Workbench page seeds its initial mode from
 * the `CustomToolsTabPayload` inputs instead of the query string, with NO
 * `ActivatedRoute`. The library/editor bodies render over a permissive
 * CoreClient stub; the assertion is on the seeded `mode()`.
 */
describe('CustomToolsPage (workspace-tab mode)', () => {
  function stubClient(): Partial<CoreClient> {
    const dispatch = vi.fn(
      async (_req: CoreRequest): Promise<CoreResponse> => ({ type: 'ack', data: {} }),
    );
    return {
      events$: new Subject<ScopedEvent>().asObservable(),
      dispatch,
      dispatchData: vi.fn(async () => ({})) as unknown as CoreClient['dispatchData'],
      dispatchExpect: (async (req: CoreRequest) => dispatch(req)) as CoreClient['dispatchExpect'],
    };
  }

  async function render(
    inputs: { mountPointId?: string; path?: string; create?: boolean } = {},
  ): Promise<ComponentFixture<CustomToolsPage>> {
    TestBed.configureTestingModule({
      imports: [CustomToolsPage],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: stubClient() },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-1' },
      ],
    });
    const fixture = TestBed.createComponent(CustomToolsPage);
    if (inputs.mountPointId !== undefined) fixture.componentRef.setInput('mountPointId', inputs.mountPointId);
    if (inputs.path !== undefined) fixture.componentRef.setInput('path', inputs.path);
    if (inputs.create !== undefined) fixture.componentRef.setInput('create', inputs.create);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    return fixture;
  }

  it('opens the library when no payload is given', async () => {
    const fixture = await render();
    expect(fixture.componentInstance.mode()).toEqual({ view: 'library' });
  });

  it('opens one definition in the editor from mountPointId + path', async () => {
    const fixture = await render({ mountPointId: 'm1', path: 'Tools/unlock.tool.json' });
    expect(fixture.componentInstance.mode()).toEqual({
      view: 'edit',
      mountPointId: 'm1',
      path: 'Tools/unlock.tool.json',
    });
  });

  it('opens the builder on a fresh draft from create, destination preselected', async () => {
    const fixture = await render({ create: true, mountPointId: 'm2' });
    expect(fixture.componentInstance.mode()).toEqual({ view: 'create', mountPointId: 'm2' });
  });
});
