import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { RecallConfig } from '../../../core/core-contract';
import { fetchRecallConfig, memoryKeys, setRecallConfig } from '../../../memory/memory.api';
import { ToastService } from '../../../ui/toast.service';

type ScopePolicy = 'down-weight' | 'exclude';

const DEFAULT_CONFIG: RecallConfig = {
  scopePolicy: 'down-weight',
  expandRelated: false,
  perTurnConversationSummaries: false,
};

const SCOPE_POLICY_OPTIONS: ReadonlyArray<{
  value: ScopePolicy;
  label: string;
  description: string;
}> = [
  {
    value: 'down-weight',
    label: 'Down-weight (recommended)',
    description:
      'Apply a strong penalty so a memory tied to another project rarely surfaces here — but never fully hide it. A powerful match can still break through.',
  },
  {
    value: 'exclude',
    label: 'Exclude',
    description:
      'Drop project-specific memories from other projects out of this chat entirely. The firmest guard against cross-project leakage.',
  },
];

/**
 * The Recall Relevance card (v4 `components/tools/memory-recall-card.tsx`): the
 * cross-project scope policy (`down-weight` | `exclude`), the expand-related
 * toggle, and the per-turn conversation-summary toggle (v4 `870a57fa`), each
 * saved on change via `memoryRecallConfigSet` (merge-patch).
 */
@Component({
  selector: 'qt-memory-recall-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (configQuery.isPending()) {
      <p class="qt-text-small qt-text-muted">Loading recall settings…</p>
    } @else {
      <div class="space-y-4">
        <p class="qt-text-small qt-text-muted">
          When a character files away something true only inside a particular project or story, that
          memory shouldn't come wandering into an unrelated conversation. This setting decides what
          becomes of such a memory when it tries to surface in a chat belonging to a different
          project.
        </p>

        <div>
          <label class="qt-text-label block mb-2">Cross-project memories:</label>
          <select
            class="qt-select w-full"
            [disabled]="saving()"
            (change)="onScopePolicyChange($any($event.target).value)"
          >
            @for (option of scopePolicyOptions; track option.value) {
              <option [value]="option.value" [selected]="scopePolicy() === option.value">
                {{ option.label }}
              </option>
            }
          </select>
          @if (activePolicy(); as policy) {
            <p class="qt-text-xs qt-text-secondary mt-1">{{ policy.description }}</p>
          }
        </div>

        <label
          class="flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer"
        >
          <input
            type="checkbox"
            class="mt-1 h-4 w-4 rounded"
            [checked]="expandRelated()"
            [disabled]="saving()"
            (change)="onExpandRelatedChange($any($event.target).checked)"
          />
          <div class="flex-1">
            <div class="font-medium text-foreground">Follow the threads between memories</div>
            <div class="qt-text-small mt-1">
              When a memory surfaces, gather the handful of others it is bound to and let them compete
              for a place in the recollection too. This rescues the memory that is plainly relevant by
              association yet never quite matched the words of the moment. A touch more work each turn;
              left off unless you ask for it.
            </div>
          </div>
        </label>

        <label
          class="flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer"
        >
          <input
            type="checkbox"
            class="mt-1 h-4 w-4 rounded"
            [checked]="perTurnConversationSummaries()"
            [disabled]="saving()"
            (change)="onPerTurnConversationsChange($any($event.target).checked)"
          />
          <div class="flex-1">
            <div class="font-medium text-foreground">Consult past conversations every turn</div>
            <div class="qt-text-small mt-1">
              Ordinarily the Book re-reads its shelf of past conversations only at the opening of a
              chat, whenever it takes down a fresh summary, and when somebody speaks of days gone by.
              Switch this on and it consults that shelf on <em>every</em> turn, so the list of
              pertinent past dialogues never lags behind the talk at hand. It rides along on the
              reckoning already made for the memory search, so it asks nothing further of your
              embedding provider — merely a little more reading, and a few more lines in each
              whisper. Off unless you ask for it, and it applies to every conversation alike.
            </div>
          </div>
        </label>

        @if (error(); as msg) {
          <p class="qt-text-small qt-text-destructive">{{ msg }}</p>
        }
      </div>
    }
  `,
})
export class MemoryRecallCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  protected readonly scopePolicyOptions = SCOPE_POLICY_OPTIONS;
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  /** Local echo of the saved config (seeded from the query, updated on save). */
  private readonly local = signal<RecallConfig | null>(null);

  protected readonly configQuery = injectQuery(() => ({
    queryKey: memoryKeys.recallConfig(),
    queryFn: async (): Promise<RecallConfig> => {
      const settings = await fetchRecallConfig(this.core);
      return { ...DEFAULT_CONFIG, ...settings };
    },
  }));

  private readonly config = computed<RecallConfig>(
    () => this.local() ?? this.configQuery.data() ?? DEFAULT_CONFIG,
  );

  protected readonly scopePolicy = computed(() => this.config().scopePolicy);
  protected readonly expandRelated = computed(() => this.config().expandRelated);
  protected readonly perTurnConversationSummaries = computed(
    () => this.config().perTurnConversationSummaries,
  );
  protected readonly activePolicy = computed(() =>
    SCOPE_POLICY_OPTIONS.find((o) => o.value === this.scopePolicy()),
  );

  protected onScopePolicyChange(value: ScopePolicy): void {
    this.local.set({ ...this.config(), scopePolicy: value });
    void this.save({ scopePolicy: value });
  }

  protected onExpandRelatedChange(value: boolean): void {
    this.local.set({ ...this.config(), expandRelated: value });
    void this.save({ expandRelated: value });
  }

  protected onPerTurnConversationsChange(value: boolean): void {
    this.local.set({ ...this.config(), perTurnConversationSummaries: value });
    void this.save({ perTurnConversationSummaries: value });
  }

  private async save(patch: Partial<RecallConfig>): Promise<void> {
    this.saving.set(true);
    this.error.set(null);
    try {
      const settings = await setRecallConfig(this.core, patch);
      this.local.set({ ...DEFAULT_CONFIG, ...settings });
      this.toasts.showSuccess('Recall settings saved');
    } catch (err) {
      const message =
        err instanceof Error && err.message ? err.message : 'Failed to save recall settings';
      this.error.set(message);
      this.toasts.showError(message);
    } finally {
      this.saving.set(false);
    }
  }
}
