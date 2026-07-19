import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { Router } from '@angular/router';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { WORKSPACE_HANDLE } from '../workspace/workspace-contract';
import { CoreClient } from '../core/core-client';
import { CoreDispatchError, type CustomToolListing } from '../core/core-contract';
import { Icon } from '../ui/icon';
import {
  CustomToolParamsForm,
  coerceParamValues,
  initialParamValues,
  type ParamChange,
  type ParameterFormValues,
} from './custom-tool-params-form';
import { customToolsKeys, fetchCustomToolsRoster, runCustomTool } from './custom-tools.api';

/**
 * The composer's Custom Tools popup — a port of v4
 * `components/chat/CustomToolsDropdown.tsx`, the operator's manual path to
 * Pascal's custom tools. A bespoke composer toolbar button (there is no gutter
 * substrate in v5) that opens an upward, anchored popup (the `speaker-selector`
 * idiom: `absolute bottom-full`, an `open` signal, a `document:click`
 * click-outside), wide enough to hold a per-tool parameter form.
 *
 * The roster is fetched fresh on every open (v4 `enabled: isOpen`) — custom-tool
 * definitions live in the user's document stores and can change mid-chat, so a
 * stale roster is worse than a round-trip. The button itself only appears when
 * there is something to say — a runnable tool OR a definition that failed to
 * load (v4 `customToolsAvailable`); a broken tool is not the same as no tool.
 *
 * Deliberate omission: odds and outcome tables are NEVER shown (the payload
 * doesn't carry them). Also OMITTED this round (P4.6bb, Pascal's Workbench):
 * the per-tool "Open on Pascal's Workbench" wrench, the "New contrivance…"
 * entry, and the repair links on broken-file badges — those badges render the
 * loader's verbatim reason but do nothing on click here.
 */
