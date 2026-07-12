import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';

import { CoreClient } from '../../core/core-client';
import { LoadingState } from '../../ui/loading-state';
import { CharacterPickerPanel } from './character-picker-panel';
import { GreenRoomDialog } from './green-room-dialog';
import { GreenRoomStore } from './green-room.state';
import { NewChatForm } from './new-chat-form';
import { NewChatState } from './new-chat.state';

/**
 * The New-Chat page (v4 `app/salon/new/page.tsx`). Reads `?projectId=`,
 * `?characterId=`, and `?autonomous=1`; composes the character picker, the form,
 * the submit spine, and the Green Room dialog; ends in navigation to the created
 * chat. Autonomous mode is deferred — `?autonomous=1` surfaces a loud
 * not-yet-available notice and the page proceeds as an ordinary new chat.
 */
@Component({
  selector: 'qt-new-chat-page',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, LoadingState, CharacterPickerPanel, NewChatForm, GreenRoomDialog],
  template: `
    <div class="qt-page-container min-h-screen text-foreground p-6">
      @if (state.loading()) {
        <qt-loading-state message="Loading..." />
      } @else {
        <a [routerLink]="backLink()" class="mb-4 inline-flex items-center qt-label text-primary transition hover:text-primary/80">
          ← Back to {{ backLabel() }}
        </a>

        <h1 class="mb-6 qt-heading-1">New Chat</h1>

        @if (autonomousRequested) {
          <div class="mb-6 rounded-lg border qt-border-warning/50 qt-bg-warning/10 p-4 qt-text-warning">
            <p class="font-medium">Autonomous rooms are not yet available</p>
            <p class="mt-1 text-sm">
              This build cannot create autonomous rooms yet. You can start an ordinary chat below.
            </p>
          </div>
        }

        @if (state.project(); as proj) {
          <div class="mb-6 rounded-lg border qt-border-default qt-bg-card/50 p-4">
            <div class="flex items-center gap-3">
              <div class="w-8 h-8 rounded-lg flex items-center justify-center" [style.background-color]="proj.color || 'var(--muted)'"></div>
              <div>
                <p class="text-sm qt-text-primary">Creating chat in project</p>
                <p class="font-medium text-foreground">{{ proj.name }}</p>
              </div>
            </div>
          </div>
        }

        @if (state.profiles().length === 0) {
          <div class="mb-6 rounded-lg border qt-border-warning/50 qt-bg-warning/10 p-4 qt-text-warning">
            <p class="font-medium">No connection profiles available</p>
            <p class="mt-1 text-sm">You need to create a connection profile before starting a chat.</p>
          </div>
        }

        @if (state.error(); as err) {
          <div class="mb-6 rounded-lg border qt-border-destructive/50 qt-bg-destructive/10 p-4 qt-text-destructive">
            {{ err }}
          </div>
        }

        <qt-new-chat-picker [state]="state" [disabled]="state.creating()" />

        <div class="mt-6">
          <qt-new-chat-form [state]="state" />
        </div>

        <div class="mt-6 flex justify-end gap-3">
          <a [routerLink]="backLink()" class="qt-button qt-button-secondary">Cancel</a>
          <button type="button" (click)="create()" [disabled]="!state.canSubmit()" class="qt-button-success">
            {{ state.creating() ? 'Creating...' : 'Create Chat' }}
          </button>
        </div>
      }
    </div>

    <qt-green-room-dialog />
  `,
})
export class NewChatPage {
  private readonly core = inject(CoreClient);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly greenRoom = inject(GreenRoomStore);

  private readonly params = this.route.snapshot.queryParamMap;
  protected readonly autonomousRequested = this.params.get('autonomous') === '1';

  protected readonly state = new NewChatState(
    this.core,
    {
      initialCharacterId: this.params.get('characterId') ?? undefined,
      projectId: this.params.get('projectId') ?? undefined,
    },
    this.greenRoom,
  );

  protected readonly backLink = computed<string>(() => {
    const proj = this.state.project();
    return proj ? `/prospero/${proj.id}` : '/salon';
  });
  protected readonly backLabel = computed<string>(() => this.state.project()?.name ?? 'Chats');

  constructor() {
    void this.state.load();
  }

  protected async create(): Promise<void> {
    const outcome = await this.state.handleCreate();
    if (outcome) {
      void this.router.navigate(['/salon', outcome.chatId]);
    }
  }
}
