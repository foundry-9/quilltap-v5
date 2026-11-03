/**
 * Tier-1 differential oracle for the W4.6a context-feeder PURE leaves — driving
 * v4's REAL exports so the byte-exact builders / formatters / parsers are proven
 * against the Rust port ([`services::off_scene`], [`services::core_whisper`],
 * [`services::memory_recap`], [`services::memory_recap::distill`],
 * [`services::suparna_mail`], [`services::scene_state_tracking`]).
 *
 * Pure-only: no DB. Every function here is a pure v4 export. The stateful reads
 * (frozen-archive ranking against a DB, the recap's vault-summary search, the
 * off-scene SCAN composition, the core-whisper packet assembly) are exercised
 * end-to-end by `build_context_tier3_equivalence`.
 *
 * Pure — runs under `npx tsx` (no DB, no jest). Emits NDJSON to stdout.
 */

import {
  buildOffSceneCharactersContent,
  buildOffSceneCharactersOpaqueContent,
  offSceneCardFields,
  renderOffSceneCard,
} from '@/lib/services/host-notifications/writer';
// P4.D347 (v4 `d68c1af88` / `bdd09b75b`) — REQUIRE a pin at or after `236c04e35`.
import { isOffSceneMentionIntroduction } from '@/lib/services/host-notifications/off-scene-kinds';
import {
  resolveCoreWhisperConfig,
  buildCoreWhisperContent,
  buildCoreWhisperOpaqueContent,
  buildCoreWhisperLLMContext,
  type CorePacket,
} from '@/lib/services/aurora-notifications/core-whisper';
import { buildSuparnaMailLLMContext } from '@/lib/services/suparna-notifications/writer';
import { renderRelevantConversationsBlock } from '@/lib/memory/recall/conversations';
import { capClothingSummary } from '@/lib/memory/cheap-llm-tasks/image-scene-tasks';
// P4.D233 (v4 `acadcc7cd`, bug 172) — REQUIRES a pin at or after `acadcc7cd`.
import { operatorSpeaksWithoutSeat, hasWhisperTargets, isInWhisperScope } from '@/lib/schemas/chat.types';

interface OffCard {
  id: string;
  name: string;
  aliases?: string[];
  pronouns?: { subject: string; object: string; possessive: string } | null;
  identity?: string | null;
  description?: string | null;
}

// ---- off-scene content builders ----
// P4.D233 (v4 `acadcc7cd`, bug 171): `reason` absent → the builders' default
// (`'mentioned'`); the three trailing cases pass it explicitly.
const offSceneCases: Array<{
  id: string;
  chars: OffCard[];
  user: string | null;
  reason?: 'mentioned' | 'left-behind';
}> = [
  {
    id: 'single-identity',
    chars: [{ id: 'c1', name: 'Ada', identity: 'A brilliant analyst named {{char}}.' }],
    user: 'Charlie',
  },
  {
    id: 'single-description-fallback',
    chars: [{ id: 'c1', name: 'Bea', description: 'A gentle beekeeper.', identity: '' }],
    user: null,
  },
  {
    id: 'plural-aliases-pronouns-sorted',
    chars: [
      {
        id: 'c2',
        name: 'Zed',
        aliases: ['Z', 'Zee'],
        pronouns: { subject: 'they', object: 'them', possessive: 'theirs' },
        identity: 'The last one.',
      },
      { id: 'c1', name: 'Ada', identity: 'The first one, {{user}} knows her.' },
    ],
    user: 'Charlie',
  },
  {
    id: 'accented-name-sort',
    chars: [
      { id: 'c2', name: 'Zoë', identity: 'z' },
      { id: 'c1', name: 'Ægis', identity: 'a' },
      { id: 'c3', name: 'ada', identity: 'lowercase ada' },
    ],
    user: null,
  },
  {
    id: 'left-behind-single',
    chars: [{ id: 'c1', name: 'Ada', identity: 'A brilliant analyst named {{char}}.' }],
    user: 'Charlie',
    reason: 'left-behind',
  },
  {
    id: 'left-behind-plural',
    chars: [
      {
        id: 'c2',
        name: 'Zed',
        aliases: ['Z', 'Zee'],
        pronouns: { subject: 'they', object: 'them', possessive: 'theirs' },
        identity: 'The last one.',
      },
      { id: 'c1', name: 'Ada', identity: 'The first one, {{user}} knows her.' },
    ],
    user: 'Charlie',
    reason: 'left-behind',
  },
  {
    id: 'mentioned-explicit',
    chars: [{ id: 'c1', name: 'Bea', description: 'A gentle beekeeper.', identity: '' }],
    user: null,
    reason: 'mentioned',
  },
];

// ---- P4.D347: the whisper scope (an EMPTY target array is the room) ----
// `findIntroducedOffSceneCharacterIds` is GONE at the pin (`9a88f862f` deleted
// the per-turn scan it served); its rows were deleted with it.
const scopeShapes: Array<{ id: string; message: Record<string, unknown> }> = [
  { id: 'null', message: { targetParticipantIds: null } },
  { id: 'absent', message: {} },
  { id: 'empty', message: { targetParticipantIds: [] } },
  { id: 'x', message: { targetParticipantIds: ['x'] } },
  { id: 'xy', message: { targetParticipantIds: ['x', 'y'] } },
];
const scopeTargets: Array<string | null> = [null, 'x', 'z'];

