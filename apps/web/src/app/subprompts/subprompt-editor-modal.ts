import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  OnInit,
  output,
  signal,
} from '@angular/core';

import type { SubpromptRecord } from '../core/core-contract';
import { MarkdownField } from '../editor/markdown-field';
import { Modal } from '../ui/modal';
import { PROMPT_FIELD_HINTS } from '../ui/prompt-field-hints';
import { PromptFieldLabel } from '../ui/prompt-field-label';
import { ToastService } from '../ui/toast.service';
import { injectCharacterSubprompts, subpromptErrorMessage } from './subprompts.api';

/** What a successful save reports back (v4 `onSaved(saved, mode)`). */
export interface SubpromptSaved {
  subprompt: SubpromptRecord;
  mode: 'created' | 'updated';
}

/**
 * `qt-subprompt-editor-form` — the body of the create/edit dialog (v4
 * `SubpromptEditorForm`, the inner half of
 * `components/subprompts/SubpromptEditorModal.tsx` at `2f4254b42`).
 *
 * It is a separate component for the same reason v4 splits it: the wrapper
 * KEYS this one on `editing?.id ?? 'new'`, so **each opening mounts a fresh
 * form** — seeded from `editing` for an edit, blank for a create — with no
 * reset effect to get wrong. Never add one here; the mount IS the reset.
 */
@Component({
  selector: 'qt-subprompt-editor-form',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, MarkdownField, PromptFieldLabel],
  template: `
    <qt-modal [title]="titleText()" maxWidth="2xl" (close)="close.emit()">
      <div class="space-y-4">
        <div>
          <label for="subprompt-title" class="qt-label">
            Title <span class="qt-text-destructive">*</span>
          </label>
          <input
            id="subprompt-title"
            type="text"
            class="qt-input"
            placeholder="e.g., Keep it brief, Speak in verse, No spoilers"
            maxlength="100"
            [value]="title()"
            [disabled]="saving()"
            (input)="title.set($any($event.target).value)"
          />
          @if (editing(); as record) {
            <p class="text-xs qt-text-secondary mt-1">
              Kept on file as <code>{{ record.path }}</code
              >. The file name stays put when the title changes, so chats that have this subprompt
              in play keep it.
            </p>
          }
        </div>

        <div>
          <qt-prompt-field-label
            [hint]="hints.subprompt"
            label="Instruction"
            required
            [helper]="instructionHelper"
          />
          <qt-markdown-field
            ariaLabel="Subprompt instruction"
            minHeight="10rem"
            [recordKey]="recordKey()"
            [value]="content()"
            [disabled]="saving()"
            (contentChange)="content.set($event)"
          />
        </div>
      </div>

      <div qt-modal-footer>
        <div class="flex justify-end gap-3">
          <button
            type="button"
            class="qt-button-secondary"
            [disabled]="saving()"
            (click)="close.emit()"
          >
            Cancel
          </button>
          <button type="button" class="qt-button-primary" [disabled]="disabled()" (click)="save()">
            {{ saveLabel() }}
          </button>
        </div>
      </div>
    </qt-modal>
  `,
})
export class SubpromptEditorForm implements OnInit {
  private readonly toasts = inject(ToastService);

  readonly characterId = input.required<string>();
  /** Shown in the dialog title so the author knows whose vault this lands in. */
  readonly characterName = input<string | undefined>(undefined);
  /** When set, the dialog EDITS this subprompt; otherwise it creates one. */
  readonly editing = input<SubpromptRecord | null>(null);
  readonly close = output<void>();
  readonly saved = output<SubpromptSaved>();

  /**
   * `enabled: false` — the editor only ever WRITES. v4 passes the same, and it
   * matters: opening "New subprompt…" from a collapsed picker must not fire the
   * list read the picker is deliberately deferring.
   */
  private readonly api = injectCharacterSubprompts(() => this.characterId(), {
    enabled: () => false,
  });

