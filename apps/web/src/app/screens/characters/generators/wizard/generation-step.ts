import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';

import { Icon } from '../../../../ui/icon';
import type { GeneratableField, GeneratedCharacterData } from '../edit-generators.api';
import { FIELD_LABELS, normalizeGeneratedScenarios } from './wizard-types';
import { WizardState } from './wizard-state';

/**
 * Wizard step 4 — v4 `ai-wizard/steps/GenerationStep.tsx` (405 lines): the
 * ready/generating/complete three-way render, a per-field checklist during
 * generation, and an expandable review list once done.
 *
 * Deviation from v4: `physicalDescription.fullDescription` renders as plain
 * pre-wrapped text rather than through ReactMarkdown — v5 has no
 * chat-independent Markdown renderer to reuse here (`markdown-renderer.ts`
 * is the Salon message pipeline, not a generic component), and the review
 * pane is a preview surface, not the field's persisted home.
 */
@Component({
  selector: 'qt-wizard-generation-step',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    @if (!wizard.generating() && !wizard.generatedData()) {
      <div class="space-y-6">
        <div>
          <h3 class="qt-heading-4 text-foreground mb-2">Ready to Generate</h3>
          <p class="text-sm qt-text-secondary">
            Click the button below to start generating content for your character.
          </p>
        </div>

        <div class="p-4 rounded-lg border qt-border-default qt-bg-muted/20">
          <h4 class="font-medium text-foreground mb-2">Fields to Generate</h4>
          <ul class="space-y-1">
            @for (field of selectedList(); track field) {
              <li class="flex items-center gap-2 text-sm qt-text-secondary">
                <svg
                  class="w-4 h-4 qt-text-secondary flex-shrink-0"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <circle cx="12" cy="12" r="10" stroke-width="2" />
                </svg>
                <span>{{ fieldLabels[field] }}</span>
              </li>
            }
          </ul>
        </div>

        @if (wizard.error()) {
          <div class="qt-alert-error">{{ wizard.error() }}</div>
        }

        <button
          type="button"
          class="w-full qt-button-primary py-3"
          (click)="wizard.startGeneration()"
        >
          <span class="flex items-center justify-center gap-2">
            <qt-icon name="zap" class="w-5 h-5" />
            Generate Character Content
          </span>
        </button>
      </div>
    } @else if (wizard.generating()) {
      <div class="space-y-6">
        <div>
          <h3 class="qt-heading-4 text-foreground mb-2">Generating Content...</h3>
          <p class="text-sm qt-text-secondary">
            Please wait while the AI generates your character content.
          </p>
        </div>

        <div class="flex items-center justify-center py-8">
          <div class="flex flex-col items-center gap-4">
            <svg class="w-12 h-12 text-primary animate-spin" fill="none" viewBox="0 0 24 24">
              <circle
                class="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                stroke-width="4"
              />
              <path
                class="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
              />
            </svg>
            <p class="qt-text-secondary">
              {{
                wizard.generationProgress().currentField
                  ? 'Generating ' + fieldLabels[wizard.generationProgress().currentField!] + '...'
                  : 'Starting generation...'
              }}
            </p>
          </div>
        </div>

        <div class="space-y-2">
          @for (field of selectedList(); track field) {
            <div [class]="'rounded-lg border transition-all ' + fieldRowClass(field)">
              <div class="flex items-center gap-3 p-3">
                @if (isCompleted(field)) {
                  <qt-icon name="check" class="w-5 h-5 qt-text-success flex-shrink-0" />
                } @else if (hasError(field)) {
                  <qt-icon name="close" class="w-5 h-5 qt-text-destructive flex-shrink-0" />
                } @else if (isCurrent(field)) {
                  <svg
                    class="w-5 h-5 text-primary animate-spin flex-shrink-0"
                    fill="none"
                    viewBox="0 0 24 24"
                  >
                    <circle
                      class="opacity-25"
                      cx="12"
                      cy="12"
                      r="10"
                      stroke="currentColor"
                      stroke-width="4"
                    />
                    <path
                      class="opacity-75"
                      fill="currentColor"
                      d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                    />
                  </svg>
                } @else {
                  <svg
                    class="w-5 h-5 qt-text-secondary flex-shrink-0"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <circle cx="12" cy="12" r="10" stroke-width="2" />
                  </svg>
                }
                <div class="flex-1 min-w-0">
                  <span [class]="'font-medium ' + (isCompleted(field) ? 'text-foreground' : 'qt-text-secondary')">
                    {{ fieldLabels[field] }}
                  </span>
                  @if (isCompleted(field) && snippet(field)) {
                    <p class="text-xs qt-text-secondary mt-1 truncate">{{ snippet(field) }}</p>
                  }
                  @if (hasError(field)) {
                    <p class="text-xs qt-text-destructive mt-1">{{ fieldError(field) }}</p>
                  }
                </div>
              </div>
            </div>
          }
        </div>
      </div>
    } @else {
      <div class="space-y-6">
        <div>
          <h3 class="qt-heading-4 text-foreground mb-2 flex items-center gap-2">
            <qt-icon name="check-circle" class="w-6 h-6 qt-text-success" />
            Generation Complete
          </h3>
          <p class="text-sm qt-text-secondary">
            Review the generated content below. Click on a field to expand it.
          </p>
        </div>

        @if (wizard.error()) {
          <div class="qt-alert-error">{{ wizard.error() }}</div>
        }

        <div class="space-y-2">
          @for (field of selectedList(); track field) {
            <div [class]="'rounded-lg border transition-colors ' + reviewRowClass(field)">
              <button
                type="button"
                class="w-full flex items-center justify-between p-3 text-left"
                (click)="toggleExpanded(field)"
              >
                <div class="flex items-center gap-3">
                  @if (fieldContent(field)) {
                    <qt-icon name="check" class="w-5 h-5 qt-text-success" />
                  } @else if (hasError(field)) {
                    <qt-icon name="close" class="w-5 h-5 qt-text-destructive" />
                  } @else {
                    <svg
                      class="w-5 h-5 qt-text-secondary"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <circle cx="12" cy="12" r="10" stroke-width="2" />
                    </svg>
                  }
                  <span class="font-medium text-foreground">{{ fieldLabels[field] }}</span>
                </div>
                <qt-icon
                  name="chevron-down"
                  [class]="
                    'w-5 h-5 qt-text-secondary transition-transform ' +
                    (expandedField() === field ? 'rotate-180' : '')
                  "
                />
              </button>

              @if (expandedField() === field) {
                <div class="px-3 pb-3 border-t qt-border-default/50 pt-3">
                  @if (hasError(field)) {
                    <p class="text-sm qt-text-destructive">{{ fieldError(field) }}</p>
                  } @else if (fieldContent(field); as content) {
                    <div class="mt-2 text-sm qt-text-secondary whitespace-pre-wrap">{{ content }}</div>
                  } @else {
                    <p class="text-sm qt-text-secondary">No content generated</p>
                  }
                </div>
              }
            </div>
          }
        </div>

        <button type="button" class="w-full qt-button-primary py-3" (click)="wizard.applyGenerated()">
          <span class="flex items-center justify-center gap-2">
            <qt-icon name="check" class="w-5 h-5" />
            Apply to Character
          </span>
        </button>
      </div>
    }
  `,
})
export class WizardGenerationStep {
  protected readonly wizard = inject(WizardState);
  protected readonly fieldLabels = FIELD_LABELS;
  protected readonly expandedField = signal<GeneratableField | null>(null);

  protected selectedList(): GeneratableField[] {
    return Array.from(this.wizard.selectedFields());
  }

  protected isCompleted(field: GeneratableField): boolean {
    return this.wizard.generationProgress().completedFields.includes(field);
  }

  protected isCurrent(field: GeneratableField): boolean {
    return this.wizard.generationProgress().currentField === field;
  }

  protected hasError(field: GeneratableField): boolean {
    return !!this.wizard.generationProgress().errors[field];
  }

  protected fieldError(field: GeneratableField): string {
    return this.wizard.generationProgress().errors[field] ?? '';
  }

  protected snippet(field: GeneratableField): string {
    return this.wizard.generationProgress().snippets[field] ?? '';
  }

  protected fieldRowClass(field: GeneratableField): string {
    if (this.isCompleted(field)) return 'qt-border-success/50 qt-bg-success/10';
    if (this.hasError(field)) return 'qt-border-destructive/50 qt-bg-destructive/10';
    if (this.isCurrent(field)) return 'qt-border-primary qt-bg-primary/5';
    return 'qt-border-default qt-bg-muted/20';
  }

  protected reviewRowClass(field: GeneratableField): string {
    if (this.hasError(field)) return 'qt-border-destructive/50 qt-bg-destructive/10';
    if (this.fieldContent(field)) return 'qt-border-success/50 qt-bg-success/10';
    return 'qt-border-default qt-bg-muted/20';
  }

  protected toggleExpanded(field: GeneratableField): void {
    this.expandedField.set(this.expandedField() === field ? null : field);
  }

  /** v4 `getFieldContent` (`GenerationStep.tsx:44-93`). */
  protected fieldContent(field: GeneratableField): string | null {
    const data = this.wizard.generatedData();
    if (!data) return null;

    if (field === 'physicalDescription') {
      return data.physicalDescription
        ? data.physicalDescription.fullDescription || 'Physical description generated'
        : null;
    }
    if (field === 'scenarios') {
      const scenarios = normalizeGeneratedScenarios(data.scenarios);
      return scenarios.length > 0
        ? scenarios.map((s) => `${s.title}\n${s.content}`).join('\n\n')
        : null;
    }
    if (field === 'wardrobeItems') {
      const items = data.wardrobeItems;
      if (!items || items.length === 0) return null;
      return items
        .map((item) => {
          const outfit = item.components && item.components.length > 0 ? ' (outfit)' : '';
          const isDefault = item.isDefault ? ' [default]' : '';
          return `${item.title}${outfit}${isDefault} — ${item.types.join(', ')}`;
        })
        .join('\n');
    }
    if (field === 'properties') {
      const props = data.properties;
      if (!props) return null;
      const parts: string[] = [];
      if (props.pronouns) {
        parts.push(`Pronouns: ${props.pronouns.subject}/${props.pronouns.object}/${props.pronouns.possessive}`);
      }
      if (props.aliases.length > 0) {
        parts.push(`Aliases: ${props.aliases.join(', ')}`);
      }
      return parts.length > 0 ? parts.join('\n') : 'No pronouns or aliases derivable';
    }

    const value = (data as unknown as Record<string, unknown>)[field];
    return typeof value === 'string' ? value || null : null;
  }
}
