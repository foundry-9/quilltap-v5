import { NgTemplateOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import { Icon } from '../../ui/icon';
import {
  buildToolHierarchy,
  extractAllGroupIds,
  getGroupCheckState,
  groupTools,
  isToolEnabled,
  makePluginGroupPattern,
  makeSubgroupPattern,
  type AvailableTool,
  type CheckState,
  type ToolGroup,
  type ToolSubgroup,
} from './tool-settings';

/**
 * The three-level tool tree (v4 `components/tools/tool-settings/ToolSettingsContent.tsx`,
 * 565 LOC) — group → subgroup → tool, each with a tri-state checkbox.
 *
 * ## The part that is easy to get wrong
 *
 * **Built-in groups and plugin groups disable by DIFFERENT mechanisms**
 * (`:110-150`). Toggling a built-in group writes every member id into
 * `disabledTools`; toggling a plugin group writes ONE glob pattern into
 * `disabledToolGroups` and, on re-enable, also clears the members' individual
 * entries — otherwise a tool disabled on its own would stay off after its group
 * was switched back on. A subgroup pattern is only added when the parent plugin
 * is not already disabled (`:170-175`): a redundant child pattern would survive
 * the parent being re-enabled.
 *
 * **The tree is expanded by default**, so the component tracks what the operator
 * has COLLAPSED and asks {@link extractAllGroupIds} what exists (`:36-59`).
 *
 * **`showAvailability` changes the arithmetic, not just the paint** (`:83-89`,
 * `:349-351`): an unreachable tool is excluded from every count as well as
 * greyed out, so "3 of 5 enabled" never counts a tool the chat cannot use.
 */
@Component({
  selector: 'qt-tool-settings-content',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, NgTemplateOutlet],
  template: `
    @if (loading()) {
      <div class="flex justify-center py-8 qt-text-secondary">Loading tools…</div>
    } @else {
      <div class="space-y-4">
        <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
          <p class="text-sm qt-text-secondary">
            {{ description() || enabledCount() + ' of ' + availableCount() + ' tools enabled' }}
          </p>
          <div class="flex gap-2">
            <button type="button" class="qt-button-secondary qt-button-sm" (click)="enableAll()">
              Enable All
            </button>
            <button type="button" class="qt-button-secondary qt-button-sm" (click)="disableAll()">
              Disable All
            </button>
          </div>
        </div>

        <div class="space-y-2">
          @for (group of hierarchy(); track group.id) {
            <div class="border qt-border-default rounded-lg overflow-hidden">
              <div
                class="flex items-center gap-2 p-3 qt-bg-muted/30 hover:qt-bg-muted/50 transition-colors"
              >
                <button
                  type="button"
                  class="flex items-center gap-2 flex-1 text-left"
                  (click)="toggleGroupExpanded(group.id)"
                >
                  <qt-icon
                    [name]="groupExpanded(group.id) ? 'chevron-down' : 'chevron-right'"
                    class="w-4 h-4 qt-text-secondary shrink-0"
                  />
                  <span class="font-medium text-sm">{{ group.displayName }}</span>
                  <span class="text-xs qt-text-secondary"
                    >({{ countableEnabled(group) }}/{{ countableTotal(group) }})</span
                  >
                </button>
                <button
                  type="button"
                  role="checkbox"
                  class="relative flex items-center justify-center w-5 h-5 rounded border border-input hover:qt-border-primary transition-colors"
                  [attr.aria-label]="'Toggle all ' + group.displayName"
                  [attr.aria-checked]="ariaChecked(groupState(group))"
                  (click)="toggleGroup(group)"
                >
                  @if (groupState(group) === 'checked') {
                    <qt-icon name="check" class="w-3.5 h-3.5 text-primary" />
                  }
                  @if (groupState(group) === 'indeterminate') {
                    <div class="w-2.5 h-0.5 bg-primary rounded-full"></div>
                  }
                </button>
              </div>

              @if (groupExpanded(group.id)) {
                <div class="p-2 space-y-1">
                  @for (tool of group.tools; track tool.id) {
                    <ng-container
                      *ngTemplateOutlet="toolRow; context: { $implicit: tool, indent: false }"
                    />
                  }

                  @for (subgroup of group.subgroups; track subgroup.id) {
                    @let pattern = subgroupPattern(subgroup);
                    <div class="ml-4 border-l-2 qt-border-default/50">
                      <div
                        class="flex items-center gap-2 p-2 pl-3 hover:qt-bg-muted/30 rounded-r transition-colors"
                      >
                        <button
                          type="button"
                          class="flex items-center gap-2 flex-1 text-left"
                          (click)="toggleSubgroupExpanded(pattern)"
                        >
                          <qt-icon
                            [name]="subgroupExpanded(pattern) ? 'chevron-down' : 'chevron-right'"
                            class="w-3 h-3 qt-text-secondary shrink-0"
                          />
                          <span class="qt-label text-foreground/80">{{
                            subgroup.displayName
                          }}</span>
                          <span class="text-xs qt-text-secondary"
                            >({{ countableEnabledIn(subgroup.tools) }}/{{
                              countableTotalIn(subgroup.tools)
                            }})</span
                          >
                        </button>
                        <button
                          type="button"
                          role="checkbox"
                          class="relative flex items-center justify-center w-5 h-5 rounded border border-input hover:qt-border-primary transition-colors"
                          [attr.aria-label]="'Toggle all ' + subgroup.displayName"
                          [attr.aria-checked]="ariaChecked(subgroupState(subgroup))"
                          (click)="toggleSubgroup(subgroup)"
                        >
                          @if (subgroupState(subgroup) === 'checked') {
                            <qt-icon name="check" class="w-3.5 h-3.5 text-primary" />
                          }
                          @if (subgroupState(subgroup) === 'indeterminate') {
                            <div class="w-2.5 h-0.5 bg-primary rounded-full"></div>
                          }
                        </button>
                      </div>

                      @if (subgroupExpanded(pattern)) {
                        <div class="pl-3 space-y-0.5">
                          @for (tool of subgroup.tools; track tool.id) {
                            <ng-container
                              *ngTemplateOutlet="toolRow; context: { $implicit: tool, indent: true }"
                            />
                          }
                        </div>
                      }
                    </div>
                  }
                </div>
              }
            </div>
          }
        </div>

        @if (availableTools().length === 0) {
          <div class="text-center py-4 qt-text-secondary">No tools available</div>
        }

        @if (footerNote(); as note) {
          <p class="text-xs qt-text-secondary pt-2 border-t qt-border-default">{{ note }}</p>
        }
      </div>
    }

    <ng-template #toolRow let-tool let-indent="indent">
      @if (isUnavailable(tool)) {
        <div
          [class]="
            'flex items-start gap-3 p-2 rounded opacity-50 cursor-not-allowed ' +
            (indent ? 'ml-4' : '')
          "
          [title]="
            tool.unavailableReason || 'This tool is not available in the current context'
          "
        >
          <input type="checkbox" class="qt-checkbox mt-0.5 cursor-not-allowed" disabled />
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <span class="font-medium text-sm text-foreground">{{ tool.name }}</span>
              <span class="text-xs qt-text-warning">(unavailable)</span>
            </div>
            <p class="text-xs qt-text-secondary mt-0.5 line-clamp-2">{{ tool.description }}</p>
            @if (tool.unavailableReason) {
              <p class="text-xs qt-text-warning mt-1 italic">{{ tool.unavailableReason }}</p>
            }
          </div>
        </div>
      } @else {
        <label
          [class]="
            'flex items-start gap-3 p-2 rounded hover:qt-bg-muted/30 cursor-pointer transition-colors ' +
            (indent ? 'ml-4' : '')
          "
        >
          <input
            type="checkbox"
            class="qt-checkbox mt-0.5"
            [attr.aria-label]="tool.name"
            [checked]="enabled(tool)"
            (change)="toggleTool(tool)"
          />
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <span class="font-medium text-sm text-foreground">{{ tool.name }}</span>
            </div>
            <p class="text-xs qt-text-secondary mt-0.5 line-clamp-2">{{ tool.description }}</p>
          </div>
        </label>
      }
    </ng-template>
  `,
})
export class ToolSettingsContent {
  readonly availableTools = input.required<AvailableTool[]>();
  readonly disabledTools = input.required<ReadonlySet<string>>();
  readonly disabledGroups = input.required<ReadonlySet<string>>();
  readonly showAvailability = input(true);
  readonly loading = input(false);
  readonly description = input<string | null>(null);
  readonly footerNote = input<string | null>(null);

