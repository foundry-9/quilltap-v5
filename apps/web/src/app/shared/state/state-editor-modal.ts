import {
  ChangeDetectionStrategy,
  Component,
  type OnInit,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../../core/core-client';
import type { GroupTier } from '../../core/core-contract';
import { Modal } from '../../ui/modal';
import {
  fetchState,
  resetState,
  setState,
  stateLabel,
  type StateEntityType,
} from './state.api';
import { ToastService } from '../../ui/toast.service';

/** One inherited tier's summary — the label + the keys it contributes. */
interface InheritedLayer {
  label: string;
  keys: string[];
}

/**
 * The four-entity State editor (v4 `components/state/StateEditorModal.tsx`): a
 * read/edit JSON textarea over the §A state verbs, generalized across the four
 * tiers (`chat` | `project` | `group` | `general`). View mode is read-only; Edit
 * reveals the textarea with live JSON validation; Save REPLACES the state
 * wholesale; Reset clears it (with a confirm). Keys starting with `_` are
 * user-only (v4 help text carried over).
 *
 * The CHAT tier alone surfaces the inherited cascade beneath its own state: a
 * note explaining that narrower tiers win, one line per non-empty inherited
 * tier, and — when more than one group applies — that group state is not merged.
 * The `general` tier takes no `entityId`.
 */
@Component({
  selector: 'qt-state-editor-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal [title]="title()" maxWidth="2xl" (close)="close.emit()">
      @if (loading()) {
        <div class="flex items-center justify-center py-8 qt-text-secondary">
          <span>Loading state...</span>
        </div>
      } @else {
        <div class="space-y-4">
          @if (entityType() === 'chat' && (inheritedLayers().length > 0 || ambiguousGroups())) {
            <div class="qt-text-xs p-3 qt-bg-muted rounded-md">
              <p class="mb-1">
                <strong>Note:</strong> The merged state below layers the cascade (chat over project
                over group over general — narrower tiers win).
              </p>
              @for (layer of inheritedLayers(); track layer.label) {
                <p class="qt-text-secondary">
                  Inherited from {{ layer.label }}: {{ layer.keys.join(', ') }}
                </p>
              }
              @if (ambiguousGroups()) {
                <p class="qt-text-secondary mt-1">
                  {{ groupTier()!.candidates.length }} groups apply — group state is not merged here.
                  Edit each group&rsquo;s state from its own page.
                </p>
              }
            </div>
          }
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
export class StateEditorModal implements OnInit {
  /** Which tier this modal edits. */
  readonly entityType = input.required<StateEntityType>();
  /** The tier's entity id. Ignored for the instance-wide `general` tier. */
  readonly entityId = input('');
  readonly entityName = input<string | undefined>(undefined);
  readonly close = output<void>();
  readonly saved = output<void>();

  protected readonly placeholder = '{\n  "key": "value"\n}';

  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  protected readonly loading = signal(true);
  protected readonly saving = signal(false);
  protected readonly resetting = signal(false);
  protected readonly editing = signal(false);
  protected readonly showResetConfirm = signal(false);
  protected readonly jsonError = signal<string | null>(null);
  // Public (read by the template AND asserted by the spec, the proving-bench idiom).
  readonly stateText = signal('{}');
  private readonly state = signal<Record<string, unknown>>({});

  // Chat-tier only: the inherited slices, kept for the cascade note.
  private readonly projectState = signal<Record<string, unknown> | undefined>(undefined);
  private readonly groupState = signal<Record<string, unknown> | undefined>(undefined);
  private readonly generalState = signal<Record<string, unknown> | undefined>(undefined);
  protected readonly groupTier = signal<GroupTier | undefined>(undefined);

  protected readonly busy = computed(() => this.loading() || this.saving() || this.resetting());
  readonly hasState = computed(() => Object.keys(this.state()).length > 0);

  /** v4's per-entity title — `General State`, else `Chat State - Aria` etc. */
  protected readonly title = computed(() => {
    if (this.entityType() === 'general') return 'General State';
    const name = this.entityName();
    return `${stateLabel(this.entityType())} State${name ? ` - ${name}` : ''}`;
  });

  /** The inherited-tier summaries the chat cascade note renders (v4's order). */
  protected readonly inheritedLayers = computed<InheritedLayer[]>(() => {
    if (this.entityType() !== 'chat') return [];
    const layers: InheritedLayer[] = [];
    const push = (label: string, slice: Record<string, unknown> | undefined) => {
      if (slice && Object.keys(slice).length > 0) layers.push({ label, keys: Object.keys(slice) });
    };
    push('project', this.projectState());
    push('group', this.groupState());
    push('general', this.generalState());
    return layers;
  });

  protected readonly ambiguousGroups = computed(
    () => this.entityType() === 'chat' && this.groupTier()?.status === 'ambiguous',
  );

  ngOnInit(): void {
    // Inputs (entityType/entityId) resolve by ngOnInit — reading a required
    // input in the constructor would throw (and the async throw would be lost).
    void this.load();
  }

  private async load(): Promise<void> {
    this.loading.set(true);
    try {
      const result = await fetchState(this.core, this.entityType(), this.entityId());
      this.state.set(result.state);
      this.stateText.set(JSON.stringify(result.state, null, 2));
      if (this.entityType() === 'chat') {
        this.projectState.set(result.projectState);
        this.groupState.set(result.groupState);
        this.generalState.set(result.generalState);
        this.groupTier.set(result.groupTier);
      }
      this.editing.set(false);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to load state');
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

  async save(): Promise<void> {
    let parsed: unknown;
    try {
      parsed = JSON.parse(this.stateText());
    } catch {
      this.jsonError.set('Invalid JSON');
      return;
    }
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      this.toasts.showError('State must be a JSON object');
      return;
    }
    this.saving.set(true);
    try {
      const next = await setState(
        this.core,
        this.entityType(),
        this.entityId(),
        parsed as Record<string, unknown>,
      );
      this.state.set(next);
      this.stateText.set(JSON.stringify(next, null, 2));
      this.editing.set(false);
      this.toasts.showSuccess('State saved');
      this.saved.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to save state');
    } finally {
      this.saving.set(false);
    }
  }

  async reset(): Promise<void> {
    this.resetting.set(true);
    try {
      await resetState(this.core, this.entityType(), this.entityId());
      this.state.set({});
      this.stateText.set('{}');
      this.editing.set(false);
      this.showResetConfirm.set(false);
      this.toasts.showSuccess('State reset');
      this.saved.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to reset state');
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
