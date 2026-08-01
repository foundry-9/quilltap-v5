import { ChangeDetectionStrategy, Component, computed, effect, inject, input, output, signal } from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';

/**
 * The per-chat visibility overrides this section edits. `null` ⇒ inherit.
 *
 * The last two are OPTIONAL because v4's chat GET never returns them (see
 * `chat-section.ts` for the full account), so no caller can supply them —
 * `undefined` means "the record cannot tell us", and the control keeps whatever
 * the operator last chose, exactly as v4's `useChatControls` does.
 */
export interface VisibilityState {
  allowCrossCharacterVaultReads: boolean;
  coreWhisperEnabled: boolean | null;
  coreWhisperInterval: number | null;
  turnSkippingEnabled: boolean | null;
  showThinking?: boolean | null;
  answerConfirmationOverride?: 'ON' | 'OFF' | null;
}

/** v4 `CORE_WHISPER_INTERVAL_OPTIONS` (`ChatSidebar.tsx:1294-1306`). */
const CORE_WHISPER_INTERVAL_OPTIONS = [
  { value: '', label: 'Inherit' },
  { value: '3', label: '3 turns' },
  { value: '6', label: '6 turns' },
  { value: '9', label: '9 turns' },
  { value: '12', label: '12 turns' },
  { value: '15', label: '15 turns' },
  { value: '20', label: '20 turns' },
  { value: '25', label: '25 turns' },
  { value: '30', label: '30 turns' },
  { value: '40', label: '40 turns' },
  { value: '50', label: '50 turns' },
] as const;

/**
 * The sidebar's "Visibility" section (v4 `ChatSidebar.tsx:1308-1481`
 * `VisibilitySection`): the All-Whispers display toggle, Shared Vaults, Turn
 * Skipping, **Aurora's Core Whisper** (offering + cadence — the M6 F3 finding's
 * third inheritance level), Thinking, and Answer Confirmation.
 *
 * v4 splits the labour: the section renders, `useChatControls` writes. v5 keeps
 * the writes here, but the shape is v4's exactly — an optimistic local set, the
 * `{chat: {…}}` PUT, and a revert plus v4's error copy when it fails
 * (`useChatControls.ts:207-345`). Local state is seeded from the chat record
 * only when the field is DEFINED (v4's `if (chat?.X !== undefined)` guards), so
 * the two columns v4's GET never returns — `showThinking` and
 * `answerConfirmationOverride` — keep the operator's in-session choice, as they
 * do in v4. (The reload gap is v4's; see `chat-section.ts`.)
 *
 * All-Whispers is display-only and belongs to the Salon, so it stays an
 * input/output pair (v4 `showAllWhispers` / `onToggleAllWhispers`).
 */
