import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  signal,
} from '@angular/core';

import type { ChatDetail, MessageDto } from '../core/core-contract';
import { resolveMessageAuthor } from './chat-view-model';
import { Avatar } from '../ui/avatar';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';

/**
 * The parsed tool-result envelope (v4 `ToolMessage.tsx:55-75`, `ToolResult`).
 * `result` is a string OR object (older RNG results were objects); `images` is
 * carried for shape-fidelity but the thumbnail render is a loud deferral (see the
 * class docs). Only the fields the card reads are typed here.
 */
interface ToolResult {
  tool?: string;
  toolName?: string;
  initiatedBy?: string;
  /** Display name for the operator on user-initiated runs (e.g. "Charles"). */
  operatorName?: string;
  /** When true, another message owns the run's single visible artifact (the
   *  Pascal bubble for `run_custom`) and this TOOL row renders nothing. */
  delegatedDisplay?: boolean;
  success?: boolean;
  result?: string | Record<string, unknown>;
  arguments?: Record<string, unknown>;
  provider?: string;
  model?: string;
  prompt?: string;
  images?: Array<{ id: string; filename: string }>;
}

/** Display name + emoji for a tool (v4 `ToolMessage.tsx:243-324`, the `toolInfo` map). */
interface ToolDisplay {
  displayName: string;
  icon: string;
}

/** v4's `WARDROBE_ACTION_TOOLS` (`ToolMessage.tsx:78-84`). */
const WARDROBE_ACTION_TOOLS = new Set([
  'wardrobe_wear',
  'wardrobe_take_off',
  'wardrobe_create',
  'wardrobe_update',
  'wardrobe_archive',
]);

/**
 * v4's `toolInfo` name/icon table (`ToolMessage.tsx:243-324`), transcribed
 * field-for-field. The `bgColor` column is dropped — every entry carried the
 * identical `qt-bg-muted border qt-border-default`, which the card applies
 * unconditionally.
 */
const TOOL_INFO: Record<string, ToolDisplay> = {
  generate_image: { displayName: 'Image Generation', icon: '🎨' },
  search: { displayName: 'Search', icon: '🧠' },
  search_web: { displayName: 'Web Search', icon: '🔍' },
  project_info: { displayName: 'Project Info', icon: '📋' },
  rng: { displayName: 'Random Number Generator', icon: '🎲' },
  state: { displayName: 'State Manager', icon: '🗃️' },
  help_search: { displayName: 'Help Search', icon: '📖' },
  help_settings: { displayName: 'Settings Reader', icon: '⚙️' },
  help_navigate: { displayName: 'Navigation', icon: '🧭' },
  wardrobe_list: { displayName: 'Wardrobe', icon: '👗' },
  wardrobe_read: { displayName: 'Wardrobe Item', icon: '👗' },
  wardrobe_wear: { displayName: 'Put On', icon: '👗' },
  wardrobe_take_off: { displayName: 'Take Off', icon: '👗' },
  wardrobe_create: { displayName: 'New Wardrobe Item', icon: '🧵' },
  wardrobe_update: { displayName: 'Edit Wardrobe Item', icon: '🧵' },
  wardrobe_archive: { displayName: 'Archive Wardrobe Item', icon: '🧵' },
};

/** A human-readable wardrobe action notice (v4 `buildWardrobeActionSummary` :90-146). */
interface WardrobeSummary {
  label: string;
  lines: string[];
}

/**
 * Build the wardrobe action summary (v4 `ToolMessage.tsx:90-146`). Returns null
 * unless the tool is a successful wardrobe action tool whose result yields at
 * least one line.
 */
