import { ChangeDetectionStrategy, Component, OnInit, inject, signal } from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { Icon } from '../../ui/icon';
import { ScenariosManager } from './shared/scenarios-manager';
import { generalScenarioMutator, type ScenarioMutator } from './scenarios.api';

/**
 * The general Scenarios page (v4 `app/scenarios/ScenariosView.tsx`). Top-level
 * CRUD for the instance-wide `Scenarios/` folder inside the "Quilltap General"
 * mount. These scenarios are offered alongside project and character scenarios
 * in every New Chat dialog, with or without a project.
 *
 * Mirrors the project Scenarios card but at page scope, fed by the
 * general-scoped mutator. When the mount is unprovisioned (`mountPointId` null)
 * the list is simply empty and mutations surface the server's "not provisioned"
 * refusal as an error — matching v4 (no special-cased empty state).
 */
@Component({
  selector: 'qt-scenarios-page',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ScenariosManager],
  template: `
    <div class="qt-page-container min-h-screen text-foreground">
      <div>
        <div class="mb-6 flex items-start gap-3">
          <qt-icon name="scenarios" class="w-8 h-8 qt-text-primary mt-1" />
          <div>
            <h1 class="qt-heading-1">General Scenarios</h1>
            <p class="qt-text-secondary qt-body mt-2 max-w-2xl">
              Starting scenes you'd like Quilltap to remember for every conversation, no matter the
              project or company kept. They live in the &ldquo;Quilltap General&rdquo; document store
              and are offered alongside any project-specific and character-specific scenarios when a
              new chat begins. When a project sets its own default, that one takes precedence;
              otherwise the general default you mark below will be pre-selected.
            </p>
          </div>
        </div>

        <div class="qt-card qt-bg-card qt-border rounded-lg p-6">
          @if (mutator(); as m) {
            <qt-scenarios-manager
              [mutator]="m"
              scopeLabel="general"
              emptyMessage="No general scenarios yet. Compose one and it'll appear in every New Chat dialog from now on."
            />
          }
        </div>
      </div>
    </div>
  `,
})
export class ScenariosPage implements OnInit {
  private readonly core = inject(CoreClient);
  protected readonly mutator = signal<ScenarioMutator | null>(null);

  ngOnInit(): void {
    const m = generalScenarioMutator(this.core);
    this.mutator.set(m);
    void m.refresh();
  }
}