@Component({
  selector: 'qt-visibility-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-chat-sidebar-section qt-chat-sidebar-section-visibility flex flex-col gap-3">
      <div class="flex items-center justify-between">
        <span class="qt-text-secondary text-xs">All Whispers</span>
        <button
          type="button"
          role="switch"
          [class]="
            'relative inline-flex h-6 w-11 items-center rounded-full transition-colors ' +
            (showAllWhispers() ? 'bg-primary' : 'qt-bg-muted')
          "
          [attr.aria-checked]="showAllWhispers()"
          aria-label="All Whispers"
          [title]="showAllWhispers() ? 'Hide private whispers' : 'Show all whispers'"
          (click)="toggleAllWhispers.emit()"
        >
          <span
            [class]="
              'inline-block h-4 w-4 transform rounded-full qt-bg-toggle-knob transition-transform ' +
              (showAllWhispers() ? 'translate-x-6' : 'translate-x-1')
            "
          ></span>
        </button>
      </div>

      <div class="flex items-center justify-between">
        <span class="qt-text-secondary text-xs">Shared Vaults</span>
        <button
          type="button"
          role="switch"
          [class]="
            'relative inline-flex h-6 w-11 items-center rounded-full transition-colors ' +
            (sharedVaults() ? 'bg-primary' : 'qt-bg-muted')
          "
          [attr.aria-checked]="sharedVaults()"
          aria-label="Shared Vaults"
          [title]="
            sharedVaults()
              ? 'Characters may read each other’s vaults (read-only) and the results are public to the chat. Click to lock.'
              : 'Each character’s vault is private; results from doc_* reads are whispered to the caller. Click to let them peek at each other’s dossiers.'
          "
          (click)="onToggleSharedVaults()"
        >
          <span
            [class]="
              'inline-block h-4 w-4 transform rounded-full qt-bg-toggle-knob transition-transform ' +
              (sharedVaults() ? 'translate-x-6' : 'translate-x-1')
            "
          ></span>
        </button>
      </div>

      @if (turnSkippingApplies()) {
        <div class="flex items-center justify-between pt-2 border-t qt-border-default">
          <span
            class="qt-text-secondary text-xs"
            title="When on, characters may pass a turn with a Host note when they have nothing to add, and the Skip button posts a Host note too. NULL/on is the default."
            >Turn Skipping</span
          >
          <button
            type="button"
            role="switch"
            [class]="
              'relative inline-flex h-6 w-11 items-center rounded-full transition-colors ' +
              (turnSkipping() !== false ? 'bg-primary' : 'qt-bg-muted')
            "
            [attr.aria-checked]="turnSkipping() !== false"
            aria-label="Turn Skipping"
            [title]="
              turnSkipping() !== false
                ? 'Characters may pass their turn. Click to require everyone to speak.'
                : 'Every selected character must speak. Click to allow passing.'
            "
            (click)="onToggleTurnSkipping()"
          >
            <span
              [class]="
                'inline-block h-4 w-4 transform rounded-full qt-bg-toggle-knob transition-transform ' +
                (turnSkipping() !== false ? 'translate-x-6' : 'translate-x-1')
              "
            ></span>
          </button>
        </div>
      }

      <div class="flex flex-col gap-2 pt-2 border-t qt-border-default">
        <span class="qt-text-secondary text-xs">Aurora&apos;s Core Whisper</span>
        <div class="flex items-center justify-between gap-2">
          <span class="qt-text-secondary text-xs">Offering</span>
          <select
            class="qt-select qt-select-sm"
            aria-label="Core whisper offering"
            title="When does Aurora offer this chat's characters their own Core/ packet? Inherit defers to per-character and global settings."
            [value]="coreWhisperValue()"
            (change)="onCoreWhisperEnabledChange($event)"
          >
            <option value="inherit" [selected]="coreWhisperValue() === 'inherit'">Inherit</option>
            <option value="on" [selected]="coreWhisperValue() === 'on'">Always</option>
            <option value="off" [selected]="coreWhisperValue() === 'off'">Never</option>
          </select>
        </div>
        <div class="flex items-center justify-between gap-2">
          <span class="qt-text-secondary text-xs">Cadence</span>
          <select
            class="qt-select qt-select-sm"
            aria-label="Core whisper cadence"
            title="Assistant turns between periodic Core whispers in this chat. Inherit defers to the global default."
            [value]="cadenceValue()"
            (change)="onCoreWhisperIntervalChange($event)"
          >
            @for (opt of intervalOptions; track opt.value) {
              <option [value]="opt.value" [selected]="opt.value === cadenceValue()">
                {{ opt.label }}
              </option>
            }
          </select>
        </div>
      </div>

      <div class="flex flex-col gap-2 pt-2 border-t qt-border-default">
        <div class="flex items-center justify-between gap-2">
          <span class="qt-text-secondary text-xs">Thinking</span>
          <select
            class="qt-select qt-select-sm"
            aria-label="Thinking visibility"
            title="Show reasoning models' chain-of-thought in this chat. Inherit defers to the global default. Display-only — never sent to any model."
            [value]="thinkingValue()"
            (change)="onShowThinkingChange($event)"
          >
            <option value="inherit" [selected]="thinkingValue() === 'inherit'">Inherit</option>
            <option value="on" [selected]="thinkingValue() === 'on'">Show</option>
            <option value="off" [selected]="thinkingValue() === 'off'">Hide</option>
          </select>
        </div>
      </div>

      <div class="flex flex-col gap-2 pt-2 border-t qt-border-default">
        <div class="flex items-center justify-between gap-2">
          <span class="qt-text-secondary text-xs">Answer Confirmation</span>
          <select
            class="qt-select qt-select-sm"
            aria-label="Answer confirmation"
            title="Vet this chat's looked-up answers against what the character actually knew. Inherit defers to the project, then the global default."
            [value]="answerConfirmationValue()"
            (change)="onAnswerConfirmationChange($event)"
          >
            <option value="inherit" [selected]="answerConfirmationValue() === 'inherit'">
              Inherit
            </option>
            <option value="ON" [selected]="answerConfirmationValue() === 'ON'">On</option>
            <option value="OFF" [selected]="answerConfirmationValue() === 'OFF'">Off</option>
          </select>
        </div>
      </div>
    </div>
  `,
})
export class VisibilitySection {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly chatId = input.required<string>();
  readonly state = input.required<VisibilityState>();
  /** v4 gates the Turn Skipping row on `qualifiesForTurnSkipping` AND `isMultiChar`. */
  readonly turnSkippingApplies = input(false);
  /** Display-only, owned by the Salon (v4 `showAllWhispers`). */
  readonly showAllWhispers = input(false);
  readonly toggleAllWhispers = output<void>();
  /** Fired after a successful write so the parent can refetch the chat record. */
  readonly chatUpdated = output<void>();

  protected readonly intervalOptions = CORE_WHISPER_INTERVAL_OPTIONS;

  private readonly localSharedVaults = signal(false);
  private readonly localCoreWhisper = signal<boolean | null>(null);
  private readonly localCadence = signal<number | null>(null);
  private readonly localTurnSkipping = signal<boolean | null>(null);
  private readonly localShowThinking = signal<boolean | null>(null);
  private readonly localAnswerConfirmation = signal<'ON' | 'OFF' | null>(null);

  constructor() {
    // v4 seeds each control from the chat record when the field is DEFINED
    // (`useChatControls.ts:104-160`); the columns the GET omits keep whatever the
    // operator last chose.
    effect(() => {
      const s = this.state();
      this.localSharedVaults.set(s.allowCrossCharacterVaultReads);
      this.localCoreWhisper.set(s.coreWhisperEnabled);
      this.localCadence.set(s.coreWhisperInterval);
      this.localTurnSkipping.set(s.turnSkippingEnabled);
      if (s.showThinking !== undefined) this.localShowThinking.set(s.showThinking);
      if (s.answerConfirmationOverride !== undefined) {
        this.localAnswerConfirmation.set(s.answerConfirmationOverride);
      }
    });
  }

  protected readonly sharedVaults = computed(() => this.localSharedVaults());
  protected readonly turnSkipping = computed(() => this.localTurnSkipping());
  protected readonly coreWhisperValue = computed(() =>
    this.localCoreWhisper() === true ? 'on' : this.localCoreWhisper() === false ? 'off' : 'inherit',
  );
  protected readonly cadenceValue = computed(() =>
    this.localCadence() == null ? '' : String(this.localCadence()),
  );
  protected readonly thinkingValue = computed(() =>
    this.localShowThinking() === true ? 'on' : this.localShowThinking() === false ? 'off' : 'inherit',
  );
  protected readonly answerConfirmationValue = computed(
    () => this.localAnswerConfirmation() ?? 'inherit',
  );

  protected onToggleSharedVaults(): void {
    const next = !this.localSharedVaults();
    const previous = this.localSharedVaults();
    this.localSharedVaults.set(next);
    void this.write(
      { allowCrossCharacterVaultReads: next },
      next
        ? 'Shared vault reads enabled — characters may peek at each other’s dossiers'
        : 'Shared vault reads disabled — each character is once more a closed book',
      'Could not update shared-vault setting',
      () => this.localSharedVaults.set(previous),
    );
  }

  /** v4 `onSetTurnSkippingEnabled(turnSkippingEnabled === false ? true : false)`. */
  protected onToggleTurnSkipping(): void {
    const previous = this.localTurnSkipping();
    const next = previous === false ? true : false;
    this.localTurnSkipping.set(next);
    void this.write(
      { turnSkippingEnabled: next },
      null,
      'Could not update turn-skipping setting',
      () => this.localTurnSkipping.set(previous),
    );
  }

  protected onCoreWhisperEnabledChange(event: Event): void {
    const select = event.target as HTMLSelectElement;
    const v = select.value;
    const next = v === 'on' ? true : v === 'off' ? false : null;
    const previous = this.localCoreWhisper();
    this.localCoreWhisper.set(next);
    void this.write(
      { coreWhisperEnabled: next },
      null,
      'Could not update Core whisper setting',
      () => {
        this.localCoreWhisper.set(previous);
        select.value = previous === true ? 'on' : previous === false ? 'off' : 'inherit';
      },
    );
  }

  protected onCoreWhisperIntervalChange(event: Event): void {
    const select = event.target as HTMLSelectElement;
    const raw = select.value;
    const next = raw === '' ? null : parseInt(raw, 10);
    const previous = this.localCadence();
    this.localCadence.set(next);
    void this.write(
      { coreWhisperInterval: next },
      null,
      'Could not update Core whisper cadence',
      () => {
        this.localCadence.set(previous);
        select.value = previous == null ? '' : String(previous);
      },
    );
  }

  protected onShowThinkingChange(event: Event): void {
    const select = event.target as HTMLSelectElement;
    const v = select.value;
    const next = v === 'on' ? true : v === 'off' ? false : null;
    const previous = this.localShowThinking();
    this.localShowThinking.set(next);
    void this.write({ showThinking: next }, null, 'Could not update Thinking visibility', () => {
      this.localShowThinking.set(previous);
      select.value = previous === true ? 'on' : previous === false ? 'off' : 'inherit';
    });
  }

  protected onAnswerConfirmationChange(event: Event): void {
    const select = event.target as HTMLSelectElement;
    const v = select.value;
    const next = v === 'ON' ? 'ON' : v === 'OFF' ? 'OFF' : null;
    const previous = this.localAnswerConfirmation();
    this.localAnswerConfirmation.set(next);
    void this.write(
      { answerConfirmationOverride: next },
      null,
      'Could not update Answer Confirmation',
      () => {
        this.localAnswerConfirmation.set(previous);
        select.value = previous ?? 'inherit';
      },
    );
  }

  /**
   * v4's shared write shape (`useChatControls`): optimistic set (done by the
   * caller), `{chat: {…}}` PUT, revert + the error copy on failure. Only the
   * shared-vault toggle has success copy in v4 (`showInfoToast`); the others
   * report nothing on success.
   */
  private async write(
    bag: Record<string, unknown>,
    successMessage: string | null,
    failureMessage: string,
    revert: () => void,
  ): Promise<void> {
    try {
      const resp = await this.core.dispatch({
        type: 'chatUpdate',
        chatId: this.chatId(),
        chat: bag,
      });
      if (resp.type === 'error') {
        throw new Error(failureMessage);
      }
      if (successMessage) {
        this.toasts.showSuccess(successMessage);
      }
      this.chatUpdated.emit();
    } catch {
      revert();
      this.toasts.showError(failureMessage);
    }
  }
}
