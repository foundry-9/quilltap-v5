import { Injectable, computed, inject, signal } from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import { shouldRehearseImpersonatedLine, type RehearsalSeat } from './gate';
import { previewImpersonationVoice } from './impersonation-voice.api';

/**
 * In Their Own Words — the Salon-scoped state machine behind the review dialog
 * (v4 `app/salon/[id]/hooks/useImpersonationVoice.ts`, `686954937`).
 *
 * When the instance setting is on and the seat the composer will attribute the
 * message to is one the human is *impersonating* (the Bug 44 overlay, not an
 * owner seat), the draft does not go straight to the chat. It is stashed, the
 * review dialog opens, and the seat's own model restates it in the character's
 * voice. Nothing reaches the chat until the operator chooses — and every exit
 * from the dialog that does not send leaves the draft exactly where it was.
 *
 * **Provided at the Salon component**, never `providedIn: 'root'` — it holds one
 * chat's in-flight submit, and a root singleton would also reach the NG0201
 * trap that produced dogfood finding #105.
 *
 * Three recorded shape divergences from v4's hook, all forced by the boundary:
 *
 *  - **No `preventDefault`.** v4's `intercept(e, args)` is called from the
 *    composer's `onSubmit` with the real form event and cancels it. v5's
 *    composer emits a `send` output that the Salon's `send()` returns out of;
 *    there is no event to cancel, so `intercept` takes the args alone.
 *  - **One error arm, not two.** v4 branches on `!res.ok` (toast
 *    `err.message || err.error || \`Failed (HTTP ${status})\``) versus a thrown
 *    exception (toast `error.message || 'Failed to restate the line in
 *    character'`). v5 dispatches rather than fetching, so a refusal ARRIVES as a
 *    thrown `CoreDispatchError` carrying the envelope's message — the two arms
 *    converge, and the HTTP-status fallback has no counterpart because there is
 *    no status at this seam.
 *  - **`sendFinal` instead of `sendMessage` + an args bundle.** v4 carries
 *    `setInput`/`setPendingToolResults`/`clearDraft`/`userStoppedStreamRef`
 *    through the hook so the dialog's send can call the streaming hook exactly
 *    as the composer would. v5's Salon owns `runTurn` and the composer's clear;
 *    the host registers ONE callback and the service never learns the rest.
 *
 * @module chat/impersonation-voice/impersonation-voice.state
 */

/** Everything the dialog needs about the seat it is rehearsing for (v4 `:72-82`). */
export interface RehearsalTarget {
  participantId: string;
  characterName: string;
  characterTitle?: string | null;
  avatarUrl?: string | null;
  /** The seat's own profile, used as the picker's default label. */
  profileName?: string | null;
  modelName?: string | null;
  systemPrompts?: Array<{ id: string; name: string; isDefault?: boolean }>;
  selectedSystemPromptId?: string | null;
}

export type RehearsalStage = 'idle' | 'generating' | 'review';

/** The submit the gate took over, held verbatim until it sends or is discarded. */
export interface PendingSend {
  seed: string;
  fileIds: string[];
  /** Rolled-but-unsent tool results riding this send (v4 `pendingToolResults`). */
  pending: readonly unknown[];
}

/** What the service needs from the Salon that hosts it. */
export interface ImpersonationVoiceHost {
  /** The chat the rehearsal belongs to. */
  chatId: () => string | null;
  /**
   * Post `final` with the stash's attachments and pending results, exactly as a
   * normal send does — including clearing the composer. v4 reaches the same
   * place by calling `sendMessage`, which owns the clear.
   */
  sendFinal: (final: string, stash: PendingSend) => void;
  /** Put the cursor back in the composer when the operator returns to it. */
  focusComposer: () => void;
}

/** The args `intercept` decides on (v4's `intercept` bundle minus the event). */
export interface InterceptArgs {
  text: string;
  seat: RehearsalSeat | null;
  seatTarget: RehearsalTarget | null;
  enabled: boolean;
  impersonatingParticipantIds: readonly string[];
  fileIds: string[];
  pending: readonly unknown[];
}

@Injectable()
export class ImpersonationVoiceState {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  private readonly pending = signal<PendingSend | null>(null);
  readonly target = signal<RehearsalTarget | null>(null);
  readonly stage = signal<RehearsalStage>('idle');
  readonly proposal = signal('');
  readonly profileOverride = signal<string | null>(null);
  readonly systemPromptOverride = signal<string | null>(null);
  readonly resolvedVoice = signal<{ profileName: string; modelName: string } | null>(null);

  /**
   * One-shot: set immediately before a send dispatched from the dialog, so the
   * gate lets it through if it is ever consulted on the way out. Consumed by the
   * next `intercept` and cleared on every `close`, so it cannot leak into a later
   * submit and skip a rehearsal the operator expected. A plain field, not a
   * signal — nothing renders from it (v4's `bypassOnceRef`).
   */
  private bypassOnce = false;

