import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  inject,
  input,
  signal,
} from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { CollapsibleCard } from '../../ui/collapsible-card';
import { groupScenarioMutator, type ScenarioMutator } from '../scenarios/scenarios.api';
import { ScenariosManager, type ScenarioShelf } from '../scenarios/shared/scenarios-manager';

/**
 * The group Scenarios card (v4 `app/aurora/groups/components/
 * GroupScenariosCard.tsx`, NEW in `08c49319d`): group scenario management on
 * the group's page. The collapsible header lives here; the CRUD body is the
 * shared {@link ScenariosManager}, fed by the group-scoped mutator — the same
 * shape as the project card. Scenarios filed here are offered whenever a
 * member of the group takes a seat in a new chat, and the shelf offers the
 * Host's Scenario Builder over the group's own stores.
 */
@Component({
  selector: 'qt-group-scenarios-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, ScenariosManager],
  template: `
    @if (mutator(); as m) {
      <qt-collapsible-card
        [title]="'Scenarios (' + m.scenarios().length + ')'"
        description="Reusable starting scenes offered whenever a member takes a seat"
        icon="scenarios"
        [defaultOpen]="defaultOpen()"
      >
        <qt-scenarios-manager
          [mutator]="m"
          scopeLabel="group"
          [shelf]="shelf()"
          emptyMessage="No scenarios yet. Create one and it'll be offered whenever a member of this group joins a new chat."
        />
      </qt-collapsible-card>
    }
  `,
})
export class GroupScenariosCard implements OnInit {
  readonly groupId = input.required<string>();
  /** v4 `useState(false)` — collapsed until opened. */
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);
  protected readonly mutator = signal<ScenarioMutator | null>(null);
  protected readonly shelf = computed<ScenarioShelf>(() => ({
    kind: 'group',
    groupId: this.groupId(),
  }));

  ngOnInit(): void {
    const m = groupScenarioMutator(this.core, this.groupId());
    this.mutator.set(m);
    void m.refresh();
  }
}
