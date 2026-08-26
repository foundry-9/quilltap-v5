import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { ChatCreateOutfitSelectionInput, EnrichedChatSummary } from '../core/core-contract';
import {
  OutfitSelector,
  type PreviousOutfitSummary,
} from '../screens/new-chat/outfit-selector';
import { chatKeys } from './chat-keys';
import { formatRelativeDate } from '../shared/format-date';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';
import { mergeConversation, readOutfitSummary } from './chat-admin.api';

/** v4 `presentCharacters` (`MergeConversationModal.tsx:63-74`). */
export function presentCharacters(chat: EnrichedChatSummary): { id: string; name: string }[] {
  const seen = new Set<string>();
  const out: { id: string; name: string }[] = [];
  for (const p of chat.participants) {
    if (p.type !== 'CHARACTER' || !p.character) continue;
    if (p.status === 'removed') continue;
    if (seen.has(p.character.id)) continue;
    seen.add(p.character.id);
    out.push({ id: p.character.id, name: p.character.name });
  }
  return out;
}

/**
 * Merge a Conversation In (v4 `components/chat/MergeConversationModal.tsx`,
 * 373 LOC) — the Organize drawer's "Merge In…", the inverse of Continue
 * Elsewhere. Two steps: pick a source conversation, then choose who comes across
 * and what they arrive wearing.
 *
 * ## Ported exactly, and why
 *
 * - **Two filters on the candidate list** (`:114-125`): not this chat, and never
 *   an autonomous room. The search then matches the title OR any present
 *   character's name.
 * - **`presentCharacters` de-duplicates and drops the removed** (`:63-74`): a
 *   character who left the source conversation is not brought back by merging it.
 * - **Everyone eligible is checked by default** (`:151-152`); the operator
 *   unchecks to exclude, and merging with nobody checked is refused with v4's
 *   own italic line (`:310-314`).
 * - **The outfit choices are filtered to the included set before sending**
 *   (`:179`) — an unchecked character's staged choice must not travel.
 * - **The merged count comes off the reply** (`:187`), falling back to the
 *   client's own count, and both singular and plural forms are v4's.
 * - **v4 mounts this only while open** (`SalonView.tsx:1586-1597`), which is why
 *   it has no reset effect and why v5 does the same.
 * - The dialog is v4's own overlay, and only a click on the scrim itself closes
 *   it (`:208-210`) — never a click that started inside the card.
 *
 * ⚠ **Step 2's outfit summary is P4.9E3B's `chatOutfitSummary`.** Until it lands,
 * the read fails and the selector falls back to no summary, which is exactly
 * what v4 renders before its own query resolves; the "Same as last conversation"
 * option still leads the list, because that is the mode v4 seeds a continuation
 * with regardless of whether the preview has arrived.
 */
