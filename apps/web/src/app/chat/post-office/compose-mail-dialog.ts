import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { MarkdownField } from '../../editor/markdown-field';
import { characterKeys, fetchCharacterList } from '../../screens/characters/characters.api';
import { formatDate } from '../../shared/format-date';
import { Modal } from '../../ui/modal';
import { fetchMailbox, mailboxKeys, sendMail } from './post-office.api';

/** One character in the scene, as the salon hands them over (v4 `ComposeMailParticipant`). */
export interface ComposeMailParticipant {
  /** The workspace character id — what the action uses as from/to. */
  id: string;
  name: string;
  controlledBy: 'llm' | 'user';
}

/** v4's sentinel select value for "No quoted reply." (`:47`). */
const NO_REPLY = '';

/**
 * The Compose Mail dialog — The Post Office (v4
 * `components/chat/ComposeMailDialog.tsx`). The operator posts a letter AS one
 * of their player-characters to another character, optionally quoting a letter
 * from the sender's own postbox; Suparṇā carries it, and the delivery whisper
 * appears once the chat refetches.
 *
 * v4's two asymmetric lists are load-bearing and carried exactly:
 *
 *  - **Signed by** is only the characters the operator controls IN THIS CHAT.
 *    You can only sign as someone you are playing, so a scene you are watching
 *    rather than playing offers nobody.
 *  - **Addressed to** is the whole workspace roster minus the sender. The server
 *    allows any character → any character, so a letter can be addressed to
 *    someone who is not in the scene at all. Self-mail is legal server-side but
 *    a confusing default, so the sender is excluded.
 *
 * The effective recipient is DERIVED, not stored-and-synced (v4 `:108-111`): an
 * explicit pick wins while it is still a valid recipient, otherwise the first
 * one. That defaults the dropdown as the roster loads and silently re-picks when
 * the chosen sender becomes the current recipient — with no setState-in-effect.
 *
 * As with the announcement dialog, v4's draggable `FloatingDialog` becomes a
 * centered `qt-modal` (the Brahma Console ruling) and v4's toasts become an
 * inline alert.
 */
