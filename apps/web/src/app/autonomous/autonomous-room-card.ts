import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import {
  isCronShapeValid,
  type AutonomousSettingsHint,
  type NewChatAutonomousState,
} from './autonomous.logic';

/**
 * The shared schedule / budget / visibility / destructive-tool editor card (v4
 * `components/new-chat/AutonomousRoomCard.tsx`). Purely controlled: it reads
 * `value` (a {@link NewChatAutonomousState} in human-friendly units — hours,
 * minutes) and emits patches via `changed`. Unit conversion to/from milliseconds
 * happens at each caller's API boundary, never in here. Shared by BOTH the
 * New-Chat form and the Edit-Enclave modal.
 *
 * LOUD DEFERRAL — the LIVE next-run preview: v4 runs `croner` client-side to
 * preview the next fire time as you type. The v5 cron engine (`enclave::cron`)
 * lives server-side, and this lane must not invent a client cron engine or add a
 * new dispatch variant. So the card validates only the cron SHAPE (five fields)
 * and notes that the authoritative next-run is computed on save — it reloads with
 * `scheduleNextRunAt` from the status snapshot afterward.
 */
@Component({
  selector: 'qt-autonomous-room-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="rounded-xl border qt-border-default qt-bg-card p-6 space-y-5">
      <h3 class="qt-section-title">Autonomous Room</h3>

      <!-- Schedule -->
      <div>
        <label for="autonomous-cron" class="mb-2 block text-sm qt-text-primary">
          Schedule (cron, optional)
        </label>
        <input
          id="autonomous-cron"
          type="text"
          [value]="value().scheduleCron"
          (input)="onCron($any($event.target).value)"
          [disabled]="disabled()"
          placeholder="0 4 * * *"
          class="qt-input font-mono"
        />
        <p class="mt-1 qt-text-xs qt-text-muted">
          Five-field cron in instance-local time (minute hour dom month dow). Leave blank to run
          only when started manually.
        </p>
        @if (value().scheduleCron.trim().length > 0) {
          @if (cronShapeValid()) {
            <p class="mt-1 qt-text-xs qt-text-secondary">
              Next run is computed on save.
              <!-- LOUD DEFERRAL: no client cron engine (server-side). -->
            </p>
          } @else {
            <p class="mt-1 qt-text-xs qt-text-destructive">
              That doesn't look like a five-field cron. Expected: minute hour dom month dow.
            </p>
          }
        }
      </div>

      <!-- Freshness -->
      <div>
        <label for="autonomous-freshness" class="mb-2 block text-sm qt-text-primary">
          Catch-up freshness window (hours)
        </label>
        <input
          id="autonomous-freshness"
          type="text"
          inputmode="numeric"
          pattern="[0-9]*"
          [value]="value().scheduleFreshnessHours == null ? '' : value().scheduleFreshnessHours"
          (input)="onNumber('scheduleFreshnessHours', $any($event.target).value)"
          [disabled]="disabled()"
          [placeholder]="freshnessPlaceholder()"
          class="qt-input w-32"
        />
        <p class="mt-1 qt-text-xs qt-text-muted">
          How long after a missed scheduled slot the scheduler should still consider catching up.
          Blank = your default.
        </p>
      </div>

      <!-- Budget caps -->
      <div>
        <p class="mb-2 block text-sm qt-text-primary">Budget caps (per run)</p>
        <div class="grid grid-cols-2 md:grid-cols-3 gap-3">
          <div>
            <label for="autonomous-budget-turns" class="block qt-text-xs qt-text-muted mb-1"
              >Max turns</label
            >
            <input
              id="autonomous-budget-turns"
              type="text"
              inputmode="numeric"
              [value]="value().budgetMaxTurns == null ? '' : value().budgetMaxTurns"
              (input)="onNumber('budgetMaxTurns', $any($event.target).value)"
              [disabled]="disabled()"
              placeholder="(none)"
              class="qt-input"
            />
          </div>
          <div>
            <label for="autonomous-budget-tokens" class="block qt-text-xs qt-text-muted mb-1"
              >Max tokens</label
            >
            <input
              id="autonomous-budget-tokens"
              type="text"
              inputmode="numeric"
              [value]="value().budgetMaxTokens == null ? '' : value().budgetMaxTokens"
              (input)="onNumber('budgetMaxTokens', $any($event.target).value)"
              [disabled]="disabled()"
              placeholder="(none)"
              class="qt-input"
            />
          </div>
          <div>
            <label for="autonomous-budget-wall" class="block qt-text-xs qt-text-muted mb-1"
              >Max wall-clock (min)</label
            >
            <input
              id="autonomous-budget-wall"
              type="text"
              inputmode="numeric"
              [value]="
                value().budgetMaxWallClockMinutes == null ? '' : value().budgetMaxWallClockMinutes
              "
              (input)="onNumber('budgetMaxWallClockMinutes', $any($event.target).value)"
              [disabled]="disabled()"
              placeholder="(none)"
              class="qt-input"
            />
          </div>
          <div class="col-span-2 md:col-span-3">
            <label for="autonomous-budget-spend" class="block qt-text-xs qt-text-muted mb-1"
              >Spend cap (USD, optional)</label
            >
            <input
              id="autonomous-budget-spend"
              type="text"
              inputmode="decimal"
              [value]="
                value().budgetEstimatedSpendCapUSD == null ? '' : value().budgetEstimatedSpendCapUSD
              "
              (input)="onNumber('budgetEstimatedSpendCapUSD', $any($event.target).value)"
              [disabled]="disabled()"
              placeholder="(none)"
              class="qt-input w-40"
            />
          </div>
        </div>
        <p class="mt-1 qt-text-xs qt-text-muted">
          The first cap to be reached draws the run toward its close; leave any blank to skip that
          cap. A cap is a courteous boundary, mind, and not a brick wall: should a run spend its
          allowance so abruptly that the near-end warning never sounds, the company are granted one
          last turn — a single grace round, a touch over budget — to bring the scene to a graceful
          rest rather than be cut off mid-sentence.
        </p>
        <label class="flex items-start gap-2 cursor-pointer mt-3">
          <input
            type="checkbox"
            [checked]="value().budgetExcludeCacheHits"
            (change)="emit({ budgetExcludeCacheHits: $any($event.target).checked })"
            [disabled]="disabled()"
            class="qt-checkbox mt-1"
          />
          <span>
            <span class="qt-text-small font-medium text-foreground"
              >Count only the dear tokens</span
            >
            <span class="block qt-text-xs qt-text-muted mt-1">
              When ticked, the token cap tallies only the costly cache-miss input and the completion
              — the tokens you truly pay full freight for. Untick it to count every token against
              the cap, prompt-cache hits and all, as the ledger once did.
            </span>
          </span>
        </label>
      </div>

      <!-- Visibility -->
      <div>
        <p class="mb-2 block text-sm qt-text-primary">Visibility</p>
        <div class="space-y-1.5">
          <label class="flex items-start gap-2 cursor-pointer">
            <input
              type="radio"
              name="autonomous-visibility"
              [checked]="value().runVisibility == null"
              (change)="emit({ runVisibility: null })"
              [disabled]="disabled()"
              class="qt-radio mt-1"
            />
            <span class="qt-text-small">
              Inherit your default
              <span class="qt-text-muted">(currently: {{ visibilityDefaultLabel() }})</span>
            </span>
          </label>
          <label class="flex items-start gap-2 cursor-pointer">
            <input
              type="radio"
              name="autonomous-visibility"
              [checked]="value().runVisibility === 'owner_only'"
              (change)="emit({ runVisibility: 'owner_only' })"
              [disabled]="disabled()"
              class="qt-radio mt-1"
            />
            <span class="qt-text-small">Owner only — hidden from the main Salon list</span>
          </label>
          <label class="flex items-start gap-2 cursor-pointer">
            <input
              type="radio"
              name="autonomous-visibility"
              [checked]="value().runVisibility === 'household'"
              (change)="emit({ runVisibility: 'household' })"
              [disabled]="disabled()"
              class="qt-radio mt-1"
            />
            <span class="qt-text-small">Household — visible per chat-sharing rules</span>
          </label>
          <label class="flex items-start gap-2 cursor-pointer">
            <input
              type="radio"
              name="autonomous-visibility"
              [checked]="value().runVisibility === 'open'"
              (change)="emit({ runVisibility: 'open' })"
              [disabled]="disabled()"
              class="qt-radio mt-1"
            />
            <span class="qt-text-small">Open — always visible in the Salon list</span>
          </label>
        </div>
      </div>

      <!-- Pre-authorize destructive tools -->
      <div>
        <label class="flex items-start gap-2 cursor-pointer">
          <input
            type="checkbox"
            [checked]="value().runDestructiveToolsAllowed && !policyAlwaysRefuse()"
            (change)="emit({ runDestructiveToolsAllowed: $any($event.target).checked })"
            [disabled]="disabled() || policyAlwaysRefuse()"
            class="qt-checkbox mt-1"
          />
          <span>
            <span class="qt-text-small font-medium text-foreground"
              >Pre-authorize destructive tools</span
            >
            <span class="block qt-text-xs qt-text-muted mt-1">
              Allows tools like <code>doc_delete_file</code> and <code>doc_delete_folder</code> in
              this room.
              @if (policyAlwaysRefuse()) {
                Your user-level policy is set to <em>always refuse</em>; this cannot be overridden
                per room.
              }
            </span>
          </span>
        </label>
      </div>
    </div>
  `,
})
export class AutonomousRoomCard {
  readonly value = input.required<NewChatAutonomousState>();
  readonly settingsHint = input<AutonomousSettingsHint | undefined>(undefined);
  readonly disabled = input(false);
  readonly changed = output<Partial<NewChatAutonomousState>>();

  protected readonly cronShapeValid = computed(() => isCronShapeValid(this.value().scheduleCron));

  protected readonly policyAlwaysRefuse = computed(
    () => this.settingsHint()?.destructiveToolPolicy === 'always_refuse',
  );

  protected readonly freshnessPlaceholder = computed(() => {
    const h = this.settingsHint()?.defaultFreshnessHours;
    return h ? `${h} (your default)` : '12';
  });

  protected readonly visibilityDefaultLabel = computed(() => {
    switch (this.settingsHint()?.visibilityDefault) {
      case 'household':
        return 'household';
      case 'open':
        return 'open';
      case 'owner_only':
      default:
        return 'owner only';
    }
  });

  protected emit(patch: Partial<NewChatAutonomousState>): void {
    this.changed.emit(patch);
  }

  protected onCron(raw: string): void {
    this.emit({ scheduleCron: raw });
  }

  /** v4 `setNumber`: blank → null, non-finite or ≤ 0 → null, else the parsed value. */
  protected onNumber(field: keyof NewChatAutonomousState, raw: string): void {
    const trimmed = raw.trim();
    if (trimmed === '') {
      this.emit({ [field]: null } as Partial<NewChatAutonomousState>);
      return;
    }
    const parsed =
      field === 'budgetEstimatedSpendCapUSD'
        ? Number.parseFloat(trimmed)
        : Number.parseInt(trimmed, 10);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      this.emit({ [field]: null } as Partial<NewChatAutonomousState>);
      return;
    }
    this.emit({ [field]: parsed } as Partial<NewChatAutonomousState>);
  }
}
