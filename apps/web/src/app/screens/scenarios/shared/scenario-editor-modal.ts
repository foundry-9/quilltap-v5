import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  input,
  output,
  signal,
} from '@angular/core';

import type { ScenarioDto } from '../../../core/core-contract';
import { FormActions } from '../../../ui/form-actions';
import { Modal } from '../../../ui/modal';
import type { ScenarioResult } from '../scenarios.api';

/** The input the modal hands back to the manager's save router. */
export interface ScenarioSaveInput {
  /** Only for the create flow (edit changes the filename via Rename instead). */
  filename?: string;
  name: string;
  description?: string;
  isDefault: boolean;
  body: string;
}

/**
 * ScenarioEditorModal (v4 `components/scenarios/ScenarioEditorModal.tsx`) — the
 * create/edit form for one scenario file. Filename (create only), Name,
 * Description, a "default" checkbox, and the body.
 *
 * v4's body editor is `MarkdownLexicalEditor`; this port ships a plain
 * `<textarea>` (the established P4.6l divergence) — byte content round-trips
 * exactly. The filename-vs-name split is never collapsed: `filename` is the
 * on-disk name (set once, changed only via Rename); `name` is the display title.
 */
@Component({
  selector: 'qt-scenario-editor-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, FormActions],
  template: `
    <qt-modal
      [title]="editing() ? 'Edit scenario — ' + scenario()!.name : 'New scenario'"
      maxWidth="3xl"
      [closeOnBackdrop]="false"
      (close)="close.emit()"
    >
      <div class="space-y-4">
        @if (!editing()) {
          <div>
            <label class="qt-text-label block mb-1" for="scenario-filename">Filename</label>
            <p class="qt-text-xs qt-text-secondary mb-1">
              Used as the file's name on disk (without <code>.md</code>). Allowed characters only;
              spaces are kept.
            </p>
            <input
              id="scenario-filename"
              type="text"
              class="qt-input w-full"
              placeholder="welcome-to-the-estate"
              maxlength="100"
              [value]="filename()"
              (input)="filename.set($any($event.target).value)"
            />
          </div>
        }

        <div>
          <label class="qt-text-label block mb-1" for="scenario-name">Name</label>
          <p class="qt-text-xs qt-text-secondary mb-1">
            Display title used in the new-chat dropdown.
            @if (editing()) {
              Stored in the file's frontmatter; if absent, the filename stands in.
            }
          </p>
          <input
            id="scenario-name"
            type="text"
            class="qt-input w-full"
            placeholder="Welcome to the Estate"
            maxlength="200"
            [value]="name()"
            (input)="name.set($any($event.target).value)"
          />
        </div>

        <div>
          <label class="qt-text-label block mb-1" for="scenario-description"
            >Description (optional)</label
          >
          <p class="qt-text-xs qt-text-secondary mb-1">
            One-line subtitle shown beneath the name in the dropdown.
          </p>
          <input
            id="scenario-description"
            type="text"
            class="qt-input w-full"
            placeholder="A summer evening at the great estate."
            maxlength="500"
            [value]="description()"
            (input)="description.set($any($event.target).value)"
          />
        </div>

        <div class="flex items-center gap-2">
          <input
            id="scenario-default"
            type="checkbox"
            class="qt-checkbox"
            [checked]="isDefault()"
            (change)="isDefault.set($any($event.target).checked)"
          />
          <label for="scenario-default" class="qt-text-small">
            Use this scenario as the {{ scopeLabel() }} default for new chats
          </label>
        </div>

        <div>
          <label class="qt-text-label block mb-1" for="scenario-body">Scenario body</label>
          <p class="qt-text-xs qt-text-secondary mb-2">
            The text that's woven into the system prompt. <code>{{ '{{char}}' }}</code> and
            <code>{{ '{{user}}' }}</code> are substituted at chat time.
          </p>
          <textarea
            id="scenario-body"
            class="qt-input w-full"
            rows="10"
            aria-label="Scenario body"
            [value]="body()"
            (input)="body.set($any($event.target).value)"
          ></textarea>
        </div>

        @if (error(); as msg) {
          <div class="qt-text-destructive text-sm" role="alert">{{ msg }}</div>
        }
      </div>

      <div qt-modal-footer>
        <qt-form-actions
          [submitLabel]="editing() ? 'Save changes' : 'Create scenario'"
          [isLoading]="saving()"
          (cancel)="close.emit()"
          (submit)="handleSave()"
        />
      </div>
    </qt-modal>
  `,
})
export class ScenarioEditorModal implements OnInit {
  /** Existing scenario being edited; null when creating. */
  readonly scenario = input<ScenarioDto | null>(null);
  /** Label used in the "Use this scenario as the {scope} default" checkbox. */
  readonly scopeLabel = input<string>('default');
  /** The save router (v4's scope-specific create/update mutator). */
  readonly onSave = input.required<(input: ScenarioSaveInput) => Promise<ScenarioResult>>();
  readonly close = output<void>();

  protected readonly filename = signal('');
  protected readonly name = signal('');
  protected readonly description = signal('');
  protected readonly isDefault = signal(false);
  protected readonly body = signal('');
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly editing = computed(() => this.scenario() !== null);

  ngOnInit(): void {
    // Seed from the scenario on open (the modal is re-created per open — @if).
    const s = this.scenario();
    if (s) {
      this.filename.set(s.filename);
      this.name.set(s.name);
      this.description.set(s.description ?? '');
      this.isDefault.set(s.isDefault);
      this.body.set(s.body);
    }
  }

  protected async handleSave(): Promise<void> {
    if (this.saving()) {
      return;
    }
    this.error.set(null);

    const trimmedBody = this.body().trim();
    if (trimmedBody.length === 0) {
      this.error.set('Scenario body cannot be empty.');
      return;
    }
    const trimmedName = this.name().trim();
    if (trimmedName.length === 0) {
      this.error.set('Scenario must have a name.');
      return;
    }
    if (!this.editing()) {
      if (this.filename().trim().length === 0) {
        this.error.set('Filename is required.');
        return;
      }
    }

    const description = this.description().trim();
    const input: ScenarioSaveInput = {
      ...(this.editing() ? {} : { filename: this.filename().trim() }),
      name: trimmedName,
      ...(description.length > 0 && { description }),
      isDefault: this.isDefault(),
      body: trimmedBody,
    };

    this.saving.set(true);
    try {
      const result = await this.onSave()(input);
      if (!result.ok) {
        this.error.set(result.error);
        return;
      }
      this.close.emit();
    } finally {
      this.saving.set(false);
    }
  }
}
