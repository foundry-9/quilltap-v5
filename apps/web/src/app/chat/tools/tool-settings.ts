/**
 * The tool-settings tree's data model and its pure helpers (v4
 * `components/tools/tool-settings/types.ts` + `utils.ts`, 74 + 184 LOC).
 *
 * Kept apart from the component because the arithmetic — which pattern a group
 * toggle writes, which bucket a tool lands in — is the part worth testing
 * directly, and because v4 shares it between the chat and project modals.
 * (v5 ports only the chat one; the Prospero rider is noted in
 * `m6-screen-parity.md`.)
 */

/** One entry of `GET /api/v1/tools` (v4 `AvailableTool`). */
export interface AvailableTool {
  id: string;
  name: string;
  description: string;
  source: 'built-in' | 'plugin';
  category?: string;
  pluginName?: string;
  subgroupId?: string;
  subgroupDisplayName?: string;
  /** Whether the tool can actually be reached in this chat. */
  available?: boolean;
  unavailableReason?: string;
  /** `false` hides the tool from the Run Tool picker (v4 `RunToolModal.tsx:85`). */
  userInvocable?: boolean;
  /** The JSON Schema for the Run Tool form (`includeSchemas=true` only). */
  parameters?: unknown;
}

export interface ToolSubgroup {
  id: string;
  displayName: string;
  pluginName: string;
  tools: AvailableTool[];
}

export interface ToolGroup {
  id: string;
  displayName: string;
  type: 'built-in' | 'plugin';
  pluginName?: string;
  subgroups: ToolSubgroup[];
  /** Tools with no subgroup — direct children. */
  tools: AvailableTool[];
}

/** v4 `CheckState`. */
export type CheckState = 'checked' | 'unchecked' | 'indeterminate';

/**
 * The two group-pattern shapes v4 writes into `disabledToolGroups`
 * (`utils.ts:8-20`). Built-in groups have no pattern at all — toggling one
 * writes every member into `disabledTools` instead.
 */
export function makePluginGroupPattern(pluginName: string): string {
  return `plugin:${pluginName}`;
}

export function makeSubgroupPattern(pluginName: string, subgroupId: string): string {
  return `plugin:${pluginName}:subgroup:${subgroupId}`;
}

/** v4 `getGroupCheckState` (`utils.ts:26-33`). */
export function getGroupCheckState(enabledCount: number, totalCount: number): CheckState {
  if (enabledCount === totalCount) return 'checked';
  if (enabledCount === 0) return 'unchecked';
  return 'indeterminate';
}

/**
 * Built-in categories that get a group of their own rather than sitting in
 * "Built-in Tools" (v4 `BUILT_IN_CATEGORY_GROUPS`, `utils.ts:41-47`).
 */
const BUILT_IN_CATEGORY_GROUPS: Record<string, string> = {
  documents: 'Document Editing',
  photos: 'Photo Albums',
  wardrobe: 'Wardrobe',
  shell: 'Workspace',
  help: 'Quilltap Help',
};

