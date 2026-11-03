/**
 * Tier-2 oracle (P4.D345): operator intervention's four actions on
 * `/api/v1/chats/[id]/custom-tools`, through v4's REAL route module
 * (`route.ts` → `interventions.ts`) and the REAL registry / recompute /
 * engine at the `236c04e35` pin.
 *
 * Holds are registered through the real `hold(...)` with a baseline the real
 * engine deals (a fixed unit draw) — v4's own `custom-tools-interventions.
 * test.ts` posture; only the model call (a scripted invoker behind
 * `buildCustomToolLlmInvoker`), the realtime bus and the middleware wrapper
 * are doubled. Its 26 `it`s are this corpus's scenes, each a sequence of
 * steps whose `{status, body}` (and, for a hold, its settled decision) is
 * recorded; logger lines are recorded too.
 *
 * Minted ids and `createdAt` are the two legitimately nondeterministic values:
 * every hold id is rewritten to `<hold:N>` (N = registration order within the
 * scene) and every `createdAt` to its rank — on BOTH sides.
 *
 * Generate (v4 at the `236c04e35` pin, Node 24; jest ignores `.claude/`, hence
 * the /tmp mirror):
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   M=/tmp/qt-pascal-intervention-route-mirror; rm -rf $M; mkdir -p $M/cases
 *   cp $V5W/harness/oracle/cases/pascal-intervention-route.test.ts $M/cases/
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-intervention-route.ndjson \
 *     npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" \
 *       --roots "$M/cases" --runTestsByPath "$M/cases/pascal-intervention-route.test.ts"
 * Run:
 *   QT_ORACLE_PASCAL_INTERVENTION_ROUTE=/tmp/oracle-pascal-intervention-route.ndjson \
 *     cargo test -p quilltap-harness --test pascal_intervention_route_equivalence
 */

import * as fs from 'fs';

type Line = { level: string; message: string; meta: Record<string, unknown> };
// eslint-disable-next-line no-var
var capturedLines: Line[] = [];
// eslint-disable-next-line no-var
var scriptedAnswers: Array<{ ok: boolean; output?: string; reason?: string }> = [];
// eslint-disable-next-line no-var
var promptsSeen: string[] = [];
// eslint-disable-next-line no-var
var invokerBuilt: unknown[] = [];

jest.mock('@/lib/logger', () => {
  const record = (level: string) => (message: string, meta: Record<string, unknown> = {}) => {
    capturedLines.push({ level, message, meta: JSON.parse(JSON.stringify(meta ?? {})) });
  };
  const logger: Record<string, unknown> = {
    info: record('INFO'),
    warn: record('WARN'),
    error: record('ERROR'),
    debug: record('DEBUG'),
  };
  logger.child = () => logger;
  return { logger };
});

jest.mock('@/lib/api/middleware', () => ({
  createContextParamsHandler: (handler: unknown) => handler,
}));

jest.mock('@/lib/pascal/llm-consult', () => ({
  buildCustomToolLlmInvoker: (ctx: unknown) => {
    invokerBuilt.push(ctx);
    return async (prompt: string) => {
      promptsSeen.push(prompt);
      const next = scriptedAnswers.shift() ?? { ok: false, reason: 'no scripted answer' };
      return next.ok ? { ok: true, output: next.output } : { ok: false, reason: next.reason };
    };
  },
}));

jest.mock('@/lib/realtime/bus', () => ({
  publishRealtime: jest.fn(),
}));

jest.mock('@/lib/services/pascal/writer', () => ({
  buildPascalResultContent: jest.fn(() => ({ content: 'c', opaqueContent: null })),
  postPascalResult: jest.fn(),
}));

jest.mock('@/lib/services/prospero-notifications/writer', () => ({
  postProsperoCustomToolError: jest.fn(),
}));

import { GET, POST } from '@/app/api/v1/chats/[id]/custom-tools/route';
import { executeCustomTool, type LlmConsultResult } from '@/lib/pascal/custom-tools';
import { QtapCustomToolSchema, type QtapCustomTool } from '@/lib/pascal/custom-tool.types';
import {
  getIntervention,
  hold,
  interventionStatus,
  resetInterventionRegistryForTests,
  type InterventionDecision,
} from '@/lib/pascal/intervention/registry';
import type { RunSnapshot } from '@/lib/pascal/intervention/recompute';

