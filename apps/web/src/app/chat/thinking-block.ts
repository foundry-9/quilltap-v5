import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import { Icon } from '../ui/icon';
import { MessageContent } from './message-content';

/**
 * A collapsible chain-of-thought block (v4 `ThinkingBlock`). Display-only,
 * right-offset + dimmed via the `qt-chat-thinking-*` classes. Rendered when the
 * chat-settings thinking display is enabled; `collapsed` seeds the `<details>`
 * open state (v4 `thinkingDisplay.defaultCollapsed`).
 */
@Component({
  selector: 'qt-thinking-block',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MessageContent],
  template: `
    <div class="qt-chat-thinking">
      <details class="qt-chat-thinking-details" [open]="!collapsed()">
        <summary class="qt-chat-thinking-summary">
          <qt-icon name="chevron-right" class="qt-chat-thinking-chevron" />
          Thinking
        </summary>
        <div class="qt-chat-thinking-body">
          <qt-message-content [content]="content()" />
        </div>
      </details>
    </div>
  `,
})
export class ThinkingBlock {
  readonly content = input.required<string>();
  readonly collapsed = input(true);
}