function buildWardrobeActionSummary(toolData: ToolResult): WardrobeSummary | null {
  if (!toolData.success || !toolData.toolName || !WARDROBE_ACTION_TOOLS.has(toolData.toolName)) {
    return null;
  }
  const result = toolData.result as Record<string, unknown> | undefined;
  if (!result || typeof result !== 'object') return null;

  const lines: string[] = [];

  if (toolData.toolName === 'wardrobe_wear' || toolData.toolName === 'wardrobe_take_off') {
    const operations =
      (result['operations'] as Array<{ effect_summary?: string; error?: string }> | undefined) ??
      [];
    const coverageSummary = result['coverage_summary'] as string | undefined;
    for (const op of operations) {
      if (op.effect_summary) lines.push(op.effect_summary);
    }
    if (coverageSummary) lines.push(coverageSummary);
    return lines.length > 0 ? { label: 'Wardrobe', lines } : null;
  }

  if (toolData.toolName === 'wardrobe_create') {
    const title = result['title'] as string | undefined;
    const equipped = result['equipped'] as boolean | undefined;
    const recipientName = result['recipient_name'] as string | undefined;
    if (recipientName) {
      lines.push(`Gifted "${title}" to ${recipientName}.`);
      if (equipped) {
        lines.push(`${recipientName} put it on immediately.`);
      }
    } else if (title) {
      if (equipped) {
        lines.push(`Created and equipped "${title}".`);
      } else {
        lines.push(`Created "${title}" and added it to the wardrobe.`);
      }
    }
    return lines.length > 0 ? { label: 'Wardrobe', lines } : null;
  }

  if (toolData.toolName === 'wardrobe_update') {
    const title = result['title'] as string | undefined;
    if (title) lines.push(`Updated "${title}".`);
    return lines.length > 0 ? { label: 'Wardrobe', lines } : null;
  }

  if (toolData.toolName === 'wardrobe_archive') {
    const title = result['title'] as string | undefined;
    if (title) lines.push(`Archived "${title}" (a human can restore it).`);
    return lines.length > 0 ? { label: 'Wardrobe', lines } : null;
  }

  return null;
}

/** First line of `content`, sliced to `maxLength` + `...` (v4 `getPreviewText` :174-178). */
function getPreviewText(content: string, maxLength = 80): string {
  const firstLine = content.split('\n')[0] || '';
  if (firstLine.length <= maxLength) return firstLine;
  return firstLine.slice(0, maxLength) + '...';
}

/** Prompt, else `JSON.stringify(arguments || {}, null, 2)` (v4 `formatRequestContent` :183-188). */
function formatRequestContent(toolData: ToolResult): string {
  if (toolData.prompt) {
    return toolData.prompt;
  }
  return JSON.stringify(toolData.arguments || {}, null, 2);
}

/**
 * Pretty-print the tool result (v4 `formatResultContent` :193-209): empty when
 * absent; an object honours `formattedText` else stringifies; a string is
 * re-parsed-and-pretty-printed when it is itself JSON, else passed through.
 */
function formatResultContent(toolData: ToolResult): string {
  if (!toolData.result) return '';
  const resultString =
    typeof toolData.result === 'object'
      ? ((toolData.result as Record<string, unknown>)['formattedText'] as string) ||
        JSON.stringify(toolData.result, null, 2)
      : toolData.result;
  try {
    const parsed = JSON.parse(resultString);
    return JSON.stringify(parsed, null, 2);
  } catch {
    return resultString;
  }
}

/**
 * One tool-result card — a port of v4 `components/chat/ToolMessage.tsx`. Renders
 * a `role === 'TOOL'` message as a collapsible Tool Request / Tool Response card
 * with a Success/Failed badge, a tool-icon header, and an attribution line,
 * instead of a raw-JSON whisper bubble (the 2026-07-22 dogfood finding).
 *
 * Two layouts (v4 `embedded` prop):
 *  - **standalone** (`embedded=false`): a full row with an author avatar column
 *    (`qt-chat-message-row-tool`), used for user-initiated / Prospero / orphan
 *    tool rows.
 *  - **embedded** (`embedded=true`): the card alone (`qt-chat-tool-embedded`),
 *    nested inside a character's message bubble by `MessageRow` for
 *    character-initiated runs. Leads with the tool emoji, drops the redundant
 *    "&lt;character&gt; ran" attribution.
 *
 * Collapse state is two independent local booleans, both default-collapsed —
 * there is NO persisted setting for this in v4 (searched; none exists), so v5
 * invents none.
 *
 * ## Deferrals (LOUD — nothing stubbed)
 *
 * - **Image thumbnails.** v4 renders `generate_image` result thumbnails inside
 *   the Tool Response collapsible (`:548-591`) and a trailing thumbnail strip
 *   for other tools (`:604-667`), with a copy-image button, a missing-image
 *   fallback, and `DeletedImagePlaceholder` cleanup. v5 renders generated chat
 *   images through the assistant message's own attachment strip + the markdown
 *   store-image rewrite (P4.6ac), so the tool card omits both blocks. The
 *   collapsed preview still reports `N image(s)` for a `generate_image` row that
 *   carries attachments but no text result, matching v4's `:529-533`. This is
 *   the order's tier-3 deferral; it is enumerated in the lane report.
 */
