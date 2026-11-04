import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import type { RoleplayTemplateDto } from '../../../core/core-contract';
import { MarkdownField } from '../../../editor/markdown-field';
import { ErrorAlert } from '../../../ui/error-alert';
import { FormActions } from '../../../ui/form-actions';
import { Modal } from '../../../ui/modal';
import { PROMPT_FIELD_HINTS } from '../../../ui/prompt-field-hints';
import { PromptFieldLabel } from '../../../ui/prompt-field-label';
import { TemplateAddOnsEditor } from './template-add-ons-editor';
import { TemplateDelimiterEditor } from './template-delimiter-editor';
import {
  EMPTY_ADD_ONS,
  STYLE_OPTIONS,
  emptyTemplateForm,
  formToTemplateBody,
  isTemplateFormValid,
  templateToForm,
  type DelimiterFormEntry,
  type FullAddOns,
  type TemplateFormData,
} from './template-form';

/** Per-instance id seed for the label ↔ control pairs (v4 uses `useId()`). */
let templateFormModalCounter = 0;
import { createRoleplayTemplate, updateRoleplayTemplate } from './templates.api';

/**
 * The create / edit template modal (v4 `roleplay-templates/index.tsx` modal):
 * Name, Description, LLM Prompt, the Narration Delimiters mode + inputs, and the
 * full Formatting Delimiters editor. On save it PUTs (edit) or POSTs (create) —
 * omitting `renderingPatterns` so the server regenerates. Duplicate-name (409)
 * and built-in-guard (403) messages surface verbatim in the alert.
 *
 * The systemPrompt is a `qt-markdown-field` (v4 `:312`, its `minHeight="14rem"`
 * and `remountKey={editingTemplate?.id ?? 'new'}`) under the shared
 * `qt-prompt-field-label` (v4 `a6870c5a`: `label="LLM Prompt"` `required` over
 * the roleplayTemplatePrompt hint, this surface's placeholder sentence
 * appended). The "Draft formatting instructions" helper button remains a named
 * deferral (the `generateFormattingPromptHint` transcription is out of scope),
 * so nothing is projected into the label row's actions slot here — v4 puts
 * that button there.
 *
 * P4.D352 (v4 `778b97b2b` at 236c04e35): a third narration choice, `None —
 * plain prose` (stored as `null`), which also turns on the new `Unmarked Text`
 * section (a style class + flourishes for whatever no delimiter claims) and
 * the Markdown-asterisks switch. The save body and the Save predicate come
 * from `template-form.ts` (v4's `:288-291` payload and `:868` predicate); the
 * labels pair with their controls through per-instance ids (v4 `eed72d60c`).
 */
