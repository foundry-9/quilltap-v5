import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';

import { Icon } from '../ui/icon';
import type { LlmLogDto, LlmLogMessageSummary } from './llm-logs.api';

/** The expanded-detail tabs (v4 `TabType`). */
type TabType = 'request' | 'response' | 'usage';

/**
 * Badge tint per log type (v4 `LLMInspectorEntry.tsx:14-27`) — VERBATIM.
 *
 * Twelve of v4's nineteen `LLMLogTypeEnum` members appear here; the rest
 * (SCENE_STATE_TRACKING, CHARACTER_OPTIMIZER, EXTERNAL_PROMPT, AUTO_CONFIGURE,
 * IMAGE_GENERATION, WARDROBE_IMAGE_ANALYSIS, ANSWER_CONFIRMATION) fall through to
 * the muted default and render their RAW type string. That is v4's behavior, not
 * an omission — do not "complete" the table.
 */
const TYPE_BADGE_CLASSES: Record<string, string> = {
  CHAT_MESSAGE: 'qt-bg-primary/15 qt-text-primary',
  TOOL_CONTINUATION: 'qt-bg-primary/15 qt-text-primary',
  MEMORY_EXTRACTION: 'qt-bg-info/15 qt-text-info',
  TITLE_GENERATION: 'qt-bg-secondary/50 qt-text-secondary',
  SUMMARIZATION: 'qt-bg-secondary/50 qt-text-secondary',
  CONTEXT_COMPRESSION: 'qt-bg-secondary/50 qt-text-secondary',
  IMAGE_PROMPT_CRAFTING: 'qt-bg-warning/15 qt-text-warning',
  IMAGE_DESCRIPTION: 'qt-bg-warning/15 qt-text-warning',
  APPEARANCE_RESOLUTION: 'qt-bg-warning/15 qt-text-warning',
  DANGER_CLASSIFICATION: 'qt-bg-destructive/15 qt-text-destructive',
  CHARACTER_WIZARD: 'qt-bg-success/15 qt-text-success',
  AI_IMPORT: 'qt-bg-success/15 qt-text-success',
  CUSTOM_TOOL_CONSULT: 'qt-bg-info/15 qt-text-info',
};

/** Badge copy per log type (v4 `:29-42`) — VERBATIM; same twelve-of-nineteen note. */
const TYPE_LABELS: Record<string, string> = {
  CHAT_MESSAGE: 'Chat',
  TOOL_CONTINUATION: 'Tool',
  MEMORY_EXTRACTION: 'Memory',
  TITLE_GENERATION: 'Title',
  SUMMARIZATION: 'Summary',
  CONTEXT_COMPRESSION: 'Compress',
  IMAGE_PROMPT_CRAFTING: 'Img Prompt',
  IMAGE_DESCRIPTION: 'Img Desc',
  APPEARANCE_RESOLUTION: 'Appearance',
  DANGER_CLASSIFICATION: 'Safety',
  CHARACTER_WIZARD: 'Wizard',
  AI_IMPORT: 'Import',
  CUSTOM_TOOL_CONSULT: 'Consult',
};

/** UI truncation threshold for display (v4 `:243` — full content is stored in the DB). */
const UI_TRUNCATE_LENGTH = 500;

/**
 * A truncating text block with a show-more/less toggle (v4's `ExpandableContent`,
 * `:245-265`). Used for the response body.
 *
 * The button copy counts the RENDERED string (`content.length`), unlike the
 * message block below, which counts the logged `contentLength` — v4 differs
 * between the two on purpose-by-accident; both are carried as written.
 */
