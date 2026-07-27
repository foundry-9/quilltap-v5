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
import type { MessageDto, ParticipantDetail } from '../core/core-contract';
import { Icon } from '../ui/icon';
import { bulkReattribute, type RoleFilter } from './chat-admin.api';

/**
 * The sentinel v4 uses for "the messages nobody is attributed to" — the
 * operator's own turns (`BulkCharacterReplaceModal.tsx:52`). A `<select>` cannot
 * carry `null`, so the value goes over the wire as a literal string here and is
 * lowered to a real `null` at the request boundary (:75).
 */
export const UNASSIGNED_USER = '__UNASSIGNED__';

/**
 * Bulk Character Replace (v4 `components/chat/BulkCharacterReplaceModal.tsx`,
 * 376 LOC) — re-attribute every message from one participant to another in a
 * single operation, reached from the sidebar's Edit Content section.
 *
 * ## Ported exactly, and why
 *
 * - **The `__UNASSIGNED__` sentinel** (:52,75). The source select can name a
 *   participant OR the operator's own unattributed turns; only the latter is
 *   `null` on the wire, and only the former is `null` when nothing is chosen
 *   yet. v4 keeps the two apart by branching on the SELECTION string, never on
 *   the derived id (:138) — a check on the id alone would refuse a legitimate
 *   unassigned source. The option only appears when such messages exist (:70).
 * - **The affected count** (:103-117) is computed client-side over the loaded
 *   messages, and it gates Submit: zero matches refuses with "No messages match
 *   the selected criteria" (:143). The role comparison upper-cases the message's
 *   own role first, because the client's copy may be lower-case (:115).
 * - **The target list excludes the source** (:120-126) — except when the source
 *   is the unassigned sentinel, where every participant is a legal target — and
 *   a target that becomes the source is cleared (:129-134).
 * - **The empty-chat message** (:224): fewer than two participants AND no
 *   unassigned messages means there is nothing this dialog can do.
 * - **The toast counts** (:172-177) are pluralised separately and the memories
 *   line only appears when memories were actually deleted.
 *
 * Note what this dialog does NOT do: it never re-attributes TO the unassigned
 * sentinel. v4's target select is built from `availableTargets`, which holds only
 * real participants, so `targetParticipantId` is always an id — even though the
 * lowering at :76 would allow a `null` that the schema forbids.
 *
 * **v5 substitutions:** v4's own overlay markup (this modal is hand-rolled, not
 * `BaseModal`) is kept as-is; the toasts become the parent's inline line, and the
 * avatars v4 renders beside each option are not reproduced — a `<option>` cannot
 * carry an image in either app, and v4's `Avatar` import there is unused.
 */