@Component({
  selector: 'qt-tool-message',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Avatar, Icon],
  template: `
    @if (!toolData().delegatedDisplay) {
      <!-- Standalone uses a full-width row + author avatar; embedded drops the
           row/avatar and renders the card directly (v4 :383-408). -->
      <div [class]="embedded() ? 'qt-chat-tool-embedded' : 'qt-chat-message-row-tool'">
        @if (!embedded()) {
          @if (headerAvatar()?.avatarUrl) {
            <div class="qt-chat-desktop-avatar">
              <qt-avatar
                [name]="headerAvatar()!.name"
                [src]="headerAvatar()!.avatarUrl ?? null"
                size="chat"
              />
            </div>
          } @else {
            <!-- No portrait — the tool's emoji on a muted circle, with a
                 provider/model tooltip (v4 :396-407). -->
            <div
              class="flex-shrink-0 flex items-center justify-center w-10 h-10 rounded-full qt-bg-muted text-lg"
              [title]="providerModel()"
            >
              {{ info().icon }}
            </div>
          }
        }

        <div [class]="embedded() ? 'min-w-0' : 'flex-1 min-w-0'">
          <!-- Wardrobe action notice — the prominent summary above the card. -->
          @if (wardrobeSummary(); as ws) {
            <div class="qt-chat-wardrobe-notice mb-2">
              <div class="qt-chat-wardrobe-label">{{ ws.label }}</div>
              <div class="qt-chat-wardrobe-summary">
                @for (line of ws.lines; track $index) {
                  <div>{{ line }}</div>
                }
              </div>
            </div>
          }

          <div class="px-4 py-3 rounded-lg qt-bg-muted border qt-border-default">
            <!-- Tool header -->
            <div class="flex items-center gap-2 mb-2">
              <div class="flex flex-col gap-1">
                @if (headerAvatar(); as ha) {
                  <div class="flex items-center gap-2">
                    <span class="font-semibold text-sm text-foreground">{{ ha.name }}</span>
                    @if (isWhisper()) {
                      <span class="qt-text-label-xs italic qt-text-secondary">whisper</span>
                    }
                  </div>
                  <div class="qt-text-label-xs">
                    {{ attributionPrefix() }}<span class="font-mono">{{ toolData().toolName }}</span>
                  </div>
                } @else {
                  <div class="flex items-center gap-2">
                    @if (embedded()) {
                      <span class="text-base leading-none" aria-hidden="true">{{ info().icon }}</span>
                    } @else if (actorName()) {
                      <span class="qt-text-label-xs">{{ actorName() }} ran</span>
                    }
                    <span class="font-semibold text-sm text-foreground">{{ info().displayName }}</span>
                  </div>
                }
              </div>
              <span
                class="inline-block px-2 py-0.5 qt-text-label-xs rounded ml-auto"
                [class.qt-badge-success]="toolData().success"
                [class.qt-badge-destructive]="!toolData().success"
              >
                {{ toolData().success ? 'Success' : 'Failed' }}
              </span>
            </div>

            <!-- Tool Request collapsible — the arguments/prompt sent to the tool. -->
            @if (hasRequest()) {
              <div class="mt-2">
                <div class="flex items-center gap-2">
                  <button
                    type="button"
                    class="qt-text-label-xs hover:text-foreground transition-colors flex items-center gap-1"
                    [attr.aria-expanded]="showRequest()"
                    (click)="showRequest.set(!showRequest())"
                  >
                    <span class="w-3 inline-block">{{ showRequest() ? '▼' : '▶' }}</span>
                    <span>Tool Request</span>
                  </button>
                  @if (!showRequest()) {
                    <span class="qt-text-xs qt-text-secondary truncate flex-1">{{ requestPreview() }}</span>
                  }
                  <button
                    type="button"
                    class="p-1 qt-text-secondary hover:text-foreground transition-colors"
                    title="Copy request"
                    aria-label="Copy request"
                    (click)="copyRequest()"
                  >
                    <qt-icon name="copy" class="w-4 h-4" />
                  </button>
                </div>
                @if (showRequest()) {
                  <div class="mt-2 bg-background rounded p-3 border qt-border-default">
                    <pre class="text-xs text-foreground font-mono whitespace-pre-wrap break-words">{{ requestContent() }}</pre>
                  </div>
                }
              </div>
            }

            <!-- Tool Response collapsible — the pretty-printed result. -->
            @if (hasResponse()) {
              <div class="mt-2">
                <div class="flex items-center gap-2">
                  <button
                    type="button"
                    class="qt-text-label-xs hover:text-foreground transition-colors flex items-center gap-1"
                    [attr.aria-expanded]="showResponse()"
                    (click)="showResponse.set(!showResponse())"
                  >
                    <span class="w-3 inline-block">{{ showResponse() ? '▼' : '▶' }}</span>
                    <span>Tool Response</span>
                  </button>
                  @if (!showResponse() && toolData().result) {
                    <span class="qt-text-xs qt-text-secondary truncate flex-1">{{ responsePreview() }}</span>
                  } @else if (!showResponse() && imageCount() > 0) {
                    <span class="qt-text-xs qt-text-secondary">{{ imageCount() }} image{{ imageCount() > 1 ? 's' : '' }}</span>
                  }
                  @if (toolData().result) {
                    <button
                      type="button"
                      class="p-1 qt-text-secondary hover:text-foreground transition-colors"
                      title="Copy response"
                      aria-label="Copy response"
                      (click)="copyResponse()"
                    >
                      <qt-icon name="copy" class="w-4 h-4" />
                    </button>
                  }
                </div>
                @if (showResponse() && toolData().result) {
                  <div class="mt-2 bg-background rounded p-3 border qt-border-default tool-response-content">
                    <pre class="text-xs text-foreground font-mono whitespace-pre-wrap break-words">{{ responseContent() }}</pre>
                  </div>
                }
              </div>
            }


            <!-- Timestamp -->
            <div class="qt-text-xs mt-2">{{ timestamp() }}</div>
          </div>
        </div>
      </div>
    }
  `,
})
export class ToolMessage {
  private readonly toasts = inject(ToastService);
  readonly message = input.required<MessageDto>();
  readonly chat = input.required<ChatDetail>();
  readonly embedded = input(false);