@Component({
  selector: 'qt-llm-inspector-expandable',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div>
      <pre
        class="font-mono whitespace-pre-wrap overflow-auto max-h-60 p-2 qt-bg-surface-alt rounded"
        >{{ shown() }}</pre>
      @if (needsTruncation()) {
        <button
          type="button"
          class="text-xs text-primary hover:underline mt-1"
          (click)="expanded.set(!expanded())"
        >
          {{
            expanded() ? 'Show less' : 'Show all ' + content().length.toLocaleString() + ' chars'
          }}
        </button>
      }
    </div>
  `,
})
export class LlmInspectorExpandable {
  readonly content = input.required<string>();
  protected readonly expanded = signal(false);

  protected readonly needsTruncation = computed(() => this.content().length > UI_TRUNCATE_LENGTH);
  protected readonly shown = computed(() =>
    this.expanded() || !this.needsTruncation()
      ? this.content()
      : this.content().slice(0, UI_TRUNCATE_LENGTH) + '...',
  );
}

/**
 * One prompt-message block in the Request tab (v4's `MessageContentBlock`,
 * `:267-299`).
 *
 * Two v4 quirks are deliberate here: the truncation test reads the BODY's length
 * while the button copy reports the logged `contentLength` (they can disagree —
 * a body that fell back to `contentPreview` is shorter than the length recorded
 * at log time), and the header always prints `contentLength` raw, un-localized.
 */
@Component({
  selector: 'qt-llm-inspector-message-block',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-bg-surface-alt p-2 rounded">
      <div class="flex justify-between mb-1">
        <span class="qt-text-secondary font-mono">{{ role() }}</span>
        <span class="qt-text-secondary">
          {{ contentLength() }} chars{{ hasAttachments() ? ' (attachments)' : '' }}
        </span>
      </div>
      <p class="qt-text whitespace-pre-wrap break-words">{{ shown() }}</p>
      @if (needsTruncation()) {
        <button
          type="button"
          class="text-xs text-primary hover:underline mt-1"
          (click)="expanded.set(!expanded())"
        >
          {{ expanded() ? 'Show less' : 'Show all ' + contentLength().toLocaleString() + ' chars' }}
        </button>
      }
    </div>
  `,
})
export class LlmInspectorMessageBlock {
  readonly role = input.required<string>();
  readonly contentLength = input.required<number>();
  readonly hasAttachments = input(false);
  readonly body = input.required<string>();

  protected readonly expanded = signal(false);
  protected readonly needsTruncation = computed(() => this.body().length > UI_TRUNCATE_LENGTH);
  protected readonly shown = computed(() =>
    this.expanded() || !this.needsTruncation()
      ? this.body()
      : this.body().slice(0, UI_TRUNCATE_LENGTH) + '...',
  );
}

/**
 * One LLM-log entry in the Inspector (v4 `components/chat/LLMInspectorEntry.tsx`):
 * a collapsed one-line summary that expands into request / response / usage tabs.
 *
 * `data-log-id` is not decoration — it is the hook the panel's scroll-to-message
 * effect queries (v4 `LLMInspectorPanel.tsx:78`).
 */