@Component({
  selector: 'qt-merge-conversation-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, OutfitSelector],
  template: `
    <div class="qt-dialog-overlay p-4" (click)="onBackdrop($event)">
      <div class="qt-dialog max-w-2xl max-h-[90vh] flex flex-col" role="dialog" aria-modal="true">
        <div class="qt-dialog-header flex items-center justify-between">
          <h2 class="qt-dialog-title">
            {{ step() === 'pick' ? 'Merge a Conversation In' : 'Merge “' + sourceTitle() + '”' }}
          </h2>
          <button
            type="button"
            class="qt-button qt-button-ghost p-2"
            aria-label="Close"
            [disabled]="merging()"
            (click)="onClose()"
          >
            <qt-icon name="close" class="w-6 h-6" />
          </button>
        </div>

        <div class="qt-dialog-body flex-1 overflow-y-auto">
          @if (step() === 'pick') {
            <div class="space-y-3">
              <p class="text-sm qt-text-secondary">
                Choose a conversation to fold into this one. Its characters and a recap of where
                it left off will be brought in at the latest point here.
              </p>

              <input
                type="text"
                class="qt-input"
                placeholder="Search conversations…"
                aria-label="Search conversations"
                [value]="search()"
                (input)="search.set($any($event.target).value)"
              />

              @if (chatsQuery.isLoading()) {
                <div class="py-12 text-center qt-text-secondary">Gathering conversations…</div>
              } @else if (candidates().length === 0) {
                <div class="py-12 text-center qt-text-secondary">
                  {{ search() ? 'No matching conversations.' : 'No other conversations to merge in.' }}
                </div>
              } @else {
                <div class="space-y-2">
                  @for (chat of candidates(); track chat.id) {
                    <button
                      type="button"
                      class="w-full rounded-lg border qt-border-default p-3 text-left transition hover:qt-border-primary/50 hover:qt-bg-muted/50"
                      (click)="pick(chat)"
                    >
                      <div class="flex items-baseline justify-between gap-3">
                        <span class="font-semibold qt-text-primary truncate">
                          {{ chat.title || 'Untitled conversation' }}
                        </span>
                        <span class="text-xs qt-text-secondary shrink-0">
                          {{ when(chat) }}
                        </span>
                      </div>
                      <div class="mt-1 text-xs qt-text-secondary truncate">
                        {{ company(chat) }}
                      </div>
                    </button>
                  }
                </div>
              }
            </div>
          } @else {
            <div class="space-y-4">
              @if (incoming().length === 0) {
                <div class="py-8 text-center qt-text-secondary">
                  Everyone from that conversation is already here — nothing to merge.
                </div>
              } @else {
                <p class="text-sm qt-text-secondary">
                  Choose who joins this conversation as LLM-driven participants. A recap of
                  “{{ sourceTitle() }}” will be added at the latest point.
                </p>

                <div>
                  <span class="mb-2 block text-sm qt-text-primary">Who joins</span>
                  <div class="space-y-1.5">
                    @for (c of incoming(); track c.id) {
                      <label
                        class="flex items-center gap-2 rounded px-2 py-1.5 text-sm transition cursor-pointer hover:qt-bg-muted/40"
                      >
                        <input
                          type="checkbox"
                          class="accent-[var(--primary)]"
                          [attr.aria-label]="c.name"
                          [checked]="included().has(c.id)"
                          [disabled]="merging()"
                          (change)="toggle(c.id)"
                        />
                        <span class="qt-text-primary">{{ c.name }}</span>
                      </label>
                    }
                  </div>
                  @if (mergeCharacters().length === 0) {
                    <p class="mt-2 text-xs qt-text-warning italic">
                      Select at least one character to merge in.
                    </p>
                  }
                </div>

                @if (mergeCharacters().length > 0) {
                  @if (outfitQuery.isLoading()) {
                    <div class="py-4 text-center qt-text-secondary">Reading their wardrobes…</div>
                  } @else {
                    <qt-outfit-selector
                      [characters]="mergeCharacters()"
                      [disabled]="merging()"
                      [sourceChatId]="sourceId()"
                      [previousOutfitSummary]="outfitSummary()"
                      (selectionsChange)="outfitSelections.set($event)"
                    />
                  }
                }
              }

            </div>
          }
        </div>

        <div class="qt-dialog-footer flex items-center justify-between">
          @if (step() === 'configure') {
            <button
              type="button"
              class="qt-button qt-button-ghost"
              [disabled]="merging()"
              (click)="step.set('pick')"
            >
              ← Back
            </button>
          } @else {
            <span></span>
          }
          <div class="flex items-center gap-2">
            <button
              type="button"
              class="qt-button qt-button-ghost"
              [disabled]="merging()"
              (click)="onClose()"
            >
              Cancel
            </button>
            @if (step() === 'configure') {
              <button
                type="button"
                class="qt-button qt-button-primary"
                [disabled]="merging() || mergeCharacters().length === 0"
                (click)="onMerge()"
              >
                {{ mergeLabel() }}
              </button>
            }
          </div>
        </div>
      </div>
    </div>
  `,
})
export class MergeConversationModal {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  /** The chat being merged INTO. */
  readonly targetChatId = input.required<string>();
  /** Character ids already present here, at any status. */
  readonly existingCharacterIds = input.required<string[]>();

  /** v4 `onMerged()` — the parent refetches; the sentence is v4's toast copy. */
  readonly merged = output<void>();
  readonly close = output<void>();

  protected readonly step = signal<'pick' | 'configure'>('pick');
  protected readonly search = signal('');
  protected readonly merging = signal(false);
  protected readonly outfitSelections = signal<ChatCreateOutfitSelectionInput[]>([]);
  private readonly source = signal<EnrichedChatSummary | null>(null);
  private readonly includedIds = signal<Set<string>>(new Set());

  protected readonly sourceId = computed(() => this.source()?.id ?? null);
  protected readonly sourceTitle = computed(() => this.source()?.title ?? '');
  protected readonly included = computed(() => this.includedIds());

