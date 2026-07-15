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
import { TemplateDelimiterEditor } from './template-delimiter-editor';
import {
  emptyTemplateForm,
  formEntriesToDelimiters,
  isTemplateFormValid,
  templateToForm,
  type DelimiterFormEntry,
} from './template-form';
import { createRoleplayTemplate, updateRoleplayTemplate } from './templates.api';

/**
 * The create / edit template modal (v4 `roleplay-templates/index.tsx` modal):
 * Name, Description, LLM Prompt, the Narration Delimiters mode + inputs, and the
 * full Formatting Delimiters editor. On save it PUTs (edit) or POSTs (create) —
 * omitting `renderingPatterns` so the server regenerates. Duplicate-name (409)
 * and built-in-guard (403) messages surface verbatim in the alert.
 *
 * The systemPrompt is a `qt-markdown-field` (v4 `:312`, its `minHeight="14rem"`
 * and `remountKey={editingTemplate?.id ?? 'new'}`). The "Draft formatting
 * instructions" helper button remains a named deferral (the
 * `generateFormattingPromptHint` transcription is out of scope).
 */
@Component({
  selector: 'qt-template-form-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions, ErrorAlert, TemplateDelimiterEditor, MarkdownField],
  template: `
    <qt-modal [title]="title()" maxWidth="3xl" (close)="close.emit()">
      @if (error()) {
        <qt-error-alert [message]="error()!" class="mb-4" />
      }

      <div class="space-y-4">
        <div>
          <label class="block qt-text-label mb-1">
            Name <span class="qt-text-destructive">*</span>
          </label>
          <input
            type="text"
            class="qt-input"
            maxlength="100"
            placeholder="My Custom RP Style"
            [value]="name()"
            (input)="name.set($any($event.target).value)"
          />
          <p class="qt-text-xs qt-text-muted mt-1">{{ name().length }}/100 characters</p>
        </div>

        <div>
          <label class="block qt-text-label mb-1">Description</label>
          <input
            type="text"
            class="qt-input"
            maxlength="500"
            placeholder="A brief description of what this template does"
            [value]="description()"
            (input)="description.set($any($event.target).value)"
          />
          <p class="qt-text-xs qt-text-muted mt-1">{{ description().length }}/500 characters</p>
        </div>

        <div>
          <label class="block qt-text-label mb-1">
            LLM Prompt <span class="qt-text-destructive">*</span>
          </label>
          <p class="qt-text-xs qt-text-muted mb-1">
            The formatting instructions prepended to the character's system prompt when this template
            is selected. You can use placeholders like {{ '{{char}}' }} and {{ '{{user}}' }}.
          </p>
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
            How narration and action text is marked in this template's formatting.
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
          </div>
          @if (narrationMode() === 'pair') {
            <div class="flex items-center gap-3">
              <label class="qt-text-xs">
                Open:
                <input
                  type="text"
                  class="qt-input font-mono w-20 ml-1"
                  maxlength="10"
                  placeholder="["
                  [value]="narrationOpen()"
                  (input)="narrationOpen.set($any($event.target).value)"
                />
              </label>
              <label class="qt-text-xs">
                Close:
                <input
                  type="text"
                  class="qt-input font-mono w-20 ml-1"
                  maxlength="10"
                  placeholder="]"
                  [value]="narrationClose()"
                  (input)="narrationClose.set($any($event.target).value)"
                />
              </label>
            </div>
            <p class="qt-text-xs qt-text-muted mt-1">e.g. <code>[narration]</code></p>
          } @else {
            <label class="qt-text-xs">
              Delimiter:
              <input
                type="text"
                class="qt-input font-mono w-20 ml-1 text-center"
                maxlength="10"
                placeholder="*"
                [value]="narrationOpen()"
                (input)="setNarrationSingle($any($event.target).value)"
              />
            </label>
            <p class="qt-text-xs qt-text-muted mt-1">e.g. <code>*narration*</code></p>
          }
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

  protected readonly name = signal('');
  protected readonly description = signal('');
  protected readonly systemPrompt = signal('');
  protected readonly narrationMode = signal<'single' | 'pair'>('single');
  protected readonly narrationOpen = signal('*');
  protected readonly narrationClose = signal('*');
  protected readonly delimiters = signal<DelimiterFormEntry[]>([]);

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

  protected readonly isValid = computed(() =>
    isTemplateFormValid({
      name: this.name(),
      description: this.description(),
      systemPrompt: this.systemPrompt(),
      narration: {
        mode: this.narrationMode(),
        open: this.narrationOpen(),
        close: this.narrationClose(),
      },
      delimiters: this.delimiters(),
    }),
  );

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
  }

  protected setNarrationMode(mode: 'single' | 'pair'): void {
    this.narrationMode.set(mode);
    // v4: switching to single forces close = open.
    if (mode === 'single') {
      this.narrationClose.set(this.narrationOpen());
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
    const body = {
      name: this.name().trim(),
      description: this.description().trim() || null,
      systemPrompt: this.systemPrompt(),
      narrationDelimiters:
        this.narrationMode() === 'pair'
          ? ([this.narrationOpen(), this.narrationClose()] as [string, string])
          : this.narrationOpen(),
      delimiters: formEntriesToDelimiters(this.delimiters()),
    };
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
