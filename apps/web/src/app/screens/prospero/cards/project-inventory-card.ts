import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { RealtimeService } from '../../../core/realtime.service';
import { InventoryPanel } from '../../../inventory/inventory-control-dialog';
import type { InventoryContainer } from '../../../inventory/inventory-container';
import { createInventoryListData } from '../../../inventory/inventory-list-data';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { onTabActivated } from '../../../workspace/workspace-contract';

/**
 * The project Inventory card — a port of v4 `app/prospero/[id]/components/
 * InventoryCard.tsx` (`4fbdad186`, `e27fb6b34`'s icon): the shared Inventory
 * browser on this project's store — its objects, kits and archetypes, carried
 * by (or instantiated for) every character in this project's chats — through
 * the pinned `qt-inventory-panel`.
 *
 * The title counts the project tier's own list (v4 `useInventoryList(container,
 * { includeArchived: false })`); it re-counts on the `inventory` realtime topic
 * and on returning to the tab (v4's shared `inventory.lists` cache, R-1).
 *
 * **Recorded divergences.** v4's own header button + `ChevronIcon` is v5's
 * `qt-collapsible-card` chrome (the wardrobe card's idiom); v4's per-card
 * expanded map (`useProjectCardState`'s `inventory` key) has no twin — v5's
 * project page opens every card on a first visit (`resolveFirstVisit`).
 */
@Component({
  selector: 'qt-project-inventory-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, InventoryPanel],
  template: `
    <qt-collapsible-card
      [title]="'Inventory (' + data.listItems().length + ')'"
      description="Objects, kits and archetypes every character in this project can carry"
      icon="inventory"
      [defaultOpen]="defaultOpen()"
    >
      <qt-inventory-panel [container]="container()" [projectId]="projectId()" />
    </qt-collapsible-card>
  `,
})
export class ProjectInventoryCard {
  readonly projectId = input.required<string>();
  readonly projectName = input<string | undefined>(undefined);
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);

  protected readonly container = computed<InventoryContainer>(() => ({ scope: 'project', id: this.projectId() }));
  protected readonly data = createInventoryListData(this.core, {
    container: () => this.container(),
    includeArchived: () => false,
  });

  constructor() {
    inject(RealtimeService).onTopic('inventory', () => void this.data.reload());
    onTabActivated(() => void this.data.reload());
  }
}