  /** v4's list query — the same one the Salon list uses, excluding autonomous. */
  protected readonly chatsQuery = injectQuery(() => ({
    queryKey: [...chatKeys.all, { includeAutonomous: false }],
    queryFn: async (): Promise<EnrichedChatSummary[]> => {
      const resp = await this.core.dispatchExpect({ type: 'listChats' }, 'chats');
      return resp.data;
    },
  }));

  /** Step 2's equipped-outfit read (P4.9E3B's `chatOutfitSummary`). */
  protected readonly outfitQuery = injectQuery(() => ({
    queryKey: ['chat', this.sourceId() ?? 'none', 'outfit-summary'],
    queryFn: () => readOutfitSummary(this.core, this.sourceId()!),
    enabled: this.step() === 'configure' && !!this.sourceId(),
  }));
  protected readonly outfitSummary = computed<PreviousOutfitSummary | null>(
    () => this.outfitQuery.data() ?? null,
  );

  /** v4 `candidateChats` (:114-125). */
  protected readonly candidates = computed(() => {
    const rows = (this.chatsQuery.data() ?? []).filter(
      (c) => c.id !== this.targetChatId() && c.chatType !== 'autonomous',
    );
    const needle = this.search().trim().toLowerCase();
    if (!needle) return rows;
    return rows.filter(
      (c) =>
        c.title.toLowerCase().includes(needle) ||
        presentCharacters(c).some((ch) => ch.name.toLowerCase().includes(needle)),
    );
  });

  /** v4 `incomingCharacters` (:128-132). */
  protected readonly incoming = computed(() => {
    const src = this.source();
    if (!src) return [];
    const here = new Set(this.existingCharacterIds());
    return presentCharacters(src).filter((c) => !here.has(c.id));
  });

  /** v4 `mergeCharacters` (:135-138) — the gate. */
  protected readonly mergeCharacters = computed(() =>
    this.incoming().filter((c) => this.includedIds().has(c.id)),
  );

  protected readonly mergeLabel = computed(() => {
    if (this.merging()) return 'Merging…';
    const n = this.mergeCharacters().length;
    return n > 1 ? `Merge In (${n})` : 'Merge In';
  });

  protected when(chat: EnrichedChatSummary): string {
    return formatRelativeDate(chat.lastMessageAt ?? chat.updatedAt);
  }

  protected company(chat: EnrichedChatSummary): string {
    const names = presentCharacters(chat).map((c) => c.name);
    return names.length > 0 ? names.join(', ') : 'No characters';
  }

  /** v4 `handlePick` (:145-156) — seeds the included set to everyone eligible. */
  protected pick(chat: EnrichedChatSummary): void {
    const here = new Set(this.existingCharacterIds());
    const eligible = presentCharacters(chat).filter((c) => !here.has(c.id));
    this.source.set(chat);
    this.outfitSelections.set([]);
    this.includedIds.set(new Set(eligible.map((c) => c.id)));
    this.step.set('configure');
  }

  protected toggle(characterId: string): void {
    this.includedIds.update((prev) => {
      const next = new Set(prev);
      if (next.has(characterId)) next.delete(characterId);
      else next.add(characterId);
      return next;
    });
  }

  protected onBackdrop(event: MouseEvent): void {
    if (event.target !== event.currentTarget) return;
    this.onClose();
  }

  /** v4 `handleClose` (:140-143) — a merge in flight is not interruptible. */
  protected onClose(): void {
    if (this.merging()) return;
    this.close.emit();
  }

  /** v4 `handleMerge` (:167-201). */
  protected async onMerge(): Promise<void> {
    const src = this.source();
    const characters = this.mergeCharacters();
    if (!src || characters.length === 0) return;

    const includedIds = new Set(characters.map((c) => c.id));
    this.merging.set(true);
    try {
      const count = await mergeConversation(this.core, {
        chatId: this.targetChatId(),
        sourceChatId: src.id,
        characterIds: characters.map((c) => c.id),
        // Only the choices for characters actually coming across (:179).
        outfitSelections: this.outfitSelections().filter((s) => includedIds.has(s.characterId)),
      });
      const merged = count ?? characters.length;
      this.toasts.showSuccess(
        merged === 1
          ? `Merged 1 character from “${src.title}”`
          : `Merged ${merged} characters from “${src.title}”`,
      );
      this.merged.emit();
      await this.queryClient.invalidateQueries({ queryKey: chatKeys.all });
      this.close.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to merge conversation');
    } finally {
      this.merging.set(false);
    }
  }
}