  protected readonly showRequest = signal(false);
  protected readonly showResponse = signal(false);

  /**
   * Parse `message.content` into the `ToolResult` shape (v4 :216-232), handling
   * BOTH the old `toolName` and the new `tool` key, with the parse-failure
   * fallback.
   */
  protected readonly toolData = computed<ToolResult>(() => {
    try {
      const parsed = JSON.parse(this.message().content) as ToolResult & Record<string, unknown>;
      return {
        ...parsed,
        toolName: (parsed.toolName as string) || (parsed.tool as string) || 'unknown',
      };
    } catch {
      return { toolName: 'unknown', success: false, result: 'Unable to parse tool result' };
    }
  });

  /** The name/icon for the tool, or the ⚙️ fallback (v4 :326-330). */
  protected readonly info = computed<ToolDisplay>(() => {
    const name = this.toolData().toolName!;
    return TOOL_INFO[name] ?? { displayName: name, icon: '⚙️' };
  });

  protected readonly wardrobeSummary = computed(() => buildWardrobeActionSummary(this.toolData()));

  protected readonly isWhisper = computed(() => {
    const ids = this.message().targetParticipantIds;
    return !!(ids && ids.length > 0);
  });

  /**
   * The standalone author (v4 `headerAvatar`, resolved upstream by
   * `getMessageAvatar` — `VirtualizedMessageList.tsx:240-247` hands that
   * function's whole result through, Staff rows included). Null when embedded:
   * the character's own avatar already heads the bubble.
   *
   * P4.26: this used to special-case a `systemSender` row to the display name
   * with a hardcoded `avatarUrl: null`, because `resolveMessageAuthor` had no
   * Staff arm to defer to. It has one now, so the special case is gone and a
   * standalone Prospero run wears Prospero's portrait, as it does in v4.
   */
  protected readonly headerAvatar = computed<{ name: string; avatarUrl: string | null } | null>(
    () => {
      if (this.embedded()) return null;
      const a = resolveMessageAuthor(this.message(), this.chat());
      return { name: a.name, avatarUrl: a.avatarUrl };
    },
  );

