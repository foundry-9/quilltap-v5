import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { Router } from '@angular/router';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { WORKSPACE_HANDLE } from '../workspace/workspace-contract';
import { CoreClient, coreErrorMessage } from '../core/core-client';
import { Icon } from '../ui/icon';
import { Modal } from '../ui/modal';
import {
  CustomToolParamsForm,
  coerceParamValues,
  initialParamValues,
  type ParamChange,
  type ParameterFormValues,
} from './custom-tool-params-form';
import {
  customToolsKeys,
  fetchCustomToolsRoster,
  runCustomTool,
  type CustomToolRosterEntry,
} from './custom-tools.api';
import { ToastService } from '../ui/toast.service';

/**
 * The composer's wand and the two-phase modal it opens — a port of v4
 * `components/chat/CustomToolsButton.tsx` + `CustomToolRunDialog.tsx`
 * (`faab6881`, which replaced the cramped 288 px upward popover
 * `CustomToolsDropdown.tsx` this file used to mirror).
 *
 * v4 splits the button from the dialog because the dialog is rendered
 * unconditionally, closed, so it keeps its memory. v5 folds them back into ONE
 * component for two reasons: the memory can simply live on the component, which
 * stays mounted whether or not the dialog is up; and the roster query is what
 * decides whether the button exists at all, so it cannot live behind the open
 * flag (v4 reads its `customToolsAvailable` off the chat payload instead — a
 * standing, documented v5 divergence).
 *
 * Phase one lists the roster (description, source store and path, a wrench to
 * the Workbench, a search box past six tools). Choosing a tool replaces the
 * dialog's body with that tool's form; "Choose another tool" goes back. Escape,
 * the backdrop, the ✕, and Cancel all dismiss it.
 *
 * The roster is fetched fresh on every open (v4 `enabled: isOpen`) — custom-tool
 * definitions live in the user's document stores and can change mid-chat.
 *
 * ## What it remembers
 *
 * Everything you last set, for as long as the composer stays mounted: the
 * parameter values per tool, the privacy toggle per tool, and which tool you
 * were on. **The selection is DERIVED from the roster rather than stored**, so a
 * tool renamed, disabled, or gated away between opens drops back to the picker
 * instead of stranding the dialog on a tool nobody can deal.
 *
 * ## What it discloses
 *
 * Odds and outcome tables are NEVER shown; the roster payload doesn't carry
 * them. What it does carry is each tool's `references` (Shared contract §1) —
 * the placeholders the definition quotes — which the reference panel lists so a
 * user can see what the tool reads about their character. Vocabulary, not odds.
 *
 * A failed run is reported through the toast, as v4 does
 * (`CustomToolRunDialog.tsx:202`); the inline `role="alert"` line in the picker
 * is v4's own ROSTER-load error (`:368`), a different surface.
 */

/** Above this many tools, the picker earns a search box. */
const SEARCH_THRESHOLD = 6;

/** One row of the reference panel: a placeholder and what it stands for. */
interface ReferenceRow {
  placeholder: string;
  meaning: string;
}

