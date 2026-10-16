import {
  ChangeDetectionStrategy,
  Component,
  booleanAttribute,
  computed,
  input,
  output,
} from '@angular/core';

/**
 * "What about the components?" — the all-or-nothing choice offered when an
 * outfit is moved or copied, whether to another wardrobe
 * (`WardrobeTransferDialog`) or to a folder (`WardrobeFolderPickerDialog`).
 * A port of v4 `components/wardrobe/WardrobeComponentModeFieldset.tsx`
 * (`bbbcd2388`, which factored the transfer dialog's radio group out so
 * folder filing could share it).
 *
 * A move offers move / copy / leave; a copy offers copy / leave (it cannot
 * move the pieces its original still wears). {@link defaultComponentMode} is
 * the starting choice: an outfit that arrives without its pieces is rarely
 * what anyone meant.
 */

/** What travels with a composite outfit: its components, or nothing. */
export type ComponentMode = 'move' | 'copy' | 'none';

/** The starting choice for a move or a copy. */
export function defaultComponentMode(mode: 'move' | 'copy'): ComponentMode {
  return mode === 'move' ? 'move' : 'copy';
}

const MOVE_OPTIONS: ReadonlyArray<{ value: ComponentMode; label: string }> = [
  { value: 'move', label: 'Move the components along with it' },
  { value: 'copy', label: 'Copy the components (originals stay behind)' },
  { value: 'none', label: 'Leave the components behind' },
];

const COPY_OPTIONS: ReadonlyArray<{ value: ComponentMode; label: string }> = [
  { value: 'copy', label: 'Copy the components along with it' },
  { value: 'none', label: 'Copy the outfit alone' },
];

const WARDROBE_FOOTNOTE =
  'All or nothing — the choice covers every component, nested pieces included. Only components living in the same wardrobe travel; pieces from a shared tier stay put. The transferred outfit is rewired to point at whichever components arrive with it.';

const FOLDER_FOOTNOTE =
  'All or nothing — the choice covers every component, nested pieces included. Only components filed in this wardrobe travel; pieces borrowed from a shared tier stay put. Copied components take a "(copy)" title, and the outfit is rewired to point at them.';

@Component({
  selector: 'qt-wardrobe-component-mode-fieldset',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <fieldset>
      <legend class="text-sm qt-text-secondary mb-1">{{ legend() }}</legend>
      <div class="space-y-1">
        @for (option of options(); track option.value) {
          <label class="flex items-center gap-2 cursor-pointer text-sm">
            <input
              type="radio"
              [attr.name]="name()"
              class="qt-radio"
              [checked]="value() === option.value"
              (change)="valueChange.emit(option.value)"
              [disabled]="disabled()"
            />
            <span>{{ option.label }}</span>
          </label>
        }
      </div>
      <p class="qt-text-xs qt-text-secondary mt-1">{{ footnote() }}</p>
    </fieldset>
  `,
})
export class WardrobeComponentModeFieldset {
  readonly mode = input.required<'move' | 'copy'>();
  readonly value = input.required<ComponentMode>();
  /** The legend — what is being carried, e.g. "This outfit bundles 3 components". */
  readonly legend = input.required<string>();
  /** Where the components go, for the footnote: another wardrobe, or a folder. */
  readonly destination = input.required<'wardrobe' | 'folder'>();
  /** Distinguishes the radio group when two dialogs are mounted at once. */
  readonly name = input.required<string>();
  readonly disabled = input(false, { transform: booleanAttribute });
  readonly valueChange = output<ComponentMode>();

  protected readonly options = computed(() =>
    this.mode() === 'move' ? MOVE_OPTIONS : COPY_OPTIONS,
  );
  protected readonly footnote = computed(() =>
    this.destination() === 'wardrobe' ? WARDROBE_FOOTNOTE : FOLDER_FOOTNOTE,
  );
}