@Component({
  selector: 'qt-llm-inspector-entry',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, LlmInspectorExpandable, LlmInspectorMessageBlock],
  template: `
    <div
      [attr.data-log-id]="log().id"
      class="qt-inspector-entry"
      [class.qt-inspector-entry-highlight]="isHighlighted()"
    >
      <!-- Collapsed summary — always visible (v4 :73-127) -->
      <button
        type="button"
        class="w-full text-left px-3 py-2.5 flex items-center gap-2 cursor-pointer"
        [attr.aria-expanded]="expanded()"
        (click)="expanded.set(!expanded())"
      >
        <span class="text-xs font-mono qt-text-secondary flex-shrink-0">{{ timestamp() }}</span>

        <span
          class="text-[10px] font-semibold uppercase px-1.5 py-0.5 rounded flex-shrink-0"
          [class]="badgeClass()"
          >{{ typeLabel() }}</span
        >

        <span class="text-xs qt-text-secondary truncate min-w-0 flex-shrink"
          >{{ log().provider }}/{{ log().modelName }}</span
        >

        <span class="flex-1"></span>

        @if (log().usage) {
          <span class="text-xs font-mono qt-text-secondary flex-shrink-0">{{
            tokenSummary()
          }}</span>
        }

        @if (log().durationMs != null) {
          <span class="text-xs qt-text-secondary flex-shrink-0">{{ duration() }}</span>
        }

        @if (log().messageId) {
          <span class="text-xs qt-text-muted flex-shrink-0" title="Linked to a message">
            <qt-icon name="chat" class="w-3.5 h-3.5" />
          </span>
        }

        @if (log().response.error) {
          <span class="qt-text-destructive flex-shrink-0" [title]="log().response.error">
            <qt-icon name="alert-circle" class="w-3.5 h-3.5" />
          </span>
        }

        <qt-icon
          name="chevron-down"
          class="w-4 h-4 qt-text-secondary transition-transform duration-150 flex-shrink-0"
          [class.rotate-180]="expanded()"
        />
      </button>

      <!-- Expanded detail — lazy rendered (v4 :130-157) -->
      @if (expanded()) {
        <div class="border-t qt-border">
          <div class="flex border-b qt-border">
            @for (tab of tabs; track tab) {
              <button
                type="button"
                class="px-3 py-2 qt-text-label-xs transition-colors border-b-2"
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

          <div class="px-3 py-3 max-h-80 overflow-y-auto text-xs">
            @if (activeTab() === 'request') {
              <!-- Request tab (v4 :162-213) -->
              <div class="space-y-3">
                <div class="qt-bg-surface-alt p-2 rounded space-y-1">
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Provider:</span>
                    <span class="qt-text font-mono">{{ log().provider }}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Model:</span>
                    <span class="qt-text font-mono">{{ log().modelName }}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Type:</span>
                    <span class="qt-text font-mono">{{ log().type }}</span>
                  </div>
                </div>

                <div class="qt-bg-surface-alt p-2 rounded space-y-1">
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Messages:</span>
                    <span class="qt-text font-mono">{{ log().request.messageCount }}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Temperature:</span>
                    <span class="qt-text font-mono">{{ temperatureText() }}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Max Tokens:</span>
                    <span class="qt-text font-mono">{{ maxTokensText() }}</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="qt-text-secondary">Tools:</span>
                    <span class="qt-text font-mono">{{ log().request.toolCount }}</span>
                  </div>
                </div>

                <div class="space-y-1.5">
                  <h4 class="font-medium qt-text">Messages</h4>
                  @for (msg of requestMessages(); track $index) {
                    <qt-llm-inspector-message-block
                      [role]="msg.role"
                      [contentLength]="msg.contentLength"
                      [hasAttachments]="msg.hasAttachments"
                      [body]="messageBody(msg)"
                    />
                  }
                </div>
              </div>
            } @else if (activeTab() === 'response') {
              <!-- Response tab (v4 :215-240) -->
              <div class="space-y-3">
                @if (log().response.error) {
                  <div class="p-2 qt-bg-destructive/10 border qt-border-destructive/20 rounded">
                    <h4 class="font-medium qt-text-destructive mb-1">Error</h4>
                    <p class="qt-text">{{ log().response.error }}</p>
                  </div>
                } @else {
                  <div class="p-2 qt-bg-success/10 border qt-border-success/20 rounded">
                    <p class="qt-text font-medium">Request completed successfully</p>
                  </div>
                }

                <div>
                  <h4 class="font-medium qt-text mb-1">
                    Response ({{ log().response.contentLength }} chars)
                  </h4>
                  <qt-llm-inspector-expandable [content]="responseContent()" />
                </div>
              </div>
            } @else {
              <!-- Usage tab (v4 :301-354) -->
              <div class="space-y-3">
                @if (log().usage; as usage) {
                  <div class="qt-bg-surface-alt p-3 rounded grid grid-cols-3 gap-3">
                    <div class="text-center">
                      <p class="qt-heading-4 qt-text">{{ usage.promptTokens.toLocaleString() }}</p>
                      <p class="qt-text-secondary mt-0.5">Prompt</p>
                    </div>
                    <div class="text-center">
                      <p class="qt-heading-4 qt-text">
                        {{ usage.completionTokens.toLocaleString() }}
                      </p>
                      <p class="qt-text-secondary mt-0.5">Completion</p>
                    </div>
                    <div class="text-center">
                      <p class="qt-heading-4 qt-text">{{ usage.totalTokens.toLocaleString() }}</p>
                      <p class="qt-text-secondary mt-0.5">Total</p>
                    </div>
                  </div>
                }

                @if (log().cacheUsage; as cache) {
                  <div class="qt-bg-surface-alt p-2 rounded space-y-1">
                    <!-- Each row renders only when its key is DEFINED (v4 :323, :330)
                         — an explicit 0 is real cache data and must show. -->
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
                }

                @if (log().durationMs != null) {
                  <div class="qt-bg-surface-alt p-2 rounded">
                    <div class="flex justify-between">
                      <span class="qt-text-secondary">Duration:</span>
                      <!-- TWO decimals here; the collapsed row uses ONE (v4 :51 vs
                           :342). v4 is inconsistent on purpose — carry it. -->
                      <span class="qt-text font-mono">{{ usageDuration() }}</span>
                    </div>
                  </div>
                }

                @if (noUsageData()) {
                  <p class="qt-text-secondary p-2 qt-bg-surface-alt rounded text-center">
                    No usage data available for this log
                  </p>
                }
              </div>
            }
          </div>
        </div>
      }
    </div>
  `,
})
export class LLMInspectorEntry {
  readonly log = input.required<LlmLogDto>();
  readonly isHighlighted = input(false);

