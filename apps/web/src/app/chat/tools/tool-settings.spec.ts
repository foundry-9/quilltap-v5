import { describe, expect, it } from 'vitest';

import {
  buildToolHierarchy,
  extractAllGroupIds,
  getGroupCheckState,
  isToolEnabled,
  makePluginGroupPattern,
  makeSubgroupPattern,
  type AvailableTool,
} from './tool-settings';

/** The tool-tree arithmetic (v4 `tool-settings/utils.ts`). */

function tool(id: string, over: Partial<AvailableTool> = {}): AvailableTool {
  return { id, name: id, description: '', source: 'built-in', ...over };
}

describe('group patterns', () => {
  it('spells v4’s two shapes', () => {
    expect(makePluginGroupPattern('mcp')).toBe('plugin:mcp');
    expect(makeSubgroupPattern('mcp', 'files')).toBe('plugin:mcp:subgroup:files');
  });
});

describe('getGroupCheckState', () => {
  it('is checked / unchecked / indeterminate at the boundaries', () => {
    expect(getGroupCheckState(3, 3)).toBe('checked');
    expect(getGroupCheckState(0, 3)).toBe('unchecked');
    expect(getGroupCheckState(1, 3)).toBe('indeterminate');
    // An empty group reads CHECKED, not unchecked (0 === 0 wins first).
    expect(getGroupCheckState(0, 0)).toBe('checked');
  });
});

describe('buildToolHierarchy', () => {
  it('splits the five categories that get groups of their own', () => {
    const groups = buildToolHierarchy([
      tool('a'),
      tool('b', { category: 'documents' }),
      tool('c', { category: 'wardrobe' }),
      // A category NOT in the map stays in Built-in Tools.
      tool('d', { category: 'memory' }),
    ]);
    expect(groups.map((g) => [g.id, g.displayName])).toEqual([
      ['built-in', 'Built-in Tools'],
      ['built-in:documents', 'Document Editing'],
      ['built-in:wardrobe', 'Wardrobe'],
    ]);
    expect(groups[0].tools.map((t) => t.id)).toEqual(['a', 'd']);
  });

  it('nests plugin subgroups and renames the MCP connector', () => {
    const groups = buildToolHierarchy([
      tool('m1', { source: 'plugin', pluginName: 'mcp', subgroupId: 'files', subgroupDisplayName: 'Files' }),
      tool('m2', { source: 'plugin', pluginName: 'mcp' }),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0].displayName).toBe('MCP Server Connector');
    expect(groups[0].tools.map((t) => t.id)).toEqual(['m2']);
    expect(groups[0].subgroups.map((s) => [s.id, s.displayName, s.tools.length])).toEqual([
      ['files', 'Files', 1],
    ]);
  });

  it('falls back to the tool’s own id for an unnamed plugin tool', () => {
    const groups = buildToolHierarchy([tool('loose', { source: 'plugin' })]);
    expect(groups[0].id).toBe('plugin:loose');
    expect(groups[0].displayName).toBe('Loose');
  });
});

describe('extractAllGroupIds', () => {
  it('always includes built-in, and every plugin group and subgroup pattern', () => {
    const { groupIds, subgroupIds } = extractAllGroupIds([
      tool('b', { category: 'photos' }),
      tool('p', { source: 'plugin', pluginName: 'mcp', subgroupId: 'files' }),
    ]);
    expect([...groupIds].sort()).toEqual(['built-in', 'built-in:photos', 'plugin:mcp']);
    expect([...subgroupIds]).toEqual(['plugin:mcp:subgroup:files']);
  });
});

describe('isToolEnabled', () => {
  const pluginTool = tool('p', { source: 'plugin', pluginName: 'mcp', subgroupId: 'files' });

  it('answers to all three disable mechanisms', () => {
    expect(isToolEnabled(pluginTool, new Set(), new Set())).toBe(true);
    expect(isToolEnabled(pluginTool, new Set(['p']), new Set())).toBe(false);
    expect(isToolEnabled(pluginTool, new Set(), new Set(['plugin:mcp']))).toBe(false);
    expect(isToolEnabled(pluginTool, new Set(), new Set(['plugin:mcp:subgroup:files']))).toBe(
      false,
    );
  });

  it('leaves a built-in tool untouched by group patterns', () => {
    expect(isToolEnabled(tool('a'), new Set(), new Set(['plugin:mcp']))).toBe(true);
  });
});