  readonly disabledToolsChange = output<Set<string>>();
  readonly disabledGroupsChange = output<Set<string>>();

  private readonly collapsedGroups = signal<ReadonlySet<string>>(new Set());
  private readonly collapsedSubgroups = signal<ReadonlySet<string>>(new Set());

  protected readonly hierarchy = computed(() => buildToolHierarchy(this.availableTools()));
  private readonly allIds = computed(() => extractAllGroupIds(this.availableTools()));

  /** v4 counts only the tools the chat can actually reach (`:83-89`). */
  private readonly countable = computed(() =>
    this.showAvailability()
      ? this.availableTools().filter((t) => t.available !== false)
      : this.availableTools(),
  );
  protected readonly availableCount = computed(() => this.countable().length);
  protected readonly enabledCount = computed(
    () => this.countable().filter((t) => this.enabled(t)).length,
  );

  protected enabled(tool: AvailableTool): boolean {
    return isToolEnabled(tool, this.disabledTools(), this.disabledGroups());
  }

  protected isUnavailable(tool: AvailableTool): boolean {
    return this.showAvailability() && tool.available === false;
  }

  protected groupExpanded(groupId: string): boolean {
    return this.allIds().groupIds.has(groupId) && !this.collapsedGroups().has(groupId);
  }

  protected subgroupExpanded(pattern: string): boolean {
    return this.allIds().subgroupIds.has(pattern) && !this.collapsedSubgroups().has(pattern);
  }

  protected subgroupPattern(subgroup: ToolSubgroup): string {
    return makeSubgroupPattern(subgroup.pluginName, subgroup.id);
  }

