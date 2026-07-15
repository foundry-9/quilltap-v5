import { ChangeDetectionStrategy, Component, input } from '@angular/core';

/**
 * The title/subtitle card shell v4's Chat-tab cards render INSIDE their
 * `CollapsibleCard` (v4 `components/ui/SettingsCard.tsx` — the plain
 * title + subtitle + children arm; its badges/metadata/actions/delete-popover
 * arms belong to the profile cards and are ported where those live).
 *
 * v4's Chat tab really does show the heading twice on these cards — the
 * collapsible's own title and then the SettingsCard's, which are near-identical
 * ("Automation" / "Automation", "Thinking / Reasoning" / "Thinking /
 * Reasoning"). That is v4's look, so it is this port's look.
 *
 * Lives here rather than in `ui/` because `ui/**` belongs to another lane; if a
 * second family ever needs it, lifting it is a one-file move.
 */
@Component({
  selector: 'qt-settings-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-border rounded-lg p-4 qt-bg-card qt-shadow-sm space-y-3">
      <div class="flex items-start justify-between">
        <div class="flex-1 min-w-0">
          <h3 class="qt-card-title truncate">{{ title() }}</h3>
          @if (subtitle()) {
            <p class="qt-card-description mt-1">{{ subtitle() }}</p>
          }
        </div>
      </div>
      <ng-content />
    </div>
  `,
})
export class SettingsCard {
  readonly title = input.required<string>();
  readonly subtitle = input<string>('');
}