  private readonly isUserInitiated = computed(() => this.toolData().initiatedBy === 'user');

  /**
   * The acting party (v4 :375-378): the operator on a user-initiated run
   * (`operatorName || 'You'`), else the resolved author name.
   */
  protected readonly actorName = computed<string | null>(() => {
    if (this.isUserInitiated()) {
      return this.toolData().operatorName || 'You';
    }
    return this.headerAvatar()?.name ?? null;
  });

  /**
   * The attribution line prefix in the named-author branch (v4 :438-443):
   * "&lt;actor&gt; ran " when the actor differs from the header name, else "ran ".
   */
  protected readonly attributionPrefix = computed(() => {
    const actor = this.actorName();
    const headerName = this.headerAvatar()?.name;
    return actor && actor !== headerName ? `${actor} ran ` : 'ran ';
  });

  protected readonly providerModel = computed(() => {
    const d = this.toolData();
    return d.provider && d.model ? `${d.provider} ${d.model}` : '';
  });

  /** v4 :604/:513 — image attachments (image/* MIME). */
  private readonly imageAttachments = computed(() =>
    (this.message().attachments || []).filter((a) => a.mimeType.startsWith('image/')),
  );
  protected readonly imageCount = computed(() => this.imageAttachments().length);

  /** Tool Request section renders when `arguments || prompt` (v4 :476). */
  protected readonly hasRequest = computed(() => {
    const d = this.toolData();
    return !!(d.arguments || d.prompt);
  });

  /**
   * Tool Response section renders when there is a result, or a `generate_image`
   * row with image attachments (v4 :513). The image branch keeps the collapsed
   * "N images" preview even though v5 defers the thumbnails themselves.
   */
  protected readonly hasResponse = computed(() => {
    const d = this.toolData();
    return !!(d.result || (d.toolName === 'generate_image' && this.imageCount() > 0));
  });

  protected readonly requestContent = computed(() => formatRequestContent(this.toolData()));
  protected readonly responseContent = computed(() => formatResultContent(this.toolData()));
  protected readonly requestPreview = computed(() => getPreviewText(this.requestContent()));
  protected readonly responsePreview = computed(() => getPreviewText(this.responseContent()));

  protected readonly timestamp = computed(() => {
    const d = new Date(this.message().createdAt);
    return Number.isNaN(d.getTime())
      ? ''
      : d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  });

  protected copyRequest(): void {
    void this.copy(this.requestContent(), 'Request copied to clipboard');
  }

  protected copyResponse(): void {
    void this.copy(this.responseContent(), 'Response copied to clipboard');
  }

  /** v4 `:337-349` — both arms are toasts. */
  private async copy(text: string, successMessage: string): Promise<void> {
    try {
      await navigator.clipboard?.writeText(text);
      this.toasts.showSuccess(successMessage);
    } catch {
      this.toasts.showError('Failed to copy to clipboard');
    }
  }
}