  private host: ImpersonationVoiceHost | null = null;

  readonly isOpen = computed(() => this.pending() !== null);
  readonly seed = computed(() => this.pending()?.seed ?? '');
  readonly generating = computed(() => this.stage() === 'generating');

  /** The Salon registers itself once, in its own constructor. */
  attach(host: ImpersonationVoiceHost): void {
    this.host = host;
  }

  /** The draft editor writes straight through — the service owns the text. */
  setSeed(value: string): void {
    const current = this.pending();
    if (!current) return;
    this.pending.set({ ...current, seed: value });
  }

  setProposal(value: string): void {
    this.proposal.set(value);
  }

  /**
   * Called from the Salon's `send()` before anything is snapshotted or cleared.
   * Returns true when it has taken the submit over.
   */
  intercept(args: InterceptArgs): boolean {
    const hasAttachmentsOnly =
      args.text.trim().length === 0 && (args.fileIds.length > 0 || args.pending.length > 0);

    const armed = shouldRehearseImpersonatedLine({
      enabled: args.enabled,
      seat: args.seat,
      impersonatingParticipantIds: args.impersonatingParticipantIds,
      text: args.text,
      hasAttachmentsOnly,
      bypassOnce: this.bypassOnce,
    });

    // The bypass is consumed by the submit it was set for, whether or not
    // anything else about the gate would have fired.
    this.bypassOnce = false;

    if (!armed || !args.seatTarget) return false;

    this.pending.set({ seed: args.text, fileIds: args.fileIds, pending: args.pending });
    this.target.set(args.seatTarget);
    this.profileOverride.set(null);
    this.systemPromptOverride.set(null);
    this.resolvedVoice.set(null);
    void this.runPreview(args.text, null, null, args.seatTarget.participantId);
    return true;
  }

  private async runPreview(
    seed: string,
    profileId: string | null,
    systemPromptId: string | null,
    participantId: string,
  ): Promise<void> {
    const chatId = this.host?.chatId() ?? null;
    this.stage.set('generating');
    this.proposal.set('');
    if (!chatId) {
      this.toasts.showError('Failed to restate the line in character');
      this.stage.set('review');
      return;
    }
    try {
      const data = await previewImpersonationVoice(this.core, {
        chatId,
        participantId,
        seedMarkdown: seed,
        connectionProfileId: profileId,
        systemPromptId,
      });
      this.proposal.set(data.proposedMarkdown);
      if (data.hasResolvedVoice) {
        this.resolvedVoice.set({ profileName: data.profileName, modelName: data.modelName });
      }
      this.stage.set('review');
    } catch (err) {
      this.toasts.showError(
        (err instanceof Error ? err.message : '') || 'Failed to restate the line in character',
      );
      // Stay in review with an empty proposal: a dead provider must never trap
      // the operator's draft behind a dialog with nothing to press.
      this.stage.set('review');
    }
  }

  close(): void {
    // Cleared on every close so a one-shot bypass can never outlive the dialog
    // and silently skip the *next* rehearsal.
    this.bypassOnce = false;
    this.pending.set(null);
    this.target.set(null);
    this.stage.set('idle');
    this.proposal.set('');
    this.profileOverride.set(null);
    this.systemPromptOverride.set(null);
    this.resolvedVoice.set(null);
  }

  send(final: string): void {
    const stash = this.pending();
    const host = this.host;
    if (!stash || !host) return;
    // The host's send clears the editor and the draft itself, so both Send
    // variants leave the composer empty exactly as a normal send does.
    this.bypassOnce = true;
    host.sendFinal(final, stash);
    this.close();
  }

  sendAsWritten(): void {
    const stash = this.pending();
    if (stash) this.send(stash.seed);
  }

  /**
   * The draft the operator can see right now — an edit in the dialog is kept, so
   * a later "Send as written" sends what they are looking at.
   */
  regenerate(): void {
    const stash = this.pending();
    const target = this.target();
    if (!target || !stash) return;
    void this.runPreview(
      stash.seed,
      this.profileOverride(),
      this.systemPromptOverride(),
      target.participantId,
    );
  }

  /** Changing a picker drops the proposal and re-runs on the current draft. */
  changeProfile(profileId: string | null): void {
    this.profileOverride.set(profileId);
    const stash = this.pending();
    const target = this.target();
    if (!target || !stash) return;
    void this.runPreview(stash.seed, profileId, this.systemPromptOverride(), target.participantId);
  }

  changeSystemPrompt(systemPromptId: string | null): void {
    this.systemPromptOverride.set(systemPromptId);
    const stash = this.pending();
    const target = this.target();
    if (!target || !stash) return;
    void this.runPreview(stash.seed, this.profileOverride(), systemPromptId, target.participantId);
  }

  /** Back to the composer — the draft is still in the editor, untouched. */
  editOriginal(): void {
    this.close();
    this.host?.focusComposer();
  }

  cancel(): void {
    this.close();
    this.host?.focusComposer();
  }
}
