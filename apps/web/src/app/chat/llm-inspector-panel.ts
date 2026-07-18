import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  input,
  output,
  signal,
  untracked,
  viewChild,
} from '@angular/core';

import { Icon } from '../ui/icon';
import { SlideOverPanel } from '../ui/slide-over-panel';
import { LLMInspectorEntry } from './llm-inspector-entry';
import type { LlmLogDto } from './llm-logs.api';

/** The filter categories (v4 `LLMInspectorPanel.tsx:9`). */
type FilterCategory = 'all' | 'chat' | 'memory' | 'system' | 'image' | 'safety' | 'other';

/**
 * Which log types each filter admits (v4 `:11-19`) — VERBATIM. `all` is null,
 * meaning "no filtering at all", NOT "every type": several of v4's nineteen log
 * types appear in no group, so they are reachable ONLY under `all`.
 */
const FILTER_GROUPS: Record<FilterCategory, string[] | null> = {
  all: null,
  chat: ['CHAT_MESSAGE', 'TOOL_CONTINUATION'],
  memory: ['MEMORY_EXTRACTION'],
  system: ['TITLE_GENERATION', 'SUMMARIZATION', 'CONTEXT_COMPRESSION'],
  image: ['IMAGE_PROMPT_CRAFTING', 'IMAGE_DESCRIPTION', 'APPEARANCE_RESOLUTION'],
  safety: ['DANGER_CLASSIFICATION'],
  other: ['CHARACTER_WIZARD', 'AI_IMPORT', 'CUSTOM_TOOL_CONSULT'],
};

/** Filter copy (v4 `:21-29`) — VERBATIM. */
const FILTER_LABELS: Record<FilterCategory, string> = {
  all: 'All',
  chat: 'Chat Messages',
  memory: 'Memory',
  system: 'System Ops',
  image: 'Image',
  safety: 'Safety',
  other: 'Other',
};

/** v4 renders the options from `Object.entries(FILTER_LABELS)` — declaration order. */
const FILTER_OPTIONS = Object.entries(FILTER_LABELS) as [FilterCategory, string][];

/** v4 `:74-82` — the delay that lets the panel's slide-in finish before scrolling. */
const SCROLL_DELAY_MS = 300;

/**
 * The LLM Inspector slide-over (v4 `components/chat/LLMInspectorPanel.tsx`): the
 * chat's LLM logs, chronological, filterable, each expandable into
 * request/response/usage detail.
 *
 * The panel does no fetching — the Salon host owns the query (v4's `useLLMLogs`
 * lives in SalonView, and the panel takes `logs`/`loading` as props).
 */
