import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';

/**
 * The "Start New Chats in Composition Mode" default (v4
 * `components/settings/chat-settings/CompositionModeDefaultSettings.tsx`). The
 * flag is `chat_settings.compositionModeDefault`, baked into each new chat's
 * `documentEditingMode` at creation (already ported server-side); the per-chat
 * toggle lives on the composer toolbar. Reads/writes the chat-settings row
 * through the existing `chatSettings` / `chatSettingsUpdate` dispatch (a partial
 * merge), the same edge the Appearance tab uses. Copy carries over verbatim in
 * v4's register.
 */
@Component({
  selector: 'qt-composition-mode-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading composition-mode setting…</p>
    } @else {
      <div>
        <label
          class="flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer"
        >
          <input
            type="checkbox"
            class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
            [checked]="enabled()"
            [disabled]="saving()"
            (change)="onChange($any($event.target).checked)"
          />
          <div class="flex-1">
            <div class="font-medium text-foreground">Start New Chats in Composition Mode</div>
            <div class="qt-text-small mt-1">
              When enabled, new chats begin in composition mode: Enter inserts a newline and
              Ctrl/Cmd+Enter sends. When disabled, new chats begin in chat mode: Enter sends and
              Shift+Enter inserts a newline. You can still toggle composition mode per-chat from the
              composer toolbar.
            </div>
          </div>
        </label>

        @if (error()) {
          <p class="qt-text-small qt-text-destructive mt-2">{{ error() }}</p>
        }
      </div>
    }
  `,
})
export class CompositionModeSettings {
  private readonly core = inject(CoreClient);

  protected readonly loading = signal(true);
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly enabled = signal(false);

  constructor() {
    void this.load();
  }

  private async load(): Promise<void> {
    try {
      const resp = await this.core.dispatchExpect({ type: 'chatSettings' }, 'chatSettings');
      this.enabled.set(resp.data['compositionModeDefault'] === true);
    } catch {
      this.error.set('Failed to load composition-mode setting');
    } finally {
      this.loading.set(false);
    }
  }

  protected async onChange(value: boolean): Promise<void> {
    this.saving.set(true);
    this.error.set(null);
    const previous = this.enabled();
    this.enabled.set(value);
    try {
      await this.core.dispatchExpect(
        { type: 'chatSettingsUpdate', settings: { compositionModeDefault: value } },
        'chatSettings',
      );
    } catch {
      this.enabled.set(previous);
      this.error.set('Failed to save composition-mode setting');
    } finally {
      this.saving.set(false);
    }
  }
}
