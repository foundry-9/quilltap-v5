import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { ActivatedRoute } from '@angular/router';

import { WorkbenchEditor } from './workbench-editor';
import { WorkbenchLibrary } from './workbench-library';

/**
 * Pascal's Workbench — the `/custom-tools` screen (v4 `app/custom-tools/page.tsx`
 * + `CustomToolsView.tsx`).
 *
 * A three-mode state machine. The landing state is the library; drilling into a
 * definition renders the editor **IN PLACE** — v4's workspace keep-alive rule,
 * which in v5 is simply component state on one routed screen rather than a route
 * change per definition.
 *
 * Deep links ride the query string, exactly as v4's own no-workspace fallback
 * writes them (`FileTable.tsx:113`, `CustomToolsDropdown.tsx:139`,
 * `CustomToolsSettings.tsx:29-31` all `router.push` these):
 *
 *   `?mount=<mountPointId>&path=Tools/<file>` — open one definition to edit
 *   `?new=1&mount=<mountPointId>`             — create, destination preselected
 *
 * **Named omission (`p4.9j`):** v4 ALSO has `redirectToWorkspaceTab`, which
 * opens the Workbench as a workspace tab instead of navigating. v5 has no
 * workspace tabs — that ruling is still open — so the query-param fallback is
 * the only path here. It is v4's own fallback, not an invention.
 */
type WorkbenchMode =
  | { view: 'library' }
  | { view: 'edit'; mountPointId: string; path: string }
  | { view: 'create'; mountPointId?: string; template?: string };

@Component({
  selector: 'qt-custom-tools-page',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [WorkbenchLibrary, WorkbenchEditor],
  template: `
    @switch (mode().view) {
      @case ('library') {
        <qt-workbench-library
          (open)="mode.set({ view: 'edit', mountPointId: $event.mountPointId, path: $event.path })"
          (create)="mode.set({ view: 'create', mountPointId: $event })"
          (duplicateTemplate)="mode.set({ view: 'create', template: $event })"
        />
      }
      @case ('edit') {
        <qt-workbench-editor
          [source]="editSource()"
          (back)="mode.set({ view: 'library' })"
          (openOther)="
            mode.set({ view: 'edit', mountPointId: $event.mountPointId, path: $event.path })
          "
        />
      }
      @default {
        <qt-workbench-editor
          [create]="createSpec()"
          (back)="mode.set({ view: 'library' })"
          (openOther)="
            mode.set({ view: 'edit', mountPointId: $event.mountPointId, path: $event.path })
          "
        />
      }
    }
  `,
})
export class CustomToolsPage {
  private readonly route = inject(ActivatedRoute);

  readonly mode = signal<WorkbenchMode>({ view: 'library' });

  constructor() {
    const params = this.route.snapshot.queryParamMap;
    const mount = params.get('mount') ?? undefined;
    const path = params.get('path') ?? undefined;
    const isNew = params.get('new') === '1';

    if (isNew) {
      this.mode.set({ view: 'create', mountPointId: mount });
    } else if (mount && path) {
      this.mode.set({ view: 'edit', mountPointId: mount, path });
    }
  }

  readonly editSource = computed(() => {
    const m = this.mode();
    return m.view === 'edit' ? { mountPointId: m.mountPointId, path: m.path } : null;
  });

  readonly createSpec = computed(() => {
    const m = this.mode();
    return m.view === 'create' ? { mountPointId: m.mountPointId, template: m.template } : null;
  });
}