@Component({
  selector: 'qt-compose-mail-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, MarkdownField],
  template: `
    <qt-modal
      title="Compose Mail"
      maxWidth="3xl"
      [closeOnBackdrop]="!isSending()"
      (close)="onDialogClose()"
    >
      <!-- From (v4 :201-234) -->
      <div class="mb-4">
        <label for="mail-from" class="block text-sm qt-text-primary mb-2">Signed by</label>
        @if (!hasSender()) {
          <div class="qt-text-secondary text-sm">
            You aren&rsquo;t playing anyone in this scene, so there&rsquo;s no one to sign the
            letter.
          </div>
        } @else if (senders().length === 1) {
          <select id="mail-from" class="qt-input w-full" disabled aria-label="Signed by">
            <option [value]="senders()[0].id" selected>{{ senders()[0].name }}</option>
          </select>
        } @else {
          <select
            id="mail-from"
            class="qt-input w-full"
            [value]="fromCharacterId()"
            [disabled]="isSending()"
            (change)="onFromChange($any($event.target).value)"
          >
            @for (s of senders(); track s.id) {
              <option [value]="s.id" [selected]="s.id === fromCharacterId()">{{ s.name }}</option>
            }
          </select>
        }
      </div>

      <!-- To (v4 :237-262) -->
      <div class="mb-4">
        <label for="mail-to" class="block text-sm qt-text-primary mb-2">Addressed to</label>
        @if (recipientsLoading()) {
          <div class="qt-text-secondary text-sm">Fetching the address book&hellip;</div>
        } @else if (!hasRecipient()) {
          <div class="qt-text-secondary text-sm">
            There&rsquo;s no one else to address a letter to.
          </div>
        } @else {
          <select
            id="mail-to"
            class="qt-input w-full"
            [value]="effectiveToCharacterId()"
            [disabled]="isSending()"
            (change)="toCharacterId.set($any($event.target).value)"
          >
            @for (r of recipients(); track r.id) {
              <option [value]="r.id" [selected]="r.id === effectiveToCharacterId()">
                {{ r.name }}{{ r.title ? ' — ' + r.title : '' }}
              </option>
            }
          </select>
        }
      </div>

      <!-- In reply to (v4 :265-286) -->
      <div class="mb-4">
        <label for="mail-reply" class="block text-sm qt-text-primary mb-2">In reply to</label>
        <select
          id="mail-reply"
          class="qt-input w-full"
          [value]="inReplyToPath()"
          [disabled]="isSending() || !hasSender() || mailboxLoading()"
          (change)="inReplyToPath.set($any($event.target).value)"
        >
          <option [value]="NO_REPLY" [selected]="inReplyToPath() === NO_REPLY">
            No quoted reply.
          </option>
          @for (l of letters(); track l.path) {
            <option [value]="l.path" [selected]="l.path === inReplyToPath()">
              From {{ l.from }} · {{ sentOn(l.sentAt) }}
            </option>
          }
        </select>
        @if (mailboxLoading()) {
          <div class="qt-text-xs mt-1">Rummaging through the postbox&hellip;</div>
        }
      </div>

      <!-- The letter (v4 :289-298) -->
      <div class="mb-2">
        <label class="block text-sm qt-text-primary mb-2">The letter</label>
        <qt-markdown-field
          [value]="body()"
          [disabled]="isSending()"
          minHeight="12rem"
          ariaLabel="The body of your letter"
          (contentChange)="body.set($event)"
        />
      </div>

      @if (errorMessage(); as msg) {
        <div class="qt-alert-error text-sm mt-2" role="alert">{{ msg }}</div>
      }

      <div qt-modal-footer class="flex items-center justify-end gap-3">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="isSending()"
          (click)="close.emit()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="!canSend()"
          (click)="onSend()"
        >
          {{ isSending() ? 'Posting…' : 'Send' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class ComposeMailDialog {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  /** The chat's active CHARACTER participants (character id + name + controlledBy). */
  readonly participants = input<readonly ComposeMailParticipant[]>([]);

  readonly close = output<void>();
  /** The letter is away — the salon refetches so Suparṇā's delivery shows (v4 `onPosted`). */
  readonly posted = output<void>();

  protected readonly NO_REPLY = NO_REPLY;

  /** v4 `senders` (`:76-79`) — only the player-characters in THIS chat. */
  protected readonly senders = computed(() =>
    this.participants().filter((p) => p.controlledBy === 'user'),
  );

  private readonly fromOverride = signal<string | null>(null);
  protected readonly toCharacterId = signal('');
  protected readonly inReplyToPath = signal<string>(NO_REPLY);
  protected readonly body = signal('');
  protected readonly errorMessage = signal<string | null>(null);
  protected readonly isSending = signal(false);

  /**
   * v4 seeds `fromCharacterId` from `senders[0]` in a `useState` initializer,
   * which runs on mount — before the participants could change. Deriving it the
   * same way (an explicit pick wins, else the first sender) keeps that behavior
   * without a setState-in-effect, and matches the recipient derivation below.
   */
  protected readonly fromCharacterId = computed(
    () => this.fromOverride() ?? this.senders()[0]?.id ?? '',
  );

  private readonly charactersQuery = injectQuery(() => ({
    queryKey: characterKeys.list(),
    queryFn: () => fetchCharacterList(this.core),
  }));

  /** v4 `recipients` (`:96-101`) — the whole roster minus the sender, name-sorted. */
  protected readonly recipients = computed<CharacterListItem[]>(() =>
    (this.charactersQuery.data() ?? [])
      .filter((c) => c.id !== this.fromCharacterId())
      .sort((a, b) => a.name.localeCompare(b.name)),
  );

  protected readonly effectiveToCharacterId = computed(() => {
    const picked = this.toCharacterId();
    if (picked && this.recipients().some((r) => r.id === picked)) return picked;
    return this.recipients()[0]?.id ?? '';
  });

  protected readonly hasSender = computed(() => this.senders().length > 0);
  protected readonly hasRecipient = computed(() => this.recipients().length > 0);
  protected readonly recipientsLoading = computed(() => this.charactersQuery.isLoading());

  /**
   * The sender's own postbox — the "In reply to" options. The character id is in
   * the query key, so switching sender refetches (v4 `:119-127`).
   */
  private readonly mailboxQuery = injectQuery(() => ({
    queryKey: mailboxKeys.byCharacter(this.chatId(), this.fromCharacterId()),
    enabled: !!this.fromCharacterId(),
    queryFn: () => fetchMailbox(this.core, this.chatId(), this.fromCharacterId()),
  }));
  protected readonly letters = computed(() => this.mailboxQuery.data() ?? []);
  protected readonly mailboxLoading = computed(() => this.mailboxQuery.isLoading());

  /** v4 `formatDate(l.sentAt, { includeYear: false })` (`:279`). */
  protected sentOn(sentAt: string): string {
    return formatDate(sentAt, { includeYear: false });
  }

  protected readonly canSend = computed(
    () =>
      !this.isSending() &&
      this.hasSender() &&
      this.hasRecipient() &&
      !!this.fromCharacterId() &&
      !!this.effectiveToCharacterId() &&
      this.body().trim().length > 0,
  );

  /** v4 `handleFromChange` (`:159-164`) — a different sender means a different postbox. */
  protected onFromChange(id: string): void {
    this.fromOverride.set(id);
    this.inReplyToPath.set(NO_REPLY);
  }

  protected onDialogClose(): void {
    if (this.isSending()) return;
    this.close.emit();
  }

  /** v4 `handleSend` + the mutation's `onSuccess`/`onError` (`:130-183`). */
  protected async onSend(): Promise<void> {
    if (!this.canSend()) return;
    this.errorMessage.set(null);
    this.isSending.set(true);
    try {
      await sendMail(this.core, {
        chatId: this.chatId(),
        fromCharacterId: this.fromCharacterId(),
        toCharacterId: this.effectiveToCharacterId(),
        bodyMarkdown: this.body().trim(),
        inReplyToPath: this.inReplyToPath() === NO_REPLY ? null : this.inReplyToPath(),
      });
      this.posted.emit();
      this.close.emit();
    } catch (err) {
      this.errorMessage.set(
        (err instanceof Error && err.message) || 'The letter could not be posted.',
      );
    } finally {
      this.isSending.set(false);
    }
  }
}
