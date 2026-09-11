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
import { announcementKeys, fetchAnnouncementProfiles } from '../post-office/post-office.api';
import { VoiceRewriteReviewPanel } from './voice-rewrite-review-panel';

/**
 * `qt-impersonation-voice-dialog` — In Their Own Words, the review dialog for an
 * impersonated seat's line (v4 `components/chat/ImpersonationVoiceDialog.tsx`,
 * `686954937`).
 *
 * The operator typed a line while wearing a character's seat; this is where they
 * read it back in that character's voice before it posts. Nothing has reached the
 * chat yet, and nothing will until a footer button says so: **Send** posts the
 * proposal (edited or not), **Regenerate** asks again, **Edit original** returns
 * to the composer with the draft intact, **Send as written** posts the operator's
 * own words, and **Cancel** changes nothing.
 *
 * The last two are never disabled once a preview has FAILED — a dead provider
 * must not trap a draft. (They ARE disabled while one is in flight; the failed
 * preview leaves `generating` false, which is what keeps them reachable.)
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
      <!-- Seat header (v4 :150-172) -->
      <div class="mb-4 flex items-center gap-3">
        @if (avatarUrl()) {
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

      <!-- The operator's draft (v4 :174-185) -->
      <div class="mb-4">
        <label class="block text-sm qt-text-primary mb-2">Your draft</label>
        <qt-markdown-field
          [value]="seed()"
          [disabled]="generating()"
          minHeight="12rem"
          ariaLabel="Your draft"
          (contentChange)="seedChange.emit($event)"
        />
      </div>

      <!-- Voice pickers — changing either re-runs on the current draft (v4 :187-232) -->
      <div class="mb-4 space-y-3">
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

      <!-- The proposal (v4 :234-247) -->
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
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="generating()"
          (click)="sendAsWritten.emit()"
        >
          Send as written
        </button>
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="generating() || seed().trim().length === 0"
          (click)="regenerate.emit()"
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
      </div>
    </qt-modal>
  `,
})
export class ImpersonationVoiceDialog {
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
  readonly generating = input(false);
  readonly profileOverride = input<string | null>(null);
  readonly systemPromptOverride = input<string | null>(null);

  readonly seedChange = output<string>();
  readonly proposalChange = output<string>();
  readonly send = output<string>();
  readonly sendAsWritten = output<void>();
  readonly regenerate = output<void>();
  readonly changeProfile = output<string | null>();
  readonly changeSystemPrompt = output<string | null>();
  readonly editOriginal = output<void>();
  readonly cancel = output<void>();

  private readonly proposalBox = viewChild<ElementRef<HTMLElement>>('proposalBox');
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
  protected readonly canSend = computed(
    () => !this.generating() && this.proposal().trim().length > 0,
  );

  /** v4 `:118-121` — null when the server named neither. */
  protected readonly voiceLine = computed(() => {
    const profile = this.profileName();
    const model = this.modelName();
    if (!profile && !model) return null;
    return model ? `Spoken through ${profile} — ${model}` : `Spoken through ${profile}`;
  });

  /** v4's `Their own voice{profileName ? ` (${profileName})` : ''}` (`:203`). */
  protected readonly ownVoiceSuffix = computed(() =>
    this.profileName() ? ` (${this.profileName()})` : '',
  );

  constructor() {
    // Bring the proposal into view the moment it arrives. On a short screen the
    // draft and the pickers can push it below the fold, and a review dialog that
    // opens with its answer off-screen is not reviewing anything (v4 `:88-94`,
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
        const host = this.proposalBox()?.nativeElement.ownerDocument;
        const profileEl = host?.getElementById('impersonation-voice-profile');
        if (profileEl instanceof HTMLSelectElement) profileEl.value = profile;
        const promptEl = host?.getElementById('impersonation-voice-prompt');
        if (promptEl instanceof HTMLSelectElement) promptEl.value = prompt;
      });
    });
  }

  /** v4 `onClose={generating ? () => {} : onCancel}` (`:136`). */
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

  /** v4 `handleProposalKeyDown` (`:123-132`) — Cmd/Ctrl+Enter sends. */
  protected onProposalKeyDown(event: KeyboardEvent): void {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter' && this.canSend()) {
      event.preventDefault();
      this.send.emit(this.proposal());
    }
  }
}
