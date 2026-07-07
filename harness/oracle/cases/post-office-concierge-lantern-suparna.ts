/**
 * Tier-1 differential oracle for the W4.6b personified-system WRITER pure
 * builders — driving v4's REAL exported functions so the byte-exact Concierge
 * danger builders, the Lantern alert resolver, and the Suparṇā mail whisper are
 * proven against the Rust port ([`services::concierge_notifications`],
 * [`services::lantern_notifications`], [`services::suparna_notifications`]).
 *
 * Only v4's EXPORTED functions can be driven here:
 *   - `buildDangerContent` / `buildDangerOpaqueContent`   (concierge writer)
 *   - `isLanternImageAlertEnabled`                        (lantern resolver)
 *   - `buildSuparnaMailWhisper`                           (suparna writer)
 * The Concierge MANUAL builders and the Lantern BODY builders are module-private
 * in v4 (static per-kind strings, no interpolation) — the Rust port transcribes
 * them verbatim and covers them with self-tests; the post paths that consume them
 * are exercised by the parent's central tier-3.
 *
 * Pure-only: no DB. Runs under `npx tsx`. Emits NDJSON to stdout.
 *
 * `TZ=UTC` is REQUIRED — `buildSuparnaMailWhisper` renders each letter's date via
 * `formatLetterDate` (system-TZ `toLocaleDateString`), while the Rust
 * `format_time` is UTC-pinned (the documented harness seam shared with
 * `context-feeders-leaves` / `mail-carina-tools`).
 *
 * Generate (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx $V5/harness/oracle/cases/post-office-concierge-lantern-suparna.ts \
 *       > /tmp/oracle-po-cls.ndjson
 * Run:
 *   QT_ORACLE_PO_CLS=/tmp/oracle-po-cls.ndjson \
 *     cargo test -p quilltap-harness --test post_office_concierge_lantern_suparna_equivalence
 */

import {
  buildDangerContent,
  buildDangerOpaqueContent,
  type ConciergeDangerDetails,
} from '@/lib/services/concierge-notifications/writer';
import { isLanternImageAlertEnabled } from '@/lib/services/lantern-notifications/resolver';
import { buildSuparnaMailWhisper } from '@/lib/services/suparna-notifications/writer';
import type { DeliveredLetterSummary } from '@/lib/post-office/mailbox';

// ---- Concierge danger builders ----
const dangerCases: Array<{ id: string; details?: ConciergeDangerDetails }> = [
  { id: 'no-details' },
  {
    id: 'crossed-single-moderation',
    details: { score: 0.92, threshold: 0.7, categories: [{ category: 'nsfw', score: 0.92 }], source: 'moderation', providerName: 'OPENAI' },
  },
  {
    id: 'crossed-two-llm',
    details: {
      score: 0.88,
      threshold: 0.7,
      categories: [{ category: 'nsfw', score: 0.88 }, { category: 'violence', score: 0.75 }],
      source: 'llm',
      providerName: 'DEEPSEEK',
    },
  },
  {
    id: 'crossed-three-plus-moderation',
    details: {
      score: 0.95,
      threshold: 0.7,
      categories: [
        { category: 'nsfw', score: 0.95 },
        { category: 'violence', score: 0.9 },
        { category: 'hate_speech', score: 0.8 },
        { category: 'self_harm', score: 0.72 },
      ],
      source: 'moderation',
      providerName: 'OPENAI',
    },
  },
  {
    id: 'crossed-mixed-crossing',
    details: {
      score: 0.88,
      threshold: 0.7,
      categories: [{ category: 'nsfw', score: 0.88 }, { category: 'violence', score: 0.6 }],
      source: 'moderation',
      providerName: 'OPENAI',
    },
  },
  {
    id: 'crossed-no-provider',
    details: { score: 0.9, threshold: 0.7, categories: [{ category: 'nsfw', score: 0.9 }] },
  },
  {
    id: 'crossed-zero-categories',
    details: { score: 0.8, threshold: 0.7, categories: [], source: 'llm', providerName: 'GROK' },
  },
  {
    id: 'below-flagged-moderation',
    details: {
      score: 0.5,
      threshold: 0.7,
      categories: [{ category: 'nsfw', score: 0.5 }, { category: 'violence', score: 0.3 }],
      source: 'moderation',
      providerName: 'OPENAI',
    },
  },
  {
    id: 'below-flagged-llm',
    details: { score: 0.6, threshold: 0.7, categories: [{ category: 'self_harm', score: 0.6 }], source: 'llm', providerName: 'DEEPSEEK' },
  },
  {
    id: 'below-no-provider',
    details: { score: 0.4, threshold: 0.7, categories: [{ category: 'nsfw', score: 0.4 }] },
  },
  {
    id: 'below-zero-categories',
    details: { score: 0.5, threshold: 0.7, categories: [], source: 'moderation', providerName: 'OPENAI' },
  },
  {
    id: 'unknown-cat-with-label',
    details: { score: 0.9, threshold: 0.7, categories: [{ category: 'weird_stuff', score: 0.9, label: 'Weird Stuff' }], source: 'llm', providerName: 'GROK' },
  },
  {
    id: 'unknown-cat-no-label',
    details: { score: 0.9, threshold: 0.7, categories: [{ category: 'mystery', score: 0.9 }], source: 'moderation', providerName: 'OPENAI' },
  },
  {
    id: 'tie-scores-stable',
    details: {
      score: 0.8,
      threshold: 0.7,
      categories: [
        { category: 'nsfw', score: 0.8 },
        { category: 'violence', score: 0.8 },
        { category: 'hate_speech', score: 0.8 },
        { category: 'self_harm', score: 0.8 },
      ],
      source: 'llm',
      providerName: 'GROK',
    },
  },
  {
    id: 'rounding-edge',
    details: {
      score: 0.925,
      threshold: 0.005,
      categories: [{ category: 'nsfw', score: 0.125 }, { category: 'violence', score: 0.005 }],
      source: 'moderation',
      providerName: 'OPENAI',
    },
  },
];