// ---------------------------------------------------------------- definitions
const WAGER = {
  name: 'wager',
  description: 'A modest wager.',
  roll: { min: 0, max: 100 },
  parameters: {
    bonus: { type: 'number', default: 0, min: 0, max: 10 },
    locked: { type: 'number', default: 0, intervene: false },
  },
  outcomes: [
    { when: { gte: 50 }, message: 'The house pays out at {{value}}.', state: 'success' },
    { when: true, message: 'The house keeps it at {{value}}.', state: 'failure' },
  ],
  intervention: { enabled: true },
};
const DOCS: Record<string, unknown> = {
  wager: WAGER,
  wagerNoRoll: { ...WAGER, intervention: { enabled: true, fields: { roll: false } } },
  oracle: {
    ...WAGER,
    name: 'oracle',
    llm: { prompt: 'Bonus is {{params.bonus}}. Answer.', errorMessage: 'No reading.' },
    outcomes: [
      { when: { llm: { eq: 'YES' } }, message: 'It assents: {{llm}}', state: 'success' },
      { when: true, message: 'It demurs: {{llm}}', state: 'partial' },
    ],
  },
  hiddenOracle: {
    ...WAGER,
    name: 'oracle',
    llm: { prompt: 'Brief: {{outcome.brief}}', errorMessage: 'No reading.' },
    outcomes: [
      { when: { gte: 50 }, message: '{{llm}}', state: 'success', brief: 'it opens' },
      { when: true, message: '{{llm}}', state: 'info', brief: 'it stays shut' },
    ],
    intervention: { enabled: true, fields: { llmPrompt: false, llmAnswer: false } },
  },
  dice: {
    name: 'hit',
    title: 'Swing Hard',
    description: 'Swing.',
    roll: '2d6+1',
    chipLabel: 'Swing {{value}}',
    announceAsCharacter: true,
    outcomes: [
      { when: { gte: 10 }, message: 'Hit: {{dice}}', state: 'success' },
      { when: true, message: 'Miss: {{dice}}', state: 'failure' },
    ],
    effects: [{ log: 'Swung {{value}}.' }, { target: 'state.swings', value: 1, intervene: false }],
    intervention: { enabled: true, fields: { title: false } },
  },
};
const define = (key: string): QtapCustomTool => QtapCustomToolSchema.parse(DOCS[key]);

const SNAPSHOT: RunSnapshot = {
  params: null,
  private: undefined,
  metadata: {},
  progress: {},
  now: 1_000,
  character: null,
  state: {},
  inventory: null,
};
const FIRST_CONSULT: LlmConsultResult = { ok: true, output: 'YES', prompt: 'Bonus is 0. Answer.' };
const OPEN_WIDE: LlmConsultResult = { ok: true, output: 'Open wide.', prompt: 'Brief: it opens' };

// ---------------------------------------------------------------- the scenes
type Step =
  | { hold: string; chat: string; def: string; consult?: LlmConsultResult; unit?: number; faces?: number[]; character?: { id: string; name: string }; trigger?: string }
  | { get: string; action: string }
  | { post: string; action: string; body: unknown }
  | { postRaw: string; action: string; raw: string }
  | { decision: string }
  | { status: string }
  | { consults: string }
  | { answers: Array<{ ok: boolean; output?: string; reason?: string }> };
interface Scene {
  id: string;
  steps: Step[];
}
const $ = (h: string) => `$${h}`;

