import { ChangeDetectionStrategy, Component, computed } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';

/**
 * The Auto-Scroll card (v4 `components/settings/chat-settings/
 * AutoScrollSettings.tsx`): whether the Salon follows a completed reply down to
 * its last word. Writes the `autoScrollOnResponseComplete` scalar; v4's default
 * when unset is **false**.
 *
 * Already consumed in v5 (`chat/message-list.ts:211` + `chat/auto-scroll.ts`) —
 * this card is the only piece that was missing. Copy carries over verbatim.
 */
@Component({
  selector: 'qt-auto-scroll-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading auto-scroll setting…</p>
    } @else {
      <div>
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <label class="qt-settings-toggle-row">
          <input
            type="checkbox"
            class="qt-checkbox mt-1"
            [checked]="enabled()"
            [disabled]="saving()"
            (change)="onChange($any($event.target).checked)"
          />
          <div class="flex-1">
            <div class="qt-settings-section-heading">Chase each reply to its end</div>
            <div class="qt-text-small mt-1">
              When the parlour falls quiet at the close of a reply, this whisks you down to the very
              last word. Left unchecked (the default), the Salon holds its position so a lengthy
              soliloquy can't spirit your place away mid-read; a discreet &ldquo;jump to
              latest&rdquo; button appears whenever you've wandered up the page. Sending a message of
              your own, and first opening a chat, always settle you at the bottom regardless.
            </div>
          </div>
        </label>
      </div>
    }
  `,
})
export class AutoScrollSettings extends ChatSettingsCard {
  /** v4 `settings.autoScrollOnResponseComplete ?? false`. */
  protected readonly enabled = computed(
    () => (this.settings()?.['autoScrollOnResponseComplete'] as boolean | undefined) ?? false,
  );

  protected async onChange(value: boolean): Promise<void> {
    await this.save(
      { autoScrollOnResponseComplete: value },
      'Failed to update the auto-scroll setting',
    );
  }
}
