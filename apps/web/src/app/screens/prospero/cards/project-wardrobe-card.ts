import { ChangeDetectionStrategy, Component, OnInit, inject, input, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ProjectWardrobeManager } from '../wardrobe/project-wardrobe-manager';
import { projectWardrobeMutator, type WardrobeMutator } from '../wardrobe.api';

/**
 * The project Wardrobe card (v4 `app/prospero/[id]/components/WardrobeCard.tsx`):
 * the collapsible header lives here, the CRUD body is
 * {@link ProjectWardrobeManager}, fed by the project-scoped mutator. Items
 * created here are the project tier of the tri-tier wardrobe model.
 */
@Component({
  selector: 'qt-project-wardrobe-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, ProjectWardrobeManager],
  template: `
    @if (mutator(); as m) {
      <qt-collapsible-card
        [title]="'Wardrobe (' + m.items().length + ')'"
        description="Shared garments every character in this project can wear"
        icon="wardrobe"
        [defaultOpen]="defaultOpen()"
      >
        <qt-project-wardrobe-manager [mutator]="m" />
      </qt-collapsible-card>
    }
  `,
})
export class ProjectWardrobeCard implements OnInit {
  readonly projectId = input.required<string>();
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);
  protected readonly mutator = signal<WardrobeMutator | null>(null);

  ngOnInit(): void {
    const m = projectWardrobeMutator(this.core, this.projectId());
    this.mutator.set(m);
    void m.refresh();
  }
}
