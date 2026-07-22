import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  OnInit,
} from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';

import { WORKSPACE_HANDLE, WORKSPACE_TAB_ID } from '../../workspace/workspace-contract';

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
 * chat (or, for an autonomous room, to the settings management list).
 *
 * **Workspace hosting (P4.d16 tier 2).** v4 opens New Chat as a MODAL; v5 never
 * ported it (the standing no-modal divergence), so this screen is what the
 * `salon-new` tab hosts. The three seeds arrive as inputs instead of query
 * params, and — like v4's modal dismissal — Back / Cancel / a completed create
 * CLOSE the tab (the `useCloseSelfTab` idiom). Routed mode is unchanged.
 *
 * The seeds are read in `ngOnInit`, not at field-init: a hosted component's
 * inputs are set after construction, so the state must be built once they have
 * landed (the routed path is identical — the snapshot is available either way).
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
        @if (canClose()) {
          <button
            type="button"
            class="mb-4 inline-flex items-center qt-label text-primary transition hover:text-primary/80"
            (click)="closeSelf()"
          >
            ← Back to {{ backLabel() }}
          </button>
        } @else {
          <a
            [routerLink]="backLink()"
            class="mb-4 inline-flex items-center qt-label text-primary transition hover:text-primary/80"
          >
            ← Back to {{ backLabel() }}
          </a>
        }

        <h1 class="mb-6 qt-heading-1">
          {{ autonomousRequested ? 'New Autonomous Room' : 'New Chat' }}
        </h1>

        @if (state.project(); as proj) {
          <div class="mb-6 rounded-lg border qt-border-default qt-bg-card/50 p-4">
            <div class="flex items-center gap-3">
              <div
                class="w-8 h-8 rounded-lg flex items-center justify-center"
                [style.background-color]="proj.color || 'var(--muted)'"
              ></div>
              <div>
                <p class="text-sm qt-text-primary">Creating chat in project</p>
                <p class="font-medium text-foreground">{{ proj.name }}</p>
              </div>
            </div>
          </div>
        }

        @if (state.profiles().length === 0) {
          <div
            class="mb-6 rounded-lg border qt-border-warning/50 qt-bg-warning/10 p-4 qt-text-warning"
          >
            <p class="font-medium">No connection profiles available</p>
            <p class="mt-1 text-sm">
              You need to create a connection profile before starting a chat.
            </p>
          </div>
        }

        @if (state.error(); as err) {
          <div
            class="mb-6 rounded-lg border qt-border-destructive/50 qt-bg-destructive/10 p-4 qt-text-destructive"
          >
            {{ err }}
          </div>
        }

        <qt-new-chat-picker [state]="state" [disabled]="state.creating()" />

        <div class="mt-6">
          <qt-new-chat-form [state]="state" />
        </div>

        <div class="mt-6 flex justify-end gap-3">
          @if (canClose()) {
            <button type="button" class="qt-button qt-button-secondary" (click)="closeSelf()">
              Cancel
            </button>
          } @else {
            <a [routerLink]="backLink()" class="qt-button qt-button-secondary">Cancel</a>
          }
          <button
            type="button"
            (click)="create()"
            [disabled]="!state.canSubmit()"
            class="qt-button-success"
          >
            {{ state.creating() ? 'Creating...' : 'Create Chat' }}
          </button>
        </div>
      }
    </div>

    <qt-green-room-dialog />
  `,
})
export class NewChatPage implements OnInit {
  private readonly core = inject(CoreClient);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly greenRoom = inject(GreenRoomStore);
  /** Workspace-tab seams (P4.d16 tier 2); null ⇒ routed mode. */
  private readonly handle = inject(WORKSPACE_HANDLE, { optional: true });
  private readonly tabId = inject(WORKSPACE_TAB_ID, { optional: true });

  /** Tab-mode seeds (v4's NewChatModal props); they win over the query params. */
  readonly characterId = input<string | null>(null);
  readonly projectId = input<string | null>(null);
  readonly autonomous = input<boolean>(false);

  protected autonomousRequested = false;
  protected state!: NewChatState;

  /** Both seams present ⇒ hosted; back/cancel/create close the tab. */
  protected canClose(): boolean {
    return this.handle != null && this.tabId != null;
  }

  protected closeSelf(): void {
    if (this.handle && this.tabId != null) this.handle.closeTab(this.tabId);
  }

  protected readonly backLink = computed<string>(() => {
    const proj = this.state.project();
    return proj ? `/prospero/${proj.id}` : '/salon';
  });
  protected readonly backLabel = computed<string>(() => this.state.project()?.name ?? 'Chats');

  ngOnInit(): void {
    const params = this.route.snapshot.queryParamMap;
    const characterId = this.characterId() ?? params.get('characterId') ?? undefined;
    const projectId = this.projectId() ?? params.get('projectId') ?? undefined;
    const autonomous = this.autonomous() || params.get('autonomous') === '1';
    this.autonomousRequested = autonomous;
    this.state = new NewChatState(
      this.core,
      {
        initialCharacterId: characterId,
        projectId,
        initialAutonomous: autonomous,
      },
      this.greenRoom,
    );
    void this.state.load();
  }

  protected async create(): Promise<void> {
    const outcome = await this.state.handleCreate();
    if (!outcome) return;
    if (outcome.isAutonomous) {
      // v4 `useNewChat.ts:789`: autonomous rooms land on the settings management
      // list, not the chat (which has no composer to open).
      void this.router.navigate(['/settings'], {
        queryParams: { tab: 'chat', section: 'autonomous-rooms' },
      });
    } else {
      void this.router.navigate(['/salon', outcome.chatId]);
    }
    // Hosted ⇒ the destination's redirect guard funnels that navigation back
    // into a tab, so this one has done its job and closes (v4's modal dismisses
    // on create).
    if (this.canClose()) this.closeSelf();
  }
}