@Component({
  selector: 'qt-template-form-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    Modal,
    FormActions,
    ErrorAlert,
    TemplateDelimiterEditor,
    MarkdownField,
    PromptFieldLabel,
    TemplateAddOnsEditor,
  ],
  template: `
    <qt-modal [title]="title()" maxWidth="3xl" (close)="close.emit()">
      @if (error()) {
        <qt-error-alert [message]="error()!" class="mb-4" />
      }

      <div class="space-y-4">
        <div>
          <label class="block qt-text-label mb-1" [for]="id + '-name'">
            Name <span class="qt-text-destructive">*</span>
          </label>
          <input
            type="text"
            class="qt-input"
            [id]="id + '-name'"
            maxlength="100"
            placeholder="My Custom RP Style"
            [value]="name()"
            (input)="name.set($any($event.target).value)"
          />
          <p class="qt-text-xs qt-text-muted mt-1">{{ name().length }}/100 characters</p>
        </div>

        <div>
          <label class="block qt-text-label mb-1" [for]="id + '-description'">Description</label>
          <input
            type="text"
            class="qt-input"
            [id]="id + '-description'"
            maxlength="500"
            placeholder="A brief description of what this template does"
            [value]="description()"
            (input)="description.set($any($event.target).value)"
          />
          <p class="qt-text-xs qt-text-muted mt-1">{{ description().length }}/500 characters</p>
        </div>

        <div>
          <qt-prompt-field-label
            [hint]="hints.roleplayTemplatePrompt"
            label="LLM Prompt"
            required
            [helper]="promptHelper"
          />
          <qt-markdown-field
            ariaLabel="Template LLM prompt"
            minHeight="14rem"
            [recordKey]="recordKey()"
            [value]="systemPrompt()"
            (contentChange)="systemPrompt.set($event)"
          />
        </div>

        <div class="p-3 rounded-lg qt-border qt-bg-surface">
          <label class="block qt-text-label mb-1">
            Narration Delimiters <span class="qt-text-destructive">*</span>
          </label>
          <p class="qt-text-xs qt-text-muted mb-2">
            How narration and action text is marked in this template's formatting &mdash; or, as
            in a proper novel, not marked at all.
          </p>
          <div class="flex items-center gap-4 mb-2">
            <label class="flex items-center gap-1 qt-text-xs">
              <input
                type="radio"
                name="narrationMode"
                [checked]="narrationMode() === 'single'"
                (change)="setNarrationMode('single')"
              />
              Same open &amp; close
            </label>
            <label class="flex items-center gap-1 qt-text-xs">
              <input
                type="radio"
                name="narrationMode"
                [checked]="narrationMode() === 'pair'"
                (change)="setNarrationMode('pair')"
              />
              Different open &amp; close
            </label>
            <label class="flex items-center gap-1 qt-text-xs">
              <input
                type="radio"
                name="narrationMode"
                [checked]="narrationMode() === 'none'"
                (change)="setNarrationMode('none')"
              />
              None &mdash; plain prose
            </label>
          </div>
          @if (narrationMode() === 'none') {
            <span class="qt-text-xs qt-text-secondary">
              Narration carries no marks; style it under <em>Unmarked Text</em> below.
            </span>
          } @else if (narrationMode() === 'pair') {
            <div class="flex items-center gap-3">
              <label class="qt-text-xs" [for]="id + '-narration-open'">Open:</label>
              <input
                type="text"
                class="qt-input font-mono w-20"
                maxlength="10"
                placeholder="["
                [id]="id + '-narration-open'"
                [value]="narrationOpen()"
                (input)="narrationOpen.set($any($event.target).value)"
              />
              <label class="qt-text-xs" [for]="id + '-narration-close'">Close:</label>
              <input
                type="text"
                class="qt-input font-mono w-20"
                maxlength="10"
                placeholder="]"
                [id]="id + '-narration-close'"
                [value]="narrationClose()"
                (input)="narrationClose.set($any($event.target).value)"
              />
            </div>
            <p class="qt-text-xs qt-text-muted mt-1">e.g. <code>[narration]</code></p>
          } @else {
            <div class="flex items-center gap-2">
              <label class="qt-text-xs" [for]="id + '-narration-delimiter'">Delimiter:</label>
              <input
                type="text"
                class="qt-input font-mono w-20 text-center"
                maxlength="10"
                placeholder="*"
                [id]="id + '-narration-delimiter'"
                [value]="narrationOpen()"
                (input)="setNarrationSingle($any($event.target).value)"
              />
            </div>
            <p class="qt-text-xs qt-text-muted mt-1">e.g. <code>*narration*</code></p>
          }
        </div>

        <!-- Unmarked text + Markdown asterisks (v4 778b97b2b, index.tsx:541-600) -->
        <div class="p-3 rounded-lg qt-border qt-bg-surface">
          <label class="block qt-text-label mb-1">Unmarked Text</label>
          <p class="qt-text-xs qt-text-muted mb-2">
            Whatever no delimiter lays claim to. Give it a style of its own and it is dressed
            accordingly &mdash; italic narration, say, for a template whose narration wears no
            marks.
          </p>
          <label class="flex items-center gap-2 cursor-pointer mb-2">
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="unmarkedEnabled()"
              (change)="unmarkedEnabled.set($any($event.target).checked)"
            />
            <span class="qt-text-xs">Style unmarked text</span>
          </label>
          @if (unmarkedEnabled()) {
            <div class="p-3 rounded-lg qt-border space-y-2">
              <div>
                <label class="qt-text-xs" [for]="id + '-unmarked-style'">Style (CSS class)</label>
                <input
                  type="text"
                  list="qt-style-options-unmarked"
                  placeholder="qt-roleplay-plain"
                  maxlength="50"
                  class="qt-input w-full text-sm font-mono"
                  [id]="id + '-unmarked-style'"
                  [value]="unmarkedStyle()"
                  (input)="unmarkedStyle.set($any($event.target).value)"
                />
                <datalist id="qt-style-options-unmarked">
                  @for (opt of styleOptions; track opt.value) {
                    <option [value]="opt.value">{{ opt.label }}</option>
                  }
                </datalist>
              </div>
              <qt-template-add-ons-editor [(addOns)]="unmarkedAddOns" />
            </div>
          }
          <label class="flex items-start gap-2 cursor-pointer mt-3">
            <input
              type="checkbox"
              class="qt-checkbox mt-0.5"
              [checked]="markdownAsterisks()"
              (change)="markdownAsterisks.set($any($event.target).checked)"
            />
            <span class="qt-text-xs"
              >Markdown reads asterisks as bold and italic. Untick it to hand a single
              <code class="font-mono">*</code> to your own delimiters (an emphasis mark, perhaps);
              doubled <code class="font-mono">**</code> then mean nothing at all and are quietly
              dropped.</span
            >
          </label>
        </div>

        <qt-template-delimiter-editor [(entries)]="delimiters" />
      </div>

      <div qt-modal-footer>
        <qt-form-actions
          [submitLabel]="saving() ? 'Saving...' : submitLabel()"
          [isLoading]="saving()"
          [isDisabled]="!isValid()"
          (cancel)="close.emit()"
          (submit)="submit()"
        />
      </div>
    </qt-modal>
  `,
})
export class TemplateFormModal implements OnInit {
  private readonly core = inject(CoreClient);

