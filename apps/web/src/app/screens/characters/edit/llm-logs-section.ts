import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import { fetchCharacterLlmLogs, type LlmLogDto } from '../../../chat/llm-logs.api';
import { LLMLogViewerModal } from '../../../chat/llm-log-viewer-modal';
import { Icon } from '../../../ui/icon';

/** v4 `formatDateTime(str, {includeYear:false})`. */
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
 * The character-edit LLM Logs section (v4 `components/characters/LLMLogsSection.tsx`
 * — the M6 F2 host of the shared viewer modal): a collapsible section on the
 * edit screen listing this character's ten most recent logs. The query is LAZY
 * (`enabled: isExpanded`, v4 `:25`) — no fetch until first expand.
 */
@Component({
  selector: 'qt-character-llm-logs-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, LLMLogViewerModal],
  template: `
    <div class="mt-8 border qt-border-default rounded-lg overflow-hidden">
      <button
        type="button"
        class="w-full px-4 py-3 flex items-center justify-between qt-bg-muted/30 hover:qt-bg-muted/50 transition-colors"
        (click)="isExpanded.set(!isExpanded())"
      >
        <div class="flex items-center gap-2">
          <qt-icon name="cpu" class="w-5 h-5" />
          <span class="font-medium">LLM Logs</span>
          @if (logs().length > 0) {
            <span class="text-xs px-2 py-0.5 rounded-full qt-bg-primary/10 text-primary">{{
              logs().length
            }}</span>
          }
        </div>
        <qt-icon
          name="chevron-down"
          [class]="'w-5 h-5 transition-transform ' + (isExpanded() ? 'rotate-180' : '')"
        />
      </button>

      @if (isExpanded()) {
        <div class="p-4">
          @if (query.isPending()) {
            <div class="text-center py-4 qt-text-secondary">
              <div class="animate-spin rounded-full h-5 w-5 border-b-2 qt-border-primary mx-auto mb-2"></div>
              Loading...
            </div>
          } @else if (logs().length === 0) {
            <p class="text-center py-4 qt-text-secondary text-sm">
              No LLM logs for this character yet. Use the AI wizard to generate character content.
            </p>
          } @else {
            <div class="space-y-2">
              @for (log of logs(); track log.id) {
                <button
                  type="button"
                  class="p-3 w-full text-left border qt-border-default rounded hover:qt-bg-muted/30 cursor-pointer transition-colors"
                  (click)="openLog(log)"
                >
                  <div class="flex items-center justify-between">
                    <div class="flex items-center gap-2">
                      <span class="px-2 py-0.5 text-xs rounded qt-bg-primary/10 text-primary">{{
                        badge(log.type)
                      }}</span>
                      <span class="text-sm">{{ log.provider }}/{{ log.modelName }}</span>
                    </div>
                    <span class="text-xs qt-text-secondary">{{ date(log.createdAt) }}</span>
                  </div>
                  @if (log.usage) {
                    <div class="mt-1 text-xs qt-text-secondary">
                      {{ log.usage.totalTokens.toLocaleString() }} tokens{{ durationSuffix(log) }}
                    </div>
                  }
                </button>
              }
            </div>
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
export class CharacterLlmLogsSection {
  private readonly core = inject(CoreClient);

  readonly characterId = input.required<string>();

  protected readonly isExpanded = signal(false);
  protected readonly selectedLog = signal<LlmLogDto | null>(null);

  protected readonly query = injectQuery(() => ({
    queryKey: ['llmLogs', 'character', this.characterId(), 10],
    enabled: this.isExpanded(),
    queryFn: () => fetchCharacterLlmLogs(this.core, this.characterId(), 10),
  }));

  protected readonly logs = computed<LlmLogDto[]>(() => this.query.data() ?? []);

  /** v4 `:80-82` — only CHARACTER_WIZARD is remapped; the rest show the raw type. */
  protected badge(type: string): string {
    return type === 'CHARACTER_WIZARD' ? 'Wizard' : type;
  }

  protected date(dateString: string): string {
    return formatDate(dateString);
  }

  /** v4 `:91-93` — ` • X.Xs` after the token count when a duration exists. */
  protected durationSuffix(log: LlmLogDto): string {
    return log.durationMs ? ` • ${(log.durationMs / 1000).toFixed(1)}s` : '';
  }

  protected openLog(log: LlmLogDto): void {
    this.selectedLog.set(log);
  }
}
