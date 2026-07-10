import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';

import { Icon } from './icon';

/**
 * Copy a conversation's UUID to the clipboard (v4 `CopyChatIdButton`, inline
 * variant). Copies `chatId` verbatim; the icon flips copy→check and the tooltip
 * shows "Copied!" on success. Microcopy is v4-verbatim.
 */
@Component({
  selector: 'qt-copy-chat-id-button',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <button
      type="button"
      class="flex-shrink-0 inline-flex items-center justify-center qt-text-muted hover:text-foreground transition-colors"
      aria-label="Copy conversation ID"
      [title]="tooltip()"
      (click)="copy()"
    >
      <qt-icon [name]="copied() ? 'check' : 'copy'" class="w-4 h-4" />
    </button>
  `,
})
export class CopyChatIdButton {
  readonly chatId = input.required<string>();

  protected readonly copied = signal(false);

  protected readonly tooltip = computed(() =>
    this.copied() ? 'Copied!' : `Copy this conversation's ID (${this.chatId()}) to the clipboard`,
  );

  protected copy(): void {
    void navigator.clipboard?.writeText(this.chatId()).then(() => {
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 1500);
    });
  }
}