@Component({
  selector: 'qt-custom-tools-popup',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Modal, CustomToolParamsForm],
  host: { '(document:keydown.escape)': 'onEscape($event)' },
  template: `
    @if (available()) {
      <!-- v4 drops the popover's positioning wrapper here (.qt-composer-gutter-dropdown,
           which v5 never had). The plain block wrapper stays: an Angular host element
           is the flex item either way, and keeping the button a BLOCK child of it is
           exactly the box the toolbar has been laying out all along. Removing the
           "relative" class — which had no positioned descendant left — is the whole
           delta. -->
      <div>
        <button
          type="button"
          class="qt-chat-toolbar-button"
          title="Custom tools"
          aria-label="Custom tools"
          aria-haspopup="dialog"
          [attr.aria-expanded]="isOpen()"
          [disabled]="disabled()"
          (click)="openDialog()"
        >
          <qt-icon name="wand" class="w-5 h-5" />
        </button>
      </div>
    }

    @if (isOpen()) {
      <qt-modal [title]="dialogTitle()" maxWidth="2xl" (close)="close()">
        <!-- Both slots below are STATIC children of qt-modal, each switching
             phase inside itself. Angular matches projection selectors against
             the compile-time children, so a [qt-modal-footer] nested inside an
             @if would land in the default slot instead of the footer. -->
        <div>
          @if (selectedTool(); as tool) {
            <!-- =============== Phase two — the chosen tool =============== -->
            <div class="space-y-5">
              <div class="flex items-start justify-between gap-4 px-1">
                <div class="min-w-0">
                  @if (tool.description) {
                    <p class="text-sm qt-text-secondary">{{ tool.description }}</p>
                  }
                  <p class="text-[10px] qt-text-secondary mt-1.5 font-mono break-all">
                    {{ tool.mountName }} · {{ tool.definitionPath
                    }}{{ tool.characterLabel ? ' · as ' + tool.characterLabel : '' }}
                  </p>
                </div>
                @if (tool.mountPointId) {
                  <button
                    type="button"
                    class="qt-text-secondary hover:opacity-70 flex-shrink-0"
                    [title]="workbenchTitle"
                    [attr.aria-label]="workbenchTitle"
                    (click)="
                      openWorkbench({ mountPointId: tool.mountPointId, path: tool.definitionPath })
                    "
                  >
                    <qt-icon name="wrench" class="w-4 h-4" />
                  </button>
                }
              </div>


              <section class="rounded-lg qt-border p-5">
                <h4 class="text-sm font-medium qt-text mb-4">
                  {{ paramNames().length > 0 ? 'What you are setting' : 'Nothing to set' }}
                </h4>
                @if (paramNames().length > 0) {
                  <div class="space-y-6">
                    <qt-custom-tool-params-form
                      [parameters]="tool.parameters"
                      [values]="valuesFor(keyOf(tool), tool)"
                      [disabled]="running()"
                      [idPrefix]="'custom-tool-' + keyOf(tool)"
                      layout="stacked"
                      (change)="onParamChange(keyOf(tool), $event)"
                    />
                  </div>
                } @else {
                  <p class="text-sm qt-text-secondary">
                    This tool takes no parameters. There is nothing to do but spin.
                  </p>
                }
              </section>

              <!-- Above the reference panel on purpose: this is the last decision
                   the operator makes about the run, and the panel below it is
                   reading matter rather than a control. -->
              <section class="rounded-lg qt-border p-5">
                <label class="flex items-start gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    class="mt-1"
                    [checked]="privateFor(keyOf(tool), tool)"
                    [disabled]="running()"
                    (change)="setPrivate(keyOf(tool), $any($event.target).checked)"
                  />
                  <span class="flex-1">
                    <span class="block text-sm font-medium qt-text">Roll privately</span>
                    <span class="block text-xs qt-text-secondary mt-1">
                      The outcome is whispered to you alone. No character sees it, and it stays
                      out of every character’s context.
                    </span>
                  </span>
                </label>
              </section>

              <!-- A tool that quotes nothing gets no panel. An empty list under a
                   heading promising a list is worse than the heading's absence. -->
              @if (referenceRows().length > 0 || writeTargets().length > 0) {
                <details class="rounded-lg qt-border p-5" open>
                  <summary
                    class="text-sm font-medium qt-text cursor-pointer list-none flex items-center gap-2"
                  >
                    <qt-icon name="info" class="w-4 h-4 qt-text-secondary flex-shrink-0" />
                    What this tool can quote
                  </summary>

                  <p class="text-xs qt-text-secondary mt-3 leading-relaxed">
                    Pascal substitutes these when he writes the outcome. They belong to the
                    tool’s own messages, which its author places on
                    <span class="whitespace-nowrap">Pascal’s Workbench</span> — what you type
                    into the form above is sent exactly as written, braces and all.
                  </p>

                  <dl class="mt-4 space-y-3">
                    @for (row of referenceRows(); track row.placeholder) {
                      <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                        <dt>
                          <code class="text-xs font-mono qt-text">{{ row.placeholder }}</code>
                        </dt>
                        <dd class="text-xs qt-text-secondary min-w-0">{{ row.meaning }}</dd>
                      </div>
                    }
                  </dl>

                  @if (missingPlaceholderNote(); as note) {
                    <p class="text-xs qt-text-secondary mt-4 leading-relaxed">{{ note }}</p>
                  }

                  @if (writeTargets().length > 0) {
                    <p class="text-xs qt-text-secondary mt-4 leading-relaxed">
                      When it runs, this tool may also write:
                      @for (target of writeTargets(); track target; let i = $index) {
                        <span
                          >@if (i > 0) {, }<code class="font-mono qt-text">{{ target }}</code></span
                        >
                      }
                      . The record of what actually changed rides with the roll itself.
                    </p>
                  }
                </details>
              }
            </div>
          } @else {
            <!-- ================= Phase one — the roster ================= -->
            <div class="space-y-4">
              @if (rosterQuery.isPending()) {
                <p class="text-sm qt-text-secondary px-1 py-2">Consulting Pascal’s ledger…</p>
              }

              @if (rosterQuery.isError()) {
                <p class="text-sm qt-text-destructive px-1 py-2">{{ rosterErrorMessage() }}</p>
              }

              @if (tools().length > SEARCH_THRESHOLD) {
                <div class="relative">
                  <qt-icon
                    name="search"
                    class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 qt-text-secondary"
                  />
                  <input
                    type="text"
                    class="qt-input w-full pl-9 text-sm"
                    placeholder="Search the table…"
                    aria-label="Search the table"
                    [value]="search()"
                    (input)="search.set($any($event.target).value)"
                  />
                </div>
              }

              <!-- "Nothing here" only when there is genuinely nothing. With a
                   failed definition below, this line would contradict the badge
                   that is about to explain why the table looks bare. -->
              @if (!rosterQuery.isPending() && !rosterQuery.isError() && tools().length === 0) {
                @if (errors().length === 0) {
                  <p class="text-sm qt-text-secondary px-1 py-2">
                    No custom tools are laid out on the table.
                  </p>
                } @else {
                  <p class="text-sm qt-text-secondary px-1 py-2">
                    Nothing is runnable — the table was set, but the cards below would not read.
                  </p>
                }
              }

              @if (!rosterQuery.isPending() && tools().length > 0 && visibleTools().length === 0) {
                <p class="text-sm qt-text-secondary px-1 py-2">
                  Nothing on the table answers to that.
                </p>
              }

              <div class="space-y-2">
                @for (tool of visibleTools(); track keyOf(tool)) {
                  <div
                    class="flex items-stretch gap-1 rounded-lg qt-bg-surface hover:qt-bg-surface-alt"
                  >
                    <button
                      type="button"
                      class="flex-1 min-w-0 text-left px-4 py-3 rounded-lg"
                      (click)="select(tool)"
                    >
                      <span class="block text-sm font-medium qt-text">
                        {{ tool.title }}
                        @if (tool.characterLabel) {
                          <span class="ml-2 text-xs qt-text-secondary font-normal">
                            as {{ tool.characterLabel }}
                          </span>
                        }
                      </span>
                      @if (tool.description) {
                        <span class="block text-xs qt-text-secondary mt-1">
                          {{ tool.description }}
                        </span>
                      }
                      <span class="block text-[10px] qt-text-secondary mt-1.5 font-mono">
                        {{ tool.mountName }} · {{ tool.definitionPath }}
                      </span>
                    </button>
                    @if (tool.mountPointId) {
                      <button
                        type="button"
                        class="px-4 qt-text-secondary hover:opacity-70 rounded-lg"
                        [title]="workbenchTitle"
                        [attr.aria-label]="workbenchLabel(tool)"
                        (click)="
                          openWorkbench({
                            mountPointId: tool.mountPointId,
                            path: tool.definitionPath
                          })
                        "
                      >
                        <qt-icon name="wrench" class="w-4 h-4" />
                      </button>
                    }
                  </div>
                }
              </div>

              <!-- Definition files that failed validation — named, never
                   swallowed. Each badge opens the Workbench's repair mode: the
                   place you fix a broken file. A badge with no mountPointId has
                   nowhere to send you, so it stays inert (v4 disables it). -->
              @if (errors().length > 0) {
                <div class="rounded-lg qt-border p-4 space-y-2">
                  <h4 class="text-xs font-semibold qt-text-secondary uppercase tracking-wide">
                    Would not read
                  </h4>
                  @for (err of errors(); track err.mountName + ':' + err.definitionPath) {
                    <button
                      type="button"
                      class="w-full px-3 py-2 text-xs qt-text-destructive text-left rounded-md hover:qt-bg-surface-alt disabled:hover:bg-transparent"
                      [disabled]="!err.mountPointId"
                      [attr.title]="repairTitle(err)"
                      (click)="
                        err.mountPointId &&
                          openWorkbench({
                            mountPointId: err.mountPointId,
                            path: err.definitionPath
                          })
                      "
                    >
                      <span class="block break-all font-mono">{{ err.definitionPath }}</span>
                      <span class="block">{{ err.reason }}</span>
                    </button>
                  }
                </div>
              }

              <!-- Roster cap — say so rather than truncate silently. -->
              @if (droppedForCap().length > 0) {
                <p class="text-xs qt-text-destructive rounded-lg qt-border p-4 leading-relaxed">
                  Too many tools on the table; these were left off:
                  {{ droppedForCap().join(', ') }}
                </p>
              }
            </div>
          }
        </div>

        <div qt-modal-footer class="flex items-center justify-between w-full gap-2">
          @if (selectedTool(); as tool) {
            <button
              type="button"
              class="qt-button qt-button-secondary"
              [disabled]="running()"
              (click)="backToPicker()"
            >
              <qt-icon name="arrow-left" class="w-4 h-4 mr-1.5" />
              Choose another tool
            </button>
            <div class="flex gap-2">
              <button
                type="button"
                class="qt-button qt-button-secondary"
                [disabled]="running()"
                (click)="close()"
              >
                Cancel
              </button>
              <button
                type="button"
                class="qt-button qt-button-primary"
                [disabled]="running()"
                (click)="run(tool)"
              >
                {{ running() ? 'Running…' : 'Run ' + tool.title }}
              </button>
            </div>
          } @else {
            <button
              type="button"
              class="qt-button qt-button-secondary"
              (click)="openWorkbench({ create: true })"
            >
              <qt-icon name="wrench" class="w-4 h-4 mr-1.5" />
              New contrivance&hellip;
            </button>
            <button type="button" class="qt-button qt-button-secondary" (click)="close()">
              Cancel
            </button>
          }
        </div>
      </qt-modal>
    }
  `,
})
export class CustomToolsPopup {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly router = inject(Router);
  /** Hosted as a workspace tab ⇒ open the Workbench as a tab (v4 redirectToWorkspaceTab). */
  private readonly workspace = inject(WORKSPACE_HANDLE, { optional: true });

