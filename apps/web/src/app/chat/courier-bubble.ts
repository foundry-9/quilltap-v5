import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { MessageDto } from '../core/core-contract';
import { formatBytes } from '../ui/format-bytes';

/**
 * The Courier bubble — a faithful port of v4 `app/salon/[id]/components/
 * CourierBubble.tsx`. Rendered in place of a pending manual/clipboard turn
 * (`message.pendingExternalPrompt`): copy the delta (or full-context) prompt into
 * your own LLM, download any referenced attachments to re-upload there, then
 * paste the reply back to settle the turn.
 *
 * v4 posted directly to `/api/v1/chats/{id}/messages/{id}?action=…`; v5 routes
 * the settle/cancel through the dispatch variants (`messageResolveExternalTurn` /
 * `messageCancelExternalTurn`). The component keeps v4's self-contained
 * submitting/cancelling/copied/error state and emits `settled` on success so the
 * conversation refetches (v4 `onCourierTurnSettled`).
 */
@Component({
  selector: 'qt-courier-bubble',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-courier-bubble">
      <div class="qt-courier-bubble-header">
        <span class="qt-courier-bubble-title">
          ✉ {{ characterName() }} — awaiting carrier
          @if (hasFullFallback()) {
            <span class="qt-courier-bubble-mode-badge">
              {{ showFull() ? 'full context' : 'delta' }}
            </span>
          }
        </span>
        <span class="qt-courier-bubble-hint">
          @if (hasFullFallback() && !showFull()) {
            Showing only what is new since the last reply. If your LLM client has
            lost the earlier conversation, switch to full context below.
          } @else {
            Copy the bundle below, paste it into your LLM, then paste the reply
            back here.
          }
        </span>
      </div>

      <div class="qt-courier-bubble-prompt">
        <div class="qt-courier-bubble-prompt-toolbar">
          @if (hasFullFallback()) {
            <button
              type="button"
              class="qt-button-secondary qt-button-sm"
              [title]="
                showFull() ? 'Back to the delta-only bundle' : 'Show the full-context bundle instead'
              "
              (click)="toggleFull()"
            >
              {{ showFull() ? '↩ Use delta' : '↧ Use full context' }}
            </button>
          }
          <button
            type="button"
            class="qt-button-sm"
            [class.qt-button-success]="copied()"
            [class.qt-button-primary]="!copied()"
            title="Copy the prompt to your clipboard"
            (click)="onCopy()"
          >
            {{ copied() ? '✓ Copied' : '📋 Copy prompt' }}
          </button>
        </div>
        <pre class="qt-courier-bubble-prompt-body">{{ prompt() }}</pre>
      </div>

      @if (attachments().length > 0) {
        <div class="qt-courier-bubble-attachments">
          <div class="qt-courier-bubble-attachments-label">
            Referenced attachments — download and re-upload in your destination client:
          </div>
          <ul class="qt-courier-bubble-attachments-list">
            @for (a of attachments(); track a.fileId) {
              <li>
                <a [href]="a.downloadUrl" [download]="a.filename" class="qt-link">{{ a.filename }}</a>
                <span class="qt-courier-bubble-attachment-meta">
                  {{ a.mimeType }}, {{ bytes(a.sizeBytes) }}
                </span>
              </li>
            }
          </ul>
        </div>
      }

      <div class="qt-courier-bubble-paste">
        <label class="qt-courier-bubble-paste-label" [attr.for]="pasteId()">
          Paste the reply from your LLM:
        </label>
        <textarea
          [id]="pasteId()"
          class="qt-textarea qt-courier-bubble-textarea"
          rows="8"
          [value]="reply()"
          [placeholder]="placeholder()"
          [disabled]="submitting() || cancelling()"
          (input)="reply.set($any($event.target).value)"
        ></textarea>
      </div>

      @if (error(); as e) {
        <div class="qt-courier-bubble-error">{{ e }}</div>
      }

      <div class="qt-courier-bubble-actions">
        <button
          type="button"
          class="qt-button-secondary qt-button-sm"
          [disabled]="submitting() || cancelling()"
          (click)="onCancel()"
        >
          {{ cancelling() ? 'Cancelling…' : 'Cancel turn' }}
        </button>
        <button
          type="button"
          class="qt-button-primary qt-button-sm"
          [disabled]="submitting() || cancelling() || reply().trim().length === 0"
          (click)="onSubmit()"
        >
          {{ submitting() ? 'Submitting…' : 'Submit reply' }}
        </button>
      </div>
    </div>
  `,
})
export class CourierBubble {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  readonly message = input.required<MessageDto>();
  readonly characterName = input.required<string>();

  /** Fired after the turn is settled (resolved or cancelled) — parent refetches. */
  readonly settled = output<string>();

  protected readonly showFull = signal(false);
  protected readonly reply = signal('');
  protected readonly copied = signal(false);
  protected readonly submitting = signal(false);
  protected readonly cancelling = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly deltaPrompt = computed(() => this.message().pendingExternalPrompt || '');
  protected readonly fullPrompt = computed(() => this.message().pendingExternalPromptFull || null);
  protected readonly hasFullFallback = computed(() => !!this.fullPrompt());
  protected readonly prompt = computed(() =>
    this.showFull() && this.fullPrompt() ? this.fullPrompt()! : this.deltaPrompt(),
  );
  protected readonly attachments = computed(() => this.message().pendingExternalAttachments || []);
  protected readonly pasteId = computed(() => `courier-paste-${this.message().id}`);
  protected readonly placeholder = computed(
    () => `${this.characterName()}'s response, exactly as your LLM returned it…`,
  );

  protected bytes(n: number): string {
    return formatBytes(n);
  }

  protected toggleFull(): void {
    this.showFull.update((prev) => !prev);
    this.copied.set(false);
  }

  protected async onCopy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(this.prompt());
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 2000);
    } catch (err) {
      this.error.set(`Couldn't reach the clipboard: ${errText(err)}`);
    }
  }

  protected async onSubmit(): Promise<void> {
    const trimmed = this.reply().trim();
    if (!trimmed) {
      this.error.set('Paste the reply from your LLM before submitting.');
      return;
    }
    this.error.set(null);
    this.submitting.set(true);
    try {
      const resp = await this.core.dispatch({
        type: 'messageResolveExternalTurn',
        chatId: this.chatId(),
        messageId: this.message().id,
        replyContent: trimmed,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message);
      }
      this.settled.emit(this.message().id);
    } catch (err) {
      this.error.set(`Couldn't dispatch the reply: ${errText(err)}`);
    } finally {
      this.submitting.set(false);
    }
  }

  protected async onCancel(): Promise<void> {
    this.error.set(null);
    this.cancelling.set(true);
    try {
      const resp = await this.core.dispatch({
        type: 'messageCancelExternalTurn',
        chatId: this.chatId(),
        messageId: this.message().id,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message);
      }
      this.settled.emit(this.message().id);
    } catch (err) {
      this.error.set(`Couldn't cancel the turn: ${errText(err)}`);
    } finally {
      this.cancelling.set(false);
    }
  }
}

function errText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
