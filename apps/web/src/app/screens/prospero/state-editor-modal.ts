import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { ErrorAlert } from '../../ui/error-alert';
import { Modal } from '../../ui/modal';
import { fetchProjectState, resetProjectState, setProjectState } from './projects.api';

/**
 * The project State editor (v4 `components/state/StateEditorModal.tsx`,
 * project arm): a read/edit JSON textarea over `projectStateGet/Set/Reset`. View
 * mode is read-only; Edit reveals the textarea with live JSON validation; Save
 * REPLACES the state wholesale; Reset clears it (with a confirm). Keys starting
 * with `_` are user-only (v4 help text carried over).
 */
@Component({
  selector: 'qt-state-editor-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, ErrorAlert],
  template: `
    <qt-modal [title]="'Project State - ' + projectName()" maxWidth="2xl" (close)="close.emit()">
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" class="mb-3" />
      }
      @if (loading()) {
        <div class="flex items-center justify-center py-8 qt-text-secondary">
          <span>Loading state...</span>
        </div>
      } @else {
        <div class="space-y-4">
          <div>
            <label class="qt-label mb-2 flex items-center justify-between">
              <span>State (JSON)</span>
              @if (!hasState() && !editing()) {
                <span class="qt-text-xs qt-text-secondary">No state data</span>
              }
            </label>
            <textarea
              [value]="stateText()"
              [readOnly]="!editing()"
              rows="12"
              aria-label="State JSON"
              class="qt-textarea font-mono text-sm w-full"
              [class.qt-bg-muted]="!editing()"
              [class.cursor-default]="!editing()"
              [class.qt-border-destructive]="!!jsonError()"
              (input)="onTextChange($event)"
              [placeholder]="editing() ? placeholder : 'No state data'"
            ></textarea>
            @if (jsonError() && editing()) {
              <p class="qt-text-xs qt-text-destructive mt-1">{{ jsonError() }}</p>
            }
          </div>
          <div class="qt-text-xs qt-text-secondary">
            <p class="mb-1">
              State is persistent JSON data that can be used for games, inventory tracking, session
              data, and other information.
            </p>
            <p>Keys starting with underscore (_) are user-only and will not be modified by AI.</p>
          </div>
        </div>
      }

      <div qt-modal-footer class="flex justify-between gap-2 w-full">
        <div>
          @if (!showResetConfirm() && hasState()) {
            <button
              type="button"
              class="qt-button qt-button-danger-outline"
              [disabled]="busy()"
              (click)="showResetConfirm.set(true)"
            >
              Reset State
            </button>
          }
          @if (showResetConfirm()) {
            <div class="flex gap-2">
              <button
                type="button"
                class="qt-button qt-button-danger"
                [disabled]="busy()"
                (click)="reset()"
              >
                {{ resetting() ? 'Resetting...' : 'Confirm Reset' }}
              </button>
              <button
                type="button"
                class="qt-button qt-button-secondary"
                [disabled]="busy()"
                (click)="showResetConfirm.set(false)"
              >
                Cancel
              </button>
            </div>
          }
        </div>
        <div class="flex gap-2">
          @if (editing()) {
            <button
              type="button"
              class="qt-button qt-button-secondary"
              [disabled]="busy()"
              (click)="cancelEdit()"
            >
              Cancel
            </button>
            <button
              type="button"
              class="qt-button qt-button-primary"
              [disabled]="busy() || !!jsonError()"
              (click)="save()"
            >
              {{ saving() ? 'Saving...' : 'Save' }}
            </button>
          } @else {
            <button
              type="button"
              class="qt-button qt-button-secondary"
              [disabled]="busy()"
              (click)="close.emit()"
            >
              Close
            </button>
            <button
              type="button"
              class="qt-button qt-button-primary"
              [disabled]="busy()"
              (click)="editing.set(true)"
            >
              Edit
            </button>
          }
        </div>
      </div>
    </qt-modal>
  `,
})
export class StateEditorModal {
  readonly projectId = input.required<string>();
  readonly projectName = input('');
  readonly close = output<void>();
  readonly saved = output<void>();

  protected readonly placeholder = '{\n  "key": "value"\n}';

  private readonly core = inject(CoreClient);

  protected readonly loading = signal(true);
  protected readonly saving = signal(false);
  protected readonly resetting = signal(false);
  protected readonly editing = signal(false);
  protected readonly showResetConfirm = signal(false);
  protected readonly jsonError = signal<string | null>(null);
  protected readonly saveError = signal<string | null>(null);
  protected readonly stateText = signal('{}');
  private readonly state = signal<Record<string, unknown>>({});

  protected readonly busy = computed(() => this.loading() || this.saving() || this.resetting());
  protected readonly hasState = computed(() => Object.keys(this.state()).length > 0);

  constructor() {
    void this.load();
  }

  private async load(): Promise<void> {
    this.loading.set(true);
    try {
      const state = await fetchProjectState(this.core, this.projectId());
      this.state.set(state);
      this.stateText.set(JSON.stringify(state, null, 2));
      this.editing.set(false);
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to load state');
    } finally {
      this.loading.set(false);
    }
  }

  protected onTextChange(event: Event): void {
    const value = (event.target as HTMLTextAreaElement).value;
    this.stateText.set(value);
    try {
      JSON.parse(value);
      this.jsonError.set(null);
    } catch (e) {
      this.jsonError.set(e instanceof Error ? e.message : 'Invalid JSON');
    }
  }

  protected async save(): Promise<void> {
    this.saveError.set(null);
    let parsed: unknown;
    try {
      parsed = JSON.parse(this.stateText());
    } catch {
      this.jsonError.set('Invalid JSON');
      return;
    }
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      this.saveError.set('State must be a JSON object');
      return;
    }
    this.saving.set(true);
    try {
      const next = await setProjectState(
        this.core,
        this.projectId(),
        parsed as Record<string, unknown>,
      );
      this.state.set(next);
      this.stateText.set(JSON.stringify(next, null, 2));
      this.editing.set(false);
      this.saved.emit();
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to save state');
    } finally {
      this.saving.set(false);
    }
  }

  protected async reset(): Promise<void> {
    this.saveError.set(null);
    this.resetting.set(true);
    try {
      await resetProjectState(this.core, this.projectId());
      this.state.set({});
      this.stateText.set('{}');
      this.editing.set(false);
      this.showResetConfirm.set(false);
      this.saved.emit();
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to reset state');
    } finally {
      this.resetting.set(false);
    }
  }

  protected cancelEdit(): void {
    this.stateText.set(JSON.stringify(this.state(), null, 2));
    this.jsonError.set(null);
    this.editing.set(false);
  }
}
