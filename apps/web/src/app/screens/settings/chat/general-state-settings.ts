import { ChangeDetectionStrategy, Component, signal } from '@angular/core';

import { StateEditorModal } from '../../../shared/state/state-editor-modal';

/**
 * The instance-wide "General State" editor entry (v4
 * `components/settings/chat-settings/GeneralStateSettings.tsx`). General state
 * is the bottom tier of the state cascade (chat → project → group → general) —
 * shared by every chat unless a narrower tier overrides a key. It lives beside
 * Pascal's custom-tools card because persistent state is Pascal the Croupier's
 * subsystem.
 */
@Component({
  selector: 'qt-general-state-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [StateEditorModal],
  template: `
    <div class="space-y-3">
      <p class="text-sm qt-text-secondary">
        General state is the instance-wide foundation of the state cascade — every chat sees it
        unless a chat, project, or group sets the same key.
      </p>
      <button type="button" class="qt-button qt-button-secondary" (click)="showModal.set(true)">
        Edit General State
      </button>

      @if (showModal()) {
        <qt-state-editor-modal entityType="general" (close)="showModal.set(false)" />
      }
    </div>
  `,
})
export class GeneralStateSettings {
  protected readonly showModal = signal(false);
}