  readonly chatId = input.required<string>();
  readonly disabled = input(false);
  /** A run landed — the salon refetches the chat (v4 `onRan`). */
  readonly ran = output<void>();

  protected readonly SEARCH_THRESHOLD = SEARCH_THRESHOLD;
  /** The string lives here because its apostrophe cannot survive a template expression. */
  protected readonly workbenchTitle = "Open on Pascal's Workbench";

  protected readonly isOpen = signal(false);
  protected readonly running = signal(false);
  protected readonly search = signal('');

  /** All three survive a close on purpose — see the class header. */
  private readonly selectedKey = signal<string | null>(null);
  private readonly formValues = signal<Record<string, ParameterFormValues>>({});
  private readonly privateByKey = signal<Record<string, boolean>>({});

  protected readonly rosterQuery = injectQuery(() => ({
    queryKey: customToolsKeys.byChat(this.chatId()),
    queryFn: () => fetchCustomToolsRoster(this.core, this.chatId()),
  }));

  protected readonly tools = computed(() => this.rosterQuery.data()?.tools ?? []);
  protected readonly errors = computed(() => this.rosterQuery.data()?.errors ?? []);
  protected readonly droppedForCap = computed(() => this.rosterQuery.data()?.droppedForCap ?? []);

  /** Show the button when there is anything to say — a runnable tool OR a broken
   *  file (v4 `customToolsAvailable`: gating on tools alone hides the error
   *  badge exactly when it is most needed). */
  protected readonly available = computed(
    () => this.tools().length > 0 || this.errors().length > 0,
  );

