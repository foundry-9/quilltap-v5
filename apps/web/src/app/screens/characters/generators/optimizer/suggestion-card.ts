import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import { Icon } from '../../../../ui/icon';
import { PromptFieldExample } from '../../../../ui/prompt-field-example';
import { PROMPT_FIELD_HINTS } from '../../../../ui/prompt-field-hints';
import type { OptimizerSuggestion, SuggestionDecision } from '../detail-generators.api';
import { FIELD_BADGE_CLASS, FIELD_HINT_KEYS, FIELD_LABELS } from './field-meta';

/**
 * v4 `components/characters/optimizer/components/SuggestionCard.tsx` — one
 * proposed refinement, reviewable as accept / reject / edit-and-accept.
 */
@Component({
  selector: 'qt-suggestion-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, PromptFieldExample],
  template: `
    <div
      [class]="
        'qt-card flex flex-col gap-4 ' +
        (editing() ? 'flex-1 ' : '') +
        (isAccepted() || isEdited()
          ? 'qt-border-success/40'
          : isRejected()
            ? 'qt-border-destructive/30 opacity-75'
            : '')
      "
    >
      <div class="flex flex-wrap items-center justify-between gap-2">
        <span [class]="fieldBadge() + ' text-xs'">{{ displayLabel() }}</span>
        <span class="qt-caption">Proposal {{ index() + 1 }} of {{ total() }}</span>
      </div>

      <div class="flex items-center gap-2">
        <div class="qt-bg-muted h-1.5 flex-1 overflow-hidden rounded-full">
          <div [class]="'h-full rounded-full transition-all ' + significanceBarClass()"></div>
        </div>
        <span class="qt-caption">{{ significanceLabel() }}</span>
      </div>

      @if (!editing() && voiceExample()) {
        <qt-prompt-field-example [example]="voiceExample()!" />
      }

      @if (!editing()) {
        <div class="flex flex-col gap-3">
          @if (safeCurrentValue()) {
            <div class="flex flex-col gap-1">
              <span class="qt-caption uppercase tracking-wider">Present Wording</span>
              <p class="qt-body-sm qt-bg-muted/50 border qt-border-default/50 rounded-md border p-3 leading-relaxed">
                {{ safeCurrentValue() }}
              </p>
            </div>
          } @else {
            <div class="flex flex-col gap-1">
              <span class="qt-caption uppercase tracking-wider">Present Wording</span>
              <p
                class="qt-body-sm qt-text-secondary qt-bg-muted/30 border-default rounded-md border border-dashed p-3 italic"
              >
                (This field is presently unoccupied — the suggestion would furnish it anew)
              </p>
            </div>
          }

          <div class="flex flex-col gap-1">
            <span class="qt-caption text-primary/70 uppercase tracking-wider">{{
              isEdited() ? 'Your Amended Wording' : 'Proposed Refinement'
            }}</span>
            <p
              [class]="
                'qt-body-sm rounded-md border p-3 leading-relaxed ' +
                (isEdited() ? 'qt-bg-primary/5 qt-border-primary/30' : 'qt-bg-success/5 qt-border-success/20')
              "
            >
              {{ isEdited() && editedValue() ? editedValue() : safeProposedValue() }}
            </p>
          </div>
        </div>
      } @else {
        <div class="flex min-h-0 flex-1 flex-col gap-2">
          <span class="qt-caption uppercase tracking-wider">Amend the Proposed Wording</span>
          <textarea
            class="qt-textarea min-h-[200px] flex-1 resize-y text-sm"
            [value]="draftValue()"
            (input)="draftValue.set($any($event.target).value)"
          ></textarea>
          <div class="flex justify-end gap-2">
            <button type="button" class="qt-button-ghost qt-button-sm" (click)="cancelEdit()">
              Abandon Edits
            </button>
            <button type="button" class="qt-button-primary qt-button-sm" (click)="acceptEdit()">
              Accept Amended Version
            </button>
          </div>
        </div>
      }

      @if (!editing()) {
        <div class="flex flex-col gap-1">
          <span class="qt-caption uppercase tracking-wider">Rationale</span>
          <p class="qt-body-sm qt-text-secondary leading-relaxed">{{ safeRationale() }}</p>
        </div>
      }

      @if (!editing() && suggestion().memoryExcerpts.length > 0) {
        <div class="qt-border-default border-t pt-3">
          <button
            type="button"
            class="qt-caption flex w-full items-center gap-1.5 text-left transition-colors hover:text-foreground"
            (click)="excerptsExpanded.set(!excerptsExpanded())"
          >
            <qt-icon
              name="chevron-right"
              [class]="'w-3.5 h-3.5 transition-transform ' + (excerptsExpanded() ? 'rotate-90' : '')"
            />
            <span
              >{{ excerptsExpanded() ? 'Conceal' : 'Consult' }} the memoirs ({{
                suggestion().memoryExcerpts.length
              }}
              {{ suggestion().memoryExcerpts.length === 1 ? 'excerpt' : 'excerpts' }})</span
            >
          </button>
          @if (excerptsExpanded()) {
            <div class="mt-2 flex flex-col gap-2">
              @for (excerpt of suggestion().memoryExcerpts; track $index) {
                <blockquote
                  class="qt-border-primary/40 qt-body-sm qt-text-secondary border-l-2 py-1 pl-3 leading-relaxed italic"
                >
                  &ldquo;{{ excerpt }}&rdquo;
                </blockquote>
              }
            </div>
          }
        </div>
      }

      @if (!editing()) {
        <div class="qt-border-default flex flex-wrap gap-2 border-t pt-1">
          @if (!isAccepted() && !isEdited()) {
            <button
              type="button"
              class="qt-button-success qt-button-sm min-w-[80px] flex-1"
              (click)="accept.emit()"
            >
              <qt-icon name="check" class="w-3.5 h-3.5" />
              Accept
            </button>
          }
          @if (isAccepted() || isEdited()) {
            <button
              type="button"
              class="qt-button-ghost qt-button-sm qt-text-success min-w-[80px] flex-1"
              (click)="accept.emit()"
            >
              <qt-icon name="check" class="w-3.5 h-3.5" />
              {{ isEdited() ? 'Accepted (Edited)' : 'Accepted' }}
            </button>
          }

          @if (!isRejected()) {
            <button
              type="button"
              class="qt-button-destructive qt-button-sm min-w-[80px] flex-1"
              (click)="reject.emit()"
            >
              <qt-icon name="close" class="w-3.5 h-3.5" />
              Reject
            </button>
          } @else {
            <button
              type="button"
              class="qt-button-ghost qt-button-sm qt-text-destructive min-w-[80px] flex-1"
              (click)="accept.emit()"
            >
              <qt-icon name="close" class="w-3.5 h-3.5" />
              Rejected
            </button>
          }

          <button
            type="button"
            class="qt-button-secondary qt-button-sm min-w-[80px] flex-1"
            (click)="startEdit()"
          >
            <qt-icon name="pencil" class="w-3.5 h-3.5" />
            Edit &amp; Accept
          </button>
        </div>
      }
    </div>
  `,
})
export class SuggestionCard {
  readonly suggestion = input.required<OptimizerSuggestion>();
  readonly decision = input<SuggestionDecision | undefined>(undefined);
  readonly editedValue = input<string | undefined>(undefined);
  readonly index = input.required<number>();
  readonly total = input.required<number>();

