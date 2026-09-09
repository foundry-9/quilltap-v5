/**
 * The v4-side recorder behind `src/app/chat/route-trail-display.oracle.spec.ts`.
 *
 * Records `lib/chat/route-trail-display.ts`'s pure functions (collapse, hover
 * text, glyphs, labels) against the REAL module, plus the three enums'
 * literal values straight off `lib/schemas/chat.types.ts`'s zod schema — the
 * ground truth for the SPA's hand-rolled `RouteAttemptVia`/`RouteAttemptOutcome`/
 * `RouteAttemptTrigger` unions (v5 has no zod; `spa-has-no-zod-schema-twins-
 * are-hand-rolled`).
 *
 * Run it from a pinned v4 worktree:
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d177-78b381a96
 * cp <V5>/apps/web/oracle/route-trail-display.ts "$PIN/"
 * cd "$PIN" && npx tsx route-trail-display.ts \
 *   > <V5>/apps/web/src/testing/fixtures/route-trail-display.oracle.ndjson
 * ```
 *
 * Expect 25 lines.
 */

import {
  ROUTE_OUTCOME_GLYPH,
  collapseRouteTrail,
  describeRouteAttempt,
  routeOutcomeLabel,
} from './lib/chat/route-trail-display';
import {
  RouteAttemptOutcomeEnum,
  RouteAttemptSchema,
  RouteAttemptViaEnum,
  type RouteAttempt,
} from './lib/schemas/chat.types';

const OPENAI = '00000000-0000-4000-8000-00000000000a';
const ANTHROPIC = '00000000-0000-4000-8000-00000000000b';
const DEEPSEEK = '00000000-0000-4000-8000-00000000000c';

function attempt(overrides: Partial<RouteAttempt> = {}): RouteAttempt {
  return {
    profileId: OPENAI,
    profileName: 'OpenAI gpt-5',
    provider: 'openai',
    modelName: 'gpt-5',
    via: 'primary',
    outcome: 'failed',
    trigger: 'network',
    detail: 'Connection error.',
    ...overrides,
  } as RouteAttempt;
}

// --- collapse: the four test-suite scenarios, row-for-row ---
const COLLAPSE_CASES: Array<[string, RouteAttempt[]]> = [
  [
    'distinct-profiles',
    [
      attempt(),
      attempt({
        profileId: ANTHROPIC,
        profileName: 'Anthropic Sonnet',
        provider: 'anthropic',
        modelName: 'claude-sonnet-5',
        via: 'understudy',
        outcome: 'refused',
        trigger: 'moderation-refusal',
        evidence: 'finish-reason',
        detail: 'finish_reason: refusal',
      }),
      attempt({
        profileId: DEEPSEEK,
        profileName: 'DeepSeek',
        provider: 'deepseek',
        modelName: 'deepseek-v4-pro',
        via: 'tier-pick',
        outcome: 'answered',
        trigger: undefined,
        detail: undefined,
      }),
    ],
  ],
  [
    'adjacent-collapse',
    [
      attempt({ outcome: 'failed', trigger: 'empty-response', detail: 'empty response' }),
      attempt({ via: 'retry', outcome: 'answered', trigger: undefined, detail: undefined }),
    ],
  ],
  [
    'non-adjacent-no-collapse',
    [
      attempt(),
      attempt({
        profileId: ANTHROPIC,
        profileName: 'Anthropic Sonnet',
        provider: 'anthropic',
        modelName: 'claude-sonnet-5',
        via: 'understudy',
      }),
      attempt({ via: 'tier-pick', outcome: 'answered', trigger: undefined, detail: undefined }),
    ],
  ],
  ['empty', []],
];
for (const [id, trail] of COLLAPSE_CASES) {
  const rows = collapseRouteTrail(trail);
  console.log(JSON.stringify({ kind: 'collapse', id, trailJson: JSON.stringify(trail), out: JSON.stringify(rows) }));
}

