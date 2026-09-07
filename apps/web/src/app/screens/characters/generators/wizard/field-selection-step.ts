import { ChangeDetectionStrategy, Component, inject } from '@angular/core';

import { Icon } from '../../../../ui/icon';
import type { GeneratableField } from '../edit-generators.api';
import { FIELD_DESCRIPTIONS, FIELD_LABELS } from './wizard-types';
import { WizardState } from './wizard-state';

/** v4's field-vantage display order (`types.ts`-adjacent, `FieldSelectionStep.tsx:37-51`). */
const ALL_FIELDS: GeneratableField[] = [
  'name',
  'title',
  'properties',
  'identity',
  'description',
  'manifesto',
  'personality',
  'scenarios',
  'exampleDialogues',
  'firstMessage',
  'systemPrompt',
  'physicalDescription',
  'wardrobeItems',
];

/**
 * Wizard step 3 — v4 `ai-wizard/steps/FieldSelectionStep.tsx` (195 lines):
 * the background textarea + the field checkbox catalogue + a live summary.
 */
@Component({
  selector: 'qt-wizard-field-selection-step',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="space-y-6">
      <div>
        <h3 class="qt-heading-4 text-foreground mb-2">Background Context</h3>
        <p class="text-sm qt-text-secondary mb-3">
          Provide context about the character's world, scenario, backstory, or any details that
          should inform the generation.
        </p>
        <textarea
          class="qt-textarea"
          rows="5"
          placeholder="Describe the world, scenario, backstory, or any context that should inform this character's creation. For example: 'A medieval fantasy world where magic is rare. This character is a blacksmith in a small village who secretly studies forbidden magic.'"
          [value]="wizard.backgroundText()"
          (input)="wizard.setBackgroundText($any($event.target).value)"
        ></textarea>
      </div>

      <div>
        <div class="flex items-center justify-between mb-3">
          <div>
            <h3 class="qt-heading-4 text-foreground">Fields to Generate</h3>
            <p class="text-sm qt-text-secondary">Select which fields you want the AI to generate.</p>
          </div>
          <div class="flex gap-2">
            <button
              type="button"
              class="text-sm text-primary hover:underline"
              (click)="wizard.selectAllFields()"
            >
              Select all
            </button>
            <span class="qt-text-secondary">|</span>
            <button
              type="button"
              class="text-sm text-primary hover:underline"
              (click)="wizard.clearAllFields()"
            >
              Clear all
            </button>
          </div>
        </div>

        <div class="space-y-2">
          @for (field of fields; track field) {
            <label
              [class]="
                'flex items-start gap-3 p-3 rounded-lg border transition-colors ' +
                (isDisabled(field)
                  ? 'qt-border-default qt-bg-muted/30 opacity-60 cursor-not-allowed'
                  : wizard.selectedFields().has(field)
                    ? 'qt-border-primary qt-bg-primary/5 cursor-pointer'
                    : 'qt-border-default hover:border-muted-foreground/50 cursor-pointer')
              "
            >
              <input
                type="checkbox"
                class="mt-1 qt-checkbox"
                [checked]="wizard.selectedFields().has(field)"
                [disabled]="isDisabled(field)"
                (change)="!isDisabled(field) && wizard.toggleField(field)"
              />
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-medium text-foreground">{{ fieldLabels[field] }}</span>
                  @if (statusText(field); as status) {
                    <span class="text-xs qt-text-secondary">{{ status }}</span>
                  }
                </div>
                <p class="text-sm qt-text-secondary mt-0.5">{{ fieldDescriptions[field] }}</p>
              </div>
            </label>
          }
        </div>
      </div>

      <div class="p-4 rounded-lg border qt-border-default qt-bg-muted/20">
        <h4 class="font-medium text-foreground mb-2">Generation Summary</h4>
        @if (wizard.selectedFields().size === 0) {
          <p class="text-sm qt-text-secondary">
            No fields selected. Please select at least one field to generate.
          </p>
        } @else {
          <div class="text-sm qt-text-secondary">
            <p>
              Will generate
              <span class="font-medium text-foreground">{{ wizard.selectedFields().size }}</span>
              field{{ wizard.selectedFields().size !== 1 ? 's' : '' }}:
            </p>
            <ul class="mt-2 space-y-1">
              @for (field of selectedList(); track field) {
                <li class="flex items-center gap-2">
                  <qt-icon name="check" class="w-4 h-4 text-primary flex-shrink-0" />
                  <span>{{ fieldLabels[field] }}</span>
                </li>
              }
            </ul>
          </div>
        }
      </div>
    </div>
  `,
})
export class WizardFieldSelectionStep {
  protected readonly wizard = inject(WizardState);
  protected readonly fields = ALL_FIELDS;
  protected readonly fieldLabels = FIELD_LABELS;
  protected readonly fieldDescriptions = FIELD_DESCRIPTIONS;

  private canGeneratePhysicalDescription(): boolean {
    return this.wizard.descriptionSource() !== 'skip';
  }

  protected isDisabled(field: GeneratableField): boolean {
    const available = this.wizard.availableFields().includes(field);
    if (field === 'physicalDescription') {
      return !this.canGeneratePhysicalDescription() || !available;
    }
    return !available;
  }

  protected statusText(field: GeneratableField): string {
    const currentData = this.wizard.currentData() as Record<string, unknown>;
    const fieldValue = currentData[field];
    const hasContent = Array.isArray(fieldValue)
      ? fieldValue.length > 0
      : typeof fieldValue === 'string' && !!fieldValue.trim();

    if (field === 'physicalDescription' && !this.canGeneratePhysicalDescription()) {
      return '(skipped in previous step)';
    }
    if (field === 'scenarios' && Array.isArray(fieldValue) && fieldValue.length > 0) {
      return `(${fieldValue.length} existing — will add more)`;
    }
    if (hasContent && field !== 'physicalDescription') {
      return '(has content)';
    }
    return '';
  }

  protected selectedList(): GeneratableField[] {
    return Array.from(this.wizard.selectedFields());
  }
}
