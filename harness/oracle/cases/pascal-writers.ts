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
const pascalCases: Array<[string, string, string]> = [
  ['plain', 'Scan Hawking Radiation', 'The needle trembles at 12.'],
  ['message-leading-trailing-space', 'Unlock', '   the tumblers fall into place   '],
  ['message-inner-newline', 'Force The Lock', 'first line\nsecond line'],
  ['message-empty', 'Probe', ''],
  ['message-only-whitespace', 'Probe', '   \t  '],
  ['title-with-markdown-chars', 'A *Bold* Title', 'rolled {{value}}'],
  ['unicode', 'Café Roll', 'the café 🎲 opens'],
  ['message-with-value-token-verbatim', 'Saving Throw', 'you rolled {{value}} — {{dice}}'],
];
for (const [id, toolTitle, message] of pascalCases) {
  const out = buildPascalResultContent({ toolTitle, message });
  rows.push({ kind: 'pascalBody', id, toolTitle, message, content: out.content, opaqueContent: out.opaqueContent });
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
