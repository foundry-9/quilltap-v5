/**
 * BrahmaToolCall (port of v4 `components/brahma-console/BrahmaToolCall.tsx`).
 *
 * Renders a single `run_sql` tool call as two collapsible panes: the Query (the
 * SQL, through the shared Markdown renderer so it picks up the Prism theme + the
 * copy affordance of any fenced block) and the Result (the returned rows as a
 * scrollable table, or the error text). Only `run_sql` gets this surfacing; other
 * console tools stay silent intermediate turns. Used both for the settled
 * transcript (parsed from a persisted TOOL message) and the in-flight turn (built
 * live from streamed events).
 *
 * @module brahma/brahma-tool-call
 */

import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { MessageContent } from '../chat/message-content';
import { Icon } from '../ui/icon';
import { formatCell, type BrahmaSqlToolCallData } from './brahma-sql-tool-call';

@Component({
  selector: 'qt-brahma-tool-call',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MessageContent],
  template: `
    <div class="qt-bg-muted border qt-border-default rounded-lg p-2.5 text-xs w-full">
      <!-- Header: tool identity + target database + status -->
      <div class="flex items-center gap-2 mb-2">
        <qt-icon name="database" class="w-3.5 h-3.5 qt-text-secondary" />
        <span class="font-semibold text-foreground">Ran SQL</span>
        <span
          class="px-1.5 py-0.5 rounded qt-bg-default border qt-border-default font-mono qt-text-xs qt-text-secondary"
          >{{ data().database }}</span
        >
        <span class="ml-auto px-2 py-0.5 rounded qt-text-label-xs" [class]="statusChipClass()">{{
          statusChipText()
        }}</span>
      </div>

      <!-- Query pane — pretty-printed, syntax-highlighted SQL -->
      @if (data().sql) {
        <details class="group" open>
          <summary
            class="flex items-center gap-1 cursor-pointer select-none qt-text-secondary hover:text-foreground text-xs font-medium list-none [&::-webkit-details-marker]:hidden"
          >
            <qt-icon
              name="chevron-right"
              class="w-3 h-3 transition-transform group-open:rotate-90"
            />
            <span>Query</span>
          </summary>
          <div class="mt-1.5">
            <qt-message-content [content]="sqlMarkdown()" />
          </div>
        </details>
      }

      <!-- Result pane — the returned rows as a table, or the error text -->
      <details class="group mt-2" open>
        <summary
          class="flex items-center gap-1 cursor-pointer select-none qt-text-secondary hover:text-foreground text-xs font-medium list-none [&::-webkit-details-marker]:hidden"
        >
          <qt-icon name="chevron-right" class="w-3 h-3 transition-transform group-open:rotate-90" />
          <span>Result</span>
          @if (truncated()) {
            <span class="qt-text-xs qt-text-secondary font-normal ml-1">truncated</span>
          }
        </summary>
        <div class="mt-1.5">
          @if (data().pending) {
            <div class="qt-text-secondary italic">Consulting the stacks…</div>
          } @else if (!data().success) {
            <div class="qt-text-destructive whitespace-pre-wrap break-words font-mono">
              {{ data().errorText || 'The query failed.' }}
            </div>
          } @else if (rows().length === 0) {
            <div class="qt-text-secondary italic">No rows returned.</div>
          } @else {
            <div class="overflow-auto max-h-80 rounded border qt-border-default bg-background">
              <table class="w-full border-collapse text-xs">
                <thead class="sticky top-0 qt-bg-muted">
                  <tr>
                    @for (col of columns(); track col) {
                      <th
                        class="text-left font-semibold px-2 py-1 border-b qt-border-default whitespace-nowrap"
                      >
                        {{ col }}
                      </th>
                    }
                  </tr>
                </thead>
                <tbody>
                  @for (row of rows(); track $index) {
                    <tr class="border-b qt-border-default last:border-0">
                      @for (col of columns(); track col) {
                        <td
                          class="px-2 py-1 align-top whitespace-pre-wrap break-words font-mono"
                          [class.qt-text-secondary]="cell(row, col).isNull"
                          [class.italic]="cell(row, col).isNull"
                          [class.text-foreground]="!cell(row, col).isNull"
                        >
                          {{ cell(row, col).text }}
                        </td>
                      }
                    </tr>
                  }
                </tbody>
              </table>
            </div>
            <div class="mt-1 qt-text-xs qt-text-secondary">
              {{ rowCount() }} row{{ rowCount() === 1 ? '' : 's' }}
              {{ truncated() ? ' · truncated at the row cap' : '' }}
            </div>
          }
        </div>
      </details>
    </div>
  `,
})
export class BrahmaToolCall {
  readonly data = input.required<BrahmaSqlToolCallData>();

  protected readonly columns = computed(() => this.data().envelope?.columns ?? []);
  protected readonly rows = computed(() => this.data().envelope?.rows ?? []);
  protected readonly rowCount = computed(
    () => this.data().envelope?.rowCount ?? this.rows().length,
  );
  protected readonly truncated = computed(() => this.data().envelope?.truncated ?? false);

  protected readonly sqlMarkdown = computed(() => {
    const sql = this.data().sql;
    return sql ? '```sql\n' + sql + '\n```' : '';
  });

  protected readonly statusChipText = computed(() => {
    const d = this.data();
    if (d.pending) return 'Running…';
    if (d.success) {
      const n = this.rowCount();
      return `${n} row${n === 1 ? '' : 's'}`;
    }
    return 'Failed';
  });

  protected readonly statusChipClass = computed(() => {
    const d = this.data();
    if (d.pending) return 'qt-text-secondary';
    return d.success ? 'qt-badge-success' : 'qt-badge-destructive';
  });

  protected cell(row: Record<string, unknown>, col: string): { text: string; isNull: boolean } {
    return formatCell(row[col]);
  }
}
