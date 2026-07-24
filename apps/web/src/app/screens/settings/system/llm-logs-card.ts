import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import { fetchRecentLlmLogs, type LlmLogDto } from '../../../chat/llm-logs.api';
import { LLMLogViewerModal } from '../../../chat/llm-log-viewer-modal';
import { Icon } from '../../../ui/icon';

/** v4 `llm-logs-card.tsx:32-46` getTypeLabel — VERBATIM (unlisted → raw type). */
const TYPE_LABELS: Record<string, string> = {
  CHAT_MESSAGE: 'Chat',
  TOOL_CONTINUATION: 'Tool',
  MEMORY_EXTRACTION: 'Memory',
  TITLE_GENERATION: 'Title',
  CONTEXT_COMPRESSION: 'Compression',
  SUMMARIZATION: 'Summary',
  IMAGE_PROMPT_CRAFTING: 'Image Prompt',
  CHARACTER_WIZARD: 'Wizard',
  IMAGE_DESCRIPTION: 'Image Desc',
  CUSTOM_TOOL_CONSULT: 'Custom Tool',
};

/** v4 `formatDateTime(str, {includeYear:false})` — month/day + time, no year. */
function formatDate(dateString: string): string {
  if (!dateString) return '';
  try {
    return new Date(dateString).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return dateString;
  }
}

/**
 * The LLM Logs card (v4 `components/tools/llm-logs-card.tsx`): the twenty most
 * recent LLM logs, each a clickable row opening the {@link LLMLogViewerModal}.
 * Fetches `GET /llm-logs?limit=20` through the `llmLogsList` verb (P4.6ar).
 */
@Component({
  selector: 'qt-llm-logs-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, LLMLogViewerModal],
  template: `
    <div>
      <div class="mb-4">
        <button
          type="button"
          class="qt-button qt-button-secondary flex items-center gap-2"
          [disabled]="query.isFetching()"
          (click)="query.refetch()"
        >
          <qt-icon name="refresh" [class]="'w-4 h-4 ' + (query.isFetching() ? 'animate-spin' : '')" />
          Refresh
        </button>
      </div>

      @if (query.isError()) {
        <div class="qt-bg-destructive/10 border qt-border-destructive qt-text-destructive px-4 py-3 rounded mb-4">
          Failed to fetch logs
        </div>
      }

      @if (query.isPending()) {
        <div class="text-center py-6 qt-text-secondary">
          <div class="animate-spin rounded-full h-6 w-6 border-b-2 qt-border-primary mx-auto mb-2"></div>
          Loading logs...
        </div>
      } @else if (logs().length === 0) {
        <div class="qt-card p-6 text-center">
          <qt-icon name="cpu" class="w-12 h-12 mx-auto mb-3 qt-text-secondary/50" />
          <p class="qt-text-small">
            No LLM logs yet. Send a message or use other LLM features to generate logs.
          </p>
        </div>
      } @else {
        <div class="space-y-2 max-h-[300px] overflow-y-auto">
          @for (log of logs(); track log.id) {
            <button
              type="button"
              class="qt-card p-3 w-full text-left flex items-center justify-between hover:qt-bg-muted/50 transition-colors cursor-pointer"
              (click)="openLog(log)"
            >
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="px-2 py-0.5 text-xs rounded qt-bg-primary/10 text-primary">{{
                    typeLabel(log.type)
                  }}</span>
                  <span class="qt-text-primary truncate text-sm"
                    >{{ log.provider }}/{{ log.modelName }}</span
                  >
                </div>
                <div class="flex gap-4 mt-1 qt-text-small">
                  <span>{{ date(log.createdAt) }}</span>
                  @if (log.usage) {
                    <span>{{ log.usage.totalTokens.toLocaleString() }} tokens</span>
                  }
                  @if (log.durationMs) {
                    <span>{{ (log.durationMs / 1000).toFixed(1) }}s</span>
                  }
                </div>
              </div>
              <div class="ml-4"><qt-icon name="chevron-right" class="w-5 h-5 qt-text-secondary" /></div>
            </button>
          }
        </div>
      }

      <qt-llm-log-viewer-modal
        [isOpen]="selectedLog() !== null"
        [logs]="selectedLog() ? [selectedLog()!] : []"
        (close)="selectedLog.set(null)"
      />
    </div>
  `,
})
export class LlmLogsCard {
  private readonly core = inject(CoreClient);

  protected readonly query = injectQuery(() => ({
    queryKey: ['llmLogs', 'recent', 20],
    queryFn: () => fetchRecentLlmLogs(this.core, 20),
  }));

  protected readonly logs = computed<LlmLogDto[]>(() => this.query.data() ?? []);
  protected readonly selectedLog = signal<LlmLogDto | null>(null);

  protected typeLabel(type: string): string {
    return TYPE_LABELS[type] || type;
  }

  protected date(dateString: string): string {
    return formatDate(dateString);
  }

  protected openLog(log: LlmLogDto): void {
    this.selectedLog.set(log);
  }
}
