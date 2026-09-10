/**
 * Oracle case (P4.D173): the route-trail chokepoint — the ONLY writer of
 * `StreamingState.routeFailures` / `routeVia` and the only composer of the
 * persisted `routeTrail` column.
 *
 * Drives the REAL exports of v4's `lib/services/chat-message/route-trail.ts`
 * (`5841a8c62`): `viaOf`, `recordRouteFailure`, `setRouteVia`,
 * `classifyEmptyBody`, `buildRouteTrail`. `truncateDetail` is module-private in
 * v4 and is exercised THROUGH `recordRouteFailure`, which is the only caller.
 *
 * The `record` / `compose` rows carry `JSON.stringify` of the entry (not a
 * parsed object), so key ORDER and key PRESENCE are both comparands — v4 spreads
 * `...(evidence ? {evidence} : {})`, so an absent optional is genuinely absent,
 * never `null`, and a `Value`-level compare would not see the difference.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/route-trail-compose.ts \
 *     > /tmp/oracle-route-trail-compose.ndjson
 */

import {
  viaOf,
  recordRouteFailure,
  setRouteVia,
  classifyEmptyBody,
  buildRouteTrail,
} from '@/lib/services/chat-message/route-trail';
import type { StreamingState } from '@/lib/services/chat-message/types';
import type { RouteAttempt, RouteAttemptVia } from '@/lib/schemas/chat.types';
import type { ConnectionProfile } from '@/lib/schemas/types';
import type { FallbackTrigger, FallbackCandidateKind } from '@/lib/llm/fallback';

type Seat = { id: string; name: string; provider: string; modelName: string };

type Row =
  | { kind: 'viaOf'; id: string; candidateKind: string; out: string }
  | {
      kind: 'classify';
      id: string;
      rawResponse: unknown;
      contentWasFlaggedDangerous: boolean;
      out: { outcome: string; trigger: string; evidence?: string; detail?: string };
    }
  | {
      kind: 'record';
      id: string;
      seat: Seat;
      via: string;
      outcome: string;
      trigger: string;
      detail?: string;
      evidence?: string;
      /** `JSON.stringify` of the single pushed entry — key order + presence. */
      out: string;
      failuresAfter: number;
    }
  | {
      kind: 'compose';
      id: string;
      failures: unknown[];
      routeVia: string;
      seat: Seat;
      /** `JSON.stringify` of the composed trail, or null. */
      out: string | null;
    }
  | { kind: 'setVia'; id: string; from: string; to: string; out: string }
  | {
      kind: 'seed';
      id: string;
      dangerProfileId: string;
      connectionProfileId: string;
      out: string;
    };

const rows: Row[] = [];

const seatOf = (s: Seat) => s as unknown as ConnectionProfile;

/** A minimal `StreamingState`-shaped bag: route-trail.ts reads exactly four fields. */
function state(opts: {
  routeFailures?: RouteAttempt[];
  routeVia?: RouteAttemptVia;
  rawResponse?: unknown;
  effectiveProfile?: Seat;
}): StreamingState {
  return {
    routeFailures: opts.routeFailures ?? [],
    routeVia: opts.routeVia ?? 'primary',
    rawResponse: opts.rawResponse,
    effectiveProfile: (opts.effectiveProfile ?? {
      id: 'cp-primary',
      name: 'House Anthropic',
      provider: 'ANTHROPIC',
      modelName: 'claude-sonnet-5',
    }) as unknown as ConnectionProfile,
  } as unknown as StreamingState;
}

const PRIMARY: Seat = {
  id: 'cp-primary',
  name: 'House Anthropic',
  provider: 'ANTHROPIC',
  modelName: 'claude-sonnet-5',
};
const UNCENSORED: Seat = {
  id: 'cp-uncensored',
  name: 'The Back Room',
  provider: 'OPENROUTER',
  modelName: 'dolphin-mixtral',
};
const UNDERSTUDY: Seat = {
  id: 'cp-understudy',
  name: 'Understudy — Haiku',
  provider: 'ANTHROPIC',
  modelName: 'claude-haiku-4-5',
};

// ---- viaOf ------------------------------------------------------------------
for (const k of ['configured', 'tier-pick', 'primary'] as FallbackCandidateKind[]) {
  rows.push({ kind: 'viaOf', id: `viaOf-${k}`, candidateKind: k, out: viaOf(k) });
}

// ---- classifyEmptyBody ------------------------------------------------------
const classify = (id: string, rawResponse: unknown, flagged: boolean) => {
  const out = classifyEmptyBody(state({ rawResponse }), flagged);
  rows.push({ kind: 'classify', id, rawResponse, contentWasFlaggedDangerous: flagged, out });
};