  protected readonly hints = PROMPT_FIELD_HINTS;
  /**
   * v4 `SubpromptEditorModal.tsx:128-133` — the shared subprompt helper with
   * this surface's own suffix. A TS constant, not template markup, so the
   * `{{char}}` / `{{user}}` braces never reach Angular's interpolation.
   */
  protected readonly instructionHelper = `${PROMPT_FIELD_HINTS.subprompt.helper} Markdown is supported, and {{char}} / {{user}} substitute the character and user names.`;

  protected readonly title = signal('');
  protected readonly content = signal('');

  protected readonly saving = computed(() => this.api.savePending());
  protected readonly disabled = computed(
    () => !this.title().trim() || !this.content().trim() || this.saving(),
  );
  protected readonly recordKey = computed(() => this.editing()?.id ?? 'new');
  protected readonly saveLabel = computed(() =>
    this.saving() ? 'Saving...' : this.editing() ? 'Update' : 'Create',
  );
  protected readonly titleText = computed(() => {
    const name = this.characterName();
    return this.editing()
      ? `Edit Subprompt${name ? ` — ${name}` : ''}`
      : `New Subprompt${name ? ` for ${name}` : ''}`;
  });

  /**
   * The seed, ONCE — v4's `useState(() => editing?.title ?? '')`. Deliberately
   * `ngOnInit` and not an effect: the wrapper's key guarantees a fresh instance
   * per opening, so there is nothing to re-seed, and an effect would fight the
   * author's typing the moment anything upstream re-emitted the record.
   */
  ngOnInit(): void {
    const record = this.editing();
    if (record) {
      this.title.set(record.title);
      this.content.set(record.content);
    }
  }

  protected async save(): Promise<void> {
    if (this.disabled()) return;
    const record = this.editing();
    try {
      if (record) {
        const subprompt = await this.api.update(record.id, {
          title: this.title().trim(),
          content: this.content(),
        });
        this.toasts.showSuccess('Subprompt updated');
        this.saved.emit({ subprompt, mode: 'updated' });
      } else {
        const subprompt = await this.api.create({
          title: this.title().trim(),
          content: this.content(),
        });
        this.toasts.showSuccess('Subprompt created');
        this.saved.emit({ subprompt, mode: 'created' });
      }
      this.close.emit();
    } catch (err) {
      this.toasts.showError(
        subpromptErrorMessage(
          err,
          record ? 'Failed to update subprompt' : 'Failed to create subprompt',
        ),
      );
    }
  }
}

/**
 * `qt-subprompt-editor-modal` — create or edit one subprompt in a dialog (v4
 * `components/subprompts/SubpromptEditorModal.tsx`): a title, a Markdown body,
 * and the standing reminder that subprompts are written to the character in the
 * second person, exactly as system prompts are.
 *
 * It owns nothing but the keyed remount. v4's wrapper returns `null` when
 * closed and renders the form under `key={editing?.id ?? 'new'}`; the `@if` +
 * `@switch`-free `[key]`-equivalent here is the two-branch `@if` below, which
 * destroys and re-creates the form whenever the edited record changes identity.
 *
 * A caller summons it with nothing but a character id and hears about a
 * successful save through `saved` — so a picker can tick the new one on.
 */
@Component({
  selector: 'qt-subprompt-editor-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [SubpromptEditorForm],
  template: `
    @if (open()) {
      @for (key of [formKey()]; track key) {
        <qt-subprompt-editor-form
          [characterId]="characterId()"
          [characterName]="characterName()"
          [editing]="editing()"
          (close)="close.emit()"
          (saved)="saved.emit($event)"
        />
      }
    }
  `,
})
export class SubpromptEditorModal {
  readonly open = input.required<boolean>();
  readonly characterId = input.required<string>();
  readonly characterName = input<string | undefined>(undefined);
  readonly editing = input<SubpromptRecord | null>(null);
  readonly close = output<void>();
  readonly saved = output<SubpromptSaved>();

  /**
   * v4's `key={props.editing?.id ?? 'new'}`. Angular has no `key` prop, so the
   * remount is expressed as a one-element `@for` tracked by that same string:
   * change it and Angular destroys the old instance and builds a new one, which
   * is exactly React's keyed-remount semantics.
   */
  protected readonly formKey = computed(() => this.editing()?.id ?? 'new');
}