  /**
   * Derived rather than stored, so a remembered tool that has since been
   * renamed, disabled, or gated away simply drops the dialog back to the picker
   * instead of leaving it pointed at a tool that is no longer dealt.
   */
  protected readonly selectedTool = computed(
    () => this.tools().find((tool) => this.keyOf(tool) === this.selectedKey()) ?? null,
  );

  protected readonly visibleTools = computed(() => {
    const query = this.search().trim().toLowerCase();
    if (query === '') return this.tools();
    return this.tools().filter((tool) => this.searchHaystack(tool).includes(query));
  });

  protected readonly dialogTitle = computed(() => {
    const tool = this.selectedTool();
    return tool ? `Run ${tool.title}` : "Pascal's Table";
  });

  /** What the picker matches a search against (v4 `searchHaystack`). */
  private searchHaystack(tool: CustomToolRosterEntry): string {
    return `${tool.title} ${tool.name} ${tool.description} ${tool.characterLabel ?? ''}`.toLowerCase();
  }

  protected rosterErrorMessage(): string {
    return coreErrorMessage(this.rosterQuery.error(), 'The tools could not be listed.');
  }

  /** Identity of a roster entry — `name` alone isn't unique (a per-character
   *  variant shadows a broader definition under the same name). */
  protected keyOf(tool: CustomToolRosterEntry): string {
    return `${tool.name}::${tool.asCharacterId ?? ''}`;
  }

