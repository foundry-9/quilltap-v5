import { ChangeDetectionStrategy, Component, inject, input, output } from '@angular/core';

import { Icon } from '../ui/icon';
import { QuickHideService } from './quick-hide.service';

/**
 * The quick-hide toggle surface (v4
 * `components/dashboard/nav-user-menu-quick-hide.tsx`, 118 lines): one eye
 * button per flagged tag, then the two content filters.
 *
 * Self-contained over {@link QuickHideService} so it can be dropped anywhere.
 * Work order P4.9d §2b: the unifier mounts this at lane P4.9c's marker inside
 * `qt-user-menu`, ABOVE the Profile/About entries (v4 `sidebar-footer.tsx:302`
 * sits above the ProfileMenu at `:309`). The selector is pinned by that
 * contract — do not rename it.
 *
 * The salon-list header's own "Show Autonomous Rooms" button is the SAME value
 * (both bind the service), a placement divergence recorded in
 * `screens/salon/autonomous-visibility.ts` that deliberately stands.
 *
 * v4's `qt-navbar-dropdown-item` rules are reproduced as component-scoped
 * styles rather than added to the shared stylesheet, so this component carries
 * its own look wherever it is mounted.
 */
@Component({
  selector: 'qt-quick-hide-menu-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  styles: `
    /* v4 app/styles/qt-components/_layout.css:192-206, transcribed. */
    .qt-navbar-dropdown-item {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 0.5rem;
      width: 100%;
      padding: 0.5rem 0.75rem;
      border-radius: 0.375rem;
      font-size: 0.875rem;
      line-height: 1.25rem;
      cursor: pointer;
      transition:
        background 150ms ease,
        color 150ms ease;
    }

    .qt-navbar-dropdown-item:hover {
      background: var(--qt-navbar-link-hover-bg);
    }

    .qt-navbar-dropdown-item-active {
      background: color-mix(in srgb, var(--color-primary) 12%, transparent);
      color: var(--color-primary);
    }
  `,
  template: `
    @if (quickHide.loading()) {
      <div class="p-3 qt-text-small">Loading tags...</div>
    } @else {
      <div class="space-y-1">
        @if (quickHide.quickHideTags().length > 0) {
          <div class="qt-navbar-dropdown-section space-y-1">
            <div class="qt-navbar-dropdown-label">Quick Hide Tags</div>
            @for (tag of quickHide.quickHideTags(); track tag.id) {
              <button
                type="button"
                class="qt-navbar-dropdown-item"
                [class.qt-navbar-dropdown-item-active]="quickHide.hiddenTagIds().has(tag.id)"
                [attr.aria-pressed]="quickHide.hiddenTagIds().has(tag.id)"
                (click)="onToggleTag(tag.id)"
              >
                <span class="qt-tag-badge qt-tag-badge-sm">{{ tag.name }}</span>
                <qt-icon
                  [name]="quickHide.hiddenTagIds().has(tag.id) ? 'eye-off' : 'eye'"
                  class="w-4 h-4 flex-shrink-0"
                />
              </button>
            }
          </div>
        }

        <div class="qt-navbar-dropdown-section space-y-1">
          <div class="qt-navbar-dropdown-label">Content Filters</div>
          <button
            type="button"
            class="qt-navbar-dropdown-item"
            [class.qt-navbar-dropdown-item-active]="quickHide.hideDangerousChats()"
            [attr.aria-pressed]="quickHide.hideDangerousChats()"
            (click)="onToggleDangerous()"
          >
            <span class="text-sm">Dangerous Chats</span>
            <qt-icon
              [name]="quickHide.hideDangerousChats() ? 'eye-off' : 'eye'"
              class="w-4 h-4 flex-shrink-0"
            />
          </button>

          <button
            type="button"
            class="qt-navbar-dropdown-item"
            [class.qt-navbar-dropdown-item-active]="quickHide.includeAutonomousRooms()"
            [attr.aria-pressed]="quickHide.includeAutonomousRooms()"
            title="Show autonomous character-to-character rooms in the Salon chat list"
            (click)="onToggleAutonomous()"
          >
            <span class="text-sm">Show Autonomous Rooms</span>
            <!-- INVERTED polarity vs the two above (v4 :105): this is an
                 INCLUDE toggle, so an open eye means "shown". -->
            <qt-icon
              [name]="quickHide.includeAutonomousRooms() ? 'eye' : 'eye-off'"
              class="w-4 h-4 flex-shrink-0"
            />
          </button>
        </div>
      </div>
    }
  `,
})
export class QuickHideMenuSection {
  protected readonly quickHide = inject(QuickHideService);

  /** v4 `onVisibilityChanged` (`:18`) — fired after any toggle. */
  readonly visibilityChanged = output<void>();

  protected onToggleTag(tagId: string): void {
    this.quickHide.toggleTag(tagId);
    this.visibilityChanged.emit();
  }

  protected onToggleDangerous(): void {
    this.quickHide.toggleHideDangerousChats();
    this.visibilityChanged.emit();
  }

  protected onToggleAutonomous(): void {
    this.quickHide.toggleIncludeAutonomousRooms();
    this.visibilityChanged.emit();
  }
}

/**
 * The menu-entry badge (v4 `QuickHideIcon`, `nav-user-menu-quick-hide.tsx:116`):
 * an open eye when nothing is hidden, a struck-through eye when something is.
 * Lane P4.9c's user menu consumes this alongside
 * `QuickHideService.hasAnyHidden` / `hasQuickHideFeatures`
 * (v4 `sidebar-footer.tsx:144-145`).
 */
@Component({
  selector: 'qt-quick-hide-icon',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `<qt-icon [name]="hasHidden() ? 'eye-off' : 'eye'" />`,
})
export class QuickHideIcon {
  readonly hasHidden = input.required<boolean>();
}