  protected ariaChecked(state: CheckState): string {
    return state === 'checked' ? 'true' : state === 'indeterminate' ? 'mixed' : 'false';
  }

  protected countableTotal(group: ToolGroup): number {
    return this.countableTotalIn(groupTools(group));
  }

  protected countableEnabled(group: ToolGroup): number {
    return this.countableEnabledIn(groupTools(group));
  }

  protected countableTotalIn(tools: AvailableTool[]): number {
    return (this.showAvailability() ? tools.filter((t) => t.available !== false) : tools).length;
  }

  protected countableEnabledIn(tools: AvailableTool[]): number {
    return (this.showAvailability() ? tools.filter((t) => t.available !== false) : tools).filter(
      (t) => this.enabled(t),
    ).length;
  }

  /**
   * ⚠ The check state is computed over ALL tools, unlike the counts beside it
   * (v4 `getGroupState`, `:230-234`, does not filter by availability) — so a
   * group whose only reachable tool is on can still read "indeterminate".
   * Carried as-is.
   */
  protected groupState(group: ToolGroup): CheckState {
    const tools = groupTools(group);
    return getGroupCheckState(tools.filter((t) => this.enabled(t)).length, tools.length);
  }

  protected subgroupState(subgroup: ToolSubgroup): CheckState {
    return getGroupCheckState(
      subgroup.tools.filter((t) => this.enabled(t)).length,
      subgroup.tools.length,
    );
  }

  protected toggleGroupExpanded(groupId: string): void {
    this.collapsedGroups.update((prev) => toggled(prev, groupId));
  }

  protected toggleSubgroupExpanded(pattern: string): void {
    this.collapsedSubgroups.update((prev) => toggled(prev, pattern));
  }

  /** v4 `handleToggleTool` (`:92-100`). */
  protected toggleTool(tool: AvailableTool): void {
    this.disabledToolsChange.emit(toggled(this.disabledTools(), tool.id));
  }

  /** v4 `handleToggleGroup` (`:103-151`) — see the class note. */
  protected toggleGroup(group: ToolGroup): void {
    const tools = groupTools(group);
    const shouldEnable = tools.filter((t) => this.enabled(t)).length < tools.length;

    if (group.type === 'built-in') {
      const next = new Set(this.disabledTools());
      for (const tool of tools) {
        if (shouldEnable) next.delete(tool.id);
        else next.add(tool.id);
      }
      this.disabledToolsChange.emit(next);
      return;
    }
    if (!group.pluginName) return;

    const groups = new Set(this.disabledGroups());
    const pattern = makePluginGroupPattern(group.pluginName);
    if (shouldEnable) {
      groups.delete(pattern);
      for (const sg of group.subgroups) {
        groups.delete(makeSubgroupPattern(group.pluginName, sg.id));
      }
      this.disabledGroupsChange.emit(groups);
      // Individual disables must go too, or a tool switched off on its own
      // would stay off after its group came back.
      const nextTools = new Set(this.disabledTools());
      for (const tool of tools) nextTools.delete(tool.id);
      this.disabledToolsChange.emit(nextTools);
    } else {
      groups.add(pattern);
      this.disabledGroupsChange.emit(groups);
    }
  }

  /** v4 `handleToggleSubgroup` (`:154-179`). */
  protected toggleSubgroup(subgroup: ToolSubgroup): void {
    const shouldEnable =
      subgroup.tools.filter((t) => this.enabled(t)).length < subgroup.tools.length;
    const groups = new Set(this.disabledGroups());
    const pattern = makeSubgroupPattern(subgroup.pluginName, subgroup.id);

    if (shouldEnable) {
      groups.delete(pattern);
      this.disabledGroupsChange.emit(groups);
      const nextTools = new Set(this.disabledTools());
      for (const tool of subgroup.tools) nextTools.delete(tool.id);
      this.disabledToolsChange.emit(nextTools);
    } else if (!groups.has(makePluginGroupPattern(subgroup.pluginName))) {
      // A child pattern under an already-disabled parent would outlive the
      // parent being re-enabled, so v4 does not add one.
      groups.add(pattern);
      this.disabledGroupsChange.emit(groups);
    }
  }

  /** v4 `handleEnableAll` (`:182-185`). */
  protected enableAll(): void {
    this.disabledToolsChange.emit(new Set());
    this.disabledGroupsChange.emit(new Set());
  }

  /** v4 `handleDisableAll` (`:187-204`) — patterns for plugins, ids for built-ins. */
  protected disableAll(): void {
    const groups = new Set<string>();
    const tools = new Set<string>();
    for (const group of this.hierarchy()) {
      if (group.type === 'built-in') {
        for (const tool of group.tools) tools.add(tool.id);
      } else if (group.pluginName) {
        groups.add(makePluginGroupPattern(group.pluginName));
      }
    }
    this.disabledToolsChange.emit(tools);
    this.disabledGroupsChange.emit(groups);
  }
}

function toggled(set: ReadonlySet<string>, value: string): Set<string> {
  const next = new Set(set);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  return next;
}