// The moderation arm, through each provider shape extractFinishReason knows.
classify('mod-openai-content_filter', { choices: [{ finish_reason: 'content_filter' }] }, false);
classify('mod-openai-content-filter-hyphen', { choices: [{ finish_reason: 'content-filter' }] }, false);
classify('mod-anthropic-refusal', { stop_reason: 'refusal' }, false);
classify('mod-google-safety', { candidates: [{ finishReason: 'SAFETY' }] }, false);
classify('mod-google-recitation', { candidates: [{ finishReason: 'RECITATION' }] }, false);
classify('mod-responses-status', { status: 'prohibited_content' }, false);
classify('mod-spii', { stop_reason: 'spii' }, false);
classify('mod-image_safety', { stop_reason: 'image_safety' }, false);
classify('mod-blocklist', { stop_reason: 'blocklist' }, false);
// Trimming + case folding inside isModerationFinishReason.
classify('mod-padded-mixed-case', { stop_reason: '  Content_Filter \n' }, false);
// The moderation arm WINS over the flagged inference.
classify('mod-beats-flagged', { choices: [{ finish_reason: 'content_filter' }] }, true);

// The inferred arm: a non-moderation (or absent) finish reason + a flagged turn.
classify('inferred-flagged-no-reason', undefined, true);
classify('inferred-flagged-null-raw', null, true);
classify('inferred-flagged-stop', { choices: [{ finish_reason: 'stop' }] }, true);

// The plain empty-response arm.
classify('empty-no-reason', undefined, false);
classify('empty-null-raw', null, false);
classify('empty-non-object-raw', 'not an object', false);
classify('empty-array-raw', [1, 2, 3], false);
classify('empty-with-stop', { choices: [{ finish_reason: 'stop' }] }, false);
classify('empty-with-length', { choices: [{ finish_reason: 'length' }] }, false);
classify('empty-anthropic-max-tokens', { stop_reason: 'max_tokens' }, false);
classify('empty-google-max-tokens', { candidates: [{ finishReason: 'MAX_TOKENS' }] }, false);
classify('empty-responses-incomplete', { status: 'incomplete' }, false);
// A finish reason that is the EMPTY string: JS truthiness omits `detail`.
classify('empty-blank-finish-reason', { choices: [{ finish_reason: '' }] }, false);
// A non-string finish reason field is not a finish reason (the probe continues).
classify('empty-numeric-finish-reason', { choices: [{ finish_reason: 7 }] }, false);
// Probe ORDER: choices wins over stop_reason wins over candidates wins over status.
classify('order-choices-beats-stop_reason', { choices: [{ finish_reason: 'stop' }], stop_reason: 'refusal' }, false);
classify('order-stop_reason-beats-candidates', { stop_reason: 'end_turn', candidates: [{ finishReason: 'SAFETY' }] }, false);
classify('order-candidates-beats-status', { candidates: [{ finishReason: 'STOP' }], status: 'refusal' }, false);

// ---- recordRouteFailure -----------------------------------------------------
const record = (
  id: string,
  seat: Seat,
  via: RouteAttemptVia,
  outcome: 'failed' | 'refused',
  trigger: FallbackTrigger,
  detail?: string,
  evidence?: 'finish-reason' | 'inferred'
) => {
  const s = state({});
  recordRouteFailure(s, seatOf(seat), via, outcome, trigger, detail, evidence);
  rows.push({
    kind: 'record',
    id,
    seat,
    via,
    outcome,
    trigger,
    detail,
    evidence,
    out: JSON.stringify(s.routeFailures[0]),
    failuresAfter: s.routeFailures.length,
  });
};

record('record-plain-failed', PRIMARY, 'primary', 'failed', 'empty-response');
record('record-auth-with-detail', UNDERSTUDY, 'understudy', 'failed', 'auth', 'no API key configured');
record('record-tier-pick', UNDERSTUDY, 'tier-pick', 'failed', 'network', 'fetch failed');
record('record-refused-finish-reason', UNCENSORED, 'concierge', 'refused', 'moderation-refusal', 'finish_reason: content_filter', 'finish-reason');
record('record-refused-inferred', PRIMARY, 'retry', 'refused', 'moderation-refusal', 'empty response on content the Concierge had flagged', 'inferred');
// `detail` shapes — truncateDetail's whole matrix, through its only caller.
record('detail-undefined', PRIMARY, 'primary', 'failed', 'provider-error');
record('detail-empty-string', PRIMARY, 'primary', 'failed', 'provider-error', '');
record('detail-whitespace-only', PRIMARY, 'primary', 'failed', 'provider-error', '   \t\n  ');
record('detail-nbsp-only', PRIMARY, 'primary', 'failed', 'provider-error', '  ﻿');
record('detail-trimmed', PRIMARY, 'primary', 'failed', 'provider-error', '  429 Too Many Requests  ');
record('detail-199', PRIMARY, 'primary', 'failed', 'provider-error', 'a'.repeat(199));
record('detail-200', PRIMARY, 'primary', 'failed', 'provider-error', 'a'.repeat(200));
record('detail-201', PRIMARY, 'primary', 'failed', 'provider-error', 'a'.repeat(201));
record('detail-201-trims-to-200', PRIMARY, 'primary', 'failed', 'provider-error', ` ${'a'.repeat(200)} `);
record('detail-long-mixed', PRIMARY, 'primary', 'failed', 'provider-error', `${'x'.repeat(150)} ${'y'.repeat(100)}`);
// Astral: 100 code points = 200 UTF-16 units → NOT truncated (the boundary).
record('detail-astral-200-units', PRIMARY, 'primary', 'failed', 'provider-error', '😀'.repeat(100));
// 101 code points = 202 units → truncated at 199, which SPLITS a surrogate pair.
// v4 emits a lone high surrogate; Rust cannot represent one. RECORDED DIVERGENCE.
record('detail-astral-splits-a-surrogate', PRIMARY, 'primary', 'failed', 'provider-error', '😀'.repeat(101));
// A BMP character straddling nothing: 201 units where unit 199 is a whole char.
record('detail-201-bmp-astral-tail', PRIMARY, 'primary', 'failed', 'provider-error', `${'a'.repeat(198)}😀x`);

