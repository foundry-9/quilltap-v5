import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
} from '@angular/core';
import { Router } from '@angular/router';

import { CoreClient } from '../../core/core-client';
import type { WardrobeWearHistoryResponse } from '../../core/core-contract';
import { formatDate } from '../../shared/format-date';
import { WORKSPACE_HANDLE } from '../../workspace/workspace-contract';
import type { WardrobeContainer } from '../wardrobe-container';
import { formatWornWhen } from '../wear-display';

/**
 * Wear history — the read-only foot of the wardrobe item editor. A port of v4
 * `components/wardrobe/wardrobe-item-editor/WardrobeWearHistorySection.tsx`
 * (`3ee3b1342`, read at the pin `f5e953a3f`).
 *
 * Shows the item's wear-ledger breakdown: when it was created, how many times
 * it has been put on, first and last wear (linking the chat it was last worn
 * in, when that chat still exists), and who has worn it. Fetched when the
 * editor opens. Edit mode only — a new item has no history. For a composite,
 * a one-line note explains that wearing the outfit also credits its garments.
 *
 * **Recorded mechanism divergences.** (1) v4 GETs `${itemUrl}?action=wear-
 * history`; v5 dispatches the one verb `wardrobeItemWearHistory { scope,
 * containerId?, itemId }` (C2 §3) naming the item's container instead of its
 * URL. (2) v4's last-worn chat is a Next `<Link href="/salon/{id}">`; v5 opens
 * chats as workspace tabs, so the anchor keeps v4's `href` (middle-click,
 * copy-link) but a plain click opens the Salon tab — or routes to
 * `/salon/{id}` outside the workspace. (3) v4's `staleTime: 0` query re-reads
 * on every mount; v5 reads on mount and whenever the item or container input
 * changes — the same "always current when opened" promise.
 */
@Component({
  selector: 'qt-wardrobe-wear-history-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section
      [attr.aria-labelledby]="'wear-history-' + itemId()"
      class="border-t qt-border-default pt-4"
    >
      <h3 [id]="'wear-history-' + itemId()" class="qt-label mb-2">Wear history</h3>

      @if (state() === 'loading') {
        <p class="qt-text-xs qt-text-secondary">Consulting the ledger…</p>
      } @else if (state() === 'error') {
        <p class="qt-text-xs qt-text-secondary">The wear ledger could not be read just now.</p>
      } @else {
        <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          @if (createdAt()) {
            <div class="contents">
              <dt class="qt-text-secondary">Created</dt>
              <dd class="text-foreground">{{ formatDate(createdAt()) }}</dd>
            </div>
          }
          @if (history(); as h) {
            <div class="contents">
              <dt class="qt-text-secondary">Times worn</dt>
              <dd class="text-foreground">{{ h.wearCount }}</dd>
            </div>
            @if (h.wearCount > 0) {
              <div class="contents">
                <dt class="qt-text-secondary">First worn</dt>
                <dd class="text-foreground">{{ formatDate(h.firstWornAt) }}</dd>
              </div>
              <div class="contents">
                <dt class="qt-text-secondary">Last worn</dt>
                <dd class="text-foreground">
                  <!-- No whitespace between the date and either arm's comma:
                       Angular keeps a collapsed space there, where v4's JSX
                       writes "Oct 8, 2026, in" (dogfood #154). -->
                  {{ formatDate(h.lastWornAt) }}@if (lastWornChat(); as chat) {, in
                    <a class="qt-link" [href]="'/salon/' + chat.id" (click)="openChat($event, chat.id)"
                      >“{{ chat.title }}”</a
                    >
                  } @else if (h.lastWornChatId) {, in a chat since deleted}
                </dd>
              </div>
            }
            @if (h.wearers.length > 0) {
              <div class="contents">
                <dt class="qt-text-secondary">Worn by</dt>
                <dd>
                  <ul class="space-y-1">
                    @for (w of h.wearers; track w.characterId ?? 'unattributed') {
                      <li class="flex items-center gap-2 text-foreground">
                        @if (avatarOf(w.characterId); as avatar) {
                          <img
                            [src]="avatar"
                            alt=""
                            class="w-5 h-5 rounded-full object-cover qt-bg-muted border qt-border-default flex-shrink-0"
                          />
                        } @else {
                          <span
                            aria-hidden="true"
                            class="w-5 h-5 rounded-full qt-bg-muted border qt-border-default flex-shrink-0"
                          ></span>
                        }
                        <span class="min-w-0 break-words">{{ nameOf(w.characterId) }}</span>
                        <span class="qt-text-xs qt-text-secondary"
                          >{{ w.wearCount }}×, last {{ relative(w.lastWornAt) }}</span
                        >
                      </li>
                    }
                  </ul>
                </dd>
              </div>
            }
          }
        </dl>
        @if (isComposite()) {
          <p class="mt-2 qt-text-xs qt-text-secondary">
            Wearing this outfit also counts a wear for each garment it put on.
          </p>
        }
      }
    </section>
  `,
})
export class WardrobeWearHistorySection {
  private readonly core = inject(CoreClient);
  private readonly router = inject(Router);
  private readonly workspace = inject(WORKSPACE_HANDLE, { optional: true });

  readonly itemId = input.required<string>();
  /** The item's home container (the editor's `itemHomeContainer`). */
  readonly container = input.required<WardrobeContainer>();
  /** The item's `createdAt` (frontmatter). */
  readonly createdAt = input<string | null | undefined>(undefined);
  /** True for an outfit bundle (has components). */
  readonly isComposite = input(false);

  protected readonly state = signal<'loading' | 'error' | 'ready'>('loading');
  private readonly data = signal<WardrobeWearHistoryResponse | null>(null);

  protected readonly history = computed(() => this.data()?.history ?? null);
  protected readonly lastWornChat = computed(() => this.data()?.lastWornChat ?? null);
  private readonly names = computed(
    () => new Map((this.data()?.wearers ?? []).map((w) => [w.characterId ?? '', w] as const)),
  );

  protected readonly formatDate = formatDate;

  constructor() {
    effect(() => {
      const itemId = this.itemId();
      const container = this.container();
      untracked(() => void this.load(itemId, container));
    });
  }

  private async load(itemId: string, container: WardrobeContainer): Promise<void> {
    this.state.set('loading');
    try {
      const body = (await this.core.dispatchData({
        type: 'wardrobeItemWearHistory',
        scope: container.scope,
        ...(container.id ? { containerId: container.id } : {}),
        itemId,
      })) as unknown as WardrobeWearHistoryResponse;
      // A slow answer for an item the editor has since left is dropped.
      if (itemId !== this.itemId()) return;
      this.data.set(body);
      this.state.set('ready');
    } catch {
      if (itemId !== this.itemId()) return;
      this.state.set('error');
    }
  }

  /** v4 `:118-121` — the server's name, else v4's two client labels. */
  protected nameOf(characterId: string | null): string {
    return (
      this.names().get(characterId ?? '')?.name ??
      (characterId === null ? 'Unattributed' : 'A departed character')
    );
  }

  protected avatarOf(characterId: string | null): string | null {
    return this.names().get(characterId ?? '')?.avatarUrl ?? null;
  }

  protected relative(value: string | null | undefined): string {
    return formatWornWhen(value);
  }

  protected openChat(event: MouseEvent, chatId: string): void {
    event.preventDefault();
    if (this.workspace) {
      this.workspace.openTab('salon', { chatId });
      return;
    }
    void this.router.navigate(['/salon', chatId]);
  }
}