// --- describeRouteAttempt: one row per hover-text scenario ---
const DESCRIBE_CASES: Array<[string, Partial<RouteAttempt>]> = [
  ['name-first', {}],
  ['via-primary', { via: 'primary' }],
  ['via-retry', { via: 'retry' }],
  ['via-concierge', { via: 'concierge' }],
  ['via-understudy', { via: 'understudy' }],
  ['via-tier-pick', { via: 'tier-pick' }],
  ['failed-with-trigger-detail', { outcome: 'failed', trigger: 'rate-limit', detail: '429 Too Many Requests' }],
  [
    'refused-finish-reason',
    { outcome: 'refused', trigger: 'moderation-refusal', evidence: 'finish-reason', detail: 'finish_reason: content_filter' },
  ],
  [
    'refused-inferred',
    {
      outcome: 'refused',
      trigger: 'moderation-refusal',
      evidence: 'inferred',
      detail: 'empty response on content the Concierge had flagged',
    },
  ],
  ['answered-plain', { outcome: 'answered', trigger: undefined, detail: undefined }],
];
for (const [id, overrides] of DESCRIBE_CASES) {
  const row = collapseRouteTrail([attempt(overrides)])[0];
  console.log(JSON.stringify({ kind: 'describe', id, rowJson: JSON.stringify(row), out: describeRouteAttempt(row) }));
}

// --- ordinal counting past the collapse ---
const ORDINAL_CASES: Array<[string, RouteAttempt[]]> = [
  [
    'second-try',
    [
      attempt({ outcome: 'failed', trigger: 'empty-response' }),
      attempt({ via: 'retry', outcome: 'answered', trigger: undefined, detail: undefined }),
    ],
  ],
  [
    'sixth-try-plain-count',
    [
      attempt({ outcome: 'failed', trigger: 'empty-response' }),
      attempt({ via: 'retry', outcome: 'failed', trigger: 'empty-response' }),
      attempt({ via: 'retry', outcome: 'failed', trigger: 'empty-response' }),
      attempt({ via: 'retry', outcome: 'failed', trigger: 'empty-response' }),
      attempt({ via: 'retry', outcome: 'failed', trigger: 'empty-response' }),
      attempt({ via: 'retry', outcome: 'answered', trigger: undefined, detail: undefined }),
    ],
  ],
];
for (const [id, trail] of ORDINAL_CASES) {
  const [row] = collapseRouteTrail(trail);
  console.log(JSON.stringify({ kind: 'describe', id, rowJson: JSON.stringify(row), out: describeRouteAttempt(row) }));
}

// --- glyphs + labels ---
console.log(JSON.stringify({ kind: 'glyph', id: 'failed', out: ROUTE_OUTCOME_GLYPH.failed }));
console.log(JSON.stringify({ kind: 'glyph', id: 'refused', out: ROUTE_OUTCOME_GLYPH.refused }));
console.log(JSON.stringify({ kind: 'glyph', id: 'answered-absent', out: JSON.stringify('answered' in ROUTE_OUTCOME_GLYPH) }));
for (const outcome of RouteAttemptOutcomeEnum.options) {
  console.log(JSON.stringify({ kind: 'label', id: outcome, out: routeOutcomeLabel(outcome) }));
}

// --- the three enums' literal values, ground truth for the SPA's hand-rolled twins ---
const triggerEnum = (RouteAttemptSchema.shape.trigger as unknown as { unwrap: () => { options: string[] } }).unwrap();
console.log(JSON.stringify({ kind: 'enum', id: 'via', out: JSON.stringify(RouteAttemptViaEnum.options) }));
console.log(JSON.stringify({ kind: 'enum', id: 'outcome', out: JSON.stringify(RouteAttemptOutcomeEnum.options) }));
console.log(JSON.stringify({ kind: 'enum', id: 'trigger', out: JSON.stringify(triggerEnum.options) }));