// ---- Lantern alert resolver ----
type AlertChat = { alertCharactersOfLanternImages: boolean | null } | null;
type AlertProject = { defaultAlertCharactersOfLanternImages: boolean | null } | null;
const alertCases: Array<{ id: string; chat: AlertChat; project: AlertProject }> = [
  { id: 'chat-true', chat: { alertCharactersOfLanternImages: true }, project: null },
  { id: 'chat-false', chat: { alertCharactersOfLanternImages: false }, project: { defaultAlertCharactersOfLanternImages: true } },
  { id: 'chat-null-project-true', chat: { alertCharactersOfLanternImages: null }, project: { defaultAlertCharactersOfLanternImages: true } },
  { id: 'chat-null-project-false', chat: { alertCharactersOfLanternImages: null }, project: { defaultAlertCharactersOfLanternImages: false } },
  { id: 'chat-null-project-null', chat: { alertCharactersOfLanternImages: null }, project: { defaultAlertCharactersOfLanternImages: null } },
  { id: 'both-absent', chat: null, project: null },
  { id: 'chat-true-project-false', chat: { alertCharactersOfLanternImages: true }, project: { defaultAlertCharactersOfLanternImages: false } },
];

// ---- Suparṇā mail whisper ----
const suparnaCases: Array<{ id: string; letters: DeliveredLetterSummary[] }> = [
  { id: 'empty', letters: [] },
  {
    id: 'single',
    letters: [{ path: 'Mail/friday-2026-02-01.md', from: 'Friday', sentAt: '2026-02-01T09:05:00.000Z', body: '  Hello there.  ', alerted: false, inReplyTo: null }],
  },
  {
    id: 'plural-with-blank',
    letters: [
      { path: 'Mail/a.md', from: 'Ada', sentAt: '2026-02-02T09:00:00.000Z', body: 'First line.\n\nSecond paragraph.', alerted: false, inReplyTo: null },
      { path: 'Mail/b.md', from: 'Bob', sentAt: '2026-02-01T09:00:00.000Z', body: '   ', alerted: false, inReplyTo: null },
    ],
  },
];

function main(): void {
  const out: string[] = [];
  const emit = (kind: string, id: string, value: unknown) =>
    out.push(JSON.stringify({ kind, id, value }));

  for (const c of dangerCases) {
    emit('danger_content', c.id, buildDangerContent(c.details));
    emit('danger_opaque', c.id, buildDangerOpaqueContent(c.details));
  }
  for (const c of alertCases) {
    emit('lantern_alert', c.id, isLanternImageAlertEnabled(c.chat as never, c.project as never));
  }
  for (const c of suparnaCases) {
    emit('suparna_whisper', c.id, buildSuparnaMailWhisper(c.letters));
  }

  for (const line of out) process.stdout.write(line + '\n');
}

main();