/** v4 `buildToolHierarchy` (`utils.ts:49-155`). */
export function buildToolHierarchy(availableTools: AvailableTool[]): ToolGroup[] {
  const builtInTools: AvailableTool[] = [];
  const builtInCategoryTools = new Map<string, AvailableTool[]>();
  const pluginGroups = new Map<
    string,
    {
      displayName: string;
      subgroups: Map<string, { displayName: string; tools: AvailableTool[] }>;
      directTools: AvailableTool[];
    }
  >();

  for (const tool of availableTools) {
    if (tool.source === 'built-in') {
      if (tool.category && BUILT_IN_CATEGORY_GROUPS[tool.category]) {
        const bucket = builtInCategoryTools.get(tool.category) ?? [];
        bucket.push(tool);
        builtInCategoryTools.set(tool.category, bucket);
      } else {
        builtInTools.push(tool);
      }
    } else if (tool.pluginName || tool.source === 'plugin') {
      // v4 falls back to the tool's own id for an ungrouped plugin tool.
      const effective = tool.pluginName || tool.id;
      if (!pluginGroups.has(effective)) {
        pluginGroups.set(effective, {
          displayName: effective.charAt(0).toUpperCase() + effective.slice(1),
          subgroups: new Map(),
          directTools: [],
        });
      }
      const group = pluginGroups.get(effective)!;
      if (tool.subgroupId) {
        if (!group.subgroups.has(tool.subgroupId)) {
          group.subgroups.set(tool.subgroupId, {
            displayName: tool.subgroupDisplayName || tool.subgroupId,
            tools: [],
          });
        }
        group.subgroups.get(tool.subgroupId)!.tools.push(tool);
      } else {
        group.directTools.push(tool);
      }
    }
  }

  const groups: ToolGroup[] = [];

  if (builtInTools.length > 0) {
    groups.push({
      id: 'built-in',
      displayName: 'Built-in Tools',
      type: 'built-in',
      subgroups: [],
      tools: builtInTools,
    });
  }

  for (const [category, tools] of builtInCategoryTools) {
    groups.push({
      id: `built-in:${category}`,
      displayName: BUILT_IN_CATEGORY_GROUPS[category] || category,
      type: 'built-in',
      subgroups: [],
      tools,
    });
  }

  for (const [pluginName, group] of pluginGroups) {
    const subgroups: ToolSubgroup[] = [];
    for (const [subgroupId, subgroup] of group.subgroups) {
      subgroups.push({
        id: subgroupId,
        displayName: subgroup.displayName,
        pluginName,
        tools: subgroup.tools,
      });
    }
    // v4's one special case: the MCP connector renames its own group.
    let displayName = group.displayName;
    const firstTool = group.directTools[0] || subgroups[0]?.tools[0];
    if (firstTool?.pluginName === 'mcp') {
      displayName = 'MCP Server Connector';
    }
    groups.push({
      id: `plugin:${pluginName}`,
      displayName,
      type: 'plugin',
      pluginName,
      subgroups,
      tools: group.directTools,
    });
  }

  return groups;
}

/**
 * Every group / subgroup id in the hierarchy (v4 `extractAllGroupIds`,
 * `utils.ts:160-186`). The tree is expanded by DEFAULT, so the component tracks
 * what has been COLLAPSED and asks this for what exists.
 */
export function extractAllGroupIds(tools: AvailableTool[]): {
  groupIds: Set<string>;
  subgroupIds: Set<string>;
} {
  const groupIds = new Set<string>(['built-in']);
  const subgroupIds = new Set<string>();

  for (const tool of tools) {
    if (tool.source === 'built-in' && tool.category && BUILT_IN_CATEGORY_GROUPS[tool.category]) {
      groupIds.add(`built-in:${tool.category}`);
    }
    if (tool.source === 'plugin' && tool.pluginName) {
      groupIds.add(`plugin:${tool.pluginName}`);
      if (tool.subgroupId) {
        subgroupIds.add(makeSubgroupPattern(tool.pluginName, tool.subgroupId));
      }
    }
  }

  return { groupIds, subgroupIds };
}

/**
 * Whether a tool survives the two disable mechanisms (v4 `isToolEnabled`,
 * `ToolSettingsContent.tsx:64-81`): its own id, its plugin's pattern, or its
 * subgroup's pattern.
 */
export function isToolEnabled(
  tool: AvailableTool,
  disabledTools: ReadonlySet<string>,
  disabledGroups: ReadonlySet<string>,
): boolean {
  if (disabledTools.has(tool.id)) return false;
  if (tool.pluginName && disabledGroups.has(makePluginGroupPattern(tool.pluginName))) return false;
  if (
    tool.pluginName &&
    tool.subgroupId &&
    disabledGroups.has(makeSubgroupPattern(tool.pluginName, tool.subgroupId))
  ) {
    return false;
  }
  return true;
}

/** Every tool a group covers — its own plus its subgroups'. */
export function groupTools(group: ToolGroup): AvailableTool[] {
  return [...group.tools, ...group.subgroups.flatMap((sg) => sg.tools)];
}
