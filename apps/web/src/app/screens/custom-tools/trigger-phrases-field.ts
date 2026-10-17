import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
  linkedSignal,
  output,
  signal,
} from '@angular/core';

import {
  MAX_TRIGGERS,
  MAX_TRIGGER_PHRASE_LENGTH,
  MIN_TRIGGER_PHRASE_LENGTH,
} from '../../pascal/custom-tool-types';
import type { DraftIssue } from '../../pascal/tool-draft';

/**
 * v4's hint under the field (`TriggerPhrasesField.tsx:96-100`), its three
 * numbers the constants. A TS constant, not template text: the en dash and em
 * dash are byte-pinned by the spec, and v4's `{' '}` before the cap renders as
 * ONE space.
 */
export const TRIGGER_PHRASES_HINT =
  'Said by anyone in the chat, these words run the tool with its defaults — only when the chat setting is on. ' +
  `Exact words, any case. ${MIN_TRIGGER_PHRASE_LENGTH}–${MAX_TRIGGER_PHRASE_LENGTH} characters each, up to ${MAX_TRIGGERS}.`;

/**
 * TriggerPhrasesField — the Workbench's editor for a tool's `triggers` (v4
 * `components/custom-tools/TriggerPhrasesField.tsx`, `362426c8f`, pinned at
 * `25b8fab6a`): a chip list of phrases that, said by anyone in a chat, run the
 * tool with its defaults (only while the chat setting is on). Add with Enter or
 * the button, remove with the chip's ×. Rule violations arrive as
 * `validateDraft` issues; the self-echo warnings render on the messages that
 * carry them, not here.
 *
 * The add path reads the INPUT ELEMENT at call time (`el.value.trim()`) rather
 * than trusting `pending` — the signal the `(input)` handler sets drives
 * `canAdd` and the placeholder, but a value set in the same tick as the Enter
 * may not have settled. Enter `preventDefault()`s first: the field sits inside
 * the Workbench form, whose Enter would otherwise act (v4 `:82-87`).
 */
@Component({
  selector: 'qt-trigger-phrases-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div>
      <label for="wb-trigger-phrase" class="qt-label">Trigger phrases</label>
      @if (triggers().length > 0) {
        <ul class="flex flex-wrap gap-2 mb-2" aria-label="Trigger phrases">
          @for (phrase of triggers(); track $index + '-' + phrase; let index = $index) {
            <li
              class="qt-badge-secondary inline-flex items-center gap-1"
              [class.qt-text-destructive]="errorAt(index) !== null"
              [attr.title]="errorAt(index)"
            >
              <span>{{ phrase }}</span>
              <button
                type="button"
                class="qt-button qt-button-ghost qt-button-sm px-1"
                (click)="remove(index)"
                [disabled]="disabled()"
                [attr.aria-label]="'Remove trigger phrase ' + phrase"
              >
                ×
              </button>
            </li>
          }
        </ul>
      }
      <div class="flex gap-2">
        <input
          #phraseInput
          id="wb-trigger-phrase"
          type="text"
          [value]="pending()"
          [attr.maxlength]="MAX_TRIGGER_PHRASE_LENGTH"
          (input)="pending.set(phraseInput.value)"
          (keydown.enter)="onEnter($event, phraseInput)"
          [placeholder]="atCap() ? 'At most ' + MAX_TRIGGERS + ' phrases' : 'abracadabra'"
          [disabled]="disabled() || atCap()"
          class="qt-input w-full"
        />
        <button
          type="button"
          class="qt-button qt-button-secondary qt-button-sm"
          (click)="add(phraseInput)"
          [disabled]="!canAdd()"
        >
          Add
        </button>
      </div>
      <p class="qt-hint">{{ HINT }}</p>
      @for (message of errorLines(); track $index) {
        <p class="text-xs qt-text-destructive mt-1">{{ message }}</p>
      }
    </div>
  `,
})
export class TriggerPhrasesField {
  readonly triggers = input.required<string[]>();
  readonly issues = input.required<DraftIssue[]>();
  readonly disabled = input(false);

  readonly triggersChange = output<string[]>();

  protected readonly MAX_TRIGGERS = MAX_TRIGGERS;
  protected readonly MAX_TRIGGER_PHRASE_LENGTH = MAX_TRIGGER_PHRASE_LENGTH;
  protected readonly HINT = TRIGGER_PHRASES_HINT;

  /** The input's text — drives `canAdd` and nothing the add path trusts. */
  protected readonly pending = signal('');

  /**
   * The list as THIS field last emitted it, re-linked whenever the parent's
   * `triggers` input moves. `add` / `remove` build from it rather than from the
   * input: zoneless change detection lands the parent's new list on the next
   * render, so a second Enter before that render would read the stale input
   * and overwrite the first add (measured by the e2e cap beat at the
   * `25b8fab6a` unification: 5–7 chips of 8 on a fast fill/Enter loop). v4's
   * React field re-renders synchronously on the event, so it never sees a
   * stale list.
   */
  private readonly current = linkedSignal(() => this.triggers());

  /** Errors anchored to the field — `validateDraft`'s `{ section: 'triggers' }`. */
  private readonly triggerErrors = computed(() =>
    this.issues().filter((i) => i.severity === 'error' && i.where.section === 'triggers'),
  );

  /** The first error naming no index — the list-level one (`at most 8 …`). */
  private readonly listError = computed(
    () =>
      this.triggerErrors().find((i) => i.where.section === 'triggers' && i.where.index === undefined)
        ?.message ?? null,
  );

  protected readonly atCap = computed(() => this.current().length >= MAX_TRIGGERS);

  protected readonly canAdd = computed(
    () =>
      !this.disabled() && !this.atCap() && this.pending().trim().length >= MIN_TRIGGER_PHRASE_LENGTH,
  );

  /** v4's `[listError, ...triggers.map(errorAt)]`, the nulls dropped. */
  protected readonly errorLines = computed(() =>
    [this.listError(), ...this.triggers().map((_, index) => this.errorAt(index))].filter(
      (message): message is string => message !== null,
    ),
  );

  /** The first error at `index`, else null — a chip's title AND its line. */
  protected errorAt(index: number): string | null {
    return (
      this.triggerErrors().find((i) => i.where.section === 'triggers' && i.where.index === index)
        ?.message ?? null
    );
  }

  protected onEnter(event: Event, el: HTMLInputElement): void {
    event.preventDefault();
    this.add(el);
  }

  /** Append the TRIMMED candidate (so a padded phrase never enters from here) and clear. */
  protected add(el: HTMLInputElement): void {
    const candidate = el.value.trim();
    const triggers = this.current();
    if (this.disabled() || triggers.length >= MAX_TRIGGERS) return;
    if (candidate.length < MIN_TRIGGER_PHRASE_LENGTH) return;
    const next = [...triggers, candidate];
    this.current.set(next);
    this.triggersChange.emit(next);
    el.value = '';
    this.pending.set('');
  }

  protected remove(index: number): void {
    const next = this.current().filter((_, i) => i !== index);
    this.current.set(next);
    this.triggersChange.emit(next);
  }
}
