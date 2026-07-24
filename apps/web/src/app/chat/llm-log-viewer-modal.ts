import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import { Modal } from '../ui/modal';
import type { LlmLogDto, LlmLogMessageSummary } from './llm-logs.api';

/** The detail tabs (v4 `LLMLogViewerModal.tsx` `TabType`). */
type TabType = 'request' | 'response' | 'usage';

/**
 * The LLM Log Viewer modal (v4 `components/chat/LLMLogViewerModal.tsx`): a
 * PURE PRESENTATIONAL modal — the caller passes `logs`, the modal renders one at
 * a time (with a selector when more than one) across request / response / usage
 * tabs. The hosts (the Data & System LLM Logs card and the character-edit
 * `LLMLogsSection`) do the fetching and pass a single-element array.
 *
 * Distinct from {@link import('./llm-inspector-entry').LLMInspectorEntry} (an
 * expandable ROW in the slide-over): v4 keeps both, and their formatting differs
 * in places (the modal's "[... truncated ...]" marker, the multi-log selector,
 * the two-decimal duration). This ports the MODAL faithfully.
 *
 * The content fields read the v5 `LlmLogDto` live-field chain (`content` →
 * `fullContent`/`contentPreview`), exactly as the inspector-entry already does —
 * v5 stores `content`, where v4's schema still names the modal's now-deprecated
 * `contentPreview`. v4's "Full Messages (Verbose)" block is dropped (v5's DTO
 * carries no `request.fullMessages`); the "Full Content (Verbose)" block renders
 * from the typed `response.fullContent` when present.
 */
