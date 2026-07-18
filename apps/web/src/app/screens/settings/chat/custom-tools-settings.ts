import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { Router } from '@angular/router';

import { Icon } from '../../../ui/icon';
import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';

/**
 * The Custom Tools card (v4 `components/settings/chat-settings/
 * CustomToolsSettings.tsx`): whether Pascal offers your custom pseudo-tools to
 * models (and posts the composer button). Writes the `customTools` scalar; v4's
 * default when unset is **true** (`DEFAULT_CUSTOM_TOOLS`).
 *
 * The "Open Pascal's Workbench" button lands with P4.6bb. It stays live even
 * while the toggle is OFF, and deliberately so: authoring a contrivance while
 * the table isn't dealing them is perfectly legitimate (v4's own why-comment).
 */
@Component({
  selector: 'qt-custom-tools-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, Icon],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading custom-tool settings…</p>
    } @else {
      <div>
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <label class="qt-settings-toggle-row">
          <input
            type="checkbox"
            class="qt-checkbox mt-1"
            [checked]="enabled()"
            [disabled]="saving()"
            (change)="onChange($any($event.target).checked)"
          />
          <div class="flex-1">
            <div class="qt-settings-section-heading">Custom tools</div>
            <div class="qt-text-small mt-1">
              Permits Pascal to lay your own contrivances upon the baize, where any model at the
              table may reach for one of its own accord, and posts the little button in the
              composer&rsquo;s gutter for when you&rsquo;d rather call the play yourself. Unchecked,
              the croupier sweeps the lot out of sight: no model is offered them, the gutter button
              retires, and your contraptions wait &mdash; undisturbed and entirely intact &mdash;
              until you see fit to invite them back.
            </div>
            <!--
              A BUTTON that navigates, not a link — v4's own choice, and the
              right one here: this control sits INSIDE the toggle's <label>, so
              anything clickable would otherwise flip the checkbox on its way
              past. preventDefault is what stops that (v4 does the same).
            -->
            <button
              type="button"
              class="qt-button qt-button-secondary qt-button-sm mt-2"
              (click)="openWorkbench($event)"
            >
              <qt-icon name="wrench" class="w-3.5 h-3.5" />
              Open Pascal&rsquo;s Workbench
            </button>
          </div>
        </label>
      </div>
    }
  `,
})
export class CustomToolsSettings extends ChatSettingsCard {
  private readonly router = inject(Router);

  /**
   * Authoring while the toggle is off is legitimate — the link stays live
   * either way (v4's why-comment, carried).
   */
  protected openWorkbench(event: Event): void {
    event.preventDefault();
    void this.router.navigate(['/custom-tools']);
  }

  /** v4 `settings.customTools ?? DEFAULT_CUSTOM_TOOLS` (true). */
  protected readonly enabled = computed(
    () => (this.settings()?.['customTools'] as boolean | undefined) ?? true,
  );

  protected async onChange(value: boolean): Promise<void> {
    await this.save({ customTools: value }, 'Failed to update custom tools setting');
  }
}
