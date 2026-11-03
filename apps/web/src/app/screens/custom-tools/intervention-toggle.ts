import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { InterventionFieldKey } from '../../pascal/custom-tool-types';
import type { ToolDraft } from '../../pascal/tool-draft';
import { Icon } from '../../ui/icon';

/**
 * InterventionToggle — the small raised hand beside a Workbench field's label
 * (v4 `components/custom-tools/InterventionToggle.tsx`, `839575d89`).
 *
 * Pressed means the field is shown to the operator in the hold dialog;
 * unpressed means it is kept out of sight. {@link ToolFieldInterventionToggle}
 * is the tool-level wrapper: it reads and writes `draft.intervention.fields[key]`,
 * and renders nothing while the master switch is off (the per-field values stay
 * in the draft regardless).
 */
@Component({
  selector: 'qt-intervention-toggle',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <button
      type="button"
      class="qt-intervene-toggle"
      [class.qt-intervene-toggle--off]="!shown()"
      [attr.aria-pressed]="shown()"
      [attr.aria-label]="label()"
      [title]="tooltip()"
      [disabled]="disabled()"
      (click)="shownChange.emit(!shown())"
    >
      <qt-icon name="hand-raised" class="w-3.5 h-3.5" />
    </button>
  `,
})
export class InterventionToggle {
  /** True when this field is shown in the hold dialog. */
  readonly shown = input.required<boolean>();
  /** Accessible name, e.g. "Show the roll in the hold dialog". */
  readonly label = input.required<string>();
  readonly disabled = input(false);
  readonly shownChange = output<boolean>();

  protected readonly tooltip = computed(() =>
    this.shown() ? `${this.label()} (shown)` : `${this.label()} (hidden)`,
  );
}

/** Whether a tool-level field is shown (absent key = shown). */
export function isToolFieldShown(draft: ToolDraft, key: InterventionFieldKey): boolean {
  return draft.intervention.fields[key] !== false;
}

/** A new draft with a tool-level field shown or hidden; showing deletes the key. */
export function withToolFieldShown(draft: ToolDraft, key: InterventionFieldKey, shown: boolean): ToolDraft {
  const fields = { ...draft.intervention.fields };
  if (shown) delete fields[key];
  else fields[key] = false;
  return { ...draft, intervention: { ...draft.intervention, fields } };
}

/** The tool-level hand: bound to `draft.intervention.fields[fieldKey]`; nothing while the master is off. */
@Component({
  selector: 'qt-tool-field-intervention-toggle',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [InterventionToggle],
  template: `
    @if (draft().intervention.enabled) {
      <qt-intervention-toggle
        [shown]="shown()"
        [label]="label()"
        [disabled]="disabled()"
        (shownChange)="draftChange.emit(withShown($event))"
      />
    }
  `,
})
export class ToolFieldInterventionToggle {
  readonly draft = input.required<ToolDraft>();
  readonly fieldKey = input.required<InterventionFieldKey>();
  readonly label = input.required<string>();
  readonly disabled = input(false);
  readonly draftChange = output<ToolDraft>();

  protected readonly shown = computed(() => isToolFieldShown(this.draft(), this.fieldKey()));

  protected withShown(next: boolean): ToolDraft {
    return withToolFieldShown(this.draft(), this.fieldKey(), next);
  }
}