@Component({
  selector: 'qt-custom-tools-popup',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, CustomToolParamsForm],
  template: `
    @if (available()) {
      <div class="relative">
        <button
          type="button"
          class="qt-chat-toolbar-button"
          title="Custom tools"
          aria-label="Custom tools"
          aria-haspopup="menu"
          [attr.aria-expanded]="isOpen()"
          [disabled]="disabled()"
          (click)="toggle()"
        >
          <qt-icon name="wand" class="w-5 h-5" />
        </button>

        @if (isOpen()) {
          <div
            class="absolute bottom-full left-0 mb-1 w-72 max-h-96 overflow-y-auto qt-card qt-shadow-lg rounded-lg border z-50"
            role="menu"
          >
            <div class="py-1">
              @if (rosterQuery.isPending()) {
                <div class="px-3 py-2 text-sm qt-text-secondary">Consulting Pascal’s ledger…</div>
              } @else if (rosterQuery.isError()) {
                <div class="px-3 py-2 text-xs qt-text-destructive">{{ rosterErrorMessage() }}</div>
              }

              <!-- "Nothing here" only when there is genuinely nothing; with a
                   failed definition below, this line would contradict the badge. -->
              @if (!rosterQuery.isPending() && !rosterQuery.isError() && tools().length === 0) {
                @if (errors().length === 0) {
                  <div class="px-3 py-2 text-sm qt-text-secondary">
                    No custom tools are laid out on the table.
                  </div>
                } @else {
                  <div class="px-3 py-2 text-sm qt-text-secondary">
                    Nothing is runnable — the table was set, but the cards below would not read.
                  </div>
                }
              }

              @for (tool of tools(); track keyOf(tool)) {
                @let key = keyOf(tool);
                <div>
                  <button
                    type="button"
                    class="w-full px-3 py-2 text-left hover:qt-bg-muted transition-colors disabled:opacity-50 flex items-start justify-between gap-2"
                    role="menuitem"
                    [attr.aria-expanded]="expandedKey() === key"
                    [disabled]="running()"
                    (click)="expand(tool)"
                  >
                    <span class="min-w-0">
                      <span class="block text-sm truncate">
                        {{ tool.title }}{{ tool.characterLabel ? ' (' + tool.characterLabel + ')' : '' }}
                      </span>
                      @if (tool.description) {
                        <span class="block text-xs qt-text-secondary">{{ tool.description }}</span>
                      }
                    </span>
                    <span class="flex items-center gap-1 mt-1 flex-shrink-0">
                      @if (tool.mountPointId) {
                        <span
                          role="button"
                          tabindex="0"
                          class="qt-text-secondary hover:opacity-70"
                          title="Open on Pascal's Workbench"
                          (click)="
                            $event.stopPropagation();
                            openWorkbench({ mountPointId: tool.mountPointId, path: tool.definitionPath })
                          "
                          (keydown.enter)="
                            $event.preventDefault();
                            $event.stopPropagation();
                            openWorkbench({ mountPointId: tool.mountPointId, path: tool.definitionPath })
                          "
                          (keydown.space)="
                            $event.preventDefault();
                            $event.stopPropagation();
                            openWorkbench({ mountPointId: tool.mountPointId, path: tool.definitionPath })
                          "
                        >
                          <qt-icon name="wrench" class="w-3 h-3" />
                        </span>
                      }
                      <qt-icon
                        name="chevron-down"
                        class="w-3 h-3 flex-shrink-0 transition-transform"
                        [class.rotate-180]="expandedKey() === key"
                      />
                    </span>
                  </button>

                  @if (expandedKey() === key) {
                    <div class="px-3 py-2 space-y-2 border-t">
                      <qt-custom-tool-params-form
                        [parameters]="tool.parameters"
                        [values]="valuesFor(key, tool)"
                        [disabled]="running()"
                        [idPrefix]="'custom-tool-' + key"
                        (change)="onParamChange(key, $event)"
                      />

                      <label class="flex items-center gap-2 text-sm">
                        <input
                          type="checkbox"
                          [checked]="privateFor(key, tool)"
                          [disabled]="running()"
                          (change)="setPrivate(key, $any($event.target).checked)"
                        />
                        <span>Roll privately</span>
                      </label>

                      @if (runError(); as msg) {
                        <div class="text-xs qt-text-destructive" role="alert">{{ msg }}</div>
                      }

                      <button
                        type="button"
                        class="w-full px-2 py-1 text-sm qt-button qt-button-primary rounded"
                        [disabled]="running()"
                        (click)="run(tool)"
                      >
                        {{ running() ? 'Running…' : 'Run ' + tool.title }}
                      </button>
                    </div>
                  }
                </div>
              }

              <!-- Definition files that failed validation — named, never
                   swallowed. Each badge opens the Workbench's repair mode: the
                   place you fix a broken file. A badge with no mountPointId has
                   nowhere to send you, so it stays inert (v4 disables it). -->
              @if (errors().length > 0) {
                <div class="border-t mt-1 pt-1">
                  @for (err of errors(); track err.mountName + ':' + err.definitionPath) {
                    <button
                      type="button"
                      class="w-full px-3 py-2 text-xs qt-text-destructive text-left hover:qt-bg-muted disabled:hover:bg-transparent"
                      [disabled]="!err.mountPointId"
                      [attr.title]="repairTitle(err)"
                      (click)="
                        err.mountPointId &&
                          openWorkbench({ mountPointId: err.mountPointId, path: err.definitionPath })
                      "
                    >
                      <span class="block break-all">{{ err.definitionPath }}</span>
                      <span class="block">{{ err.reason }}</span>
                    </button>
                  }
                </div>
              }

              <!-- Roster cap — say so rather than truncate silently. -->
              @if (droppedForCap().length > 0) {
                <div class="px-3 py-2 text-xs qt-text-destructive border-t">
                  Too many tools on the table; these were left off: {{ droppedForCap().join(', ') }}
                </div>
              }

              <!-- Pascal's Workbench — authoring lives there, not in this popup -->
              <div class="border-t mt-1 pt-1">
                <button
                  type="button"
                  class="w-full px-3 py-2 text-sm text-left hover:qt-bg-muted flex items-center gap-2"
                  (click)="openWorkbench({ create: true })"
                  role="menuitem"
                >
                  <qt-icon name="wrench" class="w-3.5 h-3.5" />
                  New contrivance&hellip;
                </button>
              </div>
            </div>
          </div>
        }
      </div>
    }
  `,
})
export class CustomToolsPopup {
  private readonly core = inject(CoreClient);
  private readonly host = inject(ElementRef<HTMLElement>);
  private readonly router = inject(Router);
  /** Hosted as a workspace tab ⇒ open the Workbench as a tab (v4 redirectToWorkspaceTab). */
  private readonly workspace = inject(WORKSPACE_HANDLE, { optional: true });