// ---- P4.D347: which Host rows are a "spoken of" introduction ----
const mentionIntroCases: Array<{ id: string; input: Record<string, unknown> }> = [
  { id: 'mention-card', input: { systemSender: 'host', systemKind: 'off-scene-characters', content: 'The Host begs leave to introduce a person spoken of.' } },
  { id: 'legacy-left-behind', input: { systemSender: 'host', systemKind: 'off-scene-characters', content: 'The Host observes that one member of the previous company did not make the journey.' } },
  { id: 'new-kind', input: { systemSender: 'host', systemKind: 'off-scene-left-behind', content: 'The Host observes that one member of the previous company did not make the journey.' } },
  { id: 'non-host', input: { systemSender: 'lantern', systemKind: 'off-scene-characters', content: 'x' } },
  { id: 'absent-content', input: { systemSender: 'host', systemKind: 'off-scene-characters' } },
  { id: 'null-content', input: { systemSender: 'host', systemKind: 'off-scene-characters', content: null } },
];

// ---- P4.D347: the one card (offSceneCardFields + renderOffSceneCard) ----
const cardCases: Array<{ id: string; card: OffCard; user: string | null }> = [
  { id: 'identity', card: { id: 'c1', name: 'Ada', aliases: ['A', 'Ace'], pronouns: { subject: 'she', object: 'her', possessive: 'hers' }, identity: '  Ada keeps the ledgers.  ', description: 'ignored' }, user: 'Charlie' },
  { id: 'description-only', card: { id: 'c2', name: 'Bea', description: '  A gentle beekeeper.  ' }, user: null },
  { id: 'neither', card: { id: 'c3', name: 'Cy', aliases: ['C'] }, user: null },
  { id: 'no-aliases', card: { id: 'c4', name: 'Di', pronouns: { subject: 'they', object: 'them', possessive: 'theirs' }, identity: 'Di.' }, user: null },
  { id: 'no-pronouns', card: { id: 'c5', name: 'Ed', aliases: ['Eddie'], identity: 'Ed.', pronouns: null }, user: null },
  { id: 'templates', card: { id: 'c6', name: 'Flo', identity: '{{char}} keeps {{user}} honest.' }, user: 'Charlie' },
  { id: 'templates-no-user', card: { id: 'c7', name: 'Gus', identity: '{{char}} keeps {{user}} honest.' }, user: null },
  { id: 'blank-identity-falls-through', card: { id: 'c8', name: 'Hal', identity: '   ', description: 'Hal hums.' }, user: null },
];

// ---- core-whisper config precedence ----
const coreConfigCases: Array<{
  id: string;
  chat: { coreWhisperEnabled?: boolean | null; coreWhisperInterval?: number | null } | null;
  character: { coreWhisperEnabled?: boolean | null } | null;
  global:
    | {
        enabled?: boolean;
        interval?: number;
        silenceThreshold?: number;
        packetTokenBudget?: number;
        fireOnContextTransition?: boolean;
      }
    | null;
}> = [
  { id: 'all-defaults', chat: null, character: null, global: null },
  { id: 'chat-disables', chat: { coreWhisperEnabled: false, coreWhisperInterval: 7 }, character: { coreWhisperEnabled: true }, global: null },
  { id: 'character-disables', chat: {}, character: { coreWhisperEnabled: false }, global: null },
  {
    id: 'global-overrides',
    chat: null,
    character: null,
    global: { enabled: false, interval: 20, silenceThreshold: 5, packetTokenBudget: 2048, fireOnContextTransition: false },
  },
  { id: 'chat-null-falls-to-character', chat: { coreWhisperEnabled: null, coreWhisperInterval: null }, character: { coreWhisperEnabled: true }, global: { enabled: false } },
];

// ---- core-whisper content builders (over synthetic packets) ----
const corePackets: Array<{ id: string; packet: CorePacket }> = [
  {
    id: 'own-and-shared',
    packet: {
      files: [
        { path: 'Core/manifesto.md', body: 'I build things.' },
        { path: 'Core/values.md', body: 'Kindness above all.', sourceLabel: 'Household' },
      ],
      approxTokens: 0,
    },
  },
  {
    id: 'single-own',
    packet: { files: [{ path: 'Core/a.md', body: 'One file.' }], approxTokens: 0 },
  },
];

