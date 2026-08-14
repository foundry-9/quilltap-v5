import { ChangeDetectionStrategy, Component, output } from '@angular/core';

import { CharPickerPanel } from './char-picker-panel';
import { UNICODE_PROFILE } from './profiles/unicode';

/**
 * `qt-unicode-picker-popover` — the toolbar's symbol picker:
 * {@link CharPickerPanel} bound to the Unicode profile (v4
 * `components/chat/UnicodePickerPopover.tsx`). Sections are Unicode blocks, in
 * block order — which is also the order that makes ← ↑ → ↓ the first things a
 * search for "arrow" turns up.
 *
 * @module editor/char-insert/unicode-picker-popover
 */
@Component({
  selector: 'qt-unicode-picker-popover',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CharPickerPanel],
  template: `
    <qt-char-picker-panel
      [profile]="profile"
      (pick)="pick.emit($event)"
      (close)="close.emit()"
    />
  `,
})
export class UnicodePickerPopover {
  readonly pick = output<string>();
  readonly close = output<void>();

  protected readonly profile = UNICODE_PROFILE;
}
