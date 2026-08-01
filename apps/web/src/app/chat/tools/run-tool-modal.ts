import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import type { ParticipantDetail } from '../../core/core-contract';
import { Icon } from '../../ui/icon';
import { Modal } from '../../ui/modal';
import { listTools, runTool } from '../chat-admin.api';
import { JsonSchemaForm, type FormValues, type ParametersSchema } from './json-schema-form';
import type { AvailableTool } from './tool-settings';

/** v4 `CATEGORY_INFO` (`RunToolModal.tsx:30-40`). */
const CATEGORY_INFO: Record<string, { label: string; icon: string }> = {
  media: { label: 'Media', icon: '🎨' },
  memory: { label: 'Memory', icon: '🧠' },
  search: { label: 'Search', icon: '🔍' },
  project: { label: 'Project', icon: '📋' },
  files: { label: 'Files', icon: '📁' },
  help: { label: 'Help', icon: '📖' },
  utility: { label: 'Utility', icon: '🔧' },
  shell: { label: 'Shell', icon: '💻' },
  plugin: { label: 'Plugin', icon: '🔌' },
};

/** v4 strips blank arguments before sending (`:127-132`). */
export function cleanArguments(values: FormValues): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(values)) {
    if (value !== undefined && value !== null && value !== '') out[key] = value;
  }
  return out;
}

/** v4's unknown-category fallback (`:283`). */
export function categoryInfo(category: string): { label: string; icon: string } {
  return (
    CATEGORY_INFO[category] ?? {
      label: category.charAt(0).toUpperCase() + category.slice(1),
      icon: '⚙️',
    }
  );
}

/**
 * Run Tool (v4 `components/chat/RunToolModal.tsx`, 409 LOC) — the Chat section's
 * "Run Tool…" entry, an operator-initiated tool call.
 *
 * ## Ported exactly, and why
 *
 * - **Only `userInvocable !== false` tools are listed** (`:85`) — a tool the
 *   model may call is not necessarily one a person should.
 * - **Search runs over name, description AND id** (`:162-168`); the results are
 *   grouped by `category` through {@link categoryInfo}, whose unknown-category
 *   fallback is title-case plus ⚙️.
 * - **An unavailable tool is listed, disabled, at half opacity, with its reason
 *   in place of its description** (`:292-312`) — it is information, not noise.
 * - **Phase 2 pre-populates from the schema's defaults** (`:99-111`) and strips
 *   blanks again before sending (`:127-132`), so a field left alone never travels
 *   as an empty string.
 * - **`toolName` is the tool's ID, not its display name** (`:139`).
 * - **"Run as character" lists ACTIVE character participants and sends the
 *   CHARACTER id**, not the participant id (`:60-62,346`).
 * - The JSON preview is labelled `(valid)`/`(incomplete)` off the form's own
 *   validity (`:391-398`), and a tool with no properties says so rather than
 *   showing an empty form (`:400-404`).
 */