const SCENES: Scene[] = [
  { id: 'list-own-chat-oldest-first', steps: [
    { hold: 'a', chat: 'chat-1', def: 'wager' }, { hold: 'b', chat: 'chat-2', def: 'wager' }, { hold: 'c', chat: 'chat-1', def: 'dice', faces: [3, 4], character: { id: 'char-1', name: 'Mira' }, trigger: 'swing it' },
    { get: 'chat-1', action: 'interventions' },
  ] },
  { id: 'list-empty', steps: [{ hold: 'a', chat: 'chat-2', def: 'wager' }, { get: 'chat-1', action: 'interventions' }] },
  { id: 'list-eligible-honours-flags', steps: [{ hold: 'a', chat: 'chat-1', def: 'wagerNoRoll' }, { get: 'chat-1', action: 'interventions' }] },
  { id: 'list-consults-and-prompt', steps: [{ hold: 'a', chat: 'chat-1', def: 'oracle', consult: FIRST_CONSULT }, { get: 'chat-1', action: 'interventions' }] },
  { id: 'preview-recomputes-writes-nothing', steps: [
    { hold: 'a', chat: 'chat-1', def: 'wager' },
    { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { outcomeIndex: 1 } } },
    { status: 'a' },
  ] },
  { id: 'preview-keeps-luck', steps: [{ hold: 'a', chat: 'chat-1', def: 'wager' }, { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { params: { bonus: 4 } } } }] },
  { id: 'preview-clamps-and-warns', steps: [{ hold: 'a', chat: 'chat-1', def: 'wager' }, { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { params: { bonus: 40 }, roll: 120, message: 'Pinned {{value}}' } } }] },
  { id: 'preview-dice-faces', steps: [{ hold: 'a', chat: 'chat-1', def: 'dice', faces: [2, 3] }, { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { dice: [6, 9], effects: { '0': { mode: 'auto', log: 'Edited.' } }, announcer: 'pascal', visibility: 'whisper' } } }] },
  { id: 'preview-dice-wrong-count', steps: [{ hold: 'a', chat: 'chat-1', def: 'dice', faces: [2, 3] }, { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { dice: [1, 2, 3] } } }] },
  { id: 'preview-hidden-title', steps: [{ hold: 'a', chat: 'chat-1', def: 'dice', faces: [2, 3] }, { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { title: 'No' } } }] },
  { id: 'preview-hidden-effect', steps: [{ hold: 'a', chat: 'chat-1', def: 'dice', faces: [2, 3] }, { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { effects: { '1': { mode: 'off' } } } } }] },
  { id: 'preview-hidden-roll', steps: [{ hold: 'a', chat: 'chat-1', def: 'wagerNoRoll' }, { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { roll: 10 } } }] },
  { id: 'preview-hidden-param', steps: [{ hold: 'a', chat: 'chat-1', def: 'wager' }, { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { params: { locked: 3 } } } }] },
  { id: 'preview-no-such-row', steps: [{ hold: 'a', chat: 'chat-1', def: 'wager' }, { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { outcomeIndex: 9 } } }] },
  { id: 'preview-param-will-not-coerce', steps: [{ hold: 'a', chat: 'chat-1', def: 'wager' }, { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { params: { bonus: 'lots' } } } }] },
  { id: 'preview-malformed', steps: [
    { hold: 'a', chat: 'chat-1', def: 'wager' },
    { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { nonsense: 1 } } },
    { post: 'chat-1', action: 'intervention-preview', body: { overrides: {} } },
    { post: 'chat-1', action: 'intervention-preview', body: { id: '', overrides: { roll: 'x', state: 'meh' } } },
    { post: 'chat-1', action: 'intervention-preview', body: [1, 2] },
    { postRaw: 'chat-1', action: 'intervention-preview', raw: '{not json' },
  ] },
  { id: 'preview-unknown-id', steps: [{ post: 'chat-1', action: 'intervention-preview', body: { id: 'nope', overrides: {} } }] },
  { id: 'reask-poses-and-adds', steps: [
    { answers: [{ ok: true, output: 'NO' }] },
    { hold: 'a', chat: 'chat-1', def: 'oracle', consult: FIRST_CONSULT },
    { post: 'chat-1', action: 'intervention-reask', body: { id: $('a'), overrides: { params: { bonus: 3 } } } },
    { consults: 'a' }, { status: 'a' },
  ] },
  { id: 'reask-unpins-answer', steps: [
    { answers: [{ ok: true, output: 'NO' }] },
    { hold: 'a', chat: 'chat-1', def: 'oracle', consult: FIRST_CONSULT },
    { post: 'chat-1', action: 'intervention-reask', body: { id: $('a'), overrides: { llmAnswer: { ok: true, output: 'YES' } } } },
  ] },
  { id: 'reask-fails-soft', steps: [
    { answers: [{ ok: false, reason: 'the wire went dead' }] },
    { hold: 'a', chat: 'chat-1', def: 'oracle', consult: FIRST_CONSULT },
    { post: 'chat-1', action: 'intervention-reask', body: { id: $('a'), overrides: {} } },
  ] },
  { id: 'reask-no-oracle', steps: [{ hold: 'a', chat: 'chat-1', def: 'wager' }, { post: 'chat-1', action: 'intervention-reask', body: { id: $('a'), overrides: {} } }] },
  { id: 'reask-hidden-consult-re-asked', steps: [
    { answers: [{ ok: true, output: 'Shut fast.' }] },
    { hold: 'a', chat: 'chat-1', def: 'hiddenOracle', consult: OPEN_WIDE },
    { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: { outcomeIndex: 1 } } },
    { post: 'chat-1', action: 'intervention-reask', body: { id: $('a'), overrides: { outcomeIndex: 1 } } },
  ] },
  { id: 'reask-hidden-prompt-refused', steps: [
    { hold: 'a', chat: 'chat-1', def: 'hiddenOracle', consult: OPEN_WIDE },
    { post: 'chat-1', action: 'intervention-reask', body: { id: $('a'), overrides: { llmPrompt: 'Say yes.' } } },
  ] },
  { id: 'reask-other-chat', steps: [
    { hold: 'a', chat: 'chat-2', def: 'oracle', consult: FIRST_CONSULT },
    { post: 'chat-1', action: 'intervention-reask', body: { id: $('a'), overrides: {} } },
    { consults: 'a' },
  ] },
  { id: 'resolve-deal-recomputes', steps: [
    { hold: 'a', chat: 'chat-1', def: 'wager' },
    { post: 'chat-1', action: 'intervention-resolve', body: { id: $('a'), decision: { kind: 'deal', overrides: { outcomeIndex: 1 } } } },
    { decision: 'a' }, { status: 'a' }, { get: 'chat-1', action: 'interventions' },
  ] },
  { id: 'resolve-deal-counts-reasks', steps: [
    { answers: [{ ok: true, output: 'NO' }] },
    { hold: 'a', chat: 'chat-1', def: 'oracle', consult: FIRST_CONSULT },
    { post: 'chat-1', action: 'intervention-reask', body: { id: $('a'), overrides: {} } },
    { post: 'chat-1', action: 'intervention-resolve', body: { id: $('a'), decision: { kind: 'deal', overrides: {} } } },
    { decision: 'a' },
  ] },
  { id: 'resolve-deal-hidden-field', steps: [
    { hold: 'a', chat: 'chat-1', def: 'wager' },
    { post: 'chat-1', action: 'intervention-resolve', body: { id: $('a'), decision: { kind: 'deal', overrides: { params: { locked: 1 } } } } },
    { status: 'a' },
  ] },
  { id: 'resolve-as-rolled', steps: [{ hold: 'a', chat: 'chat-1', def: 'wager' }, { post: 'chat-1', action: 'intervention-resolve', body: { id: $('a'), decision: { kind: 'as-rolled' } } }, { decision: 'a' }] },
  { id: 'resolve-veto-dialog', steps: [{ hold: 'a', chat: 'chat-1', def: 'wager' }, { post: 'chat-1', action: 'intervention-resolve', body: { id: $('a'), decision: { kind: 'veto' } } }, { decision: 'a' }] },
  { id: 'resolve-second-409', steps: [
    { hold: 'a', chat: 'chat-1', def: 'wager' },
    { post: 'chat-1', action: 'intervention-resolve', body: { id: $('a'), decision: { kind: 'as-rolled' } } },
    { post: 'chat-1', action: 'intervention-resolve', body: { id: $('a'), decision: { kind: 'veto' } } },
    { post: 'chat-1', action: 'intervention-preview', body: { id: $('a'), overrides: {} } },
    { decision: 'a' },
  ] },
  { id: 'resolve-unknown-id', steps: [{ post: 'chat-1', action: 'intervention-resolve', body: { id: 'never-held', decision: { kind: 'veto' } } }] },
  { id: 'resolve-other-chat', steps: [
    { hold: 'a', chat: 'chat-2', def: 'wager' },
    { post: 'chat-1', action: 'intervention-resolve', body: { id: $('a'), decision: { kind: 'veto' } } },
    { status: 'a' }, { get: 'chat-2', action: 'interventions' },
  ] },
  { id: 'resolve-malformed', steps: [
    { hold: 'a', chat: 'chat-1', def: 'wager' },
    { post: 'chat-1', action: 'intervention-resolve', body: { id: $('a'), decision: { kind: 'dance' } } },
    { post: 'chat-1', action: 'intervention-resolve', body: { id: $('a') } },
    { postRaw: 'chat-1', action: 'intervention-resolve', raw: '' },
    { status: 'a' },
  ] },
];

