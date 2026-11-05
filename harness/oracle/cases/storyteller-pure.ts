/**
 * Oracle case (P4.D343): Storyteller mode's pure modules — v4 `b15fe9404` at
 * the pin `236c04e35`.
 *
 * Drives the REAL code (no DB, a fixed clock where one is read):
 *   parseBidReply                          (lib/storyteller/bid-parse.ts)
 *   computeBidTier, effectOfNamedTool      (lib/storyteller/tiers.ts)
 *   cardFromOutcome                        (lib/storyteller/cards.ts)
 *   inviteProbability, biddableSeats,
 *   chooseInvitees                         (lib/storyteller/invite.ts)
 *   describeDisposition, buildBidSection,
 *   buildPlanSection                       (lib/storyteller/prompts.ts)
 *   buildBidOffer                          (lib/storyteller/bid-call.ts)
 *
 * Inputs live in the committed `harness/oracle/fixtures/storyteller-pure.json`;
 * both sides read it. `tierAllowance` is private in v4 — it is driven through
 * `buildPlanSection`'s last line (the allowance rows). One NDJSON row per
 * probe: `{kind, id, ..., out}`.
 *
 * Run from inside the pin (Node 24 `~/.nvm/versions/node/v24.13.1/bin`):
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   npx tsx "$V5W/harness/oracle/cases/storyteller-pure.ts" \
 *     > /tmp/oracle-storyteller-pure.ndjson
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { parseBidReply } from '@/lib/storyteller/bid-parse';
import { computeBidTier } from '@/lib/storyteller/tiers';
import { cardFromOutcome } from '@/lib/storyteller/cards';
import { biddableSeats, chooseInvitees, inviteProbability } from '@/lib/storyteller/invite';
import { buildBidSection, buildPlanSection, describeDisposition } from '@/lib/storyteller/prompts';
import { buildBidOffer } from '@/lib/storyteller/bid-call';

type Pairs = [string, string][];
interface OfferSpec {
  tools: Pairs;
  customTools: Pairs;
  destroyOffered: boolean;
  silent: boolean;
}

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'storyteller-pure.json'), 'utf8'));
const out = (row: Record<string, unknown>) => process.stdout.write(JSON.stringify(row) + '\n');
const offerOf = (o: OfferSpec) => ({
  tools: Object.fromEntries(o.tools),
  customTools: Object.fromEntries(o.customTools),
  destroyOffered: o.destroyOffered,
  silent: o.silent,
});
const offerName = (name: string) => offerOf(spec.offers[name]);

// --- parseBidReply ---------------------------------------------------------
(spec.replies as string[]).forEach((raw, i) => out({ kind: 'parse', id: `parse-${i}`, raw, out: parseBidReply(raw) }));

// --- computeBidTier ----------------------------------------------------------
for (const [id, parsed, offer] of spec.tierCases as [string, unknown, string][]) {
  out({ kind: 'tier', id, parsed, offer, out: computeBidTier(parsed as never, offerName(offer) as never) });
}

// --- cardFromOutcome ---------------------------------------------------------
for (const [id, outcome, offer] of spec.cardCases as [string, { ok: boolean }, string][]) {
  const full = outcome.ok ? { ...outcome, discardedToolCalls: [], offered: offerName(offer) } : outcome;
  out({ kind: 'card', id, outcome, offer, out: cardFromOutcome(spec.cardBase, full as never, spec.cardAnsweredAt) });
}

// --- invite --------------------------------------------------------------------
for (const t of spec.inviteProbability as number[]) {
  out({ kind: 'inviteProbability', id: `p-${t}`, talkativeness: t, out: inviteProbability(t) });
}
const characters = new Map(spec.seats.characters as [string, unknown][]);
const seats = biddableSeats(spec.seats.participants, characters as never, spec.seats.impersonating);
const seatRow = (s: (typeof seats)[number]) => ({
  participantId: s.participant.id,
  characterName: s.character?.name ?? null,
  talkativeness: s.talkativeness,
  silent: s.silent,
});
out({ kind: 'seats', id: 'seats', out: seats.map(seatRow) });
for (const [id, filter, draws] of spec.inviteCases as [string, boolean, number[]][]) {
  let i = 0;
  let consumed = 0;
  const random = () => {
    consumed += 1;
    return draws[i++];
  };
  const { invited, keptQuiet } = chooseInvitees(seats, { filter, random });
  out({
    kind: 'invite',
    id,
    filter,
    draws,
    out: { invited: invited.map((s) => s.participant.id), keptQuiet: keptQuiet.map((s) => s.participant.id), consumed },
  });
}

// --- prompts -------------------------------------------------------------------
for (const t of spec.dispositions as number[]) {
  out({ kind: 'disposition', id: `d-${t}`, talkativeness: t, out: describeDisposition(t) });
}
(spec.bidSections as { characterName: string; talkativeness: number; offer: string }[]).forEach((b, i) =>
  out({ kind: 'bidSection', id: `bid-${i}`, ...b, out: buildBidSection({ ...b, offer: offerName(b.offer) as never }) })
);
(spec.planSections as Record<string, unknown>[]).forEach((p, i) =>
  out({ kind: 'planSection', id: `plan-${i}`, params: p, out: buildPlanSection(p as never) })
);
for (const [tier, tools] of spec.allowances as [string, string[]][]) {
  const section = buildPlanSection({ plan: 'p', tier: tier as never, toolsNamed: [], spokenSince: [], allowedTools: tools });
  const lines = section.split('\n');
  out({ kind: 'allowance', id: `allow-${tier}-${tools.length}`, tier, tools, out: lines[lines.length - 1] });
}

// --- buildBidOffer -------------------------------------------------------------
for (const s of spec.slates as { id: string; tools: unknown[]; customToolEffects: Pairs; pluginToolEffects: Pairs; silent: boolean }[]) {
  const offer = buildBidOffer({
    tools: s.tools,
    customToolEffects: new Map(s.customToolEffects) as never,
    pluginToolEffects: new Map(s.pluginToolEffects) as never,
    silent: s.silent,
  });
  out({
    kind: 'offer',
    id: s.id,
    out: { tools: Object.entries(offer.tools), customTools: Object.entries(offer.customTools), destroyOffered: offer.destroyOffered, silent: offer.silent },
  });
}
