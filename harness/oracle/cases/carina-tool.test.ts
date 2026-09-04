/**
 * @jest-environment node
 *
 * Differential ORACLE for the W4.1d5 `ask_carina` tool. DB-FREE: the whole Carina
 * query engine (`runCarinaQuery`) and the Prospero writer (`postProsperoCarinaError`)
 * are jest-mocked (they are the injected seams the Rust port also mocks). Drives
 * v4's REAL `executeAskCarinaTool` + `formatAskCarinaResults` over a canned-result
 * corpus, emitting one NDJSON line per case:
 *   { label, resultJson, formatted, prospero: null | {kind, characterName, detail, whisper, askerParticipantId} }
 *
 * Run (Node 24, from the v4 checkout; STAGE outside any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; STAGE=/tmp/qt-mail-carina-tools-stage
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-carina-tool.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- carina-tool
 */

import * as fs from 'fs';

type CarinaResult =
  | { ok: true; answer: string; messageId: string; message: unknown; answererId: string; answererName: string }
  | { ok: false; error: { kind: 'not-found' | 'no-profile' | 'llm-failed'; detail: string | null; characterName: string | null } };

// Mutable holders the mocks read (set per case before the dynamic import).
let cannedResult: CarinaResult;
const prosperoCalls: Array<Record<string, unknown>> = [];

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  interface Case {
    label: string;
    args: unknown;
    result: CarinaResult;
  }
  const okResult = (answer: string): CarinaResult => ({
    ok: true,
    answer,
    messageId: 'msg-1',
    message: {},
    answererId: 'ans-1',
    answererName: 'Sage',
  });
  const cases: Case[] = [
    { label: 'answer_public', args: { character: 'Sage', question: 'What is the meaning of life?', whisper: false }, result: okResult('The answer is 42.') },
    { label: 'answer_whisper', args: { character: 'Sage', question: 'A secret?', whisper: true }, result: okResult('Between us: the treasure is under the oak.') },
    { label: 'answer_default_whisper', args: { character: 'Sage', question: 'No whisper key?' }, result: okResult('Public by default.') },
    { label: 'err_not_found', args: { character: 'Ghost', question: 'Anyone there?' }, result: { ok: false, error: { kind: 'not-found', detail: null, characterName: null } } },
    { label: 'err_no_profile', args: { character: 'Mute', question: 'Speak?' }, result: { ok: false, error: { kind: 'no-profile', detail: null, characterName: 'Mute' } } },
    { label: 'err_llm_failed_detail', args: { character: 'Sage', question: 'Overloaded?' }, result: { ok: false, error: { kind: 'llm-failed', detail: 'rate limited', characterName: 'Sage' } } },
    { label: 'err_llm_failed_no_detail', args: { character: 'Sage', question: 'Silent?' }, result: { ok: false, error: { kind: 'llm-failed', detail: null, characterName: 'Sage' } } },
    { label: 'invalid_missing_question', args: { character: 'Sage' }, result: okResult('unused') },
    { label: 'invalid_nonobject', args: 'nope', result: okResult('unused') },
  ];

  const lines: string[] = [];

  for (const c of cases) {
    jest.resetModules();
    cannedResult = c.result;
    prosperoCalls.length = 0;

    jest.doMock('@/lib/services/carina/carina.service', () => ({
      runCarinaQuery: jest.fn(async () => cannedResult),
    }));
    jest.doMock('@/lib/services/prospero-notifications/writer', () => ({
      postProsperoCarinaError: jest.fn(async (args: Record<string, unknown>) => {
        prosperoCalls.push(args);
      }),
    }));
    jest.doMock('@/lib/logger', () => {
      const l = { child: jest.fn(), debug: jest.fn(), info: jest.fn(), warn: jest.fn(), error: jest.fn() };
      l.child.mockReturnValue(l);
      return { logger: l };
    });

    const { executeAskCarinaTool, formatAskCarinaResults } = await import(
      '@/lib/tools/handlers/ask-carina-handler'
    );
    const out = await executeAskCarinaTool(c.args, {
      userId: 'user-1',
      chatId: 'chat-1',
      callingParticipantId: 'pp-asker',
    });

    const prospero =
      prosperoCalls.length > 0
        ? {
            kind: prosperoCalls[0].kind,
            characterName: prosperoCalls[0].characterName ?? null,
            detail: prosperoCalls[0].detail ?? null,
            whisper: prosperoCalls[0].whisper ?? false,
            askerParticipantId: prosperoCalls[0].askerParticipantId ?? null,
          }
        : null;

    lines.push(
      JSON.stringify({
        label: c.label,
        resultJson: JSON.stringify(out),
        formatted: formatAskCarinaResults(out),
        prospero,
      }),
    );
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`carina-tool oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('carina-tool oracle', async () => {
  await main();
});