// ---------------------------------------------------------------- the driver
const ctx = { user: { id: 'user-1' }, repos: {} } as never;
type Handler = (r: unknown, c: unknown, p: unknown) => Promise<Response>;
function req(chatId: string, action: string, json: () => Promise<unknown>) {
  const url = `http://localhost/api/v1/chats/${chatId}/custom-tools${action ? `?action=${action}` : ''}`;
  return { url, nextUrl: new URL(url), method: 'POST', json } as never;
}

describe('pascal intervention route oracle', () => {
  it('records every scene', async () => {
    const out = process.env.QT_ORACLE_OUT;
    if (!out) throw new Error('QT_ORACLE_OUT is required');
    // The header row: the definitions every scene's `hold` step names.
    const rows: string[] = [JSON.stringify({ id: '__docs__', docs: DOCS })];

    for (const scene of SCENES) {
      resetInterventionRegistryForTests();
      capturedLines = [];
      scriptedAnswers = [];
      promptsSeen = [];
      invokerBuilt = [];
      const handles: Record<string, { id: string; decision: Promise<InterventionDecision> }> = {};
      const order: string[] = [];
      const stamps: Record<string, number> = {};
      const mask = (v: unknown): unknown => {
        let text = JSON.stringify(v);
        if (text === undefined) return null;
        order.forEach((h, i) => {
          text = text.split(handles[h].id).join(`<hold:${i}>`);
          text = text.split(`"createdAt":${stamps[h]}`).join(`"createdAt":"<at:${i}>"`);
        });
        return JSON.parse(text);
      };
      const subst = (v: unknown): unknown => {
        let text = JSON.stringify(v);
        for (const [h, { id }] of Object.entries(handles)) text = text.split(`"$${h}"`).join(JSON.stringify(id));
        return JSON.parse(text);
      };
      const steps: unknown[] = [];

      for (const step of scene.steps) {
        if ('answers' in step) {
          scriptedAnswers = [...step.answers];
          steps.push({ answers: step.answers });
        } else if ('hold' in step) {
          const definition = define(step.def);
          const draw = step.faces ? { form: 'dice' as const, faces: step.faces } : { form: 'range' as const, unit: step.unit ?? 0.7 };
          const snapshot = { ...SNAPSHOT, character: step.character?.name ?? null };
          const baseline = await executeCustomTool(definition, null, {
            metadata: snapshot.metadata,
            progress: snapshot.progress,
            now: snapshot.now,
            ...(snapshot.character ? { character: snapshot.character } : {}),
            state: snapshot.state,
            inventory: null,
            draw,
            ...(step.consult ? { consult: step.consult } : {}),
          });
          handles[step.hold] = hold({
            chatId: step.chat,
            userId: 'user-1',
            entrance: step.trigger ? 'trigger' : 'llm',
            characterId: step.character?.id ?? null,
            characterName: step.character?.name ?? null,
            ...(step.trigger ? { triggerPhrase: step.trigger } : {}),
            definition,
            definitionTier: 'global',
            definitionMountId: 'mount-1',
            snapshot,
            draw: baseline.draw,
            baseline,
            consults: step.consult ? [step.consult] : [],
          });
          order.push(step.hold);
          stamps[step.hold] = getIntervention(handles[step.hold].id)!.createdAt;
          steps.push({ hold: step.hold, chat: step.chat, def: step.def, draw, consult: step.consult ?? null, character: step.character ?? null, trigger: step.trigger ?? null });
          // Distinct `createdAt` per hold (ranked on both sides anyway).
          await new Promise((r) => setTimeout(r, 3));
        } else if ('get' in step) {
          const res = await (GET as unknown as Handler)(req(step.get, step.action, async () => ({})), ctx, { id: step.get });
          const body = await res.json();
          steps.push({ get: step.get, action: step.action, status: res.status, body: mask(body) });
        } else if ('post' in step) {
          const body = subst(step.body);
          const res = await (POST as unknown as Handler)(req(step.post, step.action, async () => body), ctx, { id: step.post });
          steps.push({ post: step.post, action: step.action, input: step.body, status: res.status, body: mask(await res.json()) });
        } else if ('postRaw' in step) {
          const res = await (POST as unknown as Handler)(
            req(step.postRaw, step.action, async () => JSON.parse(step.raw)),
            ctx,
            { id: step.postRaw },
          );
          steps.push({ postRaw: step.postRaw, action: step.action, raw: step.raw, status: res.status, body: mask(await res.json()) });
        } else if ('decision' in step) {
          const d = await handles[step.decision].decision;
          const view =
            d.kind === 'deal'
              ? { kind: d.kind, overrides: d.overrides, reasks: d.reasks, message: d.run.result.message, outcomeIndex: d.run.result.outcomeIndex, presentation: d.run.presentation, staleAnswer: d.run.staleAnswer }
              : d;
          steps.push({ decision: step.decision, out: view });
        } else if ('status' in step) {
          steps.push({ status: step.status, out: interventionStatus(handles[step.status].id) });
        } else if ('consults' in step) {
          steps.push({ consults: step.consults, out: getIntervention(handles[step.consults].id)?.consults ?? null });
        }
      }
      rows.push(
        JSON.stringify({
          id: scene.id,
          steps,
          promptsSeen,
          invokerBuilt,
          lines: mask(capturedLines.filter((l) => /^(api\.v1\.chats\.custom-tools\.interventions|pascal\.intervention\.registry|pascal\.custom-tools)$/.test(String(l.meta.context)))),
        }),
      );
    }
    resetInterventionRegistryForTests();
    fs.writeFileSync(out, `${rows.join('\n')}\n`);
  });
});