// ---- setRouteVia ------------------------------------------------------------
for (const to of ['primary', 'retry', 'concierge', 'understudy', 'tier-pick'] as RouteAttemptVia[]) {
  const s = state({ routeVia: 'primary' });
  setRouteVia(s, to);
  rows.push({ kind: 'setVia', id: `setVia-${to}`, from: 'primary', to, out: s.routeVia });
}

// ---- buildRouteTrail --------------------------------------------------------
const failure = (seat: Seat, via: RouteAttemptVia, extra: Partial<RouteAttempt> = {}): RouteAttempt => ({
  profileId: seat.id,
  profileName: seat.name,
  provider: seat.provider,
  modelName: seat.modelName,
  via,
  outcome: 'failed',
  trigger: 'empty-response',
  ...extra,
});

const compose = (id: string, failures: RouteAttempt[], routeVia: RouteAttemptVia, seat: Seat) => {
  const s = state({ routeFailures: failures, routeVia, effectiveProfile: seat });
  const out = buildRouteTrail(s, { chatId: 'chat-1', messageId: 'msg-1' });
  rows.push({
    kind: 'compose',
    id,
    failures: failures as unknown[],
    routeVia,
    seat,
    out: out === null ? null : JSON.stringify(out),
  });
};

// THE null rule: nothing failed → null, whatever the via says.
compose('compose-null-no-failures', [], 'primary', PRIMARY);
compose('compose-null-no-failures-concierge-via', [], 'concierge', UNCENSORED);
// One failure → two entries; the answering entry carries NO trigger/evidence/detail.
compose('compose-retry-answered', [failure(PRIMARY, 'primary')], 'retry', PRIMARY);
compose('compose-concierge-answered', [failure(PRIMARY, 'primary')], 'concierge', UNCENSORED);
compose(
  'compose-understudy-after-two',
  [
    failure(PRIMARY, 'primary'),
    failure(UNCENSORED, 'concierge', { outcome: 'refused', trigger: 'moderation-refusal', evidence: 'finish-reason', detail: 'finish_reason: content_filter' }),
  ],
  'understudy',
  UNDERSTUDY
);
compose(
  'compose-tier-pick-after-auth',
  [failure(UNDERSTUDY, 'understudy', { trigger: 'auth', detail: 'no API key configured' })],
  'tier-pick',
  UNCENSORED
);
// The answering entry ALWAYS names state.effectiveProfile, even when it equals a
// failure's seat (the same-profile retry that finally answered).
compose('compose-same-seat-twice', [failure(PRIMARY, 'primary')], 'retry', PRIMARY);
// Order is preserved exactly as pushed.
compose(
  'compose-order-preserved',
  [failure(PRIMARY, 'primary'), failure(UNCENSORED, 'concierge'), failure(UNDERSTUDY, 'understudy', { trigger: 'network' })],
  'tier-pick',
  { id: 'cp-tier', name: 'Tier Pick — GPT', provider: 'OPENAI', modelName: 'gpt-5' }
);

// ---- the orchestrator's unconditional routeVia seeding ----------------------
// NOTE: this is orchestrator.service.ts:540's one-line ternary, transcribed
// rather than driven (importing processMessage would drag the whole spine in).
// The real proof of the SEEDING is the tier-3 orchestrator arm; this row pins
// the expression's two answers so a v4 reword is caught here too.
const seed = (id: string, dangerProfileId: string, connectionProfileId: string) => {
  const out: RouteAttemptVia = dangerProfileId !== connectionProfileId ? 'concierge' : 'primary';
  rows.push({ kind: 'seed', id, dangerProfileId, connectionProfileId, out });
};
seed('seed-no-reroute', 'cp-primary', 'cp-primary');
seed('seed-pre-call-reroute', 'cp-uncensored', 'cp-primary');

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