  readonly chatId = input.required<string>();
  readonly disabled = input(false);
  /** A run landed — the salon refetches the chat (v4 `onRan`). */
  readonly ran = output<void>();

  protected readonly isOpen = signal(false);
  protected readonly expandedKey = signal<string | null>(null);
  protected readonly running = signal(false);
  protected readonly runError = signal<string | null>(null);

  /**
   * Leave for Pascal's Workbench (v4 `openWorkbench`, `:127-139`). The popup
   * shuts first, then navigates with v4's OWN query strings — `?new=1` to
   * create, `?mount=&path=` to open one definition. Authoring lives there, not
   * in this popup.
   *
   * v4 prefers a workspace tab and falls back to this push; v5 has no workspace
   * tabs (`p4.9j`), so the fallback is the only path.
   */
  /**
   * A badge with no `mountPointId` has nowhere to send you, so it carries no
   * title (and is disabled). The string lives here rather than inline because
   * its apostrophe cannot survive an Angular template expression.
   */
  protected repairTitle(err: { mountPointId?: string }): string | null {
    return err.mountPointId ? "Open in Pascal's Workbench to repair" : null;
  }

  protected openWorkbench(payload?: {
    mountPointId?: string;
    path?: string;
    create?: boolean;
  }): void {
    this.isOpen.set(false);
    this.expandedKey.set(null);
    // Hosted ⇒ open (or focus) the Workbench tab with the same payload (v4's
    // redirectToWorkspaceTab); routed ⇒ the query-param push, unchanged.
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

  protected rosterErrorMessage(): string {
    const e = this.rosterQuery.error();
    if (e instanceof CoreDispatchError || e instanceof Error) return e.message;
    return 'The tools could not be listed.';
  }

  /** Identity of a roster entry — `name` alone isn't unique (a per-character
   *  variant shadows a broader definition under the same name). */
  protected keyOf(tool: CustomToolListing): string {
    return `${tool.name}::${tool.asCharacterId ?? ''}`;
  }

  protected valuesFor(key: string, tool: CustomToolListing): ParameterFormValues {
    return this.formValues()[key] ?? initialParamValues(tool.parameters);
  }

  protected privateFor(key: string, tool: CustomToolListing): boolean {
    return this.privateByKey()[key] ?? tool.defaultVisibility === 'whisper';
  }

  protected toggle(): void {
    if (this.disabled()) return;
    const next = !this.isOpen();
    this.isOpen.set(next);
    this.expandedKey.set(null);
    // Refetch fresh on open — definitions live in stores the user edits mid-chat.
    if (next) void this.rosterQuery.refetch();
  }

  /** Expand a tool, seeding its form + privacy default the first time it opens. */
  protected expand(tool: CustomToolListing): void {
    const key = this.keyOf(tool);
    if (this.expandedKey() === key) {
      this.expandedKey.set(null);
      return;
    }
    this.runError.set(null);
    this.formValues.update((prev) =>
      key in prev ? prev : { ...prev, [key]: initialParamValues(tool.parameters) },
    );
    this.privateByKey.update((prev) =>
      key in prev ? prev : { ...prev, [key]: tool.defaultVisibility === 'whisper' },
    );
    this.expandedKey.set(key);
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

  protected async run(tool: CustomToolListing): Promise<void> {
    if (this.disabled() || this.running()) return;
    const key = this.keyOf(tool);
    this.running.set(true);
    this.runError.set(null);
    try {
      await runCustomTool(this.core, this.chatId(), {
        tool: tool.name,
        parameters: coerceParamValues(tool.parameters, this.valuesFor(key, tool)),
        private: this.privateFor(key, tool),
        asCharacterId: tool.asCharacterId,
      });
      // Pascal's outcome is posted server-side; close and let the salon refetch.
      this.isOpen.set(false);
      this.expandedKey.set(null);
      this.ran.emit();
    } catch (err) {
      // The 400 arm's server message (v4's toast). A Prospero error was also
      // posted server-side and arrives as a message on the next refetch — this
      // inline line is a different surface, so there is no double-render.
      this.runError.set(err instanceof Error ? err.message : 'The tool could not be run.');
    } finally {
      this.running.set(false);
    }
  }

  @HostListener('document:click', ['$event'])
  protected onDocumentClick(event: Event): void {
    if (this.isOpen() && !this.host.nativeElement.contains(event.target as Node)) {
      this.isOpen.set(false);
      this.expandedKey.set(null);
    }
  }
}
