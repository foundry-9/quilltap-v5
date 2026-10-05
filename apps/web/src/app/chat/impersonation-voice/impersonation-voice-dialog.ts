import {
  ChangeDetectionStrategy,
  Component,
  afterRenderEffect,
  computed,
  inject,
  input,
  output,
  untracked,
  viewChild,
  type ElementRef,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import { MarkdownField } from '../../editor/markdown-field';
import { Modal } from '../../ui/modal';
import { ToastService } from '../../ui/toast.service';
import { injectImagesHidden } from '../hidden-image/images-hidden';
import { announcementKeys, fetchAnnouncementProfiles } from '../post-office/post-office.api';
import { VoiceRewriteReviewPanel } from './voice-rewrite-review-panel';

/**
 * `qt-impersonation-voice-dialog` — In Their Own Words, the review dialog for an
 * impersonated seat's line (v4 `components/chat/ImpersonationVoiceDialog.tsx`,
 * `686954937`; the `draft` stage and its footer from `07b8f0209`).
 *
 * The operator typed a line while wearing a character's seat. Nothing has
 * reached the chat yet, and nothing will until a footer button says so.
 *
 * It opens in one of two states. In the **draft** state (the `ask` mode, or
 * after a picker change drops a stale proposal) no model has been called: the
 * operator is presumed to be speaking for the character, so **Send as
 * written** is the primary action (and Cmd/Ctrl+Enter in the draft), and
 * **Restate in their voice** is the one button that spends a call. Once a
 * restatement is in flight or on screen (`always` opens straight into it),
 * **Send** posts the proposal (edited or not) and **Regenerate** asks again.
 * **Edit original** returns to the composer with the draft intact, and
 * **Cancel** changes nothing.
 *
 * Send as written and Edit original are never disabled once a preview has
 * failed — a dead provider must not trap a draft. (They ARE disabled while one
 * is in flight; the failed preview lands in `review`, which is what keeps them
 * reachable.)
 *
 * **Two recorded divergences from v4's component:**
 *
 *  - **`Modal`, not `FloatingDialog`.** v5 has no draggable/resizable dialog
 *    primitive; the sibling rehearsal (Insert Announcement) made the same
 *    substitution and the m6 class doc carries the row. v4's `storageKey`,
 *    `initialGeometry` (640×720) and min size have no counterpart here.
 *  - **No Lexical `namespace`.** v4 keys each Lexical editor instance by name;
 *    v5's ProseMirror-based `qt-markdown-field` needs no such key (D17).
 *
 * The host mounts this FRESH per open (`@if` on the service's `isOpen`), exactly
 * as v4's `ChatModals` does, so the profiles read and the scroll-into-view both
 * run once per rehearsal.
 *
 * @module chat/impersonation-voice/impersonation-voice-dialog
 */
@Component({
  selector: 'qt-impersonation-voice-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, MarkdownField, VoiceRewriteReviewPanel],
  template: `
    <qt-modal
      [title]="dialogTitle()"
      maxWidth="3xl"
      [closeOnBackdrop]="!generating()"
      (close)="onDialogClose()"
    >
      <!-- Seat header (v4 :184-204) -->
      <div class="mb-4 flex items-center gap-3">
        @if (avatarUrl() && !imagesHidden()) {
          <img
            [src]="avatarUrl()"
            alt=""
            class="w-10 h-10 rounded-full object-cover flex-shrink-0"
          />
        } @else {
          <div
            class="w-10 h-10 rounded-full qt-bg-secondary flex items-center justify-center flex-shrink-0"
          >
            <span class="font-bold qt-text-secondary">{{ initial() }}</span>
          </div>
        }
        <div class="min-w-0">
          <div class="font-medium truncate">{{ characterName() }}</div>
          @if (characterTitle()) {
            <div class="text-xs qt-text-secondary truncate">{{ characterTitle() }}</div>
          }
          @if (voiceLine(); as line) {
            <div class="qt-text-xs truncate">{{ line }}</div>
          }
        </div>
      </div>

      <!-- The operator's draft (v4 :206-216) -->
      <div class="mb-4" (keydown)="onSeedKeyDown($event)">
        <label class="block text-sm qt-text-primary mb-2">Your draft</label>
        <qt-markdown-field
          [value]="seed()"
          [disabled]="generating()"
          minHeight="12rem"
          ariaLabel="Your draft"
          (contentChange)="seedChange.emit($event)"
        />
      </div>

      <!-- Voice pickers — changing either drops a stale proposal; nothing re-runs until asked. (v4 :218-262) -->
      <div #pickers class="mb-4 space-y-3">
        <div>
          <label for="impersonation-voice-profile" class="block text-sm qt-text-primary mb-2">
            How should they say it?
          </label>
          <select
            id="impersonation-voice-profile"
            class="qt-input w-full"
            [disabled]="generating()"
            (change)="onProfileChange($any($event.target).value)"
          >
            <option value="">Their own voice{{ ownVoiceSuffix() }}</option>
            @for (p of profiles(); track p.id) {
              <option [value]="p.id">
                {{ p.name }} — {{ p.modelName }}{{ p.isDefault ? ' (default)' : '' }}
              </option>
            }
          </select>
        </div>

        @if (showPromptPicker()) {
          <div>
            <label for="impersonation-voice-prompt" class="block text-sm qt-text-primary mb-2">
              System prompt
            </label>
            <select
              id="impersonation-voice-prompt"
              class="qt-input w-full"
              [disabled]="generating()"
              (change)="onSystemPromptChange($any($event.target).value)"
            >
              @for (p of systemPrompts(); track p.id) {
                <option [value]="p.id">{{ p.name }}{{ p.isDefault ? ' (default)' : '' }}</option>
              }
            </select>
          </div>
        }
      </div>

      <!-- The proposal — absent until a restatement is asked for. (v4 :264-287) -->
      @if (isDraft()) {
        <div class="qt-text-xs">
          Sending as written posts your words under {{ characterName() }}'s name exactly as typed
          (Cmd/Ctrl+Enter). Ask for a restatement only if you want {{ characterName() }} to put it
          in their own voice first.
        </div>
      } @else {
        <div #proposalBox (keydown)="onProposalKeyDown($event)">
          <qt-voice-rewrite-review-panel
            [characterName]="characterName()"
            [generating]="generating()"
            [value]="proposal()"
            [ariaLabel]="proposalAriaLabel()"
            (valueChange)="proposalChange.emit($event)"
          />
          @if (!generating() && proposal().trim().length === 0) {
            <div class="qt-text-xs mt-2">
              Nothing came back. Send your own words as written, or go back and rewrite them.
            </div>
          }
        </div>
      }

      <div qt-modal-footer class="flex items-center justify-end gap-2 flex-wrap">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="generating()"
          (click)="cancel.emit()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="generating()"
          (click)="editOriginal.emit()"
        >
          Edit original
        </button>
        @if (isDraft()) {
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="!hasDraft()"
            (click)="restate.emit()"
          >
            Restate in their voice
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="!hasDraft()"
            (click)="sendAsWritten.emit()"
          >
            Send as written
          </button>
        } @else {
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="generating() || !hasDraft()"
            (click)="sendAsWritten.emit()"
          >
            Send as written
          </button>
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="generating() || !hasDraft()"
            (click)="restate.emit()"
          >
            Regenerate
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="!canSend()"
            (click)="send.emit(proposal())"
          >
            {{ generating() ? 'Rehearsing…' : 'Send' }}
          </button>
        }
      </div>
    </qt-modal>
  `,
})
export class ImpersonationVoiceDialog {
  /** Quick-hide "Salon Images" (v4 `e3937d7aa` `ImpersonationVoiceDialog.tsx:86,:119`). */
  protected readonly imagesHidden = injectImagesHidden();
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  /** The seat being spoken for. */
  readonly characterName = input.required<string>();
  readonly characterTitle = input<string | null>(null);
  readonly avatarUrl = input<string | null>(null);
  /** The profile and model the server actually rewrote through, once known. */
  readonly profileName = input<string | null>(null);
  readonly modelName = input<string | null>(null);
  /** The character's named system prompts; the picker only shows for 2+. */
  readonly systemPrompts = input<ReadonlyArray<{ id: string; name: string; isDefault?: boolean }>>(
    [],
  );
  /** The seat's own selection, used as the prompt picker's initial value. */
  readonly selectedSystemPromptId = input<string | null>(null);
  /** The operator's draft. Editable here; the service owns the text. */
  readonly seed = input('');
  readonly proposal = input('');
  /**
   * `draft` — no restatement requested; `generating` / `review` — one is in
   * flight / on screen (v4 `:56-57`). The host maps the service's `idle` to
   * `draft`, as v4's `ChatModals` does.
   */
  readonly stage = input.required<'draft' | 'generating' | 'review'>();
  readonly profileOverride = input<string | null>(null);
  readonly systemPromptOverride = input<string | null>(null);

  readonly seedChange = output<string>();
  readonly proposalChange = output<string>();
  readonly send = output<string>();
  readonly sendAsWritten = output<void>();
  /** Restate (from the draft state) or Regenerate (from review) — the only call-spending action. */
  readonly restate = output<void>();
  readonly changeProfile = output<string | null>();
  readonly changeSystemPrompt = output<string | null>();
  readonly editOriginal = output<void>();
  readonly cancel = output<void>();

  private readonly proposalBox = viewChild<ElementRef<HTMLElement>>('proposalBox');
  /** Always rendered — the controlled-select effect's door to the document (the proposal box is absent in the draft). */
  private readonly pickers = viewChild<ElementRef<HTMLElement>>('pickers');
  private readonly profileSelect = computed(() => this.profileOverride() ?? '');
  private readonly promptSelect = computed(
    () => this.systemPromptOverride() ?? this.selectedSystemPromptId() ?? '',
  );

  /**
   * The picker list. v4 fetches `/api/v1/connection-profiles` on open and toasts
   * a failure; v5 reads through the sibling rehearsal's query so two dialogs open
   * in one session dedupe into one dispatch.
   */
  private readonly profilesQuery = injectQuery(() => ({
    queryKey: announcementKeys.profiles,
    queryFn: async () => {
      try {
        return await fetchAnnouncementProfiles(this.core);
      } catch (err) {
        this.toasts.showError(
          `Failed to load connection profiles: ${err instanceof Error ? err.message : String(err)}`,
        );
        return [];
      }
    },
  }));

  protected readonly profiles = computed(() => this.profilesQuery.data() ?? []);

  protected readonly dialogTitle = computed(() => `In ${this.characterName()}'s own words`);
  protected readonly initial = computed(() => (this.characterName()[0] ?? '?').toUpperCase());
  protected readonly proposalAriaLabel = computed(() => `What ${this.characterName()} will say`);
  protected readonly showPromptPicker = computed(() => this.systemPrompts().length > 1);
  protected readonly generating = computed(() => this.stage() === 'generating');
  protected readonly isDraft = computed(() => this.stage() === 'draft');
  protected readonly canSend = computed(
    () => !this.generating() && this.proposal().trim().length > 0,
  );
  /** v4 `:133`. */
  protected readonly hasDraft = computed(() => this.seed().trim().length > 0);

  /**
   * v4 `:135-144` — null when neither is known. Before a restatement, name the
   * voice the picker has chosen; after one, the voice the server actually used.
   */
  protected readonly voiceLine = computed(() => {
    const isDraft = this.isDraft();
    const override = this.profileOverride();
    const picked = isDraft && override ? this.profiles().find((p) => p.id === override) : null;
    const name = picked ? picked.name : this.profileName();
    const model = picked ? picked.modelName : this.modelName();
    if (!name && !model) return null;
    const verb = isDraft ? 'Would be spoken through' : 'Spoken through';
    return model ? `${verb} ${name} — ${model}` : `${verb} ${name}`;
  });

  /** v4's `Their own voice{profileName ? ` (${profileName})` : ''}` (`:231`). */
  protected readonly ownVoiceSuffix = computed(() =>
    this.profileName() ? ` (${this.profileName()})` : '',
  );

  constructor() {
    // Bring the proposal into view the moment it arrives. On a short screen the
    // draft and the pickers can push it below the fold, and a review dialog that
    // opens with its answer off-screen is not reviewing anything (v4 `:102-108`,
    // an effect keyed on `generating` — including the FIRST render, where v4's
    // effect also fires because it has no "changed" guard).
    afterRenderEffect(() => {
      if (this.generating()) return;
      untracked(() => {
        // Optional-called: jsdom does not implement `scrollIntoView`, and a
        // review dialog that THROWS because it could not scroll would be worse
        // than one that did not scroll.
        this.proposalBox()?.nativeElement.scrollIntoView?.({
          block: 'nearest',
          behavior: 'smooth',
        });
      });
    });

    // The selects are CONTROLLED by the service's overrides — a re-run started
    // elsewhere (Regenerate, a second picker) must not leave a stale row showing.
    // The P4.D115 idiom: re-apply `value` on the render that fills the list,
    // which is what v4's React `value` prop does for free.
    afterRenderEffect(() => {
      const profile = this.profileSelect();
      const prompt = this.promptSelect();
      // Depend on the list too: the options do not exist before it arrives.
      this.profiles();
      this.systemPrompts();
      untracked(() => {
        const host = this.pickers()?.nativeElement.ownerDocument;
        const profileEl = host?.getElementById('impersonation-voice-profile');
        if (profileEl instanceof HTMLSelectElement) profileEl.value = profile;
        const promptEl = host?.getElementById('impersonation-voice-prompt');
        if (promptEl instanceof HTMLSelectElement) promptEl.value = prompt;
      });
    });
  }

  /** v4 `onClose={generating ? () => {} : onCancel}` (`:171`). */
  protected onDialogClose(): void {
    if (this.generating()) return;
    this.cancel.emit();
  }

  protected onProfileChange(value: string): void {
    this.changeProfile.emit(value || null);
  }

  protected onSystemPromptChange(value: string): void {
    this.changeSystemPrompt.emit(value || null);
  }

  /** v4 `handleProposalKeyDown` (`:146-154`) — Cmd/Ctrl+Enter sends. */
  protected onProposalKeyDown(event: KeyboardEvent): void {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter' && this.canSend()) {
      event.preventDefault();
      this.send.emit(this.proposal());
    }
  }

  /**
   * v4 `handleSeedKeyDown` (`:156-166`). Speaking for the character is the
   * presumption, so the draft's own shortcut passes it through untouched.
   */
  protected onSeedKeyDown(event: KeyboardEvent): void {
    if (
      (event.metaKey || event.ctrlKey) &&
      event.key === 'Enter' &&
      !this.generating() &&
      this.hasDraft()
    ) {
      event.preventDefault();
      this.sendAsWritten.emit();
    }
  }
}
