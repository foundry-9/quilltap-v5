import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import type { ChatStreamState } from '../core/chat-stream.reducer';
import { Icon } from '../ui/icon';
import { MessageContent } from './message-content';
import { QuillAnimation } from './quill-animation';
import type { DialogueDetection, RenderingPattern } from './render/roleplay-rendering';
import { ThinkingBlock } from './thinking-block';

/**
 * The live in-progress turn: a server-driven status line plus the streaming
 * assistant bubble (content + reasoning rendered through the same markdown
 * pipeline as settled messages), with minimal tool rows. Fed by the P4.5 stream
 * reducer state.
 *
 * The waiting quill (`qt-quill-animation`) appears at v5's analogs of v4's four
 * call sites — v4 spreads them over three components (`StreamingMessage`,
 * `ChatComposer`'s status strip, `PendingToolCalls`), all of which v5 folds into
 * this one. See the comment at each site for the mapping.
 */
@Component({
  selector: 'qt-streaming-message',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MessageContent, QuillAnimation, ThinkingBlock],
  template: `
    @if (state().status; as status) {
      <div class="qt-chat-response-status" [attr.data-stage]="status.stage" role="status" aria-live="polite">
        <div class="qt-chat-response-status-icon">
          <!--
            v4 ChatComposer:241 — the status strip carries the quill only while
            tokens are actually streaming; every other stage keeps the pulsing
            dot. label={null}: the strip is already a labelled live region, and
            the indicator was otherwise announced twice.
          -->
          @if (status.stage === 'streaming') {
            <qt-quill-animation size="sm" [label]="null" />
          } @else {
            <span class="inline-block w-2 h-2 rounded-full bg-current animate-pulse"></span>
          }
        </div>
        <span class="qt-chat-response-status-text">{{ status.message }}</span>
      </div>
    }

    <!--
      v4 StreamingMessage:100 — the awaiting state is the bare large quill in a
      qt-text-secondary wrapper, NOT a bubble (v5 previously rendered a bubble
      holding a literal "…").
    -->
    @if (state().waitingForResponse && !state().content && !state().reasoning) {
      <div class="qt-chat-message-row qt-chat-message-row-assistant">
        <div class="qt-chat-message-body">
          <div class="qt-text-secondary">
            <qt-quill-animation size="lg" />
          </div>
        </div>
      </div>
    }

    @if (state().content || state().reasoning) {
      <div class="qt-chat-message-row qt-chat-message-row-assistant">
        <div class="qt-chat-message-body">
          <div class="qt-chat-message qt-chat-message-assistant">
            @if (state().reasoning) {
              <qt-thinking-block
                [content]="state().reasoning"
                [collapsed]="false"
                [renderingPatterns]="renderingPatterns()"
                [dialogueDetection]="dialogueDetection()"
              />
            }
            @if (state().content) {
              <qt-message-content
                [content]="state().content"
                [renderingPatterns]="renderingPatterns()"
                [dialogueDetection]="dialogueDetection()"
              />
            }
            <!--
              v4 StreamingMessage:122 — the small quill trails the live prose
              while it streams. v4 attaches it to the LAST text segment of its
              interleaved parts array; v5 renders one prose blob followed by the
              tool batches, so the tail's v5 home is directly after the content.
            -->
            <qt-quill-animation size="sm" class="inline-block ml-2 qt-text-secondary" />
            @for (batch of state().toolBatches; track $index) {
              <div class="qt-chat-message-tools">
                @for (call of batch.calls; track call.id) {
                  <div class="qt-chat-tool-embedded qt-text-small qt-text-secondary">
                    <qt-icon name="wrench" class="w-3 h-3 inline-block mr-1" />{{ call.name }}
                    <!--
                      v4 PendingToolCalls:50 — v4 renders ONE summary row per
                      batch (names comma-joined) with a single quill when
                      "some(pending)". v5 renders a row per call, so the quill
                      takes the same slot as the ✓/✗ it replaces; "some pending
                      shows an indicator" follows. label={null}: the streaming
                      bubble is already a live region.
                    -->
                    @if (call.status === 'success') {
                      ✓
                    } @else if (call.status === 'error') {
                      ✗
                    } @else {
                      <qt-quill-animation size="sm" [label]="null" class="qt-text-secondary" />
                    }
                  </div>
                }
              </div>
            }
          </div>
        </div>
      </div>
    }

  `,
})
/**
 * v4 renders NO inline stream error: a transport failure is a toast
 * (`useSSEStreaming.ts:845`, `:1026`) and the bubble simply stops. The Salon
 * raises it from `reportStreamTransitions`; the state's `error` field stays as
 * the halt signal.
 */
export class StreamingMessage {
  readonly state = input.required<ChatStreamState>();
  /**
   * The chat's roleplay-template patterns — v4 passes the very same pair into
   * `StreamingMessage` that it passes into each settled row
   * (`VirtualizedMessageList.tsx:387-388`), so a live reply is styled the moment
   * it streams rather than snapping into the template's marks on reconcile.
   */
  readonly renderingPatterns = input<RenderingPattern[] | undefined>(undefined);
  readonly dialogueDetection = input<DialogueDetection | null | undefined>(undefined);
}
