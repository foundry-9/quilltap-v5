/**
 * The Guide's welcome card (port of v4
 * `components/help-chat/HelpWelcomeCard.tsx`).
 *
 * Shown only while the operator has fewer than three chats. Every string here is
 * BYTE-COPIED from v4 and pinned against `__fixtures__/welcome-card.html`, a
 * capture of v4's real component rendered to static markup — the Wodehouse
 * register is not something to paraphrase.
 *
 * @module help/help-welcome-card
 */

import { ChangeDetectionStrategy, Component, output } from '@angular/core';

import { Icon } from '../ui/icon';

/** v4's four `WELCOME_LINKS`, in order. */
export const WELCOME_LINKS: readonly { docId: string; label: string }[] = [
  { docId: 'homepage', label: 'Getting Started with Quilltap' },
  { docId: 'setup-wizard', label: 'AI Stack Setup Wizard' },
  { docId: 'character-creation', label: 'Creating Characters' },
  { docId: 'chats', label: 'Chats Overview' },
];

@Component({
  selector: 'qt-help-welcome-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  host: { class: 'contents' },
  template: `
    <div class="qt-help-welcome-card">
      <div class="qt-help-welcome-title">Welcome to Quilltap</div>
      <p class="qt-help-welcome-text">
        Ah, a fresh arrival! Splendid. Whether you've come to craft characters of uncommon depth or
        simply to see what all the fuss is about, these guides shall serve as your faithful valet
        through the proceedings.
      </p>
      <div class="qt-help-welcome-links">
        @for (link of links; track link.docId) {
          <button
            type="button"
            class="qt-help-welcome-link"
            (click)="openDocument.emit(link.docId)"
          >
            <qt-icon name="chevron-right" class="w-3.5 h-3.5 flex-shrink-0" />
            {{ link.label }}
          </button>
        }
      </div>
      <p class="qt-help-welcome-footer">
        Or browse the topics below, or switch to the <strong>Ask</strong> tab to chat with a help
        character.
      </p>
    </div>
  `,
})
export class HelpWelcomeCard {
  readonly openDocument = output<string>();
  protected readonly links = WELCOME_LINKS;
}
