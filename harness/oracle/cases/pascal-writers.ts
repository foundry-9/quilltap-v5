/**
 * Oracle case: the Pascal writer body + Prospero's custom-tool-error body
 * (P4.6ay units 5 + 6). Drives v4's REAL pure content builders:
 *   buildPascalResultContent            (lib/services/pascal/writer.ts)
 *   buildCustomToolErrorContent
 *   buildCustomToolErrorOpaqueContent   (lib/services/prospero-notifications/writer.ts)
 *
 * Only the pure body builders are diffed here — the `postX` writers persist a
 * MessageEvent through the repos and are exercised when the run_custom handler +
 * route land (units 4 / 7). The one non-builder bit unit 6 owns is the reason
 * normalization inside `postProsperoCustomToolError`
 * (`reason.trim().replace(/[.\s]+$/, '') || 'the table would not deal'`); it is
 * transcribed one line below and emitted as `normalized`, so the Rust
 * `normalize_custom_tool_error_reason` is diffed against it AND the full error
 * body is diffed against the real builder fed that normalized reason.
 *
 * Run (v4 @ d68638b4, Node 24):
 *   cd ~/source/quilltap-server
 *   npx tsx <V5W>/harness/oracle/cases/pascal-writers.ts > /tmp/oracle-pascal-writers.ndjson
 */

import { buildPascalResultContent } from '@/lib/services/pascal/writer';
import {
  buildCustomToolErrorContent,
  buildCustomToolErrorOpaqueContent,
} from '@/lib/services/prospero-notifications/writer';

const rows: unknown[] = [];

// -------------------------------------------------------- buildPascalResultContent
const pascalCases: Array<[string, string, string, (string | undefined)?]> = [
  ['plain', 'Scan Hawking Radiation', 'The needle trembles at 12.'],
  ['message-leading-trailing-space', 'Unlock', '   the tumblers fall into place   '],
  ['message-inner-newline', 'Force The Lock', 'first line\nsecond line'],
  ['message-empty', 'Probe', ''],
  ['message-only-whitespace', 'Probe', '   \t  '],
  ['title-with-markdown-chars', 'A *Bold* Title', 'rolled {{value}}'],
  ['unicode', 'Café Roll', 'the café 🎲 opens'],
  ['message-with-value-token-verbatim', 'Saving Throw', 'you rolled {{value}} — {{dice}}'],
  // The c4d4b0de two-block body: the blank line makes the message its OWN
  // Markdown block, so an outcome opening with a list / heading / quote / fence
  // renders as what its author wrote instead of gluing inline to the heading.
  ['message-opens-with-list', 'Loot', '- a brass key\n- a folded note'],
  ['message-opens-with-heading', 'Loot', '# The Vault\nempty'],
  ['message-opens-with-quote', 'Loot', '> nothing here'],
  ['message-opens-with-fence', 'Loot', '```\nx = 1\n```'],
  ['message-opens-with-ordered-list', 'Loot', '1. first\n2. second'],
  // The c4d4b0de chipLabel heading: it REPLACES the title when present, and
  // falls back when blank/whitespace-only (`chipLabel?.trim() || toolTitle`).
  ['chip-label-replaces-title', 'Agent Lambda', 'the drop is made', 'Agent lambda — Jackie'],
  ['chip-label-trimmed', 'Agent Lambda', 'the drop is made', '   Agent lambda — Jackie   '],
  ['chip-label-blank-falls-back', 'Agent Lambda', 'the drop is made', ''],
  ['chip-label-whitespace-falls-back', 'Agent Lambda', 'the drop is made', '   \t '],
  ['chip-label-with-markdown-chars', 'Plain', 'ok', '**already bold**'],
  ['chip-label-unicode', 'Plain', 'ok', 'Café 🎲 Roll'],
];
for (const [id, toolTitle, message, chipLabel] of pascalCases) {
  const out = buildPascalResultContent({ toolTitle, ...(chipLabel !== undefined ? { chipLabel } : {}), message });
  rows.push({
    kind: 'pascalBody',
    id,
    toolTitle,
    chipLabel: chipLabel ?? null,
    message,
    content: out.content,
    opaqueContent: out.opaqueContent,
  });
}

// -------------------------------------------- custom-tool-error body + normalization
/** v4's inline normalization, transcribed from `postProsperoCustomToolError`. */
function normalizeReason(reason: string): string {
  return reason.trim().replace(/[.\s]+$/, '') || 'the table would not deal';
}

const errorCases: Array<[string, string, string]> = [
  ['plain', 'unlock', 'no such tool'],
  ['trailing-period', 'unlock', 'no such tool.'],
  ['trailing-periods-and-space', 'unlock', 'no such tool . '],
  ['reason-empty', 'unlock', ''],
  ['reason-only-dots', 'unlock', '...'],
  ['reason-only-whitespace', 'unlock', '   \t '],
  ['reason-leading-space', 'unlock', '   the bonus was rejected'],
  ['tool-name-with-backtick-context', 'roll_2d6', 'the low bound is above the high bound'],
  ['multi-sentence', 'unlock', 'the value is a string. it cannot be ordered.'],
  ['unicode-tool', 'café_tool', 'the table would not deal'],
];
for (const [id, toolName, reason] of errorCases) {
  const normalized = normalizeReason(reason);
  rows.push({
    kind: 'customToolError',
    id,
    toolName,
    reason,
    normalized,
    content: buildCustomToolErrorContent(toolName, normalized),
    opaqueContent: buildCustomToolErrorOpaqueContent(toolName, normalized),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
