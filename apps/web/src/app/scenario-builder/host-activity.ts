/**
 * One line of the Host's activity, by tool — v4's `describeHostActivity`
 * (`components/scenario-builder/ScenarioBuilderDialog.tsx` at `d1c06cd9d`),
 * measured against v4's real function through the recorded corpus in
 * `__fixtures__/scenario-builder-oracle.json`.
 *
 * The argument shown is the most telling one; queries go inside the Host's own
 * curly quotation marks, with any the model added stripped from the ends (the
 * trim runs FIRST, so `" spaced "` keeps its inner spaces — the corpus pins it).
 *
 * ⚠ The `curl` arm can never fire on v5: v5 builds no plugin tools and has no
 * curl plugin, so real mode's `['curl']` allowlist admits nothing (the round's
 * §R.4(k) recorded divergence). It is kept for fidelity — the line the Host
 * would say is v4's, and a future curl port lights it with no SPA change.
 *
 * @module scenario-builder/host-activity
 */

import type { AgentStreamToolCall } from './agent-tool-calls';

export function describeHostActivity(
  call: Pick<AgentStreamToolCall, 'name'> & { arguments?: Record<string, unknown> },
): string {
  const args = call.arguments ?? {};
  const str = (key: string): string => (typeof args[key] === 'string' ? (args[key] as string) : '');
  // Queries go inside the Host's own quotation marks; strip any the model added.
  const quoted = (key: string): string =>
    `“${str(key)
      .trim()
      .replace(/^["'“”‘’]+|["'“”‘’]+$/g, '')}”`;
  switch (call.name) {
    case 'search_web':
      return `Consulting the wider world about ${quoted('query')}`;
    case 'curl':
      return `Reading ${str('url') || 'a page from the wider world'}`;
    case 'search':
      return `Leafing through the stores for ${quoted('query')}`;
    case 'doc_read_file':
      return `Opening ${str('path') || str('uri') || 'a document'}`;
    case 'doc_grep':
      return `Hunting through the papers for ${quoted('query')}`;
    case 'doc_list_files':
      return `Surveying the shelves${str('folder') ? ` of ${str('folder')}` : ''}`;
    case 'doc_read_frontmatter':
    case 'doc_read_heading':
      return `Consulting ${str('path') || str('uri') || 'a document'}`;
    case 'submit_final_response':
      return 'Setting the scene down on paper';
    default:
      return `Busying himself with ${call.name}`;
  }
}