@Component({
  selector: 'qt-llm-log-viewer-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    @if (isOpen() && logs().length > 0) {
      <qt-modal title="LLM Request/Response Log" maxWidth="3xl" (close)="close.emit()">
        @if (logs().length > 1) {
          <div class="mb-4">
            <label for="log-select" class="block text-sm qt-text-secondary mb-2">Select Log</label>
            <select
              id="log-select"
              class="w-full qt-input"
              [value]="safeLogIndex()"
              (change)="onSelectLog($event)"
            >
              @for (log of logs(); track log.id; let i = $index) {
                <option [value]="i">
                  {{ optionLabel(log) }}
                </option>
              }
            </select>
          </div>
        }

        <div class="pb-2 mb-2 border-b qt-border text-xs qt-text-secondary">{{ metadata() }}</div>

        <div class="flex border-b qt-border mb-4">
          @for (tab of tabs; track tab) {
            <button
              type="button"
              class="px-4 py-3 qt-label transition-colors border-b-2"
              [class]="
                activeTab() === tab
                  ? 'qt-border-primary qt-text'
                  : 'border-transparent qt-text-secondary hover:qt-text'
              "
              (click)="activeTab.set(tab)"
            >
              {{ tabLabel(tab) }}
            </button>
          }
        </div>

        @if (currentLog(); as log) {
          @if (activeTab() === 'request') {
            <div class="space-y-4">
              <div>
                <h4 class="qt-label qt-text mb-2">Provider & Model</h4>
                <div class="qt-surface-alt p-3 rounded space-y-1">
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Provider:</span>
                    <span class="qt-text font-mono text-sm">{{ log.provider }}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Model:</span>
                    <span class="qt-text font-mono text-sm">{{ log.modelName }}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Type:</span>
                    <span class="qt-text font-mono text-sm">{{ log.type }}</span>
                  </div>
                </div>
              </div>

              <div>
                <h4 class="qt-label qt-text mb-2">Request Configuration</h4>
                <div class="qt-surface-alt p-3 rounded space-y-1">
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Messages:</span>
                    <span class="qt-text font-mono text-sm">{{ log.request.messageCount }}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Temperature:</span>
                    <span class="qt-text font-mono text-sm">{{ temperatureText() }}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Max Tokens:</span>
                    <span class="qt-text font-mono text-sm">{{ maxTokensText() }}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Tools:</span>
                    <span class="qt-text font-mono text-sm">{{ log.request.toolCount }}</span>
                  </div>
                </div>
              </div>

              <div>
                <h4 class="qt-label qt-text mb-2">Message Summary</h4>
                <div class="space-y-2">
                  @for (msg of log.request.messages; track $index) {
                    <div class="qt-surface-alt p-2 rounded text-sm">
                      <div class="flex justify-between mb-1">
                        <span class="qt-text-secondary font-mono">{{ msg.role }}</span>
                        <span class="qt-text-secondary text-xs">
                          {{ msg.contentLength }} chars{{
                            msg.hasAttachments ? ' (with attachments)' : ''
                          }}
                        </span>
                      </div>
                      <p class="qt-text text-xs whitespace-pre-wrap break-words">
                        {{ messageBody(msg) }}{{ msg.contentLength > 500 ? '...' : '' }}
                      </p>
                    </div>
                  }
                </div>
              </div>
            </div>
          } @else if (activeTab() === 'response') {
            <div class="space-y-4">
              @if (log.response.error) {
                <div class="p-3 qt-bg-destructive/10 border qt-border-destructive/20 rounded">
                  <h4 class="qt-label qt-text-destructive mb-1">Error</h4>
                  <p class="text-sm qt-text">{{ log.response.error }}</p>
                </div>
              } @else {
                <div class="p-3 qt-bg-success/10 border qt-border-success/20 rounded">
                  <p class="text-sm qt-text font-medium">Request completed successfully</p>
                </div>
              }

              <div>
                <h4 class="qt-label qt-text mb-2">Content Preview ({{ log.response.contentLength }} chars)</h4>
                <pre
                  class="font-mono text-xs whitespace-pre-wrap overflow-auto max-h-64 p-3 qt-surface-alt rounded"
                  >{{ responseContent() }}{{ log.response.contentLength > 500 ? '\n\n[... truncated ...]' : '' }}</pre
                >
              </div>

              @if (log.response.fullContent) {
                <div>
                  <h4 class="qt-label qt-text mb-2">Full Content (Verbose)</h4>
                  <pre
                    class="font-mono text-xs whitespace-pre-wrap overflow-auto max-h-96 p-3 qt-surface-alt rounded"
                    >{{ log.response.fullContent }}</pre
                  >
                </div>
              }
            </div>
          } @else {
            <div class="space-y-4">
              @if (log.usage; as usage) {
                <div>
                  <h4 class="qt-label qt-text mb-2">Token Usage</h4>
                  <div class="qt-surface-alt p-4 rounded grid grid-cols-3 gap-4">
                    <div class="text-center">
                      <p class="qt-heading-2 qt-text">{{ usage.promptTokens.toLocaleString() }}</p>
                      <p class="text-xs qt-text-secondary mt-1">Prompt Tokens</p>
                    </div>
                    <div class="text-center">
                      <p class="qt-heading-2 qt-text">
                        {{ usage.completionTokens.toLocaleString() }}
                      </p>
                      <p class="text-xs qt-text-secondary mt-1">Completion Tokens</p>
                    </div>
                    <div class="text-center">
                      <p class="qt-heading-2 qt-text">{{ usage.totalTokens.toLocaleString() }}</p>
                      <p class="text-xs qt-text-secondary mt-1">Total Tokens</p>
                    </div>
                  </div>
                </div>
              }

              @if (log.cacheUsage; as cache) {
                <div>
                  <h4 class="qt-label qt-text mb-2">Cache Usage</h4>
                  <div class="qt-surface-alt p-3 rounded space-y-2">
                    @if (cache.cacheCreationInputTokens !== undefined) {
                      <div class="flex justify-between">
                        <span class="qt-text-secondary">Cache Creation:</span>
                        <span class="qt-text font-mono"
                          >{{ cache.cacheCreationInputTokens.toLocaleString() }} tokens</span
                        >
                      </div>
                    }
                    @if (cache.cacheReadInputTokens !== undefined) {
                      <div class="flex justify-between">
                        <span class="qt-text-secondary">Cache Read:</span>
                        <span class="qt-text font-mono"
                          >{{ cache.cacheReadInputTokens.toLocaleString() }} tokens</span
                        >
                      </div>
                    }
                  </div>
                </div>
              }

              @if (log.durationMs != null) {
                <div>
                  <h4 class="qt-label qt-text mb-2">Timing</h4>
                  <div class="qt-surface-alt p-3 rounded">
                    <div class="flex justify-between">
                      <span class="qt-text-secondary">Duration:</span>
                      <span class="qt-text font-mono">{{ durationText() }}</span>
                    </div>
                  </div>
                </div>
              }

              @if (noUsageData()) {
                <p class="qt-text-secondary text-sm p-3 qt-surface-alt rounded text-center">
                  No usage data available for this log
                </p>
              }
            </div>
          }
        }
      </qt-modal>
    }
  `,
})
export class LLMLogViewerModal {
  readonly isOpen = input.required<boolean>();
  readonly logs = input.required<LlmLogDto[]>();
  readonly close = output<void>();

  protected readonly activeTab = signal<TabType>('request');
  protected readonly tabs: readonly TabType[] = ['request', 'response', 'usage'] as const;
  private readonly selectedLogIndex = signal(0);

  /** v4 `:25-28` — clamp the index to the current list. */
  protected readonly safeLogIndex = computed(() => {
    const list = this.logs();
    return list.length === 0 ? 0 : Math.min(this.selectedLogIndex(), list.length - 1);
  });

  protected readonly currentLog = computed<LlmLogDto | undefined>(
    () => this.logs()[this.safeLogIndex()],
  );

  protected readonly metadata = computed(() => {
    const log = this.currentLog();
    return log ? new Date(log.createdAt).toLocaleString() : '';
  });

  protected readonly temperatureText = computed(() => {
    const t = this.currentLog()?.request.temperature;
    return t !== null && t !== undefined ? String(t) : 'default';
  });
  protected readonly maxTokensText = computed(() => {
    const m = this.currentLog()?.request.maxTokens;
    return m !== null && m !== undefined ? String(m) : 'default';
  });

  /** v5's live-field chain (the inspector-entry precedent, `:427-430`). */
  protected readonly responseContent = computed(() => {
    const r = this.currentLog()?.response;
    return r ? r.content || r.fullContent || r.contentPreview || '' : '';
  });

  protected readonly durationText = computed(
    () => `${(this.currentLog()!.durationMs! / 1000).toFixed(2)}s`,
  );

  protected readonly noUsageData = computed(() => {
    const log = this.currentLog();
    return !!log && !log.usage && !log.cacheUsage && log.durationMs == null;
  });

  protected onSelectLog(event: Event): void {
    this.selectedLogIndex.set(parseInt((event.target as HTMLSelectElement).value, 10));
  }

  protected optionLabel(log: LlmLogDto): string {
    return `${new Date(log.createdAt).toLocaleTimeString()} - ${log.type} (${log.provider}/${log.modelName})`;
  }

  protected messageBody(msg: LlmLogMessageSummary): string {
    return msg.content || msg.contentPreview || '';
  }

  protected tabLabel(tab: TabType): string {
    return tab.charAt(0).toUpperCase() + tab.slice(1);
  }
}
