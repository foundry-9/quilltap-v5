/**
 * BrahmaConsoleMessageList (port of v4
 * `components/brahma-console/BrahmaConsoleMessageList.tsx`).
 *
 * Renders the Console transcript: the operator's own messages and a single
 * character-less assistant voice (the console mark as its avatar), plus a card
 * for each `run_sql` TOOL message. Other tools stay silent intermediate turns.
 * Reuses the shared `qt-help-*` chat-bubble styling. The live streaming block
 * shows cumulative reasoning, live run_sql cards, and the streamed prose as they
 * arrive.
 *
 * v5 note: the live tool calls arrive as the shared reducer's
 * {@link PendingToolCall}s (`chat-stream.reducer.ts`) — the dialog flattens the
 * batch list into one run-wide array (tool cards persist across turns) and hands
 * them here. A failed live call has no preserved error text in the reducer, so it
 * falls back to "The query failed."; the reloaded transcript carries the full
 * error via `parseBrahmaSqlToolMessage`.
 *
 * @module brahma/brahma-console-message-list
 */

import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  afterRenderEffect,
  computed,
  input,
  signal,
  viewChild,
} from '@angular/core';

import { MessageContent } from '../chat/message-content';
import { ThinkingBlock } from '../chat/thinking-block';
import type { PendingToolCall } from '../core/chat-stream.reducer';
import { Icon } from '../ui/icon';
import {
  parseBrahmaSqlToolMessage,
  type BrahmaSqlToolCallData,
} from './brahma-sql-tool-call';
import { BrahmaToolCall } from './brahma-tool-call';
import type { BrahmaConsoleMessage } from './brahma-wire';

/** One transcript render item, in order: a bubble or a run_sql tool card. */
type RenderItem =
  | { kind: 'bubble'; msg: BrahmaConsoleMessage; isUser: boolean }
  | { kind: 'tool'; id: string; data: BrahmaSqlToolCallData };

/** Normalize a live-streamed reducer tool call into the shape the card renders. */
function pendingToolCallToData(tc: PendingToolCall): BrahmaSqlToolCallData {
  const args = tc.arguments ?? {};
  const sql = typeof args['sql'] === 'string' ? (args['sql'] as string) : null;
  const database = typeof args['database'] === 'string' ? (args['database'] as string) : 'main';
  const pending = tc.status === 'pending';
  const success = tc.status === 'success';
  const envelope =
    tc.result && typeof tc.result === 'object'
      ? (tc.result as BrahmaSqlToolCallData['envelope'])
      : null;
  const errorText = pending
    ? null
    : success
      ? null
      : typeof tc.result === 'string' && tc.result.trim()
        ? tc.result
        : 'The query failed.';
  return { success, sql, database, envelope, errorText, pending };
}

/**
 * A "copy as Markdown" affordance beneath each settled bubble (v4
 * `CopyMarkdownButton`). Self-contained per-button state, mirroring the
 * code-block copy control, so the console needs no toast plumbing.
 */
