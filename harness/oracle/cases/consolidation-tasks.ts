/**
 * Tier-1 oracle — v4's consolidation task family (P4.D271, v4 `d58548051`,
 * byte-identical at the `01a83539d` pin), driven through v4's REAL exports of
 * `lib/memory/cheap-llm-tasks/consolidation-tasks.ts`:
 * `CONSOLIDATION_TASK_TYPE`, `CONSOLIDATION_SYSTEM_PROMPT`,
 * `buildConsolidationUserMessage`, `parseConsolidationResponse` and
 * `validateConsolidationOutput` (over raw values JSON text cannot spell as an
 * object root).
 *
 * Corpus: user messages over the three buckets × both cluster kinds × canon /
 * existing digest present / whitespace-only / absent, a 1,200-unit cut (BMP and
 * through a surrogate pair), `toFixed(2)` rounding edges, non-integer counts,
 * `undated`, newline-run flattening; responses over every reason string, each
 * Zod first-issue shape, code fences, prose-wrapped JSON, a non-object root and
 * unparseable text.
 *
 * Output: one NDJSON line per case `{case, …}`.
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/qt-consolidation-tasks.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/consolidation-tasks.ts > /tmp/qt-consolidation-tasks.ndjson
 */

import {
  CONSOLIDATION_SYSTEM_PROMPT,
  CONSOLIDATION_TASK_TYPE,
  buildConsolidationUserMessage,
  parseConsolidationResponse,
  validateConsolidationOutput,
  type ConsolidationCallInput,
} from '@/lib/memory/cheap-llm-tasks/consolidation-tasks';

function emit(obj: unknown): void {
  process.stdout.write(JSON.stringify(obj) + '\n');
}

/** A string JSON can carry losslessly, or a marker for a lone surrogate. */
function hasLoneSurrogate(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const n = s.charCodeAt(i + 1);
      if (n >= 0xdc00 && n <= 0xdfff) {
        i++;
        continue;
      }
      return true;
    }
    if (c >= 0xdc00 && c <= 0xdfff) return true;
  }
  return false;
}