  protected readonly paramNames = computed(() =>
    Object.keys(this.selectedTool()?.parameters ?? {}),
  );

  /** The per-row wrench label; built here because its apostrophe is a template hazard. */
  protected workbenchLabel(tool: CustomToolRosterEntry): string {
    return `Open ${tool.title} on Pascal's Workbench`;
  }

  protected valuesFor(key: string, tool: CustomToolRosterEntry): ParameterFormValues {
    return this.formValues()[key] ?? initialParamValues(tool.parameters);
  }

  protected privateFor(key: string, tool: CustomToolRosterEntry): boolean {
    return this.privateByKey()[key] ?? tool.defaultVisibility === 'whisper';
  }

  /**
   * A badge with no `mountPointId` has nowhere to send you, so it carries no
   * title (and is disabled). The string lives here rather than inline because
   * its apostrophe cannot survive an Angular template expression.
   */
  protected repairTitle(err: { mountPointId?: string }): string | null {
    return err.mountPointId ? "Open in Pascal's Workbench to repair" : null;
  }

  protected openDialog(): void {
    if (this.disabled()) return;
    this.isOpen.set(true);
    // Refetch fresh on open — definitions live in stores the user edits mid-chat.
    void this.rosterQuery.refetch();
  }

  protected close(): void {
    // The selection survives the close on purpose — see the class header.
    this.isOpen.set(false);
  }

  protected backToPicker(): void {
    this.selectedKey.set(null);
  }

  /** Escape dismisses (v4 `BaseModal`'s `closeOnEscape`). Stopped so an outer
   *  Escape handler doesn't also fire on the same keystroke. */
  protected onEscape(event: Event): void {
    if (!this.isOpen()) return;
    event.stopPropagation();
    this.close();
  }

  /** Choose a tool, seeding its form + privacy default the first time it opens. */
  protected select(tool: CustomToolRosterEntry): void {
    const key = this.keyOf(tool);
    this.formValues.update((prev) =>
      key in prev ? prev : { ...prev, [key]: initialParamValues(tool.parameters) },
    );
    this.privateByKey.update((prev) =>
      key in prev ? prev : { ...prev, [key]: tool.defaultVisibility === 'whisper' },
    );
    this.selectedKey.set(key);
  }

  protected onParamChange(key: string, change: ParamChange): void {
    this.formValues.update((prev) => ({
      ...prev,
      [key]: { ...(prev[key] ?? {}), [change.param]: change.value },
    }));
  }

  protected setPrivate(key: string, value: boolean): void {
    this.privateByKey.update((prev) => ({ ...prev, [key]: value }));
  }

  /**
   * Leave for Pascal's Workbench (v4 `openWorkbench`). The dialog shuts first,
   * then navigates with v4's OWN query strings — `?new=1` to create,
   * `?mount=&path=` to open one definition. Authoring lives there, not here.
   *
   * Hosted as a workspace tab ⇒ open (or focus) the Workbench tab with the same
   * payload; routed ⇒ the query-param push.
   */
  protected openWorkbench(payload?: {
    mountPointId?: string;
    path?: string;
    create?: boolean;
  }): void {
    this.close();
    if (this.workspace) {
      this.workspace.openTab('custom-tools', {
        mountPointId: payload?.mountPointId,
        path: payload?.path,
        create: payload?.create,
      });
      return;
    }
    const queryParams: Record<string, string> = {};
    if (payload?.create) queryParams['new'] = '1';
    if (payload?.mountPointId) queryParams['mount'] = payload.mountPointId;
    if (payload?.path) queryParams['path'] = payload.path;
    void this.router.navigate(['/custom-tools'], {
      queryParams: Object.keys(queryParams).length > 0 ? queryParams : undefined,
    });
  }

