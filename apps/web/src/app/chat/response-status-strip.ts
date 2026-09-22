import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import type { ResponseStatus } from '../core/core-contract';
import { QuillAnimation } from './quill-animation';

/**
 * The status strip — the one line that says what the server is doing right now
 * (v4 `ChatComposer.tsx:288-305`).
 *
 * v4 keeps exactly ONE of these, in the composer, and feeds it whichever source
 * currently owns the floor: `regenerationController.regenerationStatus ??
 * sseStreaming.responseStatus` (`SalonView.tsx:1648`). v5's copy grew up inside
 * `streaming-message.ts`, which mounts only while a turn is streaming — so a
 * regeneration, which sets no stream state at all, had nowhere to say anything.
 * P4.D206 lifts the markup out here unchanged and gives it two hosts, which is
 * one home for the strip and v4's own arrangement in v5's shape.
 *
 * @module chat/response-status-strip
 */
@Component({
  selector: 'qt-response-status-strip',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [QuillAnimation],
  template: `
    @if (status(); as s) {
      <div
        class="qt-chat-response-status"
        [attr.data-stage]="s.stage"
        role="status"
        aria-live="polite"
      >
        <div class="qt-chat-response-status-icon">
          <!--
            v4 ChatComposer:296-299 — prose is actually arriving in these two
            stages (a first turn or a re-roll), so both get the quill and every
            other stage keeps the pulsing dot. label={null}: the strip is
            already a labelled live region, and the indicator was otherwise
            announced twice.
          -->
          @if (s.stage === 'streaming' || s.stage === 'regenerating') {
            <qt-quill-animation size="sm" [label]="null" />
          } @else {
            <span class="inline-block w-2 h-2 rounded-full bg-current animate-pulse"></span>
          }
        </div>
        <span class="qt-chat-response-status-text">{{ s.message }}</span>
      </div>
    }
  `,
})
export class ResponseStatusStrip {
  readonly status = input<ResponseStatus | null>(null);
}