  readonly accept = output<void>();
  readonly reject = output<void>();
  readonly edit = output<string>();

  protected readonly excerptsExpanded = signal(false);
  protected readonly editing = signal(false);
  protected readonly draftValue = signal('');

  protected readonly safeProposedValue = computed(() => asText(this.suggestion().proposedValue));
  protected readonly safeCurrentValue = computed(() => asText(this.suggestion().currentValue));
  protected readonly safeRationale = computed(() => asText(this.suggestion().rationale));

  protected readonly fieldLabel = computed(
    () => FIELD_LABELS[this.suggestion().field] ?? this.suggestion().field,
  );
  protected readonly fieldBadge = computed(
    () => FIELD_BADGE_CLASS[this.suggestion().field] ?? 'qt-badge-secondary',
  );
  protected readonly voiceExample = computed(() => {
    const hintKey = FIELD_HINT_KEYS[this.suggestion().field];
    if (!hintKey) return undefined;
    const hint = PROMPT_FIELD_HINTS[hintKey] as { example?: string };
    return hint.example;
  });
  protected readonly displayLabel = computed(() => {
    const s = this.suggestion();
    const newItemName = s.name ?? s.title;
    if (s.subName) return `${this.fieldLabel()}: ${s.subName}`;
    if (newItemName) return `${this.fieldLabel()}: ${newItemName}`;
    return this.fieldLabel();
  });

  protected readonly isAccepted = computed(() => this.decision() === 'accepted');
  protected readonly isRejected = computed(() => this.decision() === 'rejected');
  protected readonly isEdited = computed(() => this.decision() === 'edited');

  protected readonly significanceLabel = computed(() => {
    const s = this.suggestion().significance;
    return s >= 0.6 ? 'High Significance' : s >= 0.3 ? 'Moderate Significance' : 'Minor Significance';
  });
  protected significanceBarClass(): string {
    const s = this.suggestion().significance;
    const color = s >= 0.6 ? 'qt-bg-destructive' : s >= 0.3 ? 'qt-bg-warning' : 'qt-bg-muted-foreground';
    const width = s >= 0.6 ? 'w-full' : s >= 0.3 ? 'w-2/3' : 'w-1/3';
    return `${width} ${color}`;
  }

  protected startEdit(): void {
    this.draftValue.set(this.editedValue() ?? this.safeProposedValue());
    this.editing.set(true);
  }
  protected cancelEdit(): void {
    this.editing.set(false);
    this.draftValue.set(this.editedValue() ?? this.safeProposedValue());
  }
  protected acceptEdit(): void {
    this.edit.emit(this.draftValue());
    this.editing.set(false);
  }
}

/** v4 `asText` (`SuggestionCard.tsx:24-33`) — belt-and-braces text coercion. */
function asText(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value === null || value === undefined) return '';
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}
