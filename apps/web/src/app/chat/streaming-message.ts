import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import type { ChatStreamState } from '../core/chat-stream.reducer';
import { Icon } from '../ui/icon';
import { MessageContent } from './message-content';
import { ThinkingBlock } from './thinking-block';

/**
 * The live in-progress turn: a server-driven status line plus the streaming
 * assistant bubble (content + reasoning rendered through the same markdown
 * pipeline as settled messages), with minimal tool rows. Fed by the P4.5 stream
 * reducer state.
 */
@Component({
  selector: 'qt-streaming-message',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MessageContent, ThinkingBlock],
  template: `
    @if (state().status; as status) {
      <div class="qt-chat-response-status" [attr.data-stage]="status.stage" role="status" aria-live="polite">
        <div class="qt-chat-response-status-icon">
          <span class="inline-block w-2 h-2 rounded-full bg-current animate-pulse"></span>
        </div>
        <span class="qt-chat-response-status-text">{{ status.message }}</span>
      </div>
    }

    @if (state().waitingForResponse && !state().content && !state().reasoning) {
      <div class="qt-chat-message-row qt-chat-message-row-assistant">
        <div class="qt-chat-message-body">
          <div class="qt-chat-message qt-chat-message-assistant">
            <span class="qt-text-muted italic text-sm">…</span>
          </div>
        </div>
      </div>
    }

    @if (state().content || state().reasoning) {
      <div class="qt-chat-message-row qt-chat-message-row-assistant">
        <div class="qt-chat-message-body">
          <div class="qt-chat-message qt-chat-message-assistant">
            @if (state().reasoning) {
              <qt-thinking-block [content]="state().reasoning" [collapsed]="false" />
            }
            @if (state().content) {
              <qt-message-content [content]="state().content" />
            }
            @for (batch of state().toolBatches; track $index) {
              <div class="qt-chat-message-tools">
                @for (call of batch.calls; track call.id) {
                  <div class="qt-chat-tool-embedded qt-text-small qt-text-secondary">
                    <qt-icon name="wrench" class="w-3 h-3 inline-block mr-1" />{{ call.name }}
                    @if (call.status === 'success') { ✓ } @else if (call.status === 'error') { ✗ }
                  </div>
                }
              </div>
            }
          </div>
        </div>
      </div>
    }

    @if (state().error) {
      <div class="qt-chat-message-row qt-chat-message-row-assistant">
        <div class="qt-chat-message-body">
          <div class="qt-alert qt-alert-error">{{ state().error }}</div>
        </div>
      </div>
    }
  `,
})
export class StreamingMessage {
  readonly state = input.required<ChatStreamState>();
}
