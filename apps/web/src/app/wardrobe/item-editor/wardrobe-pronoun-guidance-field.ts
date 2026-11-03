/**
 * The item editor's Pronoun Guidance field — a port of v4
 * `components/wardrobe/wardrobe-item-editor/WardrobePronounGuidanceField.tsx`
 * (`eb90d9ed8`): one checkbox per standard pronoun set from Aurora's picker.
 * None ticked means the piece is offered to everyone; ticking some offers it
 * only to characters with those pronouns when a list is pronoun-aware. It
 * never stops anyone wearing it by name.
 *
 * @module wardrobe/item-editor/wardrobe-pronoun-guidance-field
 */

import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { STANDARD_PRONOUNS } from '../../characters/pronoun-presets';
import type { PronounGuidance } from '../../core/core-contract';
import { normalizePronounGuidance } from '../pronoun-guidance';

@Component({
  selector: 'qt-wardrobe-pronoun-guidance-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <fieldset>
      <legend class="qt-label mb-1">Pronoun Guidance</legend>
      <div class="flex flex-wrap gap-x-4 gap-y-1">
        @for (preset of presets; track preset.value.subject) {
          <label class="flex items-center gap-2 text-sm text-foreground">
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="value().includes(preset.value.subject)"
              (change)="toggle(preset.value.subject, $any($event.target).checked)"
            />
            {{ preset.label }}
          </label>
        }
      </div>
      <p class="mt-1 text-xs qt-text-small">
        For whom is this piece cut? Tick the pronouns of the characters it is meant for, and the
        wardrobe will tactfully keep it out of everyone else's offerings --- no gentleman need be
        handed a ball gown he never asked for. Leave every box empty and it is offered to all.
        Anyone may still wear it if they ask for it by name.
      </p>
    </fieldset>
  `,
})
export class WardrobePronounGuidanceField {
  readonly value = input.required<PronounGuidance[]>();
  readonly valueChange = output<PronounGuidance[]>();

  protected readonly presets = STANDARD_PRONOUNS;

  protected toggle(subject: PronounGuidance, on: boolean): void {
    const value = this.value();
    const next = on ? [...value, subject] : value.filter((s) => s !== subject);
    this.valueChange.emit(normalizePronounGuidance(next));
  }
}