@Component({
  selector: 'qt-run-tool-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, Icon, JsonSchemaForm],
  template: `
    <qt-modal [title]="title()" maxWidth="xl" (close)="close.emit()">
      @if (toolsQuery.isLoading()) {
        <div class="flex items-center justify-center py-8 qt-text-secondary">Loading tools…</div>
      } @else if (!selected()) {
        <div class="space-y-3">
          <div class="relative">
            <qt-icon
              name="search"
              class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 qt-text-secondary"
            />
            <input
              type="text"
              class="qt-input w-full pl-9 text-sm"
              placeholder="Search tools..."
              aria-label="Search tools"
              [value]="search()"
              (input)="search.set($any($event.target).value)"
            />
          </div>

          @if (grouped().length === 0) {
            <p class="text-sm qt-text-secondary text-center py-4">
              {{ search() ? 'No tools match your search.' : 'No tools available.' }}
            </p>
          } @else {
            @for (group of grouped(); track group.category) {
              <div>
                <div
                  class="text-xs font-semibold qt-text-secondary uppercase tracking-wide mb-1.5 flex items-center gap-1.5"
                >
                  <span>{{ group.icon }}</span>
                  <span>{{ group.label }}</span>
                </div>
                <div class="space-y-1">
                  @for (tool of group.tools; track tool.id) {
                    <button
                      type="button"
                      [class]="
                        'w-full text-left px-3 py-2 rounded-md text-sm transition-colors ' +
                        (tool.available === false
                          ? 'opacity-50 cursor-not-allowed qt-bg-surface'
                          : 'qt-bg-surface hover:qt-bg-surface-hover cursor-pointer')
                      "
                      [disabled]="tool.available === false"
                      [title]="
                        tool.available === false ? tool.unavailableReason || '' : tool.description
                      "
                      (click)="selectTool(tool)"
                    >
                      <div class="font-medium qt-text">{{ tool.name }}</div>
                      <div class="text-xs qt-text-secondary mt-0.5">
                        @if (tool.available === false) {
                          <span class="qt-text-warning">{{ tool.unavailableReason }}</span>
                        } @else {
                          {{ tool.description }}
                        }
                      </div>
                      @if (tool.source === 'plugin') {
                        <span
                          class="inline-block mt-1 text-[10px] px-1.5 py-0.5 rounded qt-bg-muted qt-text-secondary"
                          >plugin{{ tool.pluginName ? ': ' + tool.pluginName : '' }}</span
                        >
                      }
                    </button>
                  }
                </div>
              </div>
            }
          }
        </div>
      } @else {
        <div class="space-y-4">
          <div class="text-sm qt-text-secondary">{{ selected()!.description }}</div>

          @if (activeCharacters().length > 0) {
            <div class="border-t qt-border pt-3">
              <label for="qt-run-tool-as" class="block text-sm font-medium qt-text mb-1">
                Run as character
              </label>
              <select
                id="qt-run-tool-as"
                class="qt-input w-full text-sm"
                [value]="characterId() ?? ''"
                (change)="characterId.set($any($event.target).value || null)"
              >
                @for (p of activeCharacters(); track p.id) {
                  <option
                    [value]="p.character?.id || ''"
                    [selected]="characterId() === p.character?.id"
                  >
                    {{ p.character?.name || 'Unknown' }}
                  </option>
                }
              </select>
            </div>
          }

          <div class="border-t qt-border pt-3">
            <label class="flex items-start gap-2 cursor-pointer">
              <input
                type="checkbox"
                class="mt-0.5"
                aria-label="Private (whisper)"
                [checked]="isPrivate()"
                (change)="isPrivate.set($any($event.target).checked)"
              />
              <div class="flex-1">
                <div class="text-sm font-medium qt-text">Private (whisper)</div>
                <div class="text-xs qt-text-secondary">
                  Excluded from every character&apos;s context — the run stays between you and
                  Prospero, and remains in your own view of the salon.
                </div>
              </div>
            </label>
          </div>

          @if (schema(); as parameters) {
            <div class="border-t qt-border pt-3">
              <h4 class="text-sm font-medium qt-text mb-3">Parameters</h4>
              <qt-json-schema-form
                [schema]="parameters"
                [values]="formValues()"
                (valuesChange)="formValues.set($event)"
                (validChange)="formValid.set($event)"
              />
            </div>

            <details class="border-t qt-border pt-3">
              <summary class="text-xs qt-text-secondary cursor-pointer select-none">
                Arguments preview {{ formValid() ? '(valid)' : '(incomplete)' }}
              </summary>
              <pre
                class="mt-2 text-xs qt-bg-muted p-2 rounded overflow-x-auto font-mono max-h-32"
                >{{ preview() }}</pre
              >
            </details>
          } @else {
            <p class="text-sm qt-text-secondary italic">This tool requires no parameters.</p>
          }

        </div>
      }

      <div qt-modal-footer class="flex justify-between items-center w-full">
        @if (selected()) {
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="executing()"
            (click)="back()"
          >
            Back
          </button>
          <div class="flex gap-2">
            <button
              type="button"
              class="qt-button qt-button-secondary"
              [disabled]="executing()"
              (click)="close.emit()"
            >
              Cancel
            </button>
            <button
              type="button"
              class="qt-button qt-button-primary"
              [disabled]="executing() || (!!schema() && !formValid())"
              (click)="execute()"
            >
              {{ executing() ? 'Running...' : 'Run Tool' }}
            </button>
          </div>
        } @else {
          <span></span>
          <button type="button" class="qt-button qt-button-secondary" (click)="close.emit()">
            Cancel
          </button>
        }
      </div>
    </qt-modal>
  `,
})
export class RunToolModal {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly chatId = input.required<string>();
  readonly participants = input.required<ParticipantDetail[]>();