function main(): void {
  emit({ case: 'constants', taskType: CONSOLIDATION_TASK_TYPE, systemPrompt: CONSOLIDATION_SYSTEM_PROMPT });

  // ── user messages ─────────────────────────────────────────────────────────
  const members = [
    { handle: 'm1', when: '2026-08-01T10:00:00.000Z', importance: 0.6, reinforcementCount: 1, content: 'Laura likes maps.' },
    { handle: 'm2', when: null, importance: 0.8, reinforcementCount: 3, content: 'Laura drew a new chart.\n\n  It was\tlarge. ' },
    { handle: 'm3', when: '', importance: 0.125, reinforcementCount: 2.5, content: '  a  b \r\n c \n d  ' },
    { handle: 'm4', when: '2026', importance: 0.005, reinforcementCount: 0, content: 'x'.repeat(1200) },
    { handle: 'm5', when: '2026-09-09T00:00:00Z', importance: 1, reinforcementCount: 50, content: 'y'.repeat(1201) },
    { handle: 'm6', when: '2026-09-09', importance: 0.995, reinforcementCount: 7, content: 'z'.repeat(1198) + '😀tail' },
    { handle: 'm7', when: '2026-09-09', importance: 0.015, reinforcementCount: 1, content: '' },
    { handle: 'm8', when: '2026-09-09', importance: 0.045, reinforcementCount: 1, content: '﻿bom\u0085nel' },
  ];
  const variants: Array<Partial<ConsolidationCallInput>> = [];
  for (const bucket of ['self', 'other', 'none'] as const) {
    for (const clusterKind of ['semantic', 'episodic'] as const) {
      for (const [canonBlock, existingDigest] of [
        [null, null],
        ['  [IDENTITY] A cartographer.\n\n[DESCRIPTION] Tall.  ', null],
        [null, 'I keep the Estate.\nIt is large.'],
        ['   \n\t ', '  \n '],
        ['Canon.', 'Digest.'],
      ] as Array<[string | null, string | null]>) {
        variants.push({ bucket, clusterKind, canonBlock, existingDigest });
      }
    }
  }
  for (const v of variants) {
    const input: ConsolidationCallInput = {
      holderName: 'Friday',
      subjectName: v.bucket === 'self' ? 'Friday' : v.bucket === 'other' ? 'Laura' : '(no subject)',
      bucket: v.bucket!,
      clusterKind: v.clusterKind!,
      canonBlock: v.canonBlock ?? null,
      existingDigest: v.existingDigest ?? null,
      members,
    };
    const out = buildConsolidationUserMessage(input);
    const lossy = hasLoneSurrogate(out);
    emit({ case: 'userMessage', input, out: lossy ? null : out, lossy, ...(lossy ? { escaped: JSON.stringify(out) } : {}) });
  }
  // The surrogate-pair cut, alone: v4 leaves a lone high surrogate.
  {
    const input: ConsolidationCallInput = {
      holderName: 'H',
      subjectName: 'S',
      bucket: 'other',
      clusterKind: 'semantic',
      canonBlock: null,
      existingDigest: 'q'.repeat(1199) + '😀',
      members: [
        { handle: 'm1', when: null, importance: 0.5, reinforcementCount: 1, content: 'w'.repeat(1199) + '😀' },
        { handle: 'm2', when: '2026-09-09', importance: 0.995, reinforcementCount: 7, content: 'z'.repeat(1199) + '😀tail' },
      ],
    };
    const out = buildConsolidationUserMessage(input);
    emit({
      case: 'userMessage',
      input,
      out: hasLoneSurrogate(out) ? null : out,
      lossy: hasLoneSurrogate(out),
      // JSON.stringify escapes the lone unit as \udXXX; the Rust side records
      // its U+FFFD shape against this.
      escaped: JSON.stringify(out),
    });
  }

  // ── responses ─────────────────────────────────────────────────────────────
  const h = ['m1', 'm2', 'm3', 'm4'];
  const d = (memberIds: unknown, extra: Record<string, unknown> = {}) => ({
    content: 'Laura keeps maps.',
    summary: 'laura keeps maps',
    keywords: ['maps', 'past', 'scope: wide', 'trivia'],
    importance: 0.6,
    kind: 'semantic',
    memberIds,
    ...extra,
  });
  const values: unknown[] = [
    // valid shapes
    { digests: [d(['m1', 'm2'])], keepStandalone: ['m3'] },
    { digests: [d(['m1'])] },
    { digests: [] },
    { digests: [d([' m1 ', 'm2'], { content: '  padded  ', summary: '\tsum\n' })], keepStandalone: [' m3'] },
    { digests: [d(['m1'], { kind: 'episodic', occurredAt: '2026-08-01' })] },
    { digests: [d(['m1'], { occurredAt: null })] },
    { digests: [{ content: 'c', summary: 's', importance: 0, memberIds: ['m1'] }] },
    { digests: [d(['m1'], { importance: 1, extra: 'stripped' })], unknownKey: 1 },
    { digests: [d(['m1'])], keepStandalone: ['m2', 'm2', 'm3'] },
    {
      digests: [d(['m1'])],
      contradictions: [
        { olderId: 'm1', newerId: 'm2', note: 'changed' },
        { olderId: 'm9', newerId: 'm2' },
        { olderId: 'm3', newerId: 'm3' },
        { olderId: ' m2 ', newerId: 'm4' },
      ],
    },
    // membership rules
    { digests: [d(['m9'])] },
    { digests: [d(['m1']), d(['m1'])] },
    { digests: [d(['m1'])], keepStandalone: ['m1'] },
    { digests: [d(['m1'])], keepStandalone: ['m7'] },
    { digests: [d(['m1', 'm2']), d(['m3', 'm2'])] },
    // Zod first-issue shapes
    {},
    { digests: 'nope' },
    { digests: null },
    { digests: [null] },
    { digests: ['x'] },
    { digests: [d(['m1'], { content: '' })] },
    { digests: [d(['m1'], { content: '   ' })] },
    { digests: [d(['m1'], { content: 5 })] },
    { digests: [d(['m1'], { content: undefined })] },
    { digests: [d(['m1'], { summary: null })] },
    { digests: [d(['m1'], { keywords: null })] },
    { digests: [d(['m1'], { keywords: 'maps' })] },
    { digests: [d(['m1'], { keywords: ['ok', 3] })] },
    { digests: [d(['m1'], { importance: 2 })] },
    { digests: [d(['m1'], { importance: -0.1 })] },
    { digests: [d(['m1'], { importance: '0.5' })] },
    { digests: [d(['m1'], { importance: undefined })] },
    { digests: [d(['m1'], { kind: 'fact' })] },
    { digests: [d(['m1'], { kind: null })] },
    { digests: [d(['m1'], { kind: 3 })] },
    { digests: [d(['m1'], { occurredAt: 5 })] },
    { digests: [d([])] },
    { digests: [d(['m1', ''])] },
    { digests: [d(['m1', 7])] },
    { digests: [d('m1')] },
    { digests: [d(undefined)] },
    { digests: [d(['m1']), d([], { content: '' })] },
    { digests: [d(['m1'], { content: '', importance: 5 })] },
    { digests: [d(['m1'])], keepStandalone: 'm2' },
    { digests: [d(['m1'])], keepStandalone: [''] },
    { digests: [d(['m1'])], keepStandalone: null },
    { digests: [d(['m1'])], contradictions: {} },
    { digests: [d(['m1'])], contradictions: [{ olderId: '', newerId: 'm2' }] },
    { digests: [d(['m1'])], contradictions: [{ olderId: 'm1' }] },
    { digests: [d(['m1'])], contradictions: [{ olderId: 'm1', newerId: 'm2', note: 4 }] },
    { digests: [d(['m1'])], contradictions: ['x'] },
    { digests: 'nope', keepStandalone: 5 },
  ];
  for (const raw of values) {
    // Text round trip: what a model would send.
    const text = JSON.stringify(raw);
    emit({ case: 'parse', handles: h, content: text, out: parseConsolidationResponse(text, h) });
  }
  // Raw roots JSON objects cannot express through parseLLMJsonObject.
  for (const raw of [null, [], 'str', 5, true, [{ digests: [] }]]) {
    emit({ case: 'validate', handles: h, raw, out: validateConsolidationOutput(raw, h) });
  }
  // Text-level shapes.
  const texts = [
    '```json\n{"digests": [], "keepStandalone": ["m1"]}\n```',
    'Here you go:\n```\n{"digests": []}\n``` hope that helps',
    'Sure! {"digests": [], "keepStandalone": ["m2"]} — done.',
    '{"digests": [{"content": "line one\nline two", "summary": "s", "importance": 0.4, "memberIds": ["m1"]}]}',
    '{"digests": [{"content": "c", "summary": "s", "importance": 0.4, "memberIds": ["m1"]}',
    'not json at all',
    '',
    '   ',
    '[1, 2, 3]',
    '"just a string"',
    '{"digests": [}',
    '{"digests": [] , }',
    '{ digests: [] }',
    "{'digests': []}",
    '{"digests": [], "keepStandalone": ["m1",]}',
    '{"a": 1} {"b": 2}',
    '{"digests": NaN}',
    '{"digests": [], "x": tru}',
    '```json\n```',
    '{"digests": [{"content": "\\q"}]}',
    '{"digests": [], "contradictions": [{"olderId": "m1", "newerId": "m2", "note": "\\u00e9"}]}',
  ];
  for (const t of texts) {
    emit({ case: 'parse', handles: h, content: t, out: parseConsolidationResponse(t, h) });
  }
  process.exit(0);
}

try {
  main();
} catch (err) {
  process.stderr.write(`consolidation-tasks oracle failed: ${(err as Error)?.stack ?? err}\n`);
  process.exit(1);
}