@Component({
  selector: 'qt-llm-inspector-panel',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [SlideOverPanel, LLMInspectorEntry, Icon],
  template: `
    <qt-slide-over-panel
      [isOpen]="isOpen()"
      title="LLM Inspector"
      ariaLabel="LLM Inspector Panel"
      (close)="close.emit()"
    >
      <div qt-slide-over-actions class="flex items-center gap-2">
        <span class="text-xs qt-text-secondary">{{ entryCount() }}</span>

        <!-- The dogfood-#6 select audit: a plain [value] binding is SAFE here
             because the options are STATIC (a module constant, present in the
             DOM before the value is applied). The finding only bites when the
             options arrive async — then the browser drops a value that matches
             no option yet. -->
        <select
          class="qt-input text-xs py-1 px-2 h-auto"
          aria-label="Filter log entries"
          [value]="filter()"
          (change)="onFilterChange($event)"
        >
          @for (option of filterOptions; track option[0]) {
            <option [value]="option[0]">{{ option[1] }}</option>
          }
        </select>

        <button
          type="button"
          class="qt-text-secondary hover:qt-text transition-colors p-1"
          aria-label="Refresh logs"
          title="Refresh logs"
          (click)="refresh.emit()"
        >
          <qt-icon name="refresh" class="w-4 h-4" />
        </button>
      </div>

      <div #scrollContainer class="flex-1 overflow-y-auto px-3 py-3">
        <!-- The empty-state precedence is v4's, in order (v4 :131-147): loading
             beats logging-disabled beats no-entries. A disabled-logging chat
             still shows the spinner first while the query settles. -->
        @if (loading()) {
          <div class="flex items-center justify-center py-12">
            <div class="animate-spin rounded-full h-6 w-6 border-b-2 qt-border-primary"></div>
          </div>
        } @else if (!loggingEnabled()) {
          <div class="text-center py-12 px-4">
            <qt-icon name="ban" class="w-10 h-10 mx-auto mb-3 qt-text-muted" />
            <p class="text-sm qt-text-secondary">LLM logging is disabled.</p>
            <p class="text-xs qt-text-muted mt-1">
              Enable it in Chat Settings to record API interactions.
            </p>
          </div>
        } @else if (filteredLogs().length === 0) {
          <div class="text-center py-12 px-4">
            <qt-icon name="file" class="w-10 h-10 mx-auto mb-3 qt-text-muted" />
            <p class="text-sm qt-text-secondary">No log entries yet.</p>
            <p class="text-xs qt-text-muted mt-1">
              Send a message to start recording LLM interactions.
            </p>
          </div>
        } @else {
          <div class="space-y-2">
            @for (log of filteredLogs(); track log.id) {
              <qt-llm-inspector-entry [log]="log" [isHighlighted]="isHighlighted(log)" />
            }
          </div>
        }
      </div>
    </qt-slide-over-panel>
  `,
})
export class LLMInspectorPanel {
  readonly isOpen = input.required<boolean>();
  readonly logs = input.required<LlmLogDto[]>();
  readonly loading = input(false);
  /** The message whose entry to scroll to + highlight (v4 `scrollToMessageId`). */
  readonly scrollToMessageId = input<string | null>(null);
  /** v4 `loggingEnabled` — `llmLoggingSettings.enabled !== false`, resolved by the host. */
  readonly loggingEnabled = input(true);

  readonly close = output<void>();
  readonly refresh = output<void>();

  protected readonly filter = signal<FilterCategory>('all');
  protected readonly filterOptions = FILTER_OPTIONS;

  private readonly scrollContainer = viewChild<ElementRef<HTMLElement>>('scrollContainer');

  /** Logs arrive DESC; the panel shows them oldest-first (v4 `:54-55`). */
  protected readonly chronologicalLogs = computed(() => [...this.logs()].reverse());

  /** Client-side filtering (v4 `:57-62`). */
  protected readonly filteredLogs = computed(() => {
    const types = FILTER_GROUPS[this.filter()];
    if (!types) return this.chronologicalLogs();
    return this.chronologicalLogs().filter((log) => types.includes(log.type));
  });

  /** v4 `:90-92` — "1 entry" / "N entries". */
  protected readonly entryCount = computed(() => {
    const n = this.filteredLogs().length;
    return `${n} ${n === 1 ? 'entry' : 'entries'}`;
  });

  constructor() {
    /**
     * Scroll the target message's entry into view when the panel opens from a
     * per-message button (v4 `:66-85`).
     *
     * The lookup runs against the FILTERED list on purpose: if the active filter
     * excludes the target, v4 scrolls nowhere rather than to some other entry.
     * The 300 ms delay lets the slide-in animation and the entry render finish —
     * scrollIntoView against an off-screen, mid-transition panel lands wrong.
     */
    effect((onCleanup) => {
      const open = this.isOpen();
      const targetMessageId = this.scrollToMessageId();
      const logs = this.filteredLogs();
      if (!open || !targetMessageId || logs.length === 0) return;

      const targetLog = logs.find((log) => log.messageId === targetMessageId);
      if (!targetLog) return;

      const timer = setTimeout(() => {
        untracked(() => {
          const container = this.scrollContainer()?.nativeElement;
          if (!container) return;
          const targetEl = container.querySelector(`[data-log-id="${targetLog.id}"]`);
          targetEl?.scrollIntoView({ behavior: 'smooth', block: 'center' });
        });
      }, SCROLL_DELAY_MS);
      onCleanup(() => clearTimeout(timer));
    });
  }

  /** v4 `:153` — highlight only while a scroll target is set. */
  protected isHighlighted(log: LlmLogDto): boolean {
    const target = this.scrollToMessageId();
    return target ? log.messageId === target : false;
  }

  protected onFilterChange(event: Event): void {
    this.filter.set((event.target as HTMLSelectElement).value as FilterCategory);
  }
}
