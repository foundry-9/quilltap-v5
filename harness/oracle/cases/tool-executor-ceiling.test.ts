/**
 * @jest-environment node
 *
 * P4.D343 ORACLE (v4 `b15fe9404` at the pin `236c04e35`): the execution-time
 * effect ceiling — v4's REAL `executeToolCallWithContext`
 * (lib/chat/tool-executor.ts). The ceiling check sits at the top of the
 * executor, after the mountPool / operatorSurface refusal and before any
 * registry lookup or dispatch, so a refused call needs no handler mocking (v4's
 * own `tool-executor.effect-ceiling.test.ts`, whose five `it`s are the seed
 * rows). Only the logger is spliced, to capture the two lines.
 *
 * Per row: the call, the context's ceiling / maps, and — for a REFUSED call —
 * the whole result object (`{toolName, success, result: {formattedText},
 * error}`) plus the captured lines; for a call WITHIN the ceiling only the
 * captured lines up to dispatch (the DEBUG; the handler itself is never
 * reached: `doc_read_file` / `state fetch` rows use a context with no
 * character, so the handler fails on its own and that result is not compared).
 *
 * Run (Node 24, from the v4 checkout — mirror the case file to /tmp; jest
 * ignores `.claude/`):
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   M=/tmp/qt-tool-executor-ceiling-mirror; rm -rf $M; mkdir -p $M/cases
 *   cp $V5W/harness/oracle/cases/tool-executor-ceiling.test.ts $M/cases/
 *   QT_ORACLE_OUT=/tmp/oracle-tool-executor-ceiling.ndjson \
 *     npx jest --silent --roots "$PWD" --roots "$M/cases" --testTimeout=180000 --runTestsByPath "$M/cases/tool-executor-ceiling.test.ts"
 */

import * as fs from 'fs';
import { describe, it, expect } from '@jest/globals';

interface LogLine {
  level: string;
  message: string;
  fields: Record<string, unknown>;
}

const OUT = process.env.QT_ORACLE_OUT;

type Row = {
  id: string;
  call: { name: string; arguments: Record<string, unknown> };
  effectCeiling: string | null;
  customToolEffects?: [string, string][];
  pluginToolEffects?: [string, string][];
};

const ROWS: Row[] = [
  // v4's five `it`s.
  { id: 'write-under-read', call: { name: 'doc_write_file', arguments: { path: 'a.md', content: 'x' } }, effectCeiling: 'read' },
  { id: 'destroy-under-write', call: { name: 'doc_delete_file', arguments: { path: 'a.md' } }, effectCeiling: 'write' },
  { id: 'state-set-under-read', call: { name: 'state', arguments: { operation: 'set', path: 'a', value: 1 } }, effectCeiling: 'read' },
  {
    id: 'run-custom-named-write',
    call: { name: 'run_custom', arguments: { tool: 'unlock' } },
    effectCeiling: 'read',
    customToolEffects: [['unlock', 'write'], ['peek', 'read']],
  },
  { id: 'undeclared-plugin-write', call: { name: 'some_mcp_tool', arguments: {} }, effectCeiling: 'read' },
  // Grown: the remaining refusal shapes + the within / no-ceiling legs.
  { id: 'read-under-none', call: { name: 'doc_read_file', arguments: { path: 'a.md' } }, effectCeiling: 'none' },
  { id: 'ect-save-under-none', call: { name: 'edit_custom_tool', arguments: { mount_point: 'L', file: 'f', content: '{}' } }, effectCeiling: 'none' },
  { id: 'dod-blank-under-read', call: { name: 'doc_open_document', arguments: {} }, effectCeiling: 'read' },
  { id: 'state-invalid-under-read', call: { name: 'state', arguments: { operation: 'peek' } }, effectCeiling: 'read' },
  { id: 'plugin-declared-read-within', call: { name: 'some_mcp_tool', arguments: {} }, effectCeiling: 'read', pluginToolEffects: [['some_mcp_tool', 'read']] },
  { id: 'run-custom-named-read-within', call: { name: 'run_custom', arguments: { tool: 'peek' } }, effectCeiling: 'read', customToolEffects: [['unlock', 'write'], ['peek', 'read']] },
  { id: 'unknown-within-destroy', call: { name: 'some_mcp_tool', arguments: {} }, effectCeiling: 'destroy' },
  { id: 'unknown-no-ceiling', call: { name: 'some_mcp_tool', arguments: {} }, effectCeiling: null },
];

describe('tool executor effect ceiling (P4.D343 oracle)', () => {
  it('emits the rows', async () => {
    process.env.LOG_LEVEL = 'debug';
    const { logger } = await import('@/lib/logger');
    const lines: LogLine[] = [];
    const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
    internal.transports.splice(0, internal.transports.length, {
      write: (d: unknown) => {
        const data = d as { level: string; message: string; context?: Record<string, unknown> };
        if (!/effect ceiling/.test(data.message)) return;
        const { service: _s, environment: _e, ...fields } = (data.context ?? {}) as Record<string, unknown>;
        lines.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
      },
    });
    const { executeToolCallWithContext } = await import('@/lib/chat/tool-executor');
    const out: string[] = [];
    for (const row of ROWS) {
      lines.length = 0;
      const ctx = {
        chatId: 'chat-1',
        userId: 'u1',
        ...(row.effectCeiling ? { effectCeiling: row.effectCeiling } : {}),
        ...(row.customToolEffects ? { customToolEffects: new Map(row.customToolEffects) } : {}),
        ...(row.pluginToolEffects ? { pluginToolEffects: new Map(row.pluginToolEffects) } : {}),
      };
      let result: unknown = null;
      try {
        result = await executeToolCallWithContext(row.call as never, ctx as never);
      } catch (e) {
        result = { threw: (e as Error).message };
      }
      const refused = lines.some((l) => l.message.startsWith('Tool call refused'));
      out.push(
        JSON.stringify({
          kind: 'ceiling',
          ...row,
          refused,
          result: refused ? result : null,
          lines: [...lines],
        })
      );
    }
    expect(out.length).toBe(ROWS.length);
    if (OUT) fs.writeFileSync(OUT, out.join('\n') + '\n');
  });
});