  /** v4 `onToolExecuted()` — the parent refetches. */
  readonly executed = output<void>();
  readonly close = output<void>();

  protected readonly search = signal('');
  protected readonly selected = signal<AvailableTool | null>(null);
  protected readonly formValues = signal<FormValues>({});
  protected readonly formValid = signal(false);
  protected readonly executing = signal(false);
  protected readonly isPrivate = signal(false);
  protected readonly characterId = signal<string | null>(null);

  protected readonly toolsQuery = injectQuery(() => ({
    queryKey: ['tools', this.chatId(), 'schemas'],
    queryFn: () => listTools(this.core, { chatId: this.chatId(), includeSchemas: true }),
  }));

  /** v4 filters to user-invocable tools on arrival (`:85`). */
  protected readonly tools = computed(() =>
    (this.toolsQuery.data() ?? []).filter((t) => t.userInvocable !== false),
  );

  /** v4 `activeCharacters` (`:60-62`). */
  protected readonly activeCharacters = computed(() =>
    this.participants().filter((p) => p.type === 'CHARACTER' && p.isActive && !p.removedAt),
  );

  constructor() {
    // v4 answers a failed tool-inventory load with a toast.
    effect(() => {
      if (this.toolsQuery.isError()) {
        this.toasts.showError('Failed to load available tools');
      }
    });
    // v4 seeds the character select to the first active character (`:67,77`).
    effect(() => {
      const first = this.activeCharacters()[0]?.character?.id ?? null;
      if (first && this.characterId() === null) this.characterId.set(first);
    });
  }

  protected readonly title = computed(() =>
    this.selected() ? `Run Tool: ${this.selected()!.name}` : 'Run Tool',
  );

  protected readonly grouped = computed(() => {
    const needle = this.search().toLowerCase();
    const filtered = needle
      ? this.tools().filter(
          (t) =>
            t.name.toLowerCase().includes(needle) ||
            t.description.toLowerCase().includes(needle) ||
            t.id.toLowerCase().includes(needle),
        )
      : this.tools();

    const buckets = new Map<string, AvailableTool[]>();
    for (const tool of filtered) {
      const cat = tool.category || 'other';
      const bucket = buckets.get(cat) ?? [];
      bucket.push(tool);
      buckets.set(cat, bucket);
    }
    return [...buckets].map(([category, tools]) => ({
      category,
      ...categoryInfo(category),
      tools,
    }));
  });

  /** v4 `hasSchema` (`:178-181`) — a schema with no properties is no schema. */
  protected readonly schema = computed<ParametersSchema | null>(() => {
    const params = this.selected()?.parameters;
    if (!params || typeof params !== 'object') return null;
    const candidate = params as ParametersSchema;
    if (!candidate.properties || Object.keys(candidate.properties).length === 0) return null;
    return candidate;
  });

  protected readonly preview = computed(() =>
    JSON.stringify(cleanArguments(this.formValues()), null, 2),
  );

  /** v4 `handleSelectTool` (`:96-113`). */
  protected selectTool(tool: AvailableTool): void {
    if (tool.available === false) return;
    const defaults: FormValues = {};
    const params = tool.parameters as ParametersSchema | undefined;
    for (const [key, prop] of Object.entries(params?.properties ?? {})) {
      if (prop.default !== undefined) defaults[key] = prop.default;
    }
    this.formValues.set(defaults);
    this.formValid.set(false);
    this.selected.set(tool);
  }

  /** v4 `handleBack` (`:115-119`). */
  protected back(): void {
    this.selected.set(null);
    this.formValues.set({});
    this.formValid.set(false);
  }

  /** v4 `handleExecute` (`:121-159`). */
  protected async execute(): Promise<void> {
    const tool = this.selected();
    if (!tool) return;
    this.executing.set(true);
    try {
      await runTool(this.core, {
        chatId: this.chatId(),
        // The ID, not the display name (`:139`).
        toolName: tool.id,
        arguments: cleanArguments(this.formValues()),
        characterId: this.characterId() ?? undefined,
        private: this.isPrivate(),
      });
      this.executed.emit();
      this.close.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to execute tool');
    } finally {
      this.executing.set(false);
    }
  }
}
