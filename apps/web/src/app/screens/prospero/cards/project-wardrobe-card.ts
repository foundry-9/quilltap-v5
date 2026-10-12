import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ToastService } from '../../../ui/toast.service';
import { WardrobeBrowser } from '../../../wardrobe/wardrobe-browser';
import type { WardrobeContainer } from '../../../wardrobe/wardrobe-container';
import { WardrobeInstructionsSection } from '../../../wardrobe/wardrobe-instructions-section';
import { createWardrobeItemActions } from '../../../wardrobe/wardrobe-item-actions';
import { WardrobeItemOverlays } from '../../../wardrobe/wardrobe-item-overlays';
import { createWardrobeListData } from '../../../wardrobe/wardrobe-list-data';

/**
 * The project Wardrobe card — a port of v4 `app/prospero/[id]/components/
 * WardrobeCard.tsx` at `4a38222e6`. The collapsible header lives here; the
 * body is the shared wardrobe browser on this project's container — the same
 * list, folders, filters, item editor, pictures, move/copy and dressing
 * instructions the Wardrobe dialog offers when it browses a project. Items
 * here are the project tier: wearable by every character in this project's
 * chats.
 *
 * v4 deleted `ProjectWardrobeManager` (and `useProjectWardrobe`) in
 * `9ecebf986` when the card became the shared browser; v5's twin and its
 * project-only mutator went with it (P4.D285).
 *
 * **Recorded divergences.** v4's own header button + `ChevronIcon` is v5's
 * `qt-collapsible-card` chrome (the standing card idiom); v4's
 * `showConfirmation` is `window.confirm`. The overlays sit outside the
 * collapsible body, as v4 mounts them outside its `expanded` gate.
 */
@Component({
  selector: 'qt-project-wardrobe-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, WardrobeBrowser, WardrobeInstructionsSection, WardrobeItemOverlays],
  template: `
    <qt-collapsible-card
      [title]="'Wardrobe (' + data.listItems().length + ')'"
      description="Shared garments every character in this project can wear"
      icon="wardrobe"
      [defaultOpen]="defaultOpen()"
    >
      <qt-wardrobe-instructions-section [container]="container()" />
      <qt-wardrobe-browser
        [container]="container()"
        [data]="data"
        [actions]="actions"
        [showArchived]="showArchived()"
        (showArchivedChange)="showArchived.set($event)"
        [scrollList]="false"
      />
    </qt-collapsible-card>
    <qt-wardrobe-item-overlays
      [container]="container()"
      [containerLabel]="projectName() || 'This project'"
      [data]="data"
      [actions]="actions"
    />
  `,
})
export class ProjectWardrobeCard {
  readonly projectId = input.required<string>();
  readonly projectName = input<string | undefined>(undefined);
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);

  protected readonly container = computed<WardrobeContainer>(() => ({
    scope: 'project',
    id: this.projectId(),
  }));
  /** "Show archived" is a different server read, not a client-side filter. */
  protected readonly showArchived = signal(false);
  protected readonly data = createWardrobeListData(this.core, {
    container: () => this.container(),
    includeArchived: () => this.showArchived(),
  });
  protected readonly actions = createWardrobeItemActions({
    core: this.core,
    toasts: inject(ToastService),
    container: () => this.container(),
    listItems: () => this.data.listItems(),
    confirm: (message) => window.confirm(message),
    reloadLists: () => this.data.reload(),
  });
}