  /** The template being edited, or `null` for create / copy-as-new. */
  readonly template = input<RoleplayTemplateDto | null>(null);
  /** When true, seed the form from `template` but save as a NEW template. */
  readonly copy = input(false);

  readonly close = output<void>();
  readonly saved = output<void>();

  protected readonly hints = PROMPT_FIELD_HINTS;
  /**
   * v4 `roleplay-templates/index.tsx:296-301` — the shared
   * roleplayTemplatePrompt helper with this surface's placeholder sentence
   * appended. A TS constant so the `{{char}}` / `{{user}}` braces never reach
   * Angular's interpolation.
   */
  protected readonly promptHelper =
    `${PROMPT_FIELD_HINTS.roleplayTemplatePrompt.helper} You can use placeholders like {{char}} and {{user}}.`;

  protected readonly name = signal('');
  protected readonly description = signal('');
  protected readonly systemPrompt = signal('');
  protected readonly narrationMode = signal<'single' | 'pair' | 'none'>('single');
  protected readonly narrationOpen = signal('*');
  protected readonly narrationClose = signal('*');
  protected readonly delimiters = signal<DelimiterFormEntry[]>([]);
  protected readonly unmarkedEnabled = signal(false);
  protected readonly unmarkedStyle = signal('qt-roleplay-plain');
  protected readonly unmarkedAddOns = signal<FullAddOns>({ ...EMPTY_ADD_ONS });
  protected readonly markdownAsterisks = signal(true);

  protected readonly id = `qt-template-form-${++templateFormModalCounter}`;
  protected readonly styleOptions = STYLE_OPTIONS;

  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  /** v4 `remountKey={editingTemplate?.id ?? 'new'}` (`index.tsx:314`) — the
   *  identity of the record the prompt field is seeded from. */
  protected readonly recordKey = computed(() => this.template()?.id ?? 'new');

  /** Editing when a template is bound and we are NOT copying it as new. */
  private readonly editingId = computed(() => {
    const t = this.template();
    return t && !this.copy() ? t.id : null;
  });

  protected readonly title = computed(() => (this.editingId() ? 'Edit Template' : 'Create Template'));
  protected readonly submitLabel = computed(() =>
    this.editingId() ? 'Save Changes' : 'Create Template',
  );

  /** The signals as one form value — what the helpers in `template-form.ts` read. */
  private readonly formData = computed<TemplateFormData>(() => ({
    name: this.name(),
    description: this.description(),
    systemPrompt: this.systemPrompt(),
    narration: {
      mode: this.narrationMode(),
      open: this.narrationOpen(),
      close: this.narrationClose(),
    },
    delimiters: this.delimiters(),
    unmarkedEnabled: this.unmarkedEnabled(),
    unmarkedStyle: this.unmarkedStyle(),
    unmarkedAddOns: this.unmarkedAddOns(),
    markdownAsterisks: this.markdownAsterisks(),
  }));

  protected readonly isValid = computed(() => isTemplateFormValid(this.formData()));

  ngOnInit(): void {
    const t = this.template();
    const form = t ? templateToForm(t, { copy: this.copy() }) : emptyTemplateForm();
    this.name.set(form.name);
    this.description.set(form.description);
    this.systemPrompt.set(form.systemPrompt);
    this.narrationMode.set(form.narration.mode);
    this.narrationOpen.set(form.narration.open);
    this.narrationClose.set(form.narration.close);
    this.delimiters.set(form.delimiters);
    this.unmarkedEnabled.set(form.unmarkedEnabled);
    this.unmarkedStyle.set(form.unmarkedStyle);
    this.unmarkedAddOns.set(form.unmarkedAddOns);
    this.markdownAsterisks.set(form.markdownAsterisks);
  }

  protected setNarrationMode(mode: 'single' | 'pair' | 'none'): void {
    this.narrationMode.set(mode);
    // v4: switching to single forces close = open.
    if (mode === 'single') {
      this.narrationClose.set(this.narrationOpen());
    }
    // v4 `:470-480`: "Unmarked narration wants a look of its own."
    if (mode === 'none') {
      this.unmarkedEnabled.set(true);
    }
  }

  protected setNarrationSingle(value: string): void {
    // Single mode edits both open and close together (v4).
    this.narrationOpen.set(value);
    this.narrationClose.set(value);
  }

  protected async submit(): Promise<void> {
    if (!this.isValid()) {
      return;
    }
    this.saving.set(true);
    this.error.set(null);
    const body = formToTemplateBody(this.formData());
    try {
      const id = this.editingId();
      if (id) {
        await updateRoleplayTemplate(this.core, id, body);
      } else {
        await createRoleplayTemplate(this.core, body);
      }
      this.saved.emit();
      this.close.emit();
    } catch (err) {
      this.error.set(err instanceof Error && err.message ? err.message : 'An error occurred');
    } finally {
      this.saving.set(false);
    }
  }
}