  protected readonly expanded = signal(false);
  protected readonly activeTab = signal<TabType>('request');
  protected readonly tabs: readonly TabType[] = ['request', 'response', 'usage'] as const;

  protected readonly badgeClass = computed(
    () => TYPE_BADGE_CLASSES[this.log().type] || 'qt-bg-muted qt-text-secondary',
  );
  protected readonly typeLabel = computed(() => TYPE_LABELS[this.log().type] || this.log().type);

  /** v4 `formatTimestamp` (`:54-57`). */
  protected readonly timestamp = computed(() =>
    new Date(this.log().createdAt).toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    }),
  );

  /** v4 `formatTokens` (`:44-47`) — the arrow is U+2192, both counts localized. */
  protected readonly tokenSummary = computed(() => {
    const usage = this.log().usage;
    if (!usage) return '';
    return `${usage.promptTokens.toLocaleString()} → ${usage.completionTokens.toLocaleString()}`;
  });

  /** v4 `formatDuration` (`:49-52`) — ONE decimal in the collapsed row. */
  protected readonly duration = computed(() => {
    const ms = this.log().durationMs;
    return ms === null || ms === undefined ? '' : `${(ms / 1000).toFixed(1)}s`;
  });

  /** The Usage tab's duration — TWO decimals (v4 `:342`). */
  protected readonly usageDuration = computed(
    () => `${(this.log().durationMs! / 1000).toFixed(2)}s`,
  );

  protected readonly requestMessages = computed(() => this.log().request.messages ?? []);

  protected readonly temperatureText = computed(() => {
    const t = this.log().request.temperature;
    return t != null ? String(t) : 'default';
  });
  protected readonly maxTokensText = computed(() => {
    const m = this.log().request.maxTokens;
    return m != null ? String(m) : 'default';
  });

  /**
   * Backward compat: use content, fall back to fullContent, then contentPreview
   * for old entries (v4 `:216-217`). Old logs predate the full-content field, so
   * dropping the chain would blank the body on every historical entry.
   */
  protected readonly responseContent = computed(() => {
    const r = this.log().response;
    return r.content || r.fullContent || r.contentPreview || '';
  });

  /** v4 `:207` — the same fallback chain for a prompt message. */
  protected messageBody(msg: LlmLogMessageSummary): string {
    return msg.content || msg.contentPreview || '';
  }

  /** v4 `:347` — all three absent. */
  protected readonly noUsageData = computed(() => {
    const log = this.log();
    return !log.usage && !log.cacheUsage && log.durationMs == null;
  });

  /** v4 `:145` — `tab.charAt(0).toUpperCase() + tab.slice(1)`. */
  protected tabLabel(tab: TabType): string {
    return tab.charAt(0).toUpperCase() + tab.slice(1);
  }
}
