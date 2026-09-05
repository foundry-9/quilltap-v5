/**
 * The Help rail entry (port of v4
 * `components/layout/left-sidebar/sidebar-footer.tsx:203-212`).
 *
 * A self-contained sidebar-footer button that opens the Help dialog, plus the
 * dialog it hosts — so the shell mounts ONE component (§S.2, the Brahma
 * precedent). v4's footer puts this BEFORE the Brahma Console entry, which is
 * where the unifier mounts it.
 *
 * Eligibility-gated with v4's exact rule and both of its `title` strings: the
 * button is disabled only once eligibility has ANSWERED and said no
 * (`!isEligible && !eligibilityLoading`), so it is live rather than greyed while
 * the first eligibility fetch is still out.
 *
 * @module help/help-entry
 */

import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';

import { Icon } from '../ui/icon';
import { HelpDialog } from './help-dialog';
import { HelpService } from './help.service';

@Component({
  selector: 'qt-help-entry',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, HelpDialog],
  template: `
    <button
      type="button"
      class="qt-collapsed-nav-button"
      [class.opacity-40]="isDisabled()"
      [class.cursor-not-allowed]="isDisabled()"
      [disabled]="isDisabled()"
      [title]="
        isEligible()
          ? 'Help'
          : 'Help (requires a help-enabled character with a tool-capable connection)'
      "
      aria-label="Help"
      (click)="open()"
    >
      <qt-icon name="help" class="w-7 h-7" />
    </button>

    <!-- The Help dialog (renders nothing while closed). Deferred for the same
         reason as the Brahma console's: the Guide's reader pulls in the whole
         markdown render stack (unified + remark/rehype + katex), and this entry
         is mounted eagerly by the shell — so a static reference would put all of
         it in the INITIAL bundle. The dialog's own effects are already no-ops
         until isOpen flips, so gating the mount on the same signal costs nothing
         and defers the chunk to first open. -->
    @defer (when isOpen()) {
      <qt-help-dialog />
    }
  `,
})
export class HelpEntry {
  private readonly help = inject(HelpService);

  protected readonly isEligible = this.help.isEligible;
  /** The `@defer` trigger for the dialog (see the template). */
  protected readonly isOpen = this.help.isOpen;

  /**
   * v4's rule verbatim: `!isEligible && !eligibilityLoading`. The second half is
   * what keeps the button live during the first fetch instead of flashing
   * disabled and then enabling.
   */
  protected readonly isDisabled = computed(
    () => !this.help.isEligible() && !this.help.eligibilityLoading(),
  );

  protected open(): void {
    this.help.openHelpChat();
  }
}