  protected async run(tool: CustomToolRosterEntry): Promise<void> {
    if (this.disabled() || this.running()) return;
    const key = this.keyOf(tool);
    this.running.set(true);
    try {
      await runCustomTool(this.core, this.chatId(), {
        tool: tool.name,
        parameters: coerceParamValues(tool.parameters, this.valuesFor(key, tool)),
        private: this.privateFor(key, tool),
        asCharacterId: tool.asCharacterId,
      });
      // Pascal's outcome is posted server-side; close and let the salon refetch.
      this.toasts.showSuccess(`Pascal has settled ${tool.title}.`);
      this.close();
      this.ran.emit();
    } catch (err) {
      // v4 `CustomToolRunDialog.tsx:202` — the 400 arm's server message. A
      // Prospero error is ALSO posted server-side and arrives as a message on
      // the next refetch; the toast is a different surface, so no double-render.
      this.toasts.showError(coreErrorMessage(err, 'The tool could not be run.'));
    } finally {
      this.running.set(false);
    }
  }

  /**
   * What the chosen tool actually quotes, in the order a reader wants it (v4
   * `referenceRows`).
   *
   * Every row is an occurrence the server found in the definition's own strings.
   * Nothing is listed because the format merely permits it: a tool that never
   * writes `{{value}}` does not get a `{{value}}` row, and a parameter you are
   * filling in above appears here only if some message or prompt quotes it back.
   *
   * A roster from a server without §1 carries no `references` at all, which
   * reads as "quotes nothing" — no panel — rather than as an error.
   */
  protected readonly referenceRows = computed<ReferenceRow[]>(() => {
    const tool = this.selectedTool();
    const refs = tool?.references;
    if (!tool || !refs) return [];

    const rows: ReferenceRow[] = [];

    if (refs.value) {
      rows.push({ placeholder: '{{value}}', meaning: 'the value the roll finally settles on' });
    }
    if (refs.roll) {
      rows.push({
        placeholder: '{{roll}}',
        meaning: 'the raw draw, before the tool does its arithmetic',
      });
    }
    if (refs.dice) {
      rows.push({ placeholder: '{{dice}}', meaning: 'the dice, die by die, with the modifier' });
    }
    if (refs.llm) {
      rows.push({
        placeholder: '{{llm}}',
        meaning: "the oracle's answer to the question this tool poses",
      });
    }

    for (const name of refs.params) {
      rows.push({
        placeholder: `{{params.${name}}}`,
        meaning: tool.parameters[name]?.description ?? `whatever you set “${name}” to above`,
      });
    }

    for (const key of refs.metadata) {
      rows.push({
        placeholder: `{{metadata.${key}}}`,
        meaning: `“${key}” from the invoking character's own fact sheet (metadata.json)`,
      });
    }

    for (const path of refs.state) {
      rows.push({
        placeholder: `{{state.${path}}}`,
        meaning: `“${path}” from the story's persistent state`,
      });
    }

    return rows;
  });

  /**
   * What the tool may WRITE, once the deal is done — a different claim from
   * what it quotes, and so a sentence of its own: "consults the encounter
   * count" and "changes it" must not blur together.
   *
   * A roster from a server without the write vocabulary carries neither key,
   * which reads as "writes nothing" rather than as an error, exactly as the
   * absent `references` reads as "quotes nothing".
   */
  protected readonly writeTargets = computed<string[]>(() => {
    const refs = this.selectedTool()?.references;
    return [
      ...(refs?.stateWrites ?? []).map((path) => `state.${path}`),
      ...(refs?.metadataWrites ?? []).map((key) => `metadata.${key}`),
    ];
  });

  /** v4's closing note: a placeholder whose source is missing simply stands. */
  protected readonly missingPlaceholderNote = computed<string | null>(() => {
    const tool = this.selectedTool();
    const readsMetadata = (tool?.references?.metadata.length ?? 0) > 0;
    const readsState = (tool?.references?.state.length ?? 0) > 0;
    if (readsMetadata && readsState) {
      return 'This tool also reads the character sheet and the story state above — a key or path that is missing simply leaves its placeholder standing.';
    }
    if (readsMetadata) {
      return 'A metadata key the character does not have simply leaves its placeholder standing.';
    }
    if (readsState) {
      return 'A state path that has never been set simply leaves its placeholder standing.';
    }
    return null;
  });
}