@Component({
  selector: 'qt-brahma-copy-button',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <button
      type="button"
      class="qt-chat-message-action-icon opacity-50 hover:opacity-100"
      [title]="copied() ? 'Copied!' : 'Copy as Markdown'"
      [attr.aria-label]="copied() ? 'Copied' : 'Copy message as Markdown'"
      (click)="copy()"
    >
      <qt-icon [name]="copied() ? 'check' : 'copy'" />
    </button>
  `,
})
export class BrahmaCopyButton {
  readonly content = input.required<string>();
  protected readonly copied = signal(false);

  protected async copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(this.content());
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 2000);
    } catch (err) {
      console.error('Failed to copy message', {
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }
}

@Component({
  selector: 'qt-brahma-console-message-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MessageContent, ThinkingBlock, BrahmaToolCall, BrahmaCopyButton],
  template: `
    <div class="flex flex-col gap-3 p-4 overflow-y-auto flex-1">
      @if (renderItems().length === 0 && !isStreaming()) {
        <div class="text-center qt-text-secondary text-sm py-8">
          A direct line to the engine of your choosing. Pose a question to begin.
        </div>
      }

      @for (item of renderItems(); track item.kind === 'tool' ? item.id : item.msg.id) {
        @if (item.kind === 'tool') {
          <div class="flex flex-row pl-1">
            <div class="min-w-0 w-full" style="max-width: 92%">
              <qt-brahma-tool-call [data]="item.data" />
            </div>
          </div>
        } @else {
          <div class="flex items-start" [class.flex-row-reverse]="item.isUser">
            @if (!item.isUser) {
              <div class="qt-help-avatar">
                <qt-icon name="brahma-console" class="w-4 h-4" />
              </div>
            }

            <svg
              class="qt-help-tail"
              [class.qt-help-tail-user]="item.isUser"
              [class.qt-help-tail-assistant]="!item.isUser"
              viewBox="0 0 10 16"
              fill="currentColor"
            >
              @if (item.isUser) {
                <path d="M0 0 L10 8 L0 16 Z" />
              } @else {
                <path d="M10 0 L0 8 L10 16 Z" />
              }
            </svg>

            <div
              class="flex flex-col gap-0.5 min-w-0"
              [class.items-end]="item.isUser"
              [class.items-start]="!item.isUser"
              style="max-width: 80%"
            >
              @if (!item.isUser && item.msg.reasoningContent) {
                <qt-thinking-block [content]="item.msg.reasoningContent" [collapsed]="true" />
              }
              <div
                [class]="item.isUser ? 'qt-help-msg-user' : 'qt-help-msg-assistant'"
                style="max-width: 100%"
              >
                <qt-message-content [content]="item.msg.content" />
              </div>
              <qt-brahma-copy-button [content]="item.msg.content" />
            </div>
          </div>
        }
      }

      @if (isStreaming()) {
        <div class="flex items-start flex-row">
          <div class="qt-help-avatar">
            <qt-icon name="brahma-console" class="w-4 h-4" />
          </div>
          <svg
            class="qt-help-tail qt-help-tail-assistant"
            viewBox="0 0 10 16"
            fill="currentColor"
          >
            <path d="M10 0 L0 8 L10 16 Z" />
          </svg>
          <div class="flex flex-col gap-2 min-w-0 items-start" style="max-width: 80%">
            @if (streamingReasoning().trim()) {
              <qt-thinking-block [content]="streamingReasoning()" [collapsed]="false" />
            }
            @for (call of liveSqlCalls(); track $index) {
              <div class="w-full">
                <qt-brahma-tool-call [data]="call" />
              </div>
            }
            @if (streamingContent()) {
              <div class="qt-help-msg-assistant" style="max-width: 100%">
                <qt-message-content [content]="streamingContent()" />
              </div>
            }
            @if (isExecutingTools() && liveSqlCalls().length === 0) {
              <div class="qt-help-msg-assistant italic">Consulting the stacks…</div>
            } @else if (
              !streamingContent() && !streamingReasoning().trim() && liveSqlCalls().length === 0
            ) {
              <div class="qt-help-msg-assistant italic">Thinking…</div>
            }
          </div>
        </div>
      }

      <div #end></div>
    </div>
  `,
})
export class BrahmaConsoleMessageList {
  readonly messages = input.required<BrahmaConsoleMessage[]>();
  readonly streamingContent = input('');
  readonly streamingReasoning = input('');
  /** Live tool calls this run (flattened from the reducer's batch list). */
  readonly streamingToolCalls = input<PendingToolCall[]>([]);
  readonly isStreaming = input(false);
  readonly isExecutingTools = input(false);

  private readonly endRef = viewChild.required<ElementRef<HTMLDivElement>>('end');

  /**
   * Transcript render items, in order: user + assistant bubbles (assistant only
   * when it carries prose — hides empty intermediate agent turns) + a card for
   * each run_sql TOOL message (other tools stay silent).
   */
  protected readonly renderItems = computed<RenderItem[]>(() => {
    const items: RenderItem[] = [];
    for (const m of this.messages()) {
      const role = (m.role ?? '').toUpperCase();
      if (role === 'USER') {
        items.push({ kind: 'bubble', msg: m, isUser: true });
      } else if (role === 'ASSISTANT') {
        if (m.content && m.content.trim().length > 0) {
          items.push({ kind: 'bubble', msg: m, isUser: false });
        }
      } else if (role === 'TOOL') {
        const sqlData = parseBrahmaSqlToolMessage(m.content ?? '');
        if (sqlData) items.push({ kind: 'tool', id: m.id, data: sqlData });
      }
    }
    return items;
  });

  /** Live run_sql cards for the in-flight turn. */
  protected readonly liveSqlCalls = computed<BrahmaSqlToolCallData[]>(() =>
    this.streamingToolCalls()
      .filter((tc) => tc.name === 'run_sql')
      .map(pendingToolCallToData),
  );

  constructor() {
    // Chase the bottom on any transcript / stream change (v4 `endRef.scrollIntoView`).
    afterRenderEffect(() => {
      // touch the reactive inputs so the effect re-runs on each change
      this.renderItems();
      this.streamingContent();
      this.streamingReasoning();
      this.liveSqlCalls();
      this.endRef().nativeElement.scrollIntoView({ behavior: 'smooth' });
    });
  }
}
