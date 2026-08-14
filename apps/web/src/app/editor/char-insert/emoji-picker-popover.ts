import { ChangeDetectionStrategy, Component, output } from '@angular/core';

import { CharPickerPanel } from './char-picker-panel';
import { EMOJI_PROFILE } from './profiles/emoji';

/**
 * `qt-emoji-picker-popover` — the toolbar's emoji picker: {@link CharPickerPanel}
 * bound to the emoji profile (v4 `components/chat/EmojiPickerPopover.tsx`).
 * Everything except the profile is shared with the symbol picker.
 *
 * @module editor/char-insert/emoji-picker-popover
 */
@Component({
  selector: 'qt-emoji-picker-popover',
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
export class EmojiPickerPopover {
  readonly pick = output<string>();
  readonly close = output<void>();

  protected readonly profile = EMOJI_PROFILE;
}