// ---- recap: renderRelevantConversationsBlock ----
const relevantBlockCases: Array<{ id: string; matches: Array<{ conversationId: string; conversationTitle: string; relativePath: string; score: number; firstMessageAt?: string | null; lastMessageAt?: string | null }> }> = [
  { id: 'empty', matches: [] },
  {
    id: 'two-entries',
    matches: [
      { conversationId: 'cid-1', conversationTitle: 'A Talk', relativePath: 'x', score: 0.9 },
      { conversationId: 'cid-2', conversationTitle: 'Another', relativePath: 'y', score: 0.8 },
    ],
  },
  // Episodic recall (P4.d13): the date parenthetical from firstMessageAt.
  {
    id: 'dated-and-undated',
    matches: [
      {
        conversationId: 'cid-3',
        conversationTitle: 'The Harbor Visit',
        relativePath: 'z',
        score: 0.9,
        firstMessageAt: '2026-07-14T10:00:00.000Z',
        lastMessageAt: '2026-07-14T12:00:00.000Z',
      },
      {
        conversationId: 'cid-4',
        conversationTitle: 'Undated',
        relativePath: 'w',
        score: 0.5,
        firstMessageAt: null,
        lastMessageAt: null,
      },
    ],
  },
];

// ---- suparna mail llm-context ----
interface Letter {
  path: string;
  from: string;
  sentAt: string;
  body: string;
  alerted: boolean;
  inReplyTo: string | null;
}
const suparnaCases: Array<{ id: string; letters: Letter[] }> = [
  { id: 'empty', letters: [] },
  {
    id: 'single',
    letters: [{ path: 'Mail/friday-2026-02-01.md', from: 'Friday', sentAt: '2026-02-01T09:05:00.000Z', body: '  Hello there.  ', alerted: false, inReplyTo: null }],
  },
  {
    id: 'plural-with-blank',
    letters: [
      { path: 'Mail/a.md', from: 'Ada', sentAt: '2026-02-02T09:00:00.000Z', body: 'First.', alerted: false, inReplyTo: null },
      { path: 'Mail/b.md', from: 'Bob', sentAt: '2026-02-01T09:00:00.000Z', body: '   ', alerted: false, inReplyTo: null },
    ],
  },
];

// ---- operatorSpeaksWithoutSeat (P4.D233, bug 172) ----
// `undefined` rides as the absent `chatType` key; JSON cannot carry it, so the
// id names it and the emitter passes it through.
const operatorSpeaksCases: Array<{ id: string; chatType: string | null | undefined }> = [
  { id: 'autonomous', chatType: 'autonomous' },
  { id: 'salon', chatType: 'salon' },
  { id: 'help', chatType: 'help' },
  { id: 'brahma', chatType: 'brahma' },
  { id: 'empty', chatType: '' },
  { id: 'null', chatType: null },
  { id: 'undefined', chatType: undefined },
  { id: 'Autonomous-cased', chatType: 'Autonomous' },
];

// ---- scene-state capClothingSummary ----
const clothingCases: Array<{ id: string; value: unknown }> = [
  { id: 'trimmed', value: '  a shirt  ' },
  { id: 'null', value: null },
  { id: 'number', value: 42 },
  { id: 'over-200', value: 'x'.repeat(250) },
  { id: 'exactly-200', value: 'y'.repeat(200) },
];

async function main(): Promise<void> {
  const out: string[] = [];
  const emit = (kind: string, id: string, value: unknown) =>
    out.push(JSON.stringify({ kind, id, value }));

  for (const c of offSceneCases) {
    emit('off_scene_content', c.id, buildOffSceneCharactersContent(c.chars as never, c.user, c.reason));
    emit(
      'off_scene_opaque',
      c.id,
      buildOffSceneCharactersOpaqueContent(c.chars as never, c.user, c.reason),
    );
  }
  for (const m of scopeShapes) {
    emit('has_whisper_targets', m.id, hasWhisperTargets(m.message as never));
    for (const t of scopeTargets) {
      emit('whisper_scope', `${m.id}|${t ?? 'null'}`, isInWhisperScope(m.message as never, t));
    }
  }
  for (const c of mentionIntroCases) {
    emit('mention_intro', c.id, isOffSceneMentionIntroduction(c.input as never));
  }
  for (const c of cardCases) {
    emit('card_fields', c.id, offSceneCardFields(c.card as never, c.user));
    emit('render_card', c.id, renderOffSceneCard(c.card as never, c.user));
  }
  for (const c of coreConfigCases) {
    emit('core_config', c.id, resolveCoreWhisperConfig(c.chat as never, c.character as never, c.global as never));
  }
  for (const c of corePackets) {
    emit('core_persona', c.id, buildCoreWhisperContent(c.packet));
    emit('core_opaque', c.id, buildCoreWhisperOpaqueContent(c.packet));
    emit('core_llm', c.id, buildCoreWhisperLLMContext(c.packet));
  }
  for (const c of relevantBlockCases) {
    emit('relevant_block', c.id, renderRelevantConversationsBlock(c.matches as never));
  }
  for (const c of suparnaCases) {
    emit('suparna_llm', c.id, buildSuparnaMailLLMContext(c.letters as never));
  }
  for (const c of clothingCases) {
    emit('clothing_cap', c.id, capClothingSummary(c.value));
  }
  for (const c of operatorSpeaksCases) {
    emit('operator_speaks', c.id, operatorSpeaksWithoutSeat(c.chatType));
  }

  for (const line of out) process.stdout.write(line + '\n');
}

void main();
