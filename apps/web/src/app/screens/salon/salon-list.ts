import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { ChatSettingsDto, EnrichedChatSummary } from '../../core/core-contract';
import { ErrorAlert } from '../../ui/error-alert';
import { Icon } from '../../ui/icon';
import { LoadingState } from '../../ui/loading-state';
import { ChatCard } from './chat-card';
import {
  effectiveInclude,
  hasHiddenAutonomous,
  readIncludeAutonomous,
  writeIncludeAutonomous,
} from './autonomous-visibility';

/**
 * The Salon list screen (v4 `app/salon/SalonListView.tsx`) — the enriched
 * `listChats` response rendered as `ChatCard`s. Reads through TanStack Query so
 * a `chatSend` / delete elsewhere invalidates and refetches. Microcopy is
 * v4-verbatim ("Chats", "No chats yet", "Start a new chat").
 *
 * The include-autonomous toggle mirrors v4's user-menu control (see
 * `autonomous-visibility.ts` for the placement divergence): the effective
 * include is the user's visibility default OR the header toggle; the hidden-room
 * hint + the cheap room probe run only when rooms are actually excluded. Other
 * client-side quick-hide filters (dangerous-chat / tag hide) remain deferred.
 */
@Component({
  selector: 'qt-salon-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, LoadingState, ErrorAlert, ChatCard, Icon],
  template: `
    <div class="chat-page qt-page-container p-6">
      <div class="flex items-center justify-between gap-4 mb-4">
        <h1 class="qt-heading-1 leading-tight">Chats</h1>
        <div class="flex flex-wrap items-center gap-3">
          <button
            type="button"
            class="qt-button qt-button-secondary inline-flex items-center gap-1.5"
            title="Show autonomous character-to-character rooms in the Salon chat list"
            [attr.aria-pressed]="includeAutonomous()"
            (click)="toggleAutonomous()"
          >
            <span class="text-sm">Show Autonomous Rooms</span>
            <qt-icon [name]="includeAutonomous() ? 'eye' : 'eye-off'" class="w-4 h-4" />
          </button>
          <a
            routerLink="/salon/new"
            [queryParams]="{ autonomous: 1 }"
            class="qt-button qt-button-secondary"
            title="Create an autonomous character-to-character room"
            >New Autonomous Room</a
          >
          <a routerLink="/salon/new" class="qt-button-success">New Chat</a>
        </div>
      </div>

      @if (showHiddenHint()) {
        <div
          class="mb-6 rounded-lg border qt-border-default qt-bg-muted/40 px-4 py-3 qt-text-secondary qt-body-sm"
        >
          You have autonomous rooms hidden by your visibility default. Toggle
          <em>Show Autonomous Rooms</em> to include them.
        </div>
      }

      @if (chats.isPending()) {
        <qt-loading-state message="Loading chats..." />
      } @else if (chats.isError()) {
        <qt-error-alert
          [message]="'Error: ' + errorMessage()"
          [retryable]="true"
          (retry)="chats.refetch()"
        />
      } @else if (visibleChats().length === 0) {
        <div
          class="chat-empty-state mt-12 rounded-2xl border border-dashed px-8 py-12 text-center"
        >
          <p class="mb-4 text-lg qt-text-small">No chats yet</p>
          <a routerLink="/salon/new" class="font-medium text-primary hover:text-primary/80"
            >Start a new chat</a
          >
        </div>
      } @else {
        <div class="chat-card-stack space-y-4">
          @for (chat of visibleChats(); track chat.id) {
            <qt-chat-card [chat]="chat" />
          }
        </div>
      }
    </div>
  `,
})
export class SalonList {
  private readonly core = inject(CoreClient);

  /** The header toggle (persisted to the shared quick-hide localStorage key). */
  protected readonly includeAutonomous = signal(readIncludeAutonomous());

  private readonly chatSettings = injectQuery(() => ({
    queryKey: ['chatSettings'],
    queryFn: async (): Promise<ChatSettingsDto> => {
      const resp = await this.core.dispatchExpect({ type: 'chatSettings' }, 'chatSettings');
      return resp.data;
    },
  }));

  /** The user's room-visibility default (v4 `autonomousRoomSettings.visibilityDefault`). */
  private readonly visibilityDefault = computed<string | undefined>(() => {
    const ar = this.chatSettings.data()?.['autonomousRoomSettings'] as
      | { visibilityDefault?: string }
      | undefined;
    return ar?.visibilityDefault;
  });

  /** v4 `effectiveIncludeAutonomous = wantsAutonomousByDefault || toggle`. */
  protected readonly effectiveIncludeAutonomous = computed(() =>
    effectiveInclude(this.visibilityDefault(), this.includeAutonomous()),
  );

  protected readonly chats = injectQuery(() => ({
    // The flag rides the query key so flipping the toggle refetches.
    queryKey: ['chats', { includeAutonomous: this.effectiveIncludeAutonomous() }],
    queryFn: async (): Promise<EnrichedChatSummary[]> => {
      const resp = await this.core.dispatchExpect(
        this.effectiveIncludeAutonomous()
          ? { type: 'listChats', includeAutonomous: true }
          : { type: 'listChats' },
        'chats',
      );
      return resp.data;
    },
  }));

  /**
   * The cheap "do I own any autonomous rooms?" probe (v4 GET
   * `/system/autonomous-rooms`), fired ONLY when excluding — for the hint.
   */
  private readonly autonomousRooms = injectQuery(() => ({
    queryKey: ['systemAutonomousRooms'],
    enabled: !this.effectiveIncludeAutonomous(),
    queryFn: () => this.core.listAutonomousRooms(),
  }));

  protected readonly showHiddenHint = computed(() =>
    hasHiddenAutonomous(this.effectiveIncludeAutonomous(), this.autonomousRooms.data()?.length ?? 0),
  );

  protected toggleAutonomous(): void {
    this.includeAutonomous.update((v) => {
      const next = !v;
      writeIncludeAutonomous(next);
      return next;
    });
  }

  protected visibleChats(): EnrichedChatSummary[] {
    return this.chats.data() ?? [];
  }

  protected errorMessage(): string {
    const err = this.chats.error();
    return err instanceof Error ? err.message : 'Failed to load chats.';
  }
}