@Component({
  selector: 'qt-bulk-character-replace-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-dialog-overlay p-4" (click)="onBackdrop($event)">
      <div class="qt-dialog max-w-lg" role="dialog" aria-modal="true">
        <div class="qt-dialog-header flex items-center justify-between">
          <h2 class="qt-dialog-title">Bulk Character Replace</h2>
          <button
            type="button"
            class="qt-button qt-button-ghost p-2"
            aria-label="Close"
            [disabled]="submitting()"
            (click)="close.emit()"
          >
            <qt-icon name="close" class="w-6 h-6" />
          </button>
        </div>

        <div class="qt-dialog-body space-y-5">
          @if (participants().length < 2 && !hasUnassignedMessages()) {
            <div class="text-center py-8 qt-text-secondary">
              This chat needs at least 2 participants to use bulk character replace.
            </div>
          } @else {
            <div class="space-y-2">
              <label class="block qt-label" for="qt-bulk-source">Re-attribute from:</label>
              <select
                id="qt-bulk-source"
                class="qt-select w-full"
                [value]="source()"
                [disabled]="submitting()"
                (change)="onSourceChange($any($event.target).value)"
              >
                <option value="">Select participant...</option>
                @if (hasUnassignedMessages()) {
                  <option [value]="unassigned">Unassigned (You)</option>
                }
                @for (p of participants(); track p.id) {
                  <option [value]="p.id">{{ label(p) }}</option>
                }
              </select>
            </div>

            <div class="space-y-2">
              <label class="block qt-label" for="qt-bulk-target">Re-attribute to:</label>
              <select
                id="qt-bulk-target"
                class="qt-select w-full"
                [value]="target()"
                [disabled]="submitting() || !source()"
                (change)="target.set($any($event.target).value)"
              >
                <option value="">Select participant...</option>
                @for (p of availableTargets(); track p.id) {
                  <option [value]="p.id">{{ label(p) }}</option>
                }
              </select>
            </div>

            <div class="space-y-2">
              <span class="block qt-label">Which messages?</span>
              <div class="space-y-2">
                @for (option of roleOptions; track option.value) {
                  <label class="flex items-center gap-2 cursor-pointer">
                    <input
                      type="radio"
                      name="roleFilter"
                      class="qt-radio"
                      [value]="option.value"
                      [checked]="roleFilter() === option.value"
                      [disabled]="submitting()"
                      (change)="roleFilter.set(option.value)"
                    />
                    <span class="text-sm">{{ option.label }}</span>
                  </label>
                }
              </div>
            </div>

            @if (source() && target()) {
              <div class="qt-alert qt-alert-warning">
                <div class="flex items-start gap-3">
                  <qt-icon name="alert-triangle" class="w-5 h-5 flex-shrink-0 mt-0.5" />
                  <div class="text-sm">
                    <p class="font-medium">
                      {{ affectedCount() }}
                      {{ affectedCount() === 1 ? 'message' : 'messages' }} will be re-attributed
                    </p>
                    <p class="mt-1 opacity-80">
                      Memories extracted from these messages will be permanently deleted.
                    </p>
                  </div>
                </div>
              </div>
            }
          }

          @if (error(); as message) {
            <p class="text-sm qt-text-danger" role="status">{{ message }}</p>
          }
        </div>

        <div class="qt-dialog-footer flex justify-end gap-2">
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="submitting()"
            (click)="close.emit()"
          >
            Cancel
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="submitDisabled()"
            (click)="onSubmit()"
          >
            {{ submitting() ? 'Re-attributing...' : 'Re-attribute Messages' }}
          </button>
        </div>
      </div>
    </div>
  `,
})
export class BulkCharacterReplaceModal {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  readonly participants = input.required<ParticipantDetail[]>();
  readonly messages = input.required<MessageDto[]>();

  /** v4 `onSuccess()` — the parent refetches and reports the counts. */
  readonly reattributed = output<string>();
  readonly close = output<void>();

  protected readonly unassigned = UNASSIGNED_USER;
  protected readonly roleOptions: { value: RoleFilter; label: string }[] = [
    { value: 'ASSISTANT', label: 'AI responses only' },
    { value: 'USER', label: 'User messages only' },
    { value: 'both', label: 'All messages' },
  ];

  /** '' = nothing chosen; the sentinel = the operator's own turns. */
  protected readonly source = signal('');
  protected readonly target = signal('');
  protected readonly roleFilter = signal<RoleFilter>('both');
  protected readonly submitting = signal(false);
  protected readonly error = signal<string | null>(null);

  /** v4 `hasUnassignedMessages` (:70) — gates the sentinel option. */
  protected readonly hasUnassignedMessages = computed(() =>
    this.messages().some((m) => m.participantId === null || m.participantId === undefined),
  );

  /** v4 `affectedCount` (:103-117). */
  protected readonly affectedCount = computed(() => {
    const source = this.source();
    if (!source) return 0;
    const role = this.roleFilter();
    return this.messages().filter((m) => {
      if (source === UNASSIGNED_USER) {
        if (m.participantId !== null && m.participantId !== undefined) return false;
      } else if (m.participantId !== source) {
        return false;
      }
      if (role === 'both') return true;
      return m.role.toUpperCase() === role;
    }).length;
  });

  /** v4 `availableTargets` (:120-126). */
  protected readonly availableTargets = computed(() => {
    const source = this.source();
    if (source === UNASSIGNED_USER) return this.participants();
    return this.participants().filter((p) => p.id !== source);
  });

  protected readonly submitDisabled = computed(
    () =>
      this.submitting() ||
      !this.source() ||
      !this.target() ||
      this.affectedCount() === 0 ||
      (this.participants().length < 2 && !this.hasUnassignedMessages()),
  );

  /** v4 `{name} ({controlLabel})` (:245,263). */
  protected label(p: ParticipantDetail): string {
    const name = p.character?.name || 'Unknown';
    return `${name} (${p.controlledBy === 'user' ? 'User-controlled' : 'Character'})`;
  }

  /** v4 clears a target that has become the source (:129-134). */
  protected onSourceChange(value: string): void {
    this.source.set(value);
    if (this.target() && this.target() === value) {
      this.target.set('');
    }
  }

  protected onBackdrop(event: MouseEvent): void {
    // v4 `useClickOutside`, which ignores the click while a request is in flight
    // (:89-100).
    if (event.target !== event.currentTarget || this.submitting()) return;
    this.close.emit();
  }

  /** v4 `handleSubmit` (:136-188). */
  protected async onSubmit(): Promise<void> {
    if (!this.source() || !this.target()) {
      this.error.set('Please select both source and target participants');
      return;
    }
    if (this.affectedCount() === 0) {
      this.error.set('No messages match the selected criteria');
      return;
    }

    this.error.set(null);
    this.submitting.set(true);
    try {
      const result = await bulkReattribute(this.core, {
        chatId: this.chatId(),
        // The sentinel is the ONLY thing that becomes a real null (:75).
        sourceParticipantId: this.source() === UNASSIGNED_USER ? null : this.source(),
        targetParticipantId: this.target(),
        roleFilter: this.roleFilter(),
      });

      const targetName =
        this.participants().find((p) => p.id === this.target())?.character?.name || 'participant';
      let message =
        `${result.messagesUpdated} ` +
        `${result.messagesUpdated === 1 ? 'message' : 'messages'} re-attributed to ${targetName}.`;
      if (result.memoriesDeleted > 0) {
        message +=
          ` ${result.memoriesDeleted} ` +
          `${result.memoriesDeleted === 1 ? 'memory' : 'memories'} deleted.`;
      }
      this.reattributed.emit(message);
      this.close.emit();
    } catch (err) {
      this.error.set(
        err instanceof Error ? err.message : 'Failed to re-attribute messages',
      );
    } finally {
      this.submitting.set(false);
    }
  }
}
