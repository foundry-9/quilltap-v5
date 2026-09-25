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
 * Run it from a pinned v4 worktree (re-recorded by P4.D229 at the round
 * target `acadcc7cd` — #73 `8bd080267` added `profileKind` / `label` to the
 * collapsed row, widened `evidence` to five values and gave the refused arm
 * its ` — by its wording` phrase):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d229-acadcc7cd
 * cp <V5>/apps/web/oracle/route-trail-display.ts "$PIN/"
 * cd "$PIN" && PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH npx tsx route-trail-display.ts \
 *   > <V5>/apps/web/src/testing/fixtures/route-trail-display.oracle.ndjson
 * rm "$PIN/route-trail-display.ts"
 * ```
 *
 * Expect 34 lines.
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
  // v4 #73 (`8bd080267`): the image failover chokepoint files `profileKind:
  // 'image'` rows; an image profile is labelled by its NAME, a connection
  // profile by its model — v4's own "image trails" test trail, row for row.
  [
    'image-trail',
    [
      attempt({
        profileKind: 'image',
        profileName: 'House Painter',
        modelName: 'gpt-image-1',
        outcome: 'refused',
        trigger: 'moderation-refusal',
        evidence: 'typed-error',
      }),
      attempt({
        profileId: ANTHROPIC,
        profileKind: 'image',
        profileName: 'Kestrel Studio',
        modelName: 'grok-2-image',
        via: 'concierge',
        outcome: 'answered',
        trigger: undefined,
        detail: undefined,
      }),
      attempt({ profileId: DEEPSEEK, modelName: 'deepseek-chat', outcome: 'answered', trigger: undefined, detail: undefined }),
    ],
  ],
  // An explicit `profileKind: 'connection'` and an image run that collapses
  // (the collapsed row keeps the FIRST attempt's kind and label).
  [
    'image-collapse',
    [
      attempt({ profileKind: 'image', profileName: 'House Painter', modelName: 'gpt-image-1', outcome: 'failed' }),
      attempt({
        profileKind: 'image',
        profileName: 'House Painter',
        modelName: 'gpt-image-1',
        via: 'retry',
        outcome: 'answered',
        trigger: undefined,
        detail: undefined,
      }),
      attempt({ profileId: DEEPSEEK, profileKind: 'connection', modelName: 'deepseek-chat', outcome: 'answered', trigger: undefined, detail: undefined }),
    ],
  ],
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
  // The five refusal evidences (v4 #73): only `inferred` and `message-pattern`
  // add a phrase.
  [
    'refused-typed-error',
    { outcome: 'refused', trigger: 'moderation-refusal', evidence: 'typed-error', detail: 'ModerationRejectionError' },
  ],
  [
    'refused-provider-code',
    { outcome: 'refused', trigger: 'moderation-refusal', evidence: 'provider-code', detail: 'content_policy_violation' },
  ],
  [
    'refused-message-pattern',
    { outcome: 'refused', trigger: 'moderation-refusal', evidence: 'message-pattern', detail: 'content policy' },
  ],
  ['refused-no-evidence', { outcome: 'refused', trigger: 'moderation-refusal', evidence: undefined, detail: undefined }],
  // An image row's hover still prints the MODEL (only the badge prints the label).
  [
    'image-row-hover',
    { profileKind: 'image', profileName: 'House Painter', modelName: 'gpt-image-1', outcome: 'refused', trigger: 'moderation-refusal', evidence: 'typed-error' },
  ],
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
// #73's two new enums on the row, ground truth for the contract's hand-rolled unions.
const evidenceEnum = (RouteAttemptSchema.shape.evidence as unknown as { unwrap: () => { options: string[] } }).unwrap();
const profileKindEnum = (RouteAttemptSchema.shape.profileKind as unknown as { unwrap: () => { options: string[] } }).unwrap();
console.log(JSON.stringify({ kind: 'enum', id: 'evidence', out: JSON.stringify(evidenceEnum.options) }));
console.log(JSON.stringify({ kind: 'enum', id: 'profileKind', out: JSON.stringify(profileKindEnum.options) }));
