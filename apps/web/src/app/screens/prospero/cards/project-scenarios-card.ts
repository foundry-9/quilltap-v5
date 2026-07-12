import { ChangeDetectionStrategy, Component, OnInit, inject, input, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ScenariosManager } from '../../scenarios/shared/scenarios-manager';
import { projectScenarioMutator, type ScenarioMutator } from '../../scenarios/scenarios.api';

/**
 * The project Scenarios card (v4 `app/prospero/[id]/components/ScenariosCard.tsx`):
 * the collapsible header lives here; the CRUD body is the shared
 * {@link ScenariosManager}, fed by the project-scoped mutator. Items created
 * here are offered when starting new chats in this project.
 */
@Component({
  selector: 'qt-project-scenarios-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, ScenariosManager],
  template: `
    @if (mutator(); as m) {
      <qt-collapsible-card
        [title]="'Scenarios (' + m.scenarios().length + ')'"
        description="Reusable starting scenes for new chats in this project"
        icon="scenarios"
        [defaultOpen]="defaultOpen()"
      >
        <qt-scenarios-manager
          [mutator]="m"
          scopeLabel="project"
          emptyMessage="No scenarios yet. Create one and it'll be offered when starting new chats in this project."
        />
      </qt-collapsible-card>
    }
  `,
})
export class ProjectScenariosCard implements OnInit {
  readonly projectId = input.required<string>();
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);
  protected readonly mutator = signal<ScenarioMutator | null>(null);

  ngOnInit(): void {
    const m = projectScenarioMutator(this.core, this.projectId());
    this.mutator.set(m);
    void m.refresh();
  }
}
